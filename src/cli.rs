use crate::api::{send_message_async, send_message_streaming, ChatMessage};
use crate::config::Config;
use crate::memory::MemoryStore;
use crate::persona::{Persona, scan_skill_dirs};
use crate::plugin::PluginManager;
use crate::tts::{self, TtsState};
use crate::wechat;
use crate::wechat::auth::AuthStatus;
use qrcode::QrCode;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub struct AdminCmd {
    pub protocol: String,
    pub user_id: String,
    pub command: String,
    pub reply_tx: tokio::sync::oneshot::Sender<String>,
}

/// 检查是否为管理员命令，是则转发到主循环并返回回复通道
pub fn check_admin_cmd(
    user_id: &str,
    text: &str,
    admins: &[String],
    admin_tx: &mpsc::UnboundedSender<AdminCmd>,
) -> Option<tokio::sync::oneshot::Receiver<String>> {
    if !text.starts_with('/') {
        return None;
    }
    if !admins.iter().any(|a| a == user_id) {
        return None;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = admin_tx.send(AdminCmd {
        protocol: String::new(),
        user_id: user_id.to_string(),
        command: text.to_string(),
        reply_tx: tx,
    });
    Some(rx)
}

pub struct AppState {
    pub config: Config,
    pub store: Arc<MemoryStore>,
    pub personas: Vec<Persona>,
    pub active_persona: String,
    pub tts: TtsState,
    pub tts_audio_cache: Vec<u8>,
    pub wechat_logged_in: bool,
    pub wechat_running: bool,
    pub wechat_credentials: Option<wechat::types::LoginCredentials>,
    pub wechat_qr_content: Option<String>,
    pub wechat_qr_image: Option<String>,
    pub qq_running: bool,
    pub running: bool,
    pub plugin_mgr: Arc<Mutex<PluginManager>>,
}

impl AppState {
    pub fn new(config: Config, store: Arc<MemoryStore>, plugin_mgr: Arc<Mutex<PluginManager>>) -> Self {
        let personas = scan_skill_dirs(std::path::Path::new("."));
        let tts = TtsState::from_config(&config);
        let active_persona = if config.persona.is_empty() {
            "none".to_string()
        } else {
            config.persona.clone()
        };
        Self {
            config,
            store,
            personas,
            active_persona,
            tts,
            tts_audio_cache: Vec::new(),
            wechat_logged_in: false,
            wechat_running: false,
            wechat_credentials: None,
            wechat_qr_content: None,
            wechat_qr_image: None,
            qq_running: false,
            running: true,
            plugin_mgr,
        }
    }

    pub fn system_prompt(&self) -> Option<String> {
        self.personas
            .iter()
            .find(|p| p.name == self.active_persona)
            .map(|p| p.system_prompt.clone())
    }
}

pub async fn handle_command(cmd: &str, state: &mut AppState) -> Vec<String> {
    let parts = parse_command(cmd);
    if parts.is_empty() {
        return vec![];
    }

    match parts[0].as_str() {
        "/help" | "/?" => help_text(),
        "/exit" | "/quit" => {
            state.running = false;
            vec!["再见!".to_string()]
        }
        "/restart" => cmd_restart(state),

        "/chat" => cmd_chat(&parts, state).await,
        "/clear" => cmd_clear(state),
        "/config" => cmd_config(&parts, state).await,
        "/tts" => cmd_tts(&parts, state).await,
        "/wechat" | "/wx" => cmd_wechat(&parts, state).await,
        "/qq" => cmd_qq(state),
        "/onebot" | "/ob" => cmd_onebot(state),
        "/persona" | "/role" => cmd_persona(&parts, state),
        "/memory" => cmd_memory(&parts, state),
        "/status" => cmd_status(state),

        _ => {
            vec![format!(
                "未知命令: '{}'，输入 /help 查看帮助",
                parts[0]
            )]
        }
    }
}

fn parse_command(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';

    for ch in input.chars() {
        if in_quote {
            if ch == quote_char {
                in_quote = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = ch;
        } else if ch == ' ' {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn help_text() -> Vec<String> {
    vec![
        "═══════════ Character-Chat CLI 帮助 ═══════════════".to_string(),
        "".to_string(),
        "/help | /?           显示此帮助".to_string(),
        "/exit | /quit        退出程序".to_string(),
        "/restart             重启程序（重新加载配置与数据库）".to_string(),
        "".to_string(),
        "── 对话 ──".to_string(),
        "/chat <消息>         发送消息给 AI（流式输出）".to_string(),
        "/chat stream <消息>  流式输出 AI 回复".to_string(),
        "/clear               清空聊天历史".to_string(),
        "/status              显示当前状态".to_string(),
        "/memory clear        清空所有对话记忆".to_string(),
        "".to_string(),
        "── 配置 ──".to_string(),
        "/config              显示当前配置".to_string(),
        "/config set <键> <值> 修改配置".to_string(),
        "/config save         保存配置到文件".to_string(),
        "/config reload       重新加载配置".to_string(),
        "".to_string(),
        "── TTS ──".to_string(),
        "/tts connect         连接到 TTS 服务".to_string(),
        "/tts disconnect      断开 TTS 连接".to_string(),
        "/tts speak <文本>    TTS 朗读文本".to_string(),
        "/tts status          查看 TTS 状态".to_string(),
        "/tts set <键> <值>   设置 TTS 参数".to_string(),
        "/tts save <路径>     保存最后一次 TTS 音频".to_string(),
        "".to_string(),
        "── 微信 ──".to_string(),
        "/wechat login        登录微信（获取二维码）".to_string(),
        "/wechat logout       登出微信".to_string(),
        "/wechat qr           显示当前二维码（控制台打印）".to_string(),
        "/wechat qr save <路径> 保存二维码为图片".to_string(),
        "/wechat status       查看微信状态".to_string(),
        "/wechat start        启动微信机器人".to_string(),
        "/wechat stop         停止微信机器人".to_string(),
        "".to_string(),
        "── QQ ──".to_string(),
        "/qq login            配置 QQ AppID/Secret".to_string(),
        "/qq start            启动 QQ 机器人".to_string(),
        "/qq stop             停止 QQ 机器人".to_string(),
        "/qq status           查看 QQ 状态".to_string(),
        "".to_string(),
        "── OneBot ──".to_string(),
        "/onebot start        启动 OneBot WS 服务".to_string(),
        "/onebot stop         停止 OneBot WS 服务".to_string(),
        "/onebot status       查看 OneBot 状态".to_string(),
        "/config set onebot_port <端口> 设置 WS 端口".to_string(),
        "".to_string(),
        "── 角色 ──".to_string(),
        "/persona list        列出所有角色".to_string(),
        "/persona set <名称>  切换角色".to_string(),
    ]
}

// ─── Chat ───

async fn cmd_chat(parts: &[String], state: &mut AppState) -> Vec<String> {
    if state.config.api_key().is_empty() {
        return vec!["错误: 未设置 API Key，请设置环境变量 DEEPSEEK_API_KEY".to_string()];
    }

    let (stream, text) = if parts.len() >= 2 && parts[1] == "stream" {
        if parts.len() < 3 {
            return vec!["用法: /chat stream <消息>".to_string()];
        }
        (true, parts[2..].join(" "))
    } else if parts.len() >= 2 {
        (false, parts[1..].join(" "))
    } else {
        return vec!["用法: /chat <消息> 或 /chat stream <消息>".to_string()];
    };

    let text = text.trim().to_string();
    if text.is_empty() {
        return vec!["消息不能为空".to_string()];
    }

    state.store.chat_add("user", &text);

    let mut messages: Vec<ChatMessage> = Vec::new();
    if let Some(sp) = state.system_prompt() {
        messages.push(ChatMessage { role: "system".into(), content: sp });
    }
    messages.extend(state.store.chat_messages());

    if stream {
        println!("\n{}: ", state.config.ai_name);

        let api_key = state.config.api_key();
        match send_message_streaming(
            &api_key,
            &state.config.api_url,
            &state.config.model,
            state.config.max_tokens,
            state.config.temperature,
            state.config.top_p,
            messages,
            |chunk| {
                print!("{}", chunk);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            },
        )
        .await
        {
            Ok(reply) => {
                println!();
                state.store.chat_add("assistant", &reply);

                if state.tts.connected && state.tts.auto_play {
                    if let Err(e) = tts_play(&state.tts, &reply).await {
                        return vec![format!("TTS 播放失败: {}", e)];
                    }
                }
                state.tts_audio_cache.clear();
                vec![]
            }
            Err(e) => {
                println!();
                vec![format!("错误: {}", e)]
            }
        }
    } else {
        let api_key = state.config.api_key();
        match send_message_async(
            &api_key,
            &state.config.api_url,
            &state.config.model,
            state.config.max_tokens,
            state.config.temperature,
            state.config.top_p,
            messages,
        )
        .await
        {
            Ok(reply) => {
                state.store.chat_add("assistant", &reply);

                if state.tts.connected && state.tts.auto_play {
                    if let Err(e) = tts_play(&state.tts, &reply).await {
                        return vec![format!("{}: {}\nTTS 播放失败: {}", state.config.ai_name, reply, e)];
                    }
                }
                state.tts_audio_cache.clear();
                vec![format!("{}: {}", state.config.ai_name, reply)]
            }
            Err(e) => {
                vec![format!("错误: {}", e)]
            }
        }
    }
}

async fn tts_play(tts: &TtsState, text: &str) -> Result<(), String> {
    let config = tts.build_config();
    let audio = tts::generate_speech(&config, text).await?;
    tts::play_audio(&audio)
}

fn cmd_clear(state: &mut AppState) -> Vec<String> {
    state.store.chat_clear();
    vec!["聊天历史已清空".to_string()]
}

fn cmd_restart(state: &mut AppState) -> Vec<String> {
    let config = Config::load("config.json");
    match MemoryStore::open(&config.redis_url) {
        Ok(new_store) => {
            let personas = scan_skill_dirs(std::path::Path::new("."));
            let tts = TtsState::from_config(&config);
            let active_persona = if config.persona.is_empty() {
                "none".to_string()
            } else {
                config.persona.clone()
            };

            // Reload plugins
            let factories = crate::plugins::factory_list();
            let mut new_mgr = PluginManager::new();
            if let Err(e) = new_mgr.load_static(&factories, &config.plugins) {
                eprintln!("[plugin] 编译时插件加载失败: {}", e);
            }
            match new_mgr.load_dynamic(std::path::Path::new("plugins"), &config.plugins) {
                Ok(loaded) => {
                    if !loaded.is_empty() {
                        eprintln!("[plugin] 已加载 {} 个动态插件", loaded.len());
                    }
                }
                Err(e) => eprintln!("[plugin] 扫描失败: {}", e),
            }
            {
                let mut old_mgr = state.plugin_mgr.lock().unwrap();
                old_mgr.stop_all();
            }
            for msg in new_mgr.start_all() {
                eprintln!("{}", msg);
            }
            *state.plugin_mgr.lock().unwrap() = new_mgr;

            state.config = config;
            state.store = Arc::new(new_store);
            state.personas = personas;
            state.active_persona = active_persona;
            state.tts = tts;
            vec!["程序已重启, 配置、Redis、插件已重新加载".to_string()]
        }
        Err(e) => vec![format!("重启失败, Redis 连接异常: {}", e)],
    }
}

// ─── Config ───

async fn cmd_config(parts: &[String], state: &mut AppState) -> Vec<String> {
    match parts.get(1).map(|s| s.as_str()) {
        None | Some("show") => {
            let cfg = &state.config;
            let tts_status = if state.tts.connected { "已连接" } else { "未连接" };
            let key_display = match cfg.api_key_source() {
                "config" => format!("{} (配置文件)", mask_key(&cfg.api_key)),
                "env" => format!("{} (环境变量)", mask_key(&cfg.api_key())),
                _ => "未设置".to_string(),
            };
            vec![
                "═══════ 当前配置 ═══════".to_string(),
                format!("api_key:       {}", key_display),
                format!("api_url:       {}", cfg.api_url),
                format!("model:         {}", cfg.model),
                format!("max_tokens:    {}", cfg.max_tokens),
                format!("temperature:   {}", cfg.temperature),
                format!("top_p:         {}", cfg.top_p),
                format!("persona:       {}", state.active_persona),
                format!("user_name:     {}", cfg.user_name),
                format!("ai_name:       {}", cfg.ai_name),
                "".to_string(),
                format!("TTS 状态:      {}", tts_status),
                format!("tts_api_url:   {}", cfg.tts_api_url),
                format!("tts_auto_play: {}", cfg.tts_auto_play),
                format!("qq_app_id:     {}", if cfg.qq_app_id.is_empty() { "未设置".to_string() } else { format!("{}...", &cfg.qq_app_id[..cfg.qq_app_id.len().min(8)]) }),
                format!("qq_voice:      {}", if cfg.qq_voice_enabled { "开启" } else { "关闭" }),
                format!("onebot_port:   {}", cfg.onebot_ws_port),
                format!("onebot_at_only: {}", cfg.onebot_at_only),
                format!("admins:        {:?}", cfg.admins),
                format!("blacklist:     {:?}", cfg.blacklist),
                format!("auto_qq:       {}", cfg.auto_start_qq),
                format!("auto_wechat:   {}", cfg.auto_start_wechat),
                format!("auto_onebot:   {}", cfg.auto_start_onebot),
                format!("redis_url:     {}", cfg.redis_url),
            ]
        }
        Some("set") => {
            if parts.len() < 4 {
                return vec!["用法: /config set <键> <值>".to_string()];
            }
            let key = &parts[2];
            let value = parts[3..].join(" ");
            match key.as_str() {
                "api_key" => {
                    state.config.api_key = value;
                    return vec![format!("API Key 已设置（仅内存），保存时不会写入文件。建议使用环境变量 DEEPSEEK_API_KEY")];
                }
                "api_url" => state.config.api_url = value,
                "model" => state.config.model = value,
                "max_tokens" => {
                    if let Ok(v) = value.parse() { state.config.max_tokens = v; }
                    else { return vec!["max_tokens 必须是数字".to_string()]; }
                }
                "temperature" => {
                    if let Ok(v) = value.parse() { state.config.temperature = v; }
                    else { return vec!["temperature 必须是数字".to_string()]; }
                }
                "top_p" => {
                    if let Ok(v) = value.parse() { state.config.top_p = v; }
                    else { return vec!["top_p 必须是数字".to_string()]; }
                }
                "user_name" => state.config.user_name = value,
                "ai_name" => state.config.ai_name = value,
                "persona" => {
                    state.config.persona = value.clone();
                    state.active_persona = value;
                }
                "qq_app_id" => state.config.qq_app_id = value,
                "qq_app_secret" => state.config.qq_app_secret = value,
                "qq_voice" => {
                    state.config.qq_voice_enabled = value == "true" || value == "1" || value == "on";
                    return vec![format!("QQ 语音已{}", if state.config.qq_voice_enabled { "开启" } else { "关闭" })];
                }
                "onebot_port" => {
                    if let Ok(v) = value.parse() { state.config.onebot_ws_port = v; }
                    else { return vec!["onebot_port 必须是数字".to_string()]; }
                }
                "onebot_at_only" => state.config.onebot_at_only = value == "true" || value == "1" || value == "on",
                "admins" => {
                    state.config.admins = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    return vec![format!("管理员列表已更新: {:?}", state.config.admins)];
                }
                "blacklist" => {
                    state.config.blacklist = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    return vec![format!("黑名单已更新: {:?}", state.config.blacklist)];
                }
                "auto_qq" => state.config.auto_start_qq = value == "true" || value == "1" || value == "on",
                "auto_wechat" => state.config.auto_start_wechat = value == "true" || value == "1" || value == "on",
                "auto_onebot" => state.config.auto_start_onebot = value == "true" || value == "1" || value == "on",
                "redis_url" => state.config.redis_url = value,
                _ => return vec![format!("未知配置项: {}", key)],
            }
            vec![format!("已设置 {} = ***", key)]
        }
        Some("save") => {
            match state.config.save("config.json") {
                Ok(()) => vec!["配置已保存到 config.json".to_string()],
                Err(e) => vec![format!("保存失败: {}", e)],
            }
        }
        Some("reload") => {
            *state = AppState::new(
                Config::load("config.json"),
                state.store.clone(),
                state.plugin_mgr.clone(),
            );
            vec!["配置已重新加载".to_string()]
        }
        _ => vec!["未知配置子命令".to_string()],
    }
}

fn mask_key(key: &str) -> String {
    if key.is_empty() {
        "未设置".to_string()
    } else if key.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len()-4..])
    }
}

// ─── TTS ───

async fn cmd_tts(parts: &[String], state: &mut AppState) -> Vec<String> {
    match parts.get(1).map(|s| s.as_str()) {
        Some("connect") => {
            let config = state.tts.build_config();
            if config.ref_audio_path.is_empty() {
                return vec!["警告: 未设置参考音频路径，请先设置: /tts set ref_audio <路径>".to_string()];
            }
            match tts::connect(&config).await {
                Ok(msg) => {
                    state.tts.connected = true;
                    vec![msg]
                }
                Err(e) => vec![format!("连接失败: {}", e)],
            }
        }
        Some("disconnect") => {
            state.tts.connected = false;
            vec!["TTS 已断开".to_string()]
        }
        Some("speak") => {
            if !state.tts.connected {
                return vec!["TTS 未连接，请先使用 /tts connect".to_string()];
            }
            if parts.len() < 3 {
                return vec!["用法: /tts speak <文本>".to_string()];
            }
            let text = parts[2..].join(" ");
            let config = state.tts.build_config();
            match tts::generate_speech(&config, &text).await {
                Ok(audio) => {
                    state.tts_audio_cache = audio.clone();
                    match tts::play_audio(&audio) {
                        Ok(()) => vec!["TTS 播放完成".to_string()],
                        Err(e) => vec![format!("播放失败: {}", e)],
                    }
                }
                Err(e) => vec![format!("TTS 生成失败: {}", e)],
            }
        }
        Some("status") => {
            vec![
                format!("TTS 连接状态: {}", if state.tts.connected { "已连接" } else { "未连接" }),
                format!("API 地址:    {}", state.tts.api_url),
                format!("参考音频:    {}", if state.tts.ref_audio_path.is_empty() { "未设置" } else { &state.tts.ref_audio_path }),
                format!("提示文本:    {}", if state.tts.prompt_text.is_empty() { "未设置" } else { &state.tts.prompt_text }),
                format!("语速:        {}", state.tts.speed),
                format!("自动播放:    {}", state.tts.auto_play),
            ]
        }
        Some("set") => {
            if parts.len() < 4 {
                return vec!["用法: /tts set <键> <值>".to_string()];
            }
            let key = &parts[2];
            let value = parts[3..].join(" ");
            match key.as_str() {
                "api_url" => state.tts.api_url = value.clone(),
                "ref_audio" => state.tts.ref_audio_path = value.clone(),
                "prompt_text" => state.tts.prompt_text = value.clone(),
                "prompt_lang" => state.tts.prompt_lang = value.clone(),
                "text_lang" => state.tts.text_lang = value.clone(),
                "gpt_weights" => state.tts.gpt_weights = value.clone(),
                "sovits_weights" => state.tts.sovits_weights = value.clone(),
                "speed" => {
                    if let Ok(v) = value.parse() { state.tts.speed = v; }
                    else { return vec!["speed 必须是数字".to_string()]; }
                }
                "auto_play" => {
                    state.tts.auto_play = value == "true" || value == "1" || value == "on";
                }
                _ => return vec![format!("未知 TTS 参数: {}", key)],
            }
            vec![format!("TTS {} 已设置为 {}", key, value)]
        }
        Some("save") => {
            if state.tts_audio_cache.is_empty() {
                return vec!["没有缓存的音频，请先使用 /tts speak".to_string()];
            }
            let path = if parts.len() >= 3 { parts[2].clone() } else { "data/tts_output".to_string() };
            match tts::save_audio(&state.tts_audio_cache, &path) {
                Ok(p) => vec![format!("音频已保存到: {}", p.display())],
                Err(e) => vec![format!("保存失败: {}", e)],
            }
        }
        _ => vec!["用法: /tts <connect|disconnect|speak|status|set|save>".to_string()],
    }
}

// ─── WeChat ───

async fn cmd_wechat(parts: &[String], state: &mut AppState) -> Vec<String> {
    match parts.get(1).map(|s| s.as_str()) {
        Some("login") => {
            println!("[微信] 正在获取二维码...");

            // 只在 run_auth_flow 内部获取和显示二维码
            // 保存 URL 用于后续 /wechat qr 命令
            let mut qr_url_for_state = String::new();
            let mut qr_image_for_state = String::new();

            match wechat::auth::run_auth_flow(
                3,
                |status, qr_data, creds| {
                    match status {
                        AuthStatus::FetchingQr => {}
                        AuthStatus::WaitingScan => {
                            if let Some(data) = qr_data {
                                // data 可能是 URL 或 base64 图片
                                if data.starts_with("http") {
                                    qr_url_for_state = data.to_string();
                                    println!("\n请使用微信扫描下方二维码：\n");
                                    print_qr_in_terminal(data);
                                    println!("\n二维码链接: {}", data);
                                    println!("\n可随时使用 /wechat qr 重新显示");
                                    println!("使用 /wechat qr save <路径> 保存二维码图片\n");
                                } else {
                                    qr_image_for_state = data.to_string();
                                    if !qr_url_for_state.is_empty() {
                                        println!("\n请使用微信扫描下方二维码：\n");
                                        print_qr_in_terminal(&qr_url_for_state);
                                    }
                                }
                            }
                        }
                        AuthStatus::RefreshingQr => {
                            println!("[微信] 二维码已过期，正在刷新...");
                        }
                        AuthStatus::Scanned => println!("[微信] 已扫码，请在手机上确认..."),
                        AuthStatus::Confirmed => {
                            if let Some(c) = creds {
                                println!("[微信] 登录成功! account_id={}", c.account_id);
                            }
                        }
                        AuthStatus::Expired => println!("[微信] 二维码已过期"),
                        AuthStatus::Error(e) => eprintln!("[微信] 错误: {}", e),
                        AuthStatus::LoadedCredentials => println!("[微信] 使用已保存的凭证"),
                    }
                },
            )
            .await
            {
                Ok(creds) => {
                    state.wechat_credentials = Some(creds.clone());
                    state.wechat_logged_in = true;
                    state.wechat_qr_content = if qr_url_for_state.is_empty() { None } else { Some(qr_url_for_state) };
                    state.wechat_qr_image = if qr_image_for_state.is_empty() { None } else { Some(qr_image_for_state) };
                    vec![format!("微信登录成功! account_id={}", creds.account_id)]
                }
                Err(e) => vec![format!("微信登录失败: {}", e)],
            }
        }
        Some("logout") => {
            wechat::auth::clear_saved_credentials();
            state.wechat_logged_in = false;
            state.wechat_running = false;
            state.wechat_credentials = None;
            state.wechat_qr_content = None;
            vec!["微信已登出".to_string()]
        }
        Some("qr") => {
            if parts.len() >= 3 && parts[2] == "save" {
                let path = if parts.len() >= 4 {
                    parts[3].clone()
                } else {
                    "wechat_qr.png".to_string()
                };
                match state.wechat_qr_content.as_ref() {
                    Some(content) => {
                        match save_qr_as_image(content, &path) {
                            Ok(()) => vec![format!("二维码已保存到: {}", path)],
                            Err(e) => vec![format!("保存失败: {}", e)],
                        }
                    }
                    None => vec!["没有可用的二维码，请先 /wechat login".to_string()],
                }
            } else {
                match state.wechat_qr_content.as_ref() {
                    Some(content) => {
                        print_qr_in_terminal(content);
                        if content.starts_with("http") {
                            println!("\n二维码链接: {}", content);
                        }
                        vec![]
                    }
                    None => vec!["没有可用的二维码，请先 /wechat login".to_string()],
                }
            }
        }
        Some("status") => {
            vec![
                format!("微信登录: {}", if state.wechat_logged_in { "已登录" } else { "未登录" }),
                format!("机器人运行: {}", if state.wechat_running { "运行中" } else { "未运行" }),
                match &state.wechat_credentials {
                    Some(c) => format!("账号 ID: {}", c.account_id),
                    None => "账号 ID: -".to_string(),
                },
            ]
        }
        Some("start") => vec!["请在主界面直接输入 /wechat start 来启动微信机器人".to_string()],
        Some("stop") => vec!["请在主界面直接输入 /wechat stop 来停止微信机器人".to_string()],
        _ => {
            vec!["用法: /wechat <login|logout|qr|status|start|stop>".to_string()]
        }
    }
}

// ─── QQ ───

fn cmd_qq(state: &AppState) -> Vec<String> {
    vec![
        format!("QQ 机器人: {}", if state.qq_running { "运行中" } else { "未运行" }),
        format!("App ID:     {}", if state.config.qq_app_id.is_empty() { "未设置" } else { "已设置" }),
        format!("语音回复:    {}", if state.config.qq_voice_enabled { "开启" } else { "关闭" }),
        "".to_string(),
        "使用 /qq login 配置 AppID 和 Secret".to_string(),
        "使用 /qq start 启动机器人".to_string(),
        "使用 /qq stop 停止机器人".to_string(),
    ]
}

// ─── OneBot ───

fn cmd_onebot(state: &AppState) -> Vec<String> {
    vec![
        format!("OneBot WS 服务: {}", if state.config.onebot_enabled { "已启用" } else { "未启用" }),
        format!("WebSocket 端口: {}", state.config.onebot_ws_port),
        "".to_string(),
        "使用 /onebot start 启动 OneBot WS 服务".to_string(),
        "使用 /onebot stop 停止 OneBot WS 服务".to_string(),
    ]
}

// ─── Persona ───

fn cmd_persona(parts: &[String], state: &mut AppState) -> Vec<String> {
    match parts.get(1).map(|s| s.as_str()) {
        Some("list") => {
            let mut lines = vec!["可用角色:".to_string()];
            for p in &state.personas {
                let marker = if p.name == state.active_persona { " *" } else { "  " };
                lines.push(format!("{}{}: {}", marker, p.name, p.display_name));
            }
            lines
        }
        Some("set") => {
            if parts.len() < 3 {
                return vec!["用法: /persona set <名称>".to_string()];
            }
            let name = &parts[2];
            if state.personas.iter().any(|p| &p.name == name) {
                state.active_persona = name.clone();
                state.config.persona = name.clone();
                let _ = state.config.save("config.json");
                vec![format!("已切换到角色: {}", name)]
            } else {
                vec![format!("未找到角色: {}", name)]
            }
        }
        _ => vec!["用法: /persona <list|set>".to_string()],
    }
}

// ─── Memory ───

fn cmd_memory(parts: &[String], state: &mut AppState) -> Vec<String> {
    match parts.get(1).map(|s| s.as_str()) {
        Some("clear") | None => {
            state.store.chat_clear();
            state.store.bot_clear_platform("wechat");
            state.store.bot_clear_platform("qq");
            state.store.bot_clear_platform("onebot");
            vec!["所有对话记忆已清空".to_string()]
        }
        _ => vec!["用法: /memory clear".to_string()],
    }
}

// ─── Status ───

fn cmd_status(state: &AppState) -> Vec<String> {
    let chat_count = state.store.chat_count();
    vec![
        "═══════ 系统状态 ═══════".to_string(),
        format!("API Key:       {}", if state.config.api_key().is_empty() { "未设置".to_string() } else { "已设置".to_string() }),
        format!("角色:          {}", state.active_persona),
        format!("TTS:           {}", if state.tts.connected { "已连接" } else { "未连接" }),
        format!("微信:          {}", if state.wechat_logged_in { "已登录" } else { "未登录" }),
        format!("微信机器人:    {}", if state.wechat_running { "运行中" } else { "未运行" }),
        format!("QQ 机器人:     {}", if state.qq_running { "运行中" } else { "未运行" }),
        format!("QQ 语音:       {}", if state.config.qq_voice_enabled { "开启" } else { "关闭" }),
        format!("聊天历史:      {} 条", chat_count),
    ]
}

// ─── QR Code utilities ───

fn print_qr_in_terminal(content: &str) {
    match QrCode::new(content.as_bytes()) {
        Ok(code) => {
            let image = code.render::<image::Luma<u8>>().build();
            let (w, h) = image.dimensions();
            let scale = 2u32;

            for y in (0..h).step_by(scale as usize * 2) {
                let mut line = String::new();
                for x in (0..w).step_by(scale as usize) {
                    let mut dark = 0u32;
                    let mut total = 0u32;
                    for dy in 0..(scale * 2) {
                        for dx in 0..scale {
                            let px = (x + dx).min(w - 1);
                            let py = (y + dy).min(h - 1);
                            let pixel = image.get_pixel(px, py).0[0];
                            if pixel < 128 {
                                dark += 1;
                            }
                            total += 1;
                        }
                    }
                    if total > 0 && dark > total / 2 {
                        line.push_str("\u{2588}\u{2588}");
                    } else {
                        line.push_str("  ");
                    }
                }
                println!("{}", line);
            }
        }
        Err(e) => {
            println!("无法生成二维码: {}", e);
        }
    }
}

fn save_qr_as_image(content: &str, path: &str) -> Result<(), String> {
    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("生成二维码失败: {}", e))?;
    let image = code.render::<image::Luma<u8>>().build();
    image.save(path).map_err(|e| format!("保存图片失败: {}", e))
}
