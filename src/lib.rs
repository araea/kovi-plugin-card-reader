//! kovi-plugin-card-reader
//!
//! 一个用于解析 SillyTavern (酒馆) 图片角色卡的插件。
//! 支持读取 PNG 图片中的元数据（V2 和 V3 格式），并导出为 JSON 和易读的 TXT 文件。

// =============================
//          Modules
// =============================

mod config {
    use kovi::toml;
    use kovi::utils::{load_toml_data, save_toml_data};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    pub static CONFIG: std::sync::OnceLock<Arc<RwLock<Config>>> = std::sync::OnceLock::new();

    pub fn get() -> Arc<RwLock<Config>> {
        CONFIG.get().cloned().expect("Config not initialized")
    }

    const DEFAULT_CONFIG: &str = r#"
# 插件开关
enabled = true

# 触发指令
commands = ["读卡", "解析卡", "看卡", "card"]

# 指令前缀 (留空则直接匹配指令)
prefixes = []

# 是否在解析完成后，发送简短的文本预览（除了发送文件外）
text_preview = true
"#;

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct Config {
        pub enabled: bool,
        pub commands: Vec<String>,
        pub prefixes: Vec<String>,
        pub text_preview: bool,

        #[serde(skip)]
        config_path: PathBuf,
    }

    impl Config {
        pub fn load(data_dir: PathBuf) -> Arc<RwLock<Self>> {
            if !data_dir.exists() {
                std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");
            }
            let config_path = data_dir.join("config.toml");

            let default: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
            let mut config = load_toml_data(default, config_path.clone()).unwrap_or_else(|_| {
                let c: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
                c
            });

            config.config_path = config_path;

            Arc::new(RwLock::new(config))
        }

        pub fn save(&self) {
            let _ = save_toml_data(self, &self.config_path);
        }
    }
}

mod types {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    // 通用的角色卡结构，用于提取字段生成 TXT
    // 实际数据可能比这个复杂，但这包含了核心阅读字段
    #[derive(Debug, Serialize, Deserialize, Clone, Default)]
    pub struct CharacterCard {
        #[serde(default)]
        pub name: String,
        #[serde(default)]
        pub description: String,
        #[serde(default)]
        pub personality: String,
        #[serde(default)]
        pub first_mes: String,
        #[serde(default)]
        pub scenario: String,
        #[serde(default)]
        pub creator_notes: String,
        #[serde(default)]
        pub system_prompt: String,
        #[serde(default)]
        pub post_history_instructions: String,
        #[serde(default)]
        pub alternate_greetings: Vec<String>,
        #[serde(default)]
        pub tags: Vec<String>,
        #[serde(default)]
        pub creator: String,
        #[serde(default)]
        pub character_version: String,

        // 捕获其他所有字段以便完整导出 JSON
        #[serde(flatten)]
        pub other: serde_json::Map<String, Value>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct V3Wrapper {
        pub spec: String,
        pub spec_version: String,
        pub data: CharacterCard,
    }
}

mod parser {
    use super::types::{CharacterCard, V3Wrapper};
    use anyhow::{Result, anyhow};
    use base64::{Engine as _, engine::general_purpose};
    use kovi::serde_json;
    use std::io::{Cursor, Read, Seek, SeekFrom};

    /// 从 PNG 字节中解析角色卡数据
    pub fn parse_png(bytes: &[u8]) -> Result<(CharacterCard, String)> {
        let mut cursor = Cursor::new(bytes);

        // 1. 验证 PNG 头 (8 bytes)
        let mut header = [0u8; 8];
        cursor.read_exact(&mut header)?;
        if header != [137, 80, 78, 71, 13, 10, 26, 10] {
            return Err(anyhow!("不是有效的 PNG 图片"));
        }

        let mut ccv3_data: Option<String> = None;
        let mut chara_data: Option<String> = None;

        // 2. 遍历 Chunks
        loop {
            // 读取长度 (4 bytes, Big Endian)
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
                // 读取数据
                let mut data_buf = vec![0u8; length as usize];
                cursor.read_exact(&mut data_buf)?;

                // tEXt 格式: Keyword + Null separator (0x00) + Text
                if let Some(null_pos) = data_buf.iter().position(|&b| b == 0) {
                    if let Ok(keyword) = std::str::from_utf8(&data_buf[..null_pos]) {
                        let text_bytes = &data_buf[null_pos + 1..];
                        if let Ok(text) = std::str::from_utf8(text_bytes) {
                            let key_lower = keyword.to_lowercase();
                            if key_lower == "ccv3" {
                                ccv3_data = Some(text.to_string());
                            } else if key_lower == "chara" {
                                chara_data = Some(text.to_string());
                            }
                        }
                    }
                }

                // 跳过 CRC (4 bytes)
                cursor.seek(SeekFrom::Current(4))?;
            } else {
                // 跳过数据 + CRC
                cursor.seek(SeekFrom::Current((length + 4) as i64))?;
            }
        }

