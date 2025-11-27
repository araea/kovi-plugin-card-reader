//! kovi-plugin-card-reader
//!
//! 一个用于解析 SillyTavern (酒馆) 图片角色卡的插件。
//! 支持读取 PNG 图片中的元数据（V2 和 V3 格式），并导出为 JSON 和易读的 TXT 文件。
//!
//! 修改说明：
//! 1. 使用了完整的 V3 数据结构，支持解析世界书、正则脚本等。
//! 2. 导出 TXT 时添加 UTF-8 BOM 头，确保在移动端和 Windows 上不乱码。

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

    /// 根结构体：角色卡 V3 规范
    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct CharaCardV3 {
        #[serde(default)]
        pub spec: String,
        #[serde(default)]
        pub spec_version: String,
        pub data: CharacterData,
    }

    /// 核心角色数据
    #[derive(Debug, Serialize, Deserialize, Clone, Default)]
    pub struct CharacterData {
        /// 基础信息
        #[serde(default)]
        pub name: String,
        #[serde(default)]
        pub description: String,
        #[serde(default)]
        pub personality: String,
        #[serde(default)]
        pub scenario: String,
        #[serde(default)]
        pub first_mes: String,
        #[serde(default)]
        pub mes_example: String,

        /// 创建者信息
        #[serde(default)]
        pub creator_notes: String,
        #[serde(default)]
        pub system_prompt: String,
        #[serde(default)]
        pub post_history_instructions: String,
        #[serde(default)]
        pub creator: String,
        #[serde(default)]
        pub character_version: String,

        /// 列表数据
        #[serde(default)]
        pub alternate_greetings: Vec<String>,
        #[serde(default)]
        pub tags: Vec<String>,
        #[serde(default)]
        pub group_only_greetings: Vec<String>,

        /// 嵌套结构 (使用 Option 处理 V2 格式或缺失情况)
        #[serde(default)]
        pub character_book: Option<CharacterBook>,
        #[serde(default)]
        pub extensions: Option<CardExtensions>,
    }

    /// 世界书/传说书结构
    #[derive(Debug, Serialize, Deserialize, Clone, Default)]
    pub struct CharacterBook {
        #[serde(default)]
        pub entries: Vec<LoreEntry>,
        #[serde(default)]
        pub name: String,
        #[serde(default)]
        pub description: Option<String>,
        #[serde(default)]
        pub scan_depth: Option<i32>,
    }

    /// 世界书条目 (Lore Entry)
    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct LoreEntry {
        #[serde(default)]
        pub id: i32,
        #[serde(default)]
        pub keys: Vec<String>,
        #[serde(default)]
        pub secondary_keys: Vec<String>,
        #[serde(default)]
        pub comment: String,
        #[serde(default)]
        pub content: String,

        #[serde(default)]
        pub constant: bool,
        #[serde(default)]
        pub selective: bool,
        #[serde(default)]
        pub insertion_order: i32,
        #[serde(default)]
        pub enabled: bool,
        #[serde(default)]
        pub position: String,
        #[serde(default)]
        pub use_regex: bool,

        #[serde(default)]
        pub extensions: serde_json::Value, // 简化处理，只存不读细节，除非需要打印
    }

    /// 角色卡扩展功能
    #[derive(Debug, Serialize, Deserialize, Clone, Default)]
    pub struct CardExtensions {
        #[serde(default)]
        pub fav: bool,
        #[serde(default)]
        pub world: String,
        #[serde(default)]
        pub talkativeness: String,
        #[serde(default)]
        pub depth_prompt: Option<DepthPrompt>,
        #[serde(default)]
        pub regex_scripts: Vec<RegexScript>,
    }

    /// 深度提示词配置
    #[derive(Debug, Serialize, Deserialize, Clone, Default)]
    pub struct DepthPrompt {
        #[serde(default)]
        pub depth: i32,
        #[serde(default)]
        pub prompt: String,
        #[serde(default)]
        pub role: String,
    }

    /// 正则脚本配置
    #[derive(Debug, Serialize, Deserialize, Clone, Default)]
    pub struct RegexScript {
        #[serde(default)]
        pub id: String,
        #[serde(rename = "scriptName", default)]
        pub script_name: String,
        #[serde(rename = "findRegex", default)]
        pub find_regex: String,
        #[serde(rename = "replaceString", default)]
        pub replace_string: String,
        #[serde(rename = "runOnEdit", default)]
        pub run_on_edit: bool,
        #[serde(default)]
        pub disabled: bool,
        #[serde(rename = "markdownOnly", default)]
        pub markdown_only: bool,
        #[serde(rename = "promptOnly", default)]
        pub prompt_only: bool,
        #[serde(rename = "minDepth")]
        pub min_depth: Option<i32>,
        #[serde(rename = "maxDepth")]
        pub max_depth: Option<i32>,
    }
}

