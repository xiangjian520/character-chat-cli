use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// ─── 消息上下文 ───

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageContext {
    pub protocol: String,
    pub user_id: String,
    pub group_id: Option<String>,
    pub text: String,
    pub is_admin: bool,
}

// ─── 插件元数据 ───

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PluginMeta {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub config: serde_json::Value,
}

// ─── C ABI 类型别名 ───

pub type FnName = unsafe extern "C" fn() -> *const c_char;
pub type FnInit = unsafe extern "C" fn(config_json: *const c_char) -> i32;
pub type FnStart = unsafe extern "C" fn() -> i32;
pub type FnStop = unsafe extern "C" fn() -> i32;
pub type FnRunning = unsafe extern "C" fn() -> i32;
pub type FnOnMsg = unsafe extern "C" fn(ctx_json: *const c_char) -> *const c_char;
pub type FnOnReply = unsafe extern "C" fn(ctx_json: *const c_char, reply: *const c_char);
pub type FnFreeStr = unsafe extern "C" fn(s: *mut c_char);

// ─── Plugin trait ───

pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn init(&mut self, _meta: &PluginMeta) -> Result<(), String> { Ok(()) }
    fn start(&mut self) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) -> Result<(), String> { Ok(()) }
    fn is_running(&self) -> bool { false }
    fn on_message(&self, _ctx: &MessageContext) -> Option<String> { None }
    fn on_reply(&self, _ctx: &MessageContext, _reply: &str) {}
}

// ─── 动态插件（封装 .dll/.so） ───

pub struct DynamicPlugin {
    name: String,
    running: bool,
    _lib: libloading::Library,
    free_str: FnFreeStr,
    init_fn: FnInit,
    start_fn: FnStart,
    stop_fn: FnStop,
    running_fn: FnRunning,
    on_msg_fn: FnOnMsg,
    on_reply_fn: FnOnReply,
}

impl DynamicPlugin {
    pub unsafe fn load(path: &std::path::Path) -> Result<Self, String> {
        let lib = libloading::Library::new(path)
            .map_err(|e| format!("加载 {}: {}", path.display(), e))?;

        let name_fn: FnName = unsafe {
            let sym: libloading::Symbol<FnName> = lib.get(b"plugin_name")
                .map_err(|_| format!("{}: 缺少 plugin_name", path.display()))?;
            *sym
        };
        let init_fn: FnInit = unsafe {
            let sym: libloading::Symbol<FnInit> = lib.get(b"plugin_init")
                .map_err(|_| format!("{}: 缺少 plugin_init", path.display()))?;
            *sym
        };
        let start_fn: FnStart = unsafe {
            let sym: libloading::Symbol<FnStart> = lib.get(b"plugin_start")
                .map_err(|_| format!("{}: 缺少 plugin_start", path.display()))?;
            *sym
        };
        let stop_fn: FnStop = unsafe {
            let sym: libloading::Symbol<FnStop> = lib.get(b"plugin_stop")
                .map_err(|_| format!("{}: 缺少 plugin_stop", path.display()))?;
            *sym
        };
        let running_fn: FnRunning = unsafe {
            let sym: libloading::Symbol<FnRunning> = lib.get(b"plugin_running")
                .map_err(|_| format!("{}: 缺少 plugin_running", path.display()))?;
            *sym
        };
        let on_msg_fn: FnOnMsg = unsafe {
            let sym: libloading::Symbol<FnOnMsg> = lib.get(b"plugin_on_message")
                .map_err(|_| format!("{}: 缺少 plugin_on_message", path.display()))?;
            *sym
        };
        let on_reply_fn: FnOnReply = unsafe {
            let sym: libloading::Symbol<FnOnReply> = lib.get(b"plugin_on_reply")
                .map_err(|_| format!("{}: 缺少 plugin_on_reply", path.display()))?;
            *sym
        };
        let free_str: FnFreeStr = unsafe {
            let sym: libloading::Symbol<FnFreeStr> = lib.get(b"plugin_free_string")
                .map_err(|_| format!("{}: 缺少 plugin_free_string", path.display()))?;
            *sym
        };

        let name = unsafe {
            let ptr = name_fn();
            let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            free_str(ptr as *mut c_char);
            s
        };

        Ok(Self {
            name,
            running: false,
            _lib: lib,
            free_str,
            init_fn,
            start_fn,
            stop_fn,
            running_fn,
            on_msg_fn,
            on_reply_fn,
        })
    }

    fn read_cstr(&self, ptr: *const c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe {
            let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            (self.free_str)(ptr as *mut c_char);
            s
        }
    }
}

impl Plugin for DynamicPlugin {
    fn name(&self) -> &str { &self.name }