        // 3. 优先处理 V3，其次 V2
        if let Some(b64) = ccv3_data {
            let json_str = decode_base64(&b64)?;
            let wrapper: V3Wrapper =
                serde_json::from_str(&json_str).map_err(|e| anyhow!("V3 JSON 解析失败: {}", e))?;
            // 重新序列化完整的 JSON 字符串以供导出
            let full_json = serde_json::to_string_pretty(&wrapper)?;
            return Ok((wrapper.data, full_json));
        }

        if let Some(b64) = chara_data {
            let json_str = decode_base64(&b64)?;
            let card: CharacterCard =
                serde_json::from_str(&json_str).map_err(|e| anyhow!("V2 JSON 解析失败: {}", e))?;
            let full_json = serde_json::to_string_pretty(&card)?;
            return Ok((card, full_json));
        }

        Err(anyhow!("未在图片中找到角色卡信息 (chara/ccv3)"))
    }

    fn decode_base64(input: &str) -> Result<String> {
        let bytes = general_purpose::STANDARD.decode(input)?;
        let s = String::from_utf8(bytes)?;
        Ok(s)
    }

    pub fn format_readable_text(card: &CharacterCard) -> String {
        let mut s = String::new();
        let sep = "-".repeat(40);

        s.push_str(&format!("【角色名称】: {}\n", card.name));
        if !card.creator.is_empty() {
            s.push_str(&format!("【创 建 者】: {}\n", card.creator));
        }
        if !card.character_version.is_empty() {
            s.push_str(&format!("【版    本】: {}\n", card.character_version));
        }
        if !card.tags.is_empty() {
            s.push_str(&format!("【标    签】: {}\n", card.tags.join(", ")));
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

        s
    }
}

mod utils {
    use kovi::MsgEvent;
    use std::sync::Arc;

    pub async fn get_image_url(
        event: &Arc<MsgEvent>,
        bot: &Arc<kovi::RuntimeBot>,
    ) -> Option<String> {
        // 1. 检查当前消息
        for seg in event.message.iter() {
            if seg.type_ == "image"
                && let Some(url) = seg.data.get("url").and_then(|u| u.as_str())
            {
                return Some(url.to_string());
            }
        }

        // 2. 检查引用消息
        let reply_id = event.message.iter().find_map(|seg| {
            if seg.type_ == "reply" {
                seg.data.get("id").and_then(|v| v.as_str())
            } else {
                None
            }
        })?;

        if let Ok(reply_id_int) = reply_id.parse::<i32>()
            && let Ok(msg_res) = bot.get_msg(reply_id_int).await
            && let Some(segments) = msg_res.data.get("message").and_then(|v| v.as_array())
        {
            for seg in segments {
                if let Some(type_) = seg.get("type").and_then(|t| t.as_str())
                    && type_ == "image"
                    && let Some(url) = seg
                        .get("data")
                        .and_then(|d| d.get("url"))
                        .and_then(|u| u.as_str())
                {
                    return Some(url.to_string());
                }
            }
        }
        None
    }