mod parser {
    use super::types::{CharaCardV3, CharacterData};
    use anyhow::{Result, anyhow};
    use base64::{Engine as _, engine::general_purpose};
    use kovi::serde_json;
    use std::io::{Cursor, Read, Seek, SeekFrom};

    /// 从 PNG 字节中解析角色卡数据
    /// 返回: (核心数据结构, 完整的 JSON 字符串)
    pub fn parse_png(bytes: &[u8]) -> Result<(CharacterData, String)> {
        let mut cursor = Cursor::new(bytes);

        // 1. 验证 PNG 头
        let mut header = [0u8; 8];
        cursor.read_exact(&mut header)?;
        if header != [137, 80, 78, 71, 13, 10, 26, 10] {
            return Err(anyhow!("不是有效的 PNG 图片"));
        }

        let mut ccv3_data: Option<String> = None;
        let mut chara_data: Option<String> = None;

        // 2. 遍历 Chunks
        loop {
            let mut len_buf = [0u8; 4];
            if cursor.read_exact(&mut len_buf).is_err() {
                break;
            }
            let length = u32::from_be_bytes(len_buf) as u64;

            let mut type_buf = [0u8; 4];
            cursor.read_exact(&mut type_buf)?;
            let chunk_type = std::str::from_utf8(&type_buf).unwrap_or("");

            if chunk_type == "tEXt" {
                let mut data_buf = vec![0u8; length as usize];
                cursor.read_exact(&mut data_buf)?;

                if let Some(null_pos) = data_buf.iter().position(|&b| b == 0)
                    && let Ok(keyword) = std::str::from_utf8(&data_buf[..null_pos])
                {
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
                cursor.seek(SeekFrom::Current(4))?; // Skip CRC
            } else {
                cursor.seek(SeekFrom::Current((length + 4) as i64))?;
            }
        }

        // 3. 优先处理 V3 (ccv3)
        if let Some(b64) = ccv3_data {
            let json_str = decode_base64(&b64)?;
            let wrapper: CharaCardV3 =
                serde_json::from_str(&json_str).map_err(|e| anyhow!("V3 JSON 解析失败: {}", e))?;
            let full_json = serde_json::to_string_pretty(&wrapper)?;
            return Ok((wrapper.data, full_json));
        }

        // 4. 降级处理 V2 (chara)
        if let Some(b64) = chara_data {
            let json_str = decode_base64(&b64)?;
            // V2 直接对应 CharacterData 的字段，只是没有 extensions 和 character_book
            let card: CharacterData =
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

    /// 生成易读的文本报告
    pub fn format_readable_text(card: &CharacterData) -> String {
        let mut s = String::new();
        let sep_line = "-".repeat(40);
        let sep_block = format!("\n{}\n", sep_line);

        // --- 头部信息 ---
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

        // --- 核心设定 ---

        // 描述
        s.push_str(&sep_block);
        s.push_str("【角色描述 (Description)】\n\n");
        s.push_str(&card.description);
        s.push('\n');

        // 开场白
        s.push_str(&sep_block);
        s.push_str("【开场白 (First Message)】\n\n");
        s.push_str(&card.first_mes);
        s.push('\n');

        // 备用开场白
        if !card.alternate_greetings.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【备用开场白 (Alternate Greetings)】\n");
            for (i, msg) in card.alternate_greetings.iter().enumerate() {
                s.push_str(&format!("\n# 备用 {}\n{}\n", i + 1, msg));
            }
        }

        // 性格
        if !card.personality.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【性格 (Personality)】\n\n");
            s.push_str(&card.personality);
            s.push('\n');
        }

        // 场景
        if !card.scenario.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【场景 (Scenario)】\n\n");
            s.push_str(&card.scenario);
            s.push('\n');
        }

        // 对话示例
        if !card.mes_example.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【对话示例 (Example Messages)】\n\n");
            s.push_str(&card.mes_example);
            s.push('\n');
        }