    fn init(&mut self, meta: &PluginMeta) -> Result<(), String> {
        let json = serde_json::to_string(meta).unwrap_or_default();
        let cjson = CString::new(json).unwrap();
        let ret = unsafe { (self.init_fn)(cjson.as_ptr()) };
        if ret != 0 { Err(format!("init 返回 {}", ret)) } else { Ok(()) }
    }

    fn start(&mut self) -> Result<(), String> {
        let ret = unsafe { (self.start_fn)() };
        if ret != 0 { Err(format!("start 返回 {}", ret)) } else { self.running = true; Ok(()) }
    }

    fn stop(&mut self) -> Result<(), String> {
        let ret = unsafe { (self.stop_fn)() };
        self.running = false;
        if ret != 0 { Err(format!("stop 返回 {}", ret)) } else { Ok(()) }
    }

    fn is_running(&self) -> bool { self.running }

    fn on_message(&self, ctx: &MessageContext) -> Option<String> {
        let json = serde_json::to_string(ctx).unwrap_or_default();
        let cjson = CString::new(json).unwrap();
        let ptr = unsafe { (self.on_msg_fn)(cjson.as_ptr()) };
        let reply = self.read_cstr(ptr);
        if reply.is_empty() { None } else { Some(reply) }
    }

    fn on_reply(&self, ctx: &MessageContext, reply: &str) {
        let json = serde_json::to_string(ctx).unwrap_or_default();
        let cjson = CString::new(json).unwrap();
        let creply = CString::new(reply).unwrap();
        unsafe { (self.on_reply_fn)(cjson.as_ptr(), creply.as_ptr()) };
    }
}

// ─── PluginManager ───

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self { Self { plugins: Vec::new() } }

    pub fn load_static(
        &mut self,
        factories: &[fn() -> Box<dyn Plugin>],
        plugins_config: &HashMap<String, PluginMeta>,
    ) -> Result<(), String> {
        for f in factories {
            let mut plugin = f();
            let name = plugin.name().to_string();
            let has_config = plugins_config.contains_key(&name);
            let meta = plugins_config.get(&name).cloned().unwrap_or_default();
            let enabled = if has_config { meta.enabled } else { true };
            if enabled {
                plugin.init(&meta)?;
                self.plugins.push(plugin);
            }
        }
        Ok(())
    }

    pub fn load_dynamic(
        &mut self,
        plugins_dir: &std::path::Path,
        plugins_config: &HashMap<String, PluginMeta>,
    ) -> Result<Vec<String>, String> {
        let mut loaded = Vec::new();
        if !plugins_dir.is_dir() { return Ok(loaded); }

        let entries = std::fs::read_dir(plugins_dir)
            .map_err(|e| format!("扫描目录失败: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            #[cfg(target_os = "windows")]
            let is_lib = ext == "dll";
            #[cfg(not(target_os = "windows"))]
            let is_lib = ext == "so";

            if !is_lib { continue; }

            match unsafe { DynamicPlugin::load(&path) } {
                Ok(mut plugin) => {
                    let name = plugin.name().to_string();
                    // 没有配置项 → 默认启用；有配置项 → 按 enabled 字段决定
                    let has_config = plugins_config.contains_key(&name);
                    let meta = plugins_config.get(&name).cloned().unwrap_or_default();
                    let enabled = if has_config { meta.enabled } else { true };

                    eprintln!("[plugin] 发现 {}（{}）→ {}", name, path.file_name().unwrap_or_default().to_string_lossy(),
                        if enabled { "启用" } else { "禁用（配置关闭）" });

                    if enabled {
                        if let Err(e) = plugin.init(&meta) {
                            eprintln!("[plugin] {} 初始化失败: {}", name, e);
                            continue;
                        }
                        loaded.push(plugin.name().to_string());
                        self.plugins.push(Box::new(plugin));
                    }
                }
                Err(e) => eprintln!("[plugin] {} 加载失败: {}", path.display(), e),
            }
        }
        Ok(loaded)
    }

    pub fn start_all(&mut self) -> Vec<String> {
        let mut msgs = Vec::new();
        for p in &mut self.plugins {
            if !p.is_running() {
                match p.start() {
                    Ok(()) => msgs.push(format!("[plugin] {} 已启动", p.name())),
                    Err(e) => msgs.push(format!("[plugin] {} 启动失败: {}", p.name(), e)),
                }
            }
        }
        msgs
    }

    pub fn stop_all(&mut self) {
        for p in &mut self.plugins {
            if p.is_running() { let _ = p.stop(); }
        }
    }

    pub fn dispatch_message(&self, ctx: &MessageContext) -> Option<String> {
        for p in &self.plugins {
            if p.is_running() {
                if let Some(reply) = p.on_message(ctx) { return Some(reply); }
            }
        }
        None
    }

    pub fn dispatch_reply(&self, ctx: &MessageContext, reply: &str) {
        for p in &self.plugins {
            if p.is_running() { p.on_reply(ctx, reply); }
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self { Self::new() }
}
