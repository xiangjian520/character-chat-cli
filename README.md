# Character-Chat CLI

基于OpenAI格式（目前只支持OpenAI格式）的API的 AI 角色扮演对话客户端，支持多协议接入（QQ、微信、OneBot 11）、TTS 语音合成、动态插件系统。

对了，我们非常**快**！！！

注意！Windows和Linux的插件不通用，需要额外编译

## 功能

- **CLI 交互式对话** — Reedline 驱动的命令行界面，支持流式/非流式 AI 回复
- **角色扮演** — `personas/` 目录下放置 `.txt` 文件即可定义角色人设，支持切换
- **QQ 机器人** — 通过 QQ 官方 API 接入私聊消息，自动 AI 回复，支持语音
- **微信机器人** — 通过自定义 API 接入，QR 码扫码登录
- **OneBot 11 协议** — 反向 WebSocket 服务端，兼容 go-cqhttp / NapCat 等实现
- **GPT-SoVITS TTS** — 连接本地 TTS 服务，命令行文字转语音，机器人自动语音回复（需要**额外部署**，需要一定的**算力**）
- **动态插件** — `plugins/*.dll` (Windows) / `plugins/*.so` (Linux) 即插即用，启动时自动发现加载，C ABI 接口
- **管理员系统** — 白名单模式，管理员可通过消息远程执行命令
- **Redis 会话管理** — 使用 Redis 持久化聊天记录与机器人会话上下文
- **热重启** — `/restart` 原地重载配置、Redis 连接、插件，无需退出进程

## 快速开始

### 前置条件

- Rust 工具链（[rustup](https://rustup.rs)）
- DeepSeek API Key（[获取](https://platform.deepseek.com)）
- Redis 服务（默认 `127.0.0.1:6379`）
- GPT-SoVITS 服务用于 TTS
- QQ / 微信 / OneBot 实现端用于机器人接入
- Windows可以使用[Windows安装程序](https://github.com/xiangjian520/character-chat-cli/releases/tag/release)
- Linux可以使用一键脚本



### linux安装脚本(支持大多数发行版)

    curl -fsSL https://github.com/xiangjian520/character-chat-cli/raw/master/install.sh | bash 

### 编译运行（未在MacOS测试过）

```bash
git clone https://github.com/xiangjian520/character-chat-cli.git
cd character-chat-cli
cargo build --release
cargo run --release
```

启动时会自动检测 Redis 连接，若不可达则报错退出：

```
[init] 检测 Redis 连接: redis://127.0.0.1:6379 ...
[init] Redis 连接正常
```

### 配置

首次运行自动生成 `config.json`，或手动创建（当然，随便你）：

```json
{
  "api_key": "",
  "api_url": "https://api.deepseek.com/v1/chat/completions",
  "model": "deepseek-chat",
  "redis_url": "redis://127.0.0.1:6379",
  "admins": [],
  "plugins": {}
}
```

API Key 也可通过环境变量 `DEEPSEEK_API_KEY` 设置。

### Redis 安装

```bash
# Linux
sudo apt install redis && sudo systemctl start redis

# macOS
brew install redis && brew services start redis

# Windows
#Windows下载Windows版redis
```

## 命令一览（部分键值不在）

```
/help | /?            显示帮助
/exit | /quit         退出程序
/restart              热重启（重载配置、Redis、插件）

── 对话 ──
/chat <消息>          发送消息给 AI
/chat stream <消息>   流式输出 AI 回复
/clear                清空聊天历史
/status               查看系统状态
/memory clear         清空所有对话记忆（含机器人的）

── 配置 ──
/config               显示当前配置
/config set <键> <值> 修改配置（含 redis_url）
/config save          保存配置到文件
/config reload        重新加载配置

── TTS ──
/tts connect          连接 TTS 服务
/tts speak <文本>     TTS 朗读
/tts status           查看 TTS 状态
/tts set <键> <值>    设置 TTS 参数
/tts save <路径>      保存最后一次 TTS 音频

── QQ ──
/qq login             配置 QQ AppID/Secret
/qq start             启动 QQ 机器人
/qq stop              停止

── 微信 ──
/wechat login         登录微信
/wechat start         启动微信机器人
/wechat stop          停止

── OneBot ──
/onebot start         启动 OneBot WS 服务（默认端口 6700）
/onebot stop          停止

── 角色 ──
/persona list         列出所有角色
/persona set <名称>   切换角色
```

## 角色人设

在 `personas/` 下放置 `<角色名>.txt` 文件，内容为系统提示词。可选 `<角色名>.display_name.txt` 设置显示名称。

```
personas/
    ****.txt
```

`/persona set <名称>` 切换后自动保存到 config.json，下次启动沿用。

## 插件开发

详见 [PLUGIN.md](PLUGIN.md)。

两种方式：

| 方式  | 位置                               | 适用场景       |
| --- | -------------------------------- | ---------- |
| 编译时 | `src/plugins/*.rs`               | 内置功能，需重新编译 |
| 动态  | `plugins/*.dll` / `plugins/*.so` | 用户扩展，即插即用  |

动态插件只需把 `.dll` (Windows) / `.so` (Linux) 放入 `plugins/` 目录，启动时自动发现并启用。禁用需在 `config.json` 中显式设置 `"plugins":{"xxx":{"enabled":false}}`。

使用 `/restart` 可直接重载插件，无需退出重新启动。

## 管理员

配置管理员 QQ 号（逗号分隔）：

```
/config set admins 114514114,123456789
/config save
```

管理员可通过任意接入协议的消息发送 CLI 命令（如 `/status`、`/persona list`、`/restart`），结果直接返回。

## 平台差异

### Linux

编译前需安装系统依赖：

```bash
# Debian / Ubuntu
sudo apt install build-essential libssl-dev pkg-config libasound2-dev -y

# RHEL / CentOS / Fedora
sudo dnf groupinstall "Development Tools"
sudo dnf install openssl-devel pkg-config alsa-lib-devel -y

# Arch Linux
sudo pacman -S base-devel openssl pkg-config alsa-lib
```

动态插件使用 `.so` 后缀，放入 `plugins/` 目录即可自动加载。

若无需 TTS 音频播放（`rodio` / ALSA），可自行在 `Cargo.toml` 中移除相关依赖后编译。

### Windows

* Windows可以使用安装程序
- 无需额外系统依赖，可直接编译
- 动态插件使用 `.dll` 后缀
- 编译时如遇 `拒绝访问 (os error 5)`，请先关闭已运行的实例：
  
  ```
  taskkill /F /IM character-chat-cli.exe
  ```

## 项目结构

```
src/
  main.rs         入口 + 事件循环
  cli.rs          命令处理
  config.rs       配置管理
  api.rs          DeepSeek API 客户端
  memory.rs       Redis 会话存储
  persona.rs      角色系统
  tts.rs          TTS 客户端
  plugin.rs       插件框架（trait + PluginManager + 动态加载）
  plugins/        编译时插件目录
  qq/             QQ 机器人模块
  wechat/         微信机器人模块
  onebot/         OneBot 11 协议模块
data/
  wechat_credentials.json   微信登录凭证
```

## License

[GNU Affero General Public License v3.0](LICENSE)
