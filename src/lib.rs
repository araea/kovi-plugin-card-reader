use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use kovi::bot::runtimebot::RuntimeBot;
use kovi::serde_json::Value;
use kovi::{MsgEvent, PluginBuilder, log};
use kovi_plugin_expand_napcat::NapCatApi;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::sync::Arc;

// --- 数据结构定义 ---

// 简化版的角色卡结构，用于提取关键信息生成 TXT
#[derive(Debug, Serialize, Deserialize, Clone)]
struct CharacterCard {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    personality: String,
    #[serde(default)]
    first_mes: String,
    #[serde(default)]
    scenario: String,
    #[serde(default)]
    creator_notes: String,
    #[serde(default)]
    system_prompt: String,
    #[serde(default)]
    post_history_instructions: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    creator: String,
    #[serde(default)]
    character_version: String,
    // 捕获其他所有字段以便导出完整的 JSON
    #[serde(flatten)]
    extra: std::collections::HashMap<String, Value>,
}

// V3 格式通常包裹在 spec 字段中
#[derive(Debug, Serialize, Deserialize)]
struct V3Wrapper {
    spec: String,
    spec_version: String,
    data: CharacterCard,
}

// --- 核心逻辑 ---

#[kovi::plugin]
async fn main() {
    let bot = PluginBuilder::get_runtime_bot();

    PluginBuilder::on_msg(move |event| {
        let bot = bot.clone();
        async move {
            // 1. 简单的指令匹配
            let text = event.borrow_text().unwrap_or("");
            if !["读卡", "解析卡", "看卡"].contains(&text.trim()) {
                return;
            }

            // 2. 获取图片 URL (支持直接发送或引用)
            let img_url = get_image_url(&event, &bot).await;
            let img_url = match img_url {
                Some(url) => url,
                None => {
                    event.reply("⚠️ 请附带图片或引用一张含有角色卡信息的图片。");
                    return;
                }
            };

            event.reply("🔍 正在读取角色卡，请稍候...");

            // 3. 下载并解析
            match process_card(&bot, &event, &img_url).await {
                Ok(_) => {
                    // 成功不做额外处理，过程里已经发了文件
                }
                Err(e) => {
                    log::error!("解析角色卡失败: {:?}", e);
                    event.reply(format!("❌ 解析失败: {}", e));
                }
            }
        }
    });
}

// --- 辅助函数 ---

