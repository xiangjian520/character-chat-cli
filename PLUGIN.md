# Plugin 开发指南

## 两种插件模式

| 类型 | 文件位置 | 加载方式 | 适用场景 |
|------|----------|----------|----------|
| 编译时插件 | `src/plugins/*.rs` | 编译到二进制，需重新构建 | 内置功能 |
| 动态插件 | `plugins/*.dll` / `plugins/*.so` | 启动时扫描加载，打包后即插即用 | 用户扩展 |

---

## 动态插件（推荐）

### 目录结构

```
character-chat-cli.exe
plugins/
  example_plugin.dll     ← 动态插件，程序自动发现
  my_feature.dll         ← 可以放多个
```

### 创建动态插件

动态插件是独立的 Rust `cdylib` crate，必须导出以下 8 个 C 函数：

```rust
// plugins/my_plugin/src/lib.rs

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn plugin_name() -> *const c_char {
    CString::new("my_plugin").unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn plugin_init(config_json: *const c_char) -> i32 {
    // config_json 是 PluginMeta 的 JSON 字符串
    // 从 PluginMeta.config 中读取自定义配置
    0  // 0=成功
}

#[no_mangle]
pub extern "C" fn plugin_start() -> i32 { 0 }

#[no_mangle]
pub extern "C" fn plugin_stop() -> i32 { 0 }

#[no_mangle]
pub extern "C" fn plugin_running() -> i32 { 1 }  // 1=运行中, 0=未运行

#[no_mangle]
pub extern "C" fn plugin_on_message(ctx_json: *const c_char) -> *const c_char {
    // ctx_json: {"protocol":"onebot","user_id":"123","text":"/hello",...}
    // 返回 C 字符串作为回复（程序负责释放），或返回 null 不拦截
    let json = unsafe { CStr::from_ptr(ctx_json).to_string_lossy() };
    // 解析 json，判断是否需要处理...
    std::ptr::null()
}

#[no_mangle]
pub extern "C" fn plugin_on_reply(ctx_json: *const c_char, reply: *const c_char) {
    // AI 回复后的后置通知
}

#[no_mangle]
pub extern "C" fn plugin_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)); }
    }
}
```

### Cargo.toml

```toml
[package]
name = "my_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### 编译

```bash
cd plugins/my_plugin
cargo build --release
cp target/release/my_plugin.dll ../    # Windows
# cp target/release/libmy_plugin.so ../  # Linux
```

### 启用

在 `config.json` 中配置：

```json
{
  "plugins": {
    "my_plugin": {
      "enabled": true,
      "config": {
        "reply_prefix": "🎉"
      }
    }
  }
}
```

`PluginMeta.config` 中的自定义字段会在 `plugin_init()` 时作为 JSON 传入。

---

## 编译时插件

编译时插件放在 `src/plugins/` 下，实现 `Plugin` trait。

### 1. 创建插件文件 `src/plugins/myplg.rs`

```rust
use crate::plugin::{MessageContext, Plugin, PluginMeta};

pub struct MyPlugin { running: bool }

impl MyPlugin { pub fn new() -> Self { Self { running: false } } }

impl Plugin for MyPlugin {
    fn name(&self) -> &'static str { "myplg" }
    fn start(&mut self) -> Result<(), String> { self.running = true; Ok(()) }
    fn stop(&mut self) -> Result<(), String> { self.running = false; Ok(()) }
    fn is_running(&self) -> bool { self.running }
    fn on_message(&self, ctx: &MessageContext) -> Option<String> {
        if ctx.text == "/ping" { Some("pong".into()) } else { None }
    }
}
```

### 2. 注册 `src/plugins/mod.rs`

```rust
mod myplg;           // ← 新增一行

pub fn factory_list() -> Vec<fn() -> Box<dyn Plugin>> {
    vec![
        // ...已有...
        || Box::new(myplg::MyPlugin::new()),  // ← 新增一行
    ]
}
```

### 3. 启用

```json
{ "plugins": { "myplg": { "enabled": true } } }
```

---

## MessageContext 字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `protocol` | `String` | `"qq"` / `"wechat"` / `"onebot"` |
| `user_id` | `String` | 发送者 ID |
| `group_id` | `Option<String>` | 群 ID，私聊为 `None` |
| `text` | `String` | 消息文本 |
| `is_admin` | `bool` | 是否为管理员 |