    pub fn parse_command(text: &str, prefixes: &[String], commands: &[String]) -> bool {
        let text = text.trim();
        let clean_text = if !prefixes.is_empty() {
            let mut found = None;
            let mut sorted_prefixes = prefixes.to_vec();
            sorted_prefixes.sort_by_key(|b| std::cmp::Reverse(b.len()));

            for p in sorted_prefixes {
                if text.starts_with(&p) {
                    found = Some(&text[p.len()..]);
                    break;
                }
            }
            match found {
                Some(t) => t.trim(),
                None => return false,
            }
        } else {
            text
        };
        commands.contains(&clean_text.to_string())
    }
}

// =============================
//      Main Plugin Logic
// =============================

use kovi::{PluginBuilder, log};
use kovi_plugin_expand_napcat::NapCatApi;
use std::fs::File;
use std::io::Write;

#[kovi::plugin]
async fn main() {
    let bot = PluginBuilder::get_runtime_bot();
    let data_dir = bot.get_data_path();

    let config_lock = config::Config::load(data_dir.clone());
    config::CONFIG.set(config_lock.clone()).ok();

    PluginBuilder::on_msg(move |event| {
        let bot = bot.clone();
        let config_lock = config_lock.clone();

        async move {
            let text = match event.borrow_text() {
                Some(t) => t,
                None => return,
            };

            let (commands, prefixes, enabled, text_preview) = {
                let cfg = config_lock.read().unwrap();
                (
                    cfg.commands.clone(),
                    cfg.prefixes.clone(),
                    cfg.enabled,
                    cfg.text_preview,
                )
            };

            if !enabled {
                return;
            }

            // 1. 匹配指令
            if utils::parse_command(text, &prefixes, &commands) {
                // 2. 获取图片
                let img_url = match utils::get_image_url(&event, &bot).await {
                    Some(u) => u,
                    None => {
                        event.reply("⚠️ 请附带角色卡图片或引用图片消息");
                        return;
                    }
                };

                event.reply("🔍 正在读取角色卡，请稍候...");

                // 3. 下载图片
                let img_bytes = match reqwest::get(&img_url).await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(b) => b,
                        Err(e) => {
                            event.reply(format!("❌ 图片下载失败: {}", e));
                            return;
                        }
                    },
                    Err(e) => {
                        event.reply(format!("❌ 网络请求失败: {}", e));
                        return;
                    }
                };

                // 4. 解析 PNG
                let parse_result = parser::parse_png(&img_bytes);

                match parse_result {
                    Ok((card, json_str)) => {
                        let safe_name = card
                            .name
                            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
                        let safe_name = if safe_name.trim().is_empty() {
                            "character".to_string()
                        } else {
                            safe_name
                        };

                        // 5. 生成文件内容
                        let readable_text = parser::format_readable_text(&card);

                        // 6. 保存临时文件
                        // 增加时间戳防止文件名冲突
                        let timestamp = kovi::chrono::Local::now().format("%H%M%S").to_string();
                        let json_filename = format!("{}_{}.json", safe_name, timestamp);
                        let txt_filename = format!("{}_{}_read.txt", safe_name, timestamp);

                        let data_path = bot.get_data_path();
                        let json_path = data_path.join(&json_filename);
                        let txt_path = data_path.join(&txt_filename);

                        // 写入 JSON
                        if let Ok(mut f) = File::create(&json_path) {
                            let _ = f.write_all(json_str.as_bytes());
                        }

                        // 写入 TXT
                        if let Ok(mut f) = File::create(&txt_path) {
                            let _ = f.write_all(readable_text.as_bytes());
                        }

                        // 7. 发送文件
                        let json_path_str = json_path.to_string_lossy().to_string();
                        let txt_path_str = txt_path.to_string_lossy().to_string();

                        let mut success = true;

                        if let Some(group_id) = event.group_id {
                            if let Err(e) = bot
                                .upload_group_file(group_id, &json_path_str, &json_filename, None)
                                .await
                            {
                                log::error!("Failed to upload group file JSON: {}", e);
                                success = false;
                            }
                            // 稍微延迟一下防止并发上传冲突
                            kovi::tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            if let Err(e) = bot
                                .upload_group_file(group_id, &txt_path_str, &txt_filename, None)
                                .await
                            {
                                log::error!("Failed to upload group file TXT: {}", e);
                                success = false;
                            }
                        } else {
                            if let Err(e) = bot
                                .upload_private_file(event.user_id, &json_path_str, &json_filename)
                                .await
                            {
                                log::error!("Failed to upload private file JSON: {}", e);
                                success = false;
                            }
                            kovi::tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            if let Err(e) = bot
                                .upload_private_file(event.user_id, &txt_path_str, &txt_filename)
                                .await
                            {
                                log::error!("Failed to upload private file TXT: {}", e);
                                success = false;
                            }
                        }

                        if !success {
                            event.reply("⚠️ 文件上传过程中出现部分错误，请检查日志。");
                        } else if text_preview {
                            let preview = format!(
                                "✅ 解析成功: {}\n作者: {}\n字数: {}\n(详细内容请查看已发送的文件)",
                                card.name,
                                if card.creator.is_empty() {
                                    "未知"
                                } else {
                                    &card.creator
                                },
                                readable_text.chars().count()
                            );
                            event.reply(preview);
                        }

                        // 8. 删除临时文件
                        let _ = std::fs::remove_file(&json_path);
                        let _ = std::fs::remove_file(&txt_path);
                    }
                    Err(e) => {
                        event.reply(format!("❌ 解析失败: {}", e));
                    }
                }
            }
        }
    });

    PluginBuilder::drop(move || {
        let config_lock = config::get();
        async move {
            let config = {
                let guard = config_lock.read().unwrap();
                guard.clone()
            };
            config.save();
        }
    });
}
