
kovi-plugin-card-reader
=======================

[<img alt="github" src="https://img.shields.io/badge/github-araea/kovi__plugin__card__reader-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/araea/kovi-plugin-card-reader)
[<img alt="crates.io" src="https://img.shields.io/crates/v/kovi-plugin-card-reader.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kovi-plugin-card-reader)

Kovi 的 SillyTavern (酒馆) 角色卡解析插件。

自动识别 PNG 图片中的元数据，一键提取角色设定，支持导出原始数据与易读文本。

## 特性

- 🔍 **深度解析** - 原生解析 PNG `tEXt` 数据块，不依赖大型图像库
- 🏷️ **全版本兼容** - 支持 SillyTavern V2 (chara) 和 V3 (ccv3) 格式
- 📂 **双重导出** - 同时生成 `.json` (原始数据) 和 `.txt` (易读排版)
- 📝 **自动美化** - 将复杂的 JSON 结构转换为人类可读的键值对文档
- 💬 **便捷交互** - 支持直接发送图片或引用图片进行解析

## 前置

1. 创建 Kovi 项目
2. 执行 `cargo kovi add card-reader`
3. 在 `src/main.rs` 中添加 `kovi_plugin_card_reader`

## 快速开始

1. 在聊天中发送一张 **SillyTavern 角色卡 PNG 图片**，并附带文字 `读卡`。
2. 或者，**引用** 别人发送的角色卡图片，发送指令 `解析卡`。
3. 机器人将回复解析结果，并上传 `.json` 和 `.txt` 文件。

## 指令速查

默认指令列表如下（可在配置中修改）：

| 指令 | 说明 |
|------|------|
| `读卡` | 解析附带或引用的图片 |
| `解析卡` | 同上 |
| `看卡` | 同上 |
| `card` | 同上 |

## 配置

资源目录：`data/kovi-plugin-card-reader/*`

> 首次运行时自动生成。

### `config.toml` - 插件配置

```toml
# 插件开关
enabled = true

# 触发指令
commands = ["读卡", "解析卡", "看卡", "card"]

# 指令前缀 (留空则直接匹配指令，如需前缀可设为 ["/", "#"])
prefixes = []

# 是否在解析完成后，发送简短的文本预览（除了发送文件外）
text_preview = true
```

## 解析逻辑说明

插件会按照以下优先级尝试读取 PNG 图片中的元数据：

1. **CCV3 (Spec V3)**: 优先读取 Base64 编码的 V3 数据块，包含更丰富的角色细节。
2. **Chara (Spec V2)**: 如果没有 V3 数据，则尝试读取 V2 格式。

解析成功后生成的 `.txt` 文件将包含以下字段（如果存在）：
- 角色名称、版本、作者、标签
- 角色描述 (Description)
- 开场白 (First Message)
- 性格 (Personality)
- 场景 (Scenario)
- 系统提示词 (System Prompt)
- 作者注释 (Creator Notes)

## 致谢

- [Kovi](https://kovi.threkork.com/)
- [SillyTavern](https://github.com/SillyTavern/SillyTavern)

<br>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
