# Character-Chat CLI

基于 DeepSeek API 的 AI 角色扮演对话客户端，支持多协议接入（QQ、微信、OneBot 11）、TTS 语音合成、动态插件系统。

## 功能

- **CLI 交互式对话** — Reedline 驱动的命令行界面，支持流式/非流式 AI 回复
- **角色扮演** — `personas/` 目录下放置 `.txt` 文件即可定义角色人设，支持切换
- **QQ 机器人** — 通过 QQ 官方 API 接入私聊消息，自动 AI 回复，支持语音
- **微信机器人** — 通过自定义 API 接入，QR 码扫码登录
- **OneBot 11 协议** — 反向 WebSocket 服务端，兼容 go-cqhttp / NapCat 等实现
- **GPT-SoVITS TTS** — 连接本地 TTS 服务，命令行文字转语音，机器人自动语音回复
- **动态插件** — `plugins/*.dll` 即插即用，启动时自动发现加载，C ABI 接口
- **管理员系统** — 白名单模式，管理员可通过消息远程执行命令
- **SQLite 记忆** — 聊天记录持久化，支持上下文记忆

## 快速开始

### 前置条件

- Rust 工具链（[rustup](https://rustup.rs)）
- DeepSeek API Key（[获取](https://platform.deepseek.com)）
- （可选）GPT-SoVITS 服务用于 TTS
- （可选）QQ / 微信 / OneBot 实现端用于机器人接入

### 编译运行

```bash
git clone https://github.com/xiangjian520/character-chat-cli.git
cd character-chat-cli
cargo build --release
cargo run --release
```

### 配置

首次运行自动生成 `config.json`，或手动创建：

```json
{
  "api_key": "",
  "api_url": "https://api.deepseek.com/v1/chat/completions",
  "model": "deepseek-chat",
  "admins": [],
  "plugins": {}
}
```

API Key 也可通过环境变量 `DEEPSEEK_API_KEY` 设置。

## 命令一览

```
/help                 显示帮助
/chat <消息>          发送消息给 AI
/chat stream <消息>   流式输出
/clear                清空聊天历史
/status               查看系统状态

/config               显示配置
/config set <键> <值> 修改配置
/config save          保存配置

/persona list         列出所有角色
/persona set <名称>   切换角色

/tts connect          连接 TTS 服务
/tts speak <文本>     TTS 朗读

/qq login             配置 QQ
/qq start             启动 QQ 机器人
/qq stop              停止

/wechat login         登录微信
/wechat start         启动微信机器人
/wechat stop          停止

/onebot start         启动 OneBot WS 服务（默认端口 6700）
/onebot stop          停止

/memory clear         清空所有对话记忆
```

## 角色人设

在 `personas/` 下放置 `<角色名>.txt` 文件，内容为系统提示词。可选 `<角色名>.display_name.txt` 设置显示名称。

```
personas/
  雌小鬼人设.txt
  cyrene.txt
```

`/persona set <名称>` 切换后自动保存到 config.json，下次启动沿用。

## 插件开发

详见 [PLUGIN.md](PLUGIN.md)。

两种方式：

| 方式 | 位置 | 适用场景 |
|------|------|----------|
| 编译时 | `src/plugins/*.rs` | 内置功能，需重新编译 |
| 动态 | `plugins/*.dll` | 用户扩展，即插即用 |

动态插件只需把 `.dll` 放入 `plugins/` 目录，启动时自动发现并启用。禁用需在 `config.json` 中显式设置 `"plugins":{"xxx":{"enabled":false}}`。

## 管理员

配置管理员 QQ 号（逗号分隔）：

```
/config set admins 1003007712,123456789
/config save
```

管理员可通过任意接入协议的消息发送 CLI 命令（如 `/status`、`/persona list`），结果直接返回。

## 项目结构

```
src/
  main.rs         入口 + 事件循环
  cli.rs          命令处理
  config.rs       配置管理
  api.rs          DeepSeek API 客户端
  memory.rs       SQLite 记忆存储
  persona.rs      角色系统
  tts.rs          TTS 客户端
  plugin.rs       插件框架（trait + PluginManager + 动态加载）
  plugins/        编译时插件目录
  qq/             QQ 机器人模块
  wechat/         微信机器人模块
  onebot/         OneBot 11 协议模块
```

## License

MIT