        // --- 高级设定 ---

        // 系统提示词
        if !card.system_prompt.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【系统提示词 (System Prompt)】\n\n");
            s.push_str(&card.system_prompt);
            s.push('\n');
        }

        if !card.post_history_instructions.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【历史后提示词 (Post History Instructions)】\n\n");
            s.push_str(&card.post_history_instructions);
            s.push('\n');
        }

        // --- 扩展内容 (正则 & 深度提示) ---
        if let Some(ext) = &card.extensions {
            // 深度提示词
            if let Some(dp) = &ext.depth_prompt {
                s.push_str(&sep_block);
                s.push_str("【深度提示词 (Depth Prompt)】\n");
                s.push_str(&format!("Depth: {} | Role: {}\n\n", dp.depth, dp.role));
                s.push_str(&dp.prompt);
                s.push('\n');
            }

            // 正则脚本
            if !ext.regex_scripts.is_empty() {
                s.push_str(&sep_block);
                s.push_str("【正则脚本 (Regex Scripts)】\n");
                for (i, script) in ext.regex_scripts.iter().enumerate() {
                    let status = if script.disabled {
                        "(禁用)"
                    } else {
                        "(启用)"
                    };
                    s.push_str(&format!(
                        "\n## {} - {} {}\n",
                        i + 1,
                        script.script_name,
                        status
                    ));
                    s.push_str(&format!("Regex: {}\n", script.find_regex));
                    // 替换内容可能很长，只取前200字或者完整显示取决于需求，这里完整显示
                    s.push_str("Replace:\n");
                    s.push_str(&script.replace_string);
                    s.push('\n');
                }
            }
        }

        // --- 世界书 (Character Book) ---
        // 这是用户特别提到缺失的部分
        if let Some(book) = &card.character_book
            && !book.entries.is_empty()
        {
            s.push_str(&sep_block);
            s.push_str(&format!(
                "【世界书 / 设定集 (World Info)】 - 共 {} 条\n",
                book.entries.len()
            ));

            // 按插入顺序排序，方便阅读
            let mut entries = book.entries.clone();
            entries.sort_by_key(|e| e.insertion_order);

            for entry in entries {
                let status = if !entry.enabled { "[未启用] " } else { "" };
                s.push_str(&format!(
                    "\n>> {}Key: [{}]\n",
                    status,
                    entry.keys.join(", ")
                ));
                if !entry.comment.is_empty() {
                    s.push_str(&format!("注释: {}\n", entry.comment));
                }
                s.push_str("内容:\n");
                s.push_str(&entry.content);
                s.push('\n');
            }
        }

        // --- 作者注释 (通常包含更新日志和玩法指南) ---
        // 放在最后，类似附录
        if !card.creator_notes.is_empty() {
            s.push_str(&sep_block);
            s.push_str("【作者注释 (Creator Notes)】\n\n");
            s.push_str(&card.creator_notes);
            s.push('\n');
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

                        // 5. 生成文件内容 (易读文本)
                        let readable_text = parser::format_readable_text(&card);

                        // 6. 保存临时文件
                        let timestamp = kovi::chrono::Local::now().format("%H%M%S").to_string();
                        let json_filename = format!("{}_{}.json", safe_name, timestamp);
                        let txt_filename = format!("{}_{}_read.txt", safe_name, timestamp);

                        let data_path = bot.get_data_path();
                        let json_path = data_path.join(&json_filename);
                        let txt_path = data_path.join(&txt_filename);

                        // 写入 JSON (UTF-8, 无需 BOM 只要编辑器支持即可，但TXT需要)
                        if let Ok(mut f) = File::create(&json_path) {
                            let _ = f.write_all(json_str.as_bytes());
                        }

                        // 写入 TXT (UTF-8 with BOM)
                        // 关键修改：添加 UTF-8 BOM 头 [0xEF, 0xBB, 0xBF]
                        // 这有助于 Windows 记事本和手机阅读器正确识别编码
                        if let Ok(mut f) = File::create(&txt_path) {
                            let bom = [0xEF, 0xBB, 0xBF];
                            let _ = f.write_all(&bom);
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
                                "✅ 解析成功: {}\n作者: {}\n字数: {}\n(详细设定请查看TXT，配置请查看JSON)",
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