/// 获取图片链接
async fn get_image_url(event: &Arc<MsgEvent>, bot: &Arc<RuntimeBot>) -> Option<String> {
    // 检查当前消息
    for seg in event.message.iter() {
        if seg.type_ == "image"
            && let Some(url) = seg.data.get("url").and_then(|v| v.as_str())
        {
            return Some(url.to_string());
        }
    }
    // 检查引用消息
    if let Some(reply) = event.message.iter().find(|s| s.type_ == "reply")
        && let Some(id_str) = reply.data.get("id").and_then(|v| v.as_str())
        && let Ok(id) = id_str.parse::<i32>()
        && let Ok(res) = bot.get_msg(id).await
        && let Some(segs) = res.data.get("message").and_then(|v| v.as_array())
    {
        for seg in segs {
            if seg["type"] == "image"
                && let Some(url) = seg["data"]["url"].as_str()
            {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// 处理流程：下载 -> 解析 -> 生成文件 -> 上传
async fn process_card(bot: &Arc<RuntimeBot>, event: &Arc<MsgEvent>, url: &str) -> Result<()> {
    // 1. 下载图片
    let resp = reqwest::get(url).await?;
    let bytes = resp.bytes().await?;

    // 2. 解析 PNG 数据 (参考 JS 逻辑)
    let (card_data, json_string) = parse_png_chunks(&bytes)?;

    // 3. 准备文件路径
    let data_dir = bot.get_data_path(); // Kovi 提供的插件数据目录
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
    }

    // 清理文件名防止非法字符
    let safe_name = card_data
        .name
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let json_filename = format!("{}.json", safe_name);
    let txt_filename = format!("{}_read.txt", safe_name);

    let json_path = data_dir.join(&json_filename);
    let txt_path = data_dir.join(&txt_filename);

    // 4. 写入文件
    // 写入 JSON
    let mut json_file = File::create(&json_path)?;
    json_file.write_all(json_string.as_bytes())?;

    // 写入美化后的 TXT
    let readable_text = format_readable_text(&card_data);
    let mut txt_file = File::create(&txt_path)?;
    txt_file.write_all(readable_text.as_bytes())?;

    // 5. 发送文件 (使用 NapCat 扩展 API)
    let json_path_str = json_path.to_string_lossy().to_string();
    let txt_path_str = txt_path.to_string_lossy().to_string();

    if let Some(group_id) = event.group_id {
        // 群聊上传
        bot.upload_group_file(group_id, &json_path_str, &json_filename, None)
            .await
            .map_err(|e| anyhow!("上传JSON失败: {:?}", e))?;

        // 稍微延迟避免并发冲突
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        bot.upload_group_file(group_id, &txt_path_str, &txt_filename, None)
            .await
            .map_err(|e| anyhow!("上传TXT失败: {:?}", e))?;
    } else {
        // 私聊上传
        bot.upload_private_file(event.user_id, &json_path_str, &json_filename)
            .await
            .map_err(|e| anyhow!("上传JSON失败: {:?}", e))?;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        bot.upload_private_file(event.user_id, &txt_path_str, &txt_filename)
            .await
            .map_err(|e| anyhow!("上传TXT失败: {:?}", e))?;
    }

    // 6. 发送简报
    let preview = format!(
        "✅ 解析成功: {}\n作者: {}\n文件已上传，请查看详细设定。",
        card_data.name,
        if card_data.creator.is_empty() {
            "未知"
        } else {
            &card_data.creator
        }
    );
    event.reply(preview);

    Ok(())
}

/// 核心解析逻辑：遍历 PNG Chunks
fn parse_png_chunks(bytes: &[u8]) -> Result<(CharacterCard, String)> {
    let mut cursor = Cursor::new(bytes);
    let mut header = [0u8; 8];
    cursor.read_exact(&mut header)?;

    // 验证 PNG 头
    if header != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err(anyhow!("不是有效的 PNG 图片"));
    }

    let mut ccv3_data: Option<String> = None;
    let mut chara_data: Option<String> = None;

    loop {
        // 读取长度 (4 bytes, big endian)
        let mut len_buf = [0u8; 4];
        if cursor.read_exact(&mut len_buf).is_err() {
            break; // EOF
        }
        let length = u32::from_be_bytes(len_buf) as u64;

        // 读取类型 (4 bytes)
        let mut type_buf = [0u8; 4];
        cursor.read_exact(&mut type_buf)?;
        let chunk_type = std::str::from_utf8(&type_buf).unwrap_or("");

        if chunk_type == "tEXt" {
            // tEXt 格式: Keyword + Null(0x00) + Text
            let mut data_buf = vec![0u8; length as usize];
            cursor.read_exact(&mut data_buf)?;

            if let Some(null_pos) = data_buf.iter().position(|&b| b == 0) {
                let keyword = std::str::from_utf8(&data_buf[..null_pos]).unwrap_or("");
                let text_data = std::str::from_utf8(&data_buf[null_pos + 1..]).unwrap_or("");

                if keyword.eq_ignore_ascii_case("ccv3") {
                    ccv3_data = Some(text_data.to_string());
                } else if keyword.eq_ignore_ascii_case("chara") {
                    chara_data = Some(text_data.to_string());
                }
            }

            // Skip CRC
            cursor.seek(SeekFrom::Current(4))?;
        } else {
            // Skip Data + CRC
            cursor.seek(SeekFrom::Current((length + 4) as i64))?;
        }
    }

    // 优先处理 V3，其次 V2
    if let Some(b64) = ccv3_data {
        let json_str = decode_base64(&b64)?;
        let wrapper: V3Wrapper =
            serde_json::from_str(&json_str).map_err(|e| anyhow!("V3 JSON 解析错误: {}", e))?;
        // 重新序列化以便获得格式化的 JSON 字符串
        let pretty_json = serde_json::to_string_pretty(&wrapper)?;
        return Ok((wrapper.data, pretty_json));
    }

    if let Some(b64) = chara_data {
        let json_str = decode_base64(&b64)?;
        let card: CharacterCard =
            serde_json::from_str(&json_str).map_err(|e| anyhow!("V2 JSON 解析错误: {}", e))?;
        let pretty_json = serde_json::to_string_pretty(&card)?;
        return Ok((card, pretty_json));
    }

    Err(anyhow!("未在图片中找到角色卡元数据 (chara/ccv3)"))
}

fn decode_base64(input: &str) -> Result<String> {
    let bytes = general_purpose::STANDARD.decode(input)?;
    let s = String::from_utf8(bytes)?;
    Ok(s)
}

/// 生成易读的 TXT 内容
fn format_readable_text(card: &CharacterCard) -> String {
    let mut s = String::new();
    let sep = "=".repeat(30);

    s.push_str(&format!("【角色名称】: {}\n", card.name));
    if !card.character_version.is_empty() {
        s.push_str(&format!("【版本】: {}\n", card.character_version));
    }
    if !card.creator.is_empty() {
        s.push_str(&format!("【作者】: {}\n", card.creator));
    }
    if !card.tags.is_empty() {
        s.push_str(&format!("【标签】: {}\n", card.tags.join(", ")));
    }

    s.push_str(&format!(
        "\n{}\n【角色描述 (Description)】\n{}\n",
        sep, card.description
    ));

    s.push_str(&format!(
        "\n{}\n【开场白 (First Message)】\n{}\n",
        sep, card.first_mes
    ));

    if !card.personality.is_empty() {
        s.push_str(&format!(
            "\n{}\n【性格 (Personality)】\n{}\n",
            sep, card.personality
        ));
    }

    if !card.scenario.is_empty() {
        s.push_str(&format!(
            "\n{}\n【场景 (Scenario)】\n{}\n",
            sep, card.scenario
        ));
    }

    if !card.system_prompt.is_empty() {
        s.push_str(&format!(
            "\n{}\n【系统提示词 (System Prompt)】\n{}\n",
            sep, card.system_prompt
        ));
    }

    if !card.creator_notes.is_empty() {
        s.push_str(&format!(
            "\n{}\n【作者注释 (Creator Notes)】\n{}\n",
            sep, card.creator_notes
        ));
    }

    if !card.post_history_instructions.is_empty() {
        s.push_str(&format!(
            "\n{}\n【历史后指令 (Post History Instructions)】\n{}\n",
            sep, card.post_history_instructions
        ));
    }

    s
}
