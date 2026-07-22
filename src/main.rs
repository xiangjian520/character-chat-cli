#![allow(dead_code, unused_variables, unused_mut)]
mod api;
mod cli;
mod config;
mod memory;
mod onebot;
mod persona;
mod plugin;
mod plugins;
mod qq;
mod tts;
mod wechat;

use cli::AppState;
use config::Config;
use memory::MemoryStore;
use reedline::{Prompt, PromptEditMode, PromptHistorySearch, Reedline, Signal};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;

fn safe_truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_chars).collect::<String>())
    }
}

struct SimplePrompt {
    text: String,
}

impl Prompt for SimplePrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.text)
    }
    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_indicator(&self, _prompt_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_history_search_indicator(&self, _history_search: PromptHistorySearch) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn right_prompt_on_last_line(&self) -> bool {
        false
    }
}

#[derive(Debug)]
enum CliInput {
    Line(String),
    Exit,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    println!("╔═══════════════════════════════════════════╗");
    println!("║                                           ║");
    println!("║   ██████╗██╗  ██╗ █████╗ ██████╗  █████╗  ║");
    println!("║  ██╔════╝██║  ██║██╔══██╗██╔══██╗██╔══██╗ ║");
    println!("║  ██║     ███████║███████║██████╔╝███████║ ║");
    println!("║  ██║     ██╔══██║██╔══██║██╔══██╗██╔══██║ ║");
    println!("║  ╚██████╗██║  ██║██║  ██║██║  ██║██║  ██║ ║");
    println!("║   ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝ ║");
    println!("║                                           ║");
    println!("║       Character-Chat CLI  v0.1.1          ║");
    println!("║      欢迎使用 AI 角色扮演对话客户端       ║");
    println!("║                                           ║");
    println!("╚═══════════════════════════════════════════╝");
    println!();
    println!("输入 /help 查看命令列表");

    let config = Config::load("config.json");
    let redis_url = config.redis_url.clone();

    println!("[init] 检测 Redis 连接: {} ...", redis_url);
    let store = Arc::new(MemoryStore::open(&redis_url).unwrap_or_else(|e| {
        eprintln!("\n  Redis 连接失败!");
        eprintln!("  地址: {}", redis_url);
        eprintln!("  原因: {}", e);
        eprintln!("\n  请确认:");
        eprintln!("    1. Redis 服务已启动 (systemctl start redis / redis-server)");
        eprintln!("    2. 地址端口正确 (/config set redis_url <url>)");
        eprintln!("    3. 防火墙允许连接\n");
        std::process::exit(1);
    }));
    println!("[init] Redis 连接正常");

    // Plugin system (loaded before state so it can be passed in)
    let mut plugin_mgr = plugin::PluginManager::new();
    let factories = plugins::factory_list();
    if let Err(e) = plugin_mgr.load_static(&factories, &config.plugins) {
        eprintln!("[plugin] 编译时插件加载失败: {}", e);
    }
    match plugin_mgr.load_dynamic(std::path::Path::new("plugins"), &config.plugins) {
        Ok(loaded) => {
            if loaded.is_empty() {
                let dir = std::path::Path::new("plugins");
                if !dir.is_dir() {
                    eprintln!("[plugin] plugins/ 目录不存在，跳过动态插件");
                } else {
                    let dll_count = std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0);
                    if dll_count == 0 {
                        eprintln!("[plugin] plugins/ 目录为空，未发现动态插件");
                    }
                }
            }
        }
        Err(e) => eprintln!("[plugin] 扫描失败: {}", e),
    }
    let plugin_mgr = Arc::new(std::sync::Mutex::new(plugin_mgr));
    {
        let mut mgr = plugin_mgr.lock().unwrap();
        for msg in mgr.start_all() {
            eprintln!("{}", msg);
        }
    }

    let mut state = AppState::new(config, store, plugin_mgr);

    if let Some(creds) = wechat::auth::load_saved_credentials() {
        state.wechat_logged_in = true;
        state.wechat_credentials = Some(creds);
        println!("[微信] 已加载保存的凭证");
    }

    // QQ bot channel
    let (qq_event_tx, mut qq_event_rx) = mpsc::unbounded_channel::<qq::QqEvent>();
    let qq_running = Arc::new(AtomicBool::new(false));
    let mut qq_stop_tx: Option<tokio::sync::watch::Sender<bool>> = None;

    // WeChat bot channel
    let (wechat_event_tx, mut wechat_event_rx) = mpsc::unbounded_channel::<wechat::WeChatEvent>();
    let mut wechat_stop_tx: Option<tokio::sync::watch::Sender<bool>> = None;

    // OneBot channel
    let (ob_event_tx, mut ob_event_rx) = mpsc::unbounded_channel::<onebot::ObEvent>();
    let ob_connections: onebot::server::ConnMap =
        Arc::new(tokio::sync::RwLock::new(HashMap::new()));
    let ob_running = Arc::new(AtomicBool::new(false));
    let mut ob_stop_tx: Option<tokio::sync::watch::Sender<bool>> = None;

    // Admin command channel (shared across all bots)
    let (admin_tx, mut admin_rx) = mpsc::unbounded_channel::<cli::AdminCmd>();

    // CLI input channel — background thread reads Reedline, sends here
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<CliInput>();

    let admins = state.config.admins.clone();
    let blacklist = state.config.blacklist.clone();

    // Spawn CLI input reader in a background thread so it doesn't block async tasks
    std::thread::spawn(move || {
        let mut line_editor = Reedline::create();
        let prompt = SimplePrompt {
            text: "character-chat> ".to_string(),
        };
        loop {
            match line_editor.read_line(&prompt) {
                Ok(Signal::Success(buf)) => {
                    let _ = input_tx.send(CliInput::Line(buf.trim().to_string()));
                }
                Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                    let _ = input_tx.send(CliInput::Exit);
                    break;
                }
                Err(_) => {}
            }
        }
    });

    // Auto-start bots
    let auto_start = state.config.auto_start_onebot || state.config.auto_start_qq || state.config.auto_start_wechat;
    if auto_start {
        eprintln!("[auto-start] OneBot: {}, QQ: {}, WeChat: {}",
            if state.config.auto_start_onebot { "开启" } else { "关闭" },
            if state.config.auto_start_qq { "开启" } else { "关闭" },
            if state.config.auto_start_wechat { "开启" } else { "关闭" });

        // Auto-start OneBot
        if state.config.auto_start_onebot {
            let ob_api_key = state.config.api_key();
            let ob_api_url = state.config.api_url.clone();
            let ob_model = state.config.model.clone();
            let ob_max_tokens = state.config.max_tokens;
            let ob_temperature = state.config.temperature;
            let ob_top_p = state.config.top_p;
            let ob_system_prompt = state.system_prompt();
            let ob_active_persona = state.active_persona.clone();
            let ob_tts_cfg = if state.config.qq_voice_enabled {
                Some(state.tts.build_config())
            } else {
                None
            };
            let ob_store = state.store.clone();
            let ob_bind_addr = format!("127.0.0.1:{}", state.config.onebot_ws_port);
            let ob_at_only = state.config.onebot_at_only;
            let ob_ev_tx = ob_event_tx.clone();
            let ob_conn = ob_connections.clone();
            let ob_admins = admins.clone();
            let ob_blacklist = blacklist.clone();
            let ob_admin_tx = admin_tx.clone();
            let ob_plugin_mgr = state.plugin_mgr.clone();

            let (ob_stop, mut ob_stop_rx) = tokio::sync::watch::channel(false);
            ob_stop_tx = Some(ob_stop);
            ob_running.store(true, std::sync::atomic::Ordering::SeqCst);

            tokio::spawn(async move {
                let (inner_tx, mut inner_rx) = mpsc::unbounded_channel::<onebot::types::OneBotEvent>();
                let server_ev_tx = inner_tx.clone();
                let server_conn = ob_conn.clone();
                let server_stop = ob_stop_rx.clone();
                tokio::spawn(async move {
                    if let Err(e) = onebot::server::run_server(
                        ob_bind_addr, server_ev_tx, server_conn, server_stop,
                    ).await {
                        eprintln!("[onebot] 服务端错误: {}", e);
                    }
                });

                let mut handler: Option<onebot::bot::OneBotHandler> = None;
                loop {
                    tokio::select! {
                        _ = ob_stop_rx.changed() => break,
                        event = inner_rx.recv() => {
                            match event {
                                Some(e) => {
                                    let self_id = e.self_id.unwrap_or(0);
                                    if handler.is_none() && self_id != 0 {
                                        let mut h = onebot::bot::OneBotHandler::new(
                                            self_id,
                                            ob_conn.clone(),
                                            ob_store.clone(),
                                            ob_ev_tx.clone(),
                                        );
                                        h.tts_config = ob_tts_cfg.clone();
                                        h.at_only = ob_at_only;
                                        h.admins = ob_admins.clone();
                                        h.blacklist = ob_blacklist.clone();
                                        h.admin_tx = Some(ob_admin_tx.clone());
                                        h.plugin_mgr = Some(ob_plugin_mgr.clone());
                                        handler = Some(h);
                                    }
                                    if let Some(ref h) = handler {
                                        h.handle_event(
                                            e,
                                            &ob_api_key, &ob_api_url, &ob_model,
                                            ob_max_tokens, ob_temperature, ob_top_p,
                                            ob_system_prompt.as_deref(),
                                        ).await;
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
            });

            eprintln!("[auto-start] OneBot 已启动");
        }

        // Auto-start QQ
        if state.config.auto_start_qq
            && !state.config.qq_app_id.is_empty()
            && !state.config.qq_app_secret.is_empty()
        {
            let qq_app_id = state.config.qq_app_id.clone();
            let qq_app_secret = state.config.qq_app_secret.clone();
            let qq_store = state.store.clone();
            let qq_api_key = state.config.api_key();
            let qq_api_url = state.config.api_url.clone();
            let qq_model = state.config.model.clone();
            let qq_max_tokens = state.config.max_tokens;
            let qq_temperature = state.config.temperature;
            let qq_top_p = state.config.top_p;
            let qq_system_prompt = state.system_prompt();
            let qq_ev_tx = qq_event_tx.clone();
            let qq_tts_cfg = if state.config.qq_voice_enabled {
                Some(state.tts.build_config())
            } else {
                None
            };
            let qq_admins = admins.clone();
            let qq_blacklist = blacklist.clone();
            let qq_admin_tx = admin_tx.clone();
            let qq_plugin_mgr = state.plugin_mgr.clone();

            let (qq_stop, qq_stop_rx) = tokio::sync::watch::channel(false);
            qq_stop_tx = Some(qq_stop);
            state.qq_running = true;

            tokio::spawn(async move {
                let mut bot = qq::bot::QqBot::new(qq_app_id, qq_app_secret, qq_store, qq_ev_tx);
                bot.tts_config = qq_tts_cfg;
                bot.admins = qq_admins;
                bot.blacklist = qq_blacklist;
                bot.admin_tx = Some(qq_admin_tx);
                bot.plugin_mgr = Some(qq_plugin_mgr);
                let _ = bot.start(
                    qq_api_key, qq_api_url, qq_model, qq_max_tokens, qq_temperature, qq_top_p,
                    qq_system_prompt, qq_stop_rx,
                ).await;
            });

            eprintln!("[auto-start] QQ 已启动");
        }

        // Auto-start WeChat
        if state.config.auto_start_wechat && state.wechat_logged_in {
            let wx_creds = state.wechat_credentials.clone().unwrap();
            let wx_api_key = state.config.api_key();
            let wx_api_url = state.config.api_url.clone();
            let wx_model = state.config.model.clone();
            let wx_max_tokens = state.config.max_tokens;
            let wx_temperature = state.config.temperature;
            let wx_top_p = state.config.top_p;
            let wx_system_prompt = state.system_prompt();
            let wx_store = state.store.clone();
            let wx_ev_tx = wechat_event_tx.clone();
            let wx_admins = admins.clone();
            let wx_blacklist = blacklist.clone();
            let wx_admin_tx = admin_tx.clone();
            let wx_plugin_mgr = state.plugin_mgr.clone();

            let (wx_stop, wx_stop_rx) = tokio::sync::watch::channel(false);
            wechat_stop_tx = Some(wx_stop);
            state.wechat_running = true;

            tokio::spawn(async move {
                let mut bot = wechat::bot::WeChatBot::new(wx_creds, wx_store, wx_ev_tx);
                bot.admins = wx_admins;
                bot.blacklist = wx_blacklist;
                bot.admin_tx = Some(wx_admin_tx);
                bot.plugin_mgr = Some(wx_plugin_mgr);
                bot.start(
                    wx_api_key, wx_api_url, wx_model, wx_max_tokens, wx_temperature, wx_top_p,
                    wx_system_prompt, wx_stop_rx,
                ).await;
            });

            eprintln!("[auto-start] WeChat 已启动");
        }
    }

    loop {
        // Process bot events + admin commands + CLI input concurrently
        tokio::select! {
            // QQ events (await + batch drain)
            qq_ev = qq_event_rx.recv() => {
                let mut batch: Vec<qq::QqEvent> = Vec::new();
                if let Some(e) = qq_ev { batch.push(e); }
                while let Ok(e) = qq_event_rx.try_recv() { batch.push(e); }
                for event in batch {
                    match event {
                        qq::QqEvent::MessageReceived { from_user, text } => {
                            eprintln!("\n[QQ] <{}>: {}", safe_truncate(&from_user, 16), text);
                        }
                        qq::QqEvent::BotReply { to_user: _, text } => {
                            eprintln!("\n[QQ] 机器人回复: {}", safe_truncate(&text, 100));
                        }
                        qq::QqEvent::Error(e) => {
                            eprintln!("\n[QQ] 错误: {}", e);
                        }
                        qq::QqEvent::Connected { username } => {
                            eprintln!("\n[QQ] 已连接, 用户: {}", username);
                            qq_running.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        qq::QqEvent::Disconnected => {
                            eprintln!("\n[QQ] 已断开");
                            qq_running.store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                        qq::QqEvent::StatusChanged { running } => {
                            state.qq_running = running;
                            eprintln!("\n[QQ] 状态: {}", if running { "运行中" } else { "已停止" });
                        }
                        _ => {}
                    }
                }
            }

            // WeChat events (await + batch drain)
            wx_ev = wechat_event_rx.recv() => {
                let mut batch: Vec<wechat::WeChatEvent> = Vec::new();
                if let Some(e) = wx_ev { batch.push(e); }
                while let Ok(e) = wechat_event_rx.try_recv() { batch.push(e); }
                for event in batch {
                    match event {
                        wechat::WeChatEvent::MessageReceived { from_user, text } => {
                            eprintln!("\n[微信] <{}>: {}", safe_truncate(&from_user, 16), text);
                        }
                        wechat::WeChatEvent::BotReply { to_user: _, text } => {
                            eprintln!("\n[微信] 机器人回复: {}", safe_truncate(&text, 100));
                        }
                        wechat::WeChatEvent::BotError(e) => {
                            eprintln!("\n[微信] 错误: {}", e);
                        }
                        wechat::WeChatEvent::BotStatus { running } => {
                            state.wechat_running = running;
                            eprintln!("\n[微信] 状态: {}", if running { "运行中" } else { "已停止" });
                        }
                        _ => {}
                    }
                }
            }

            // OneBot events (await + batch drain)
            ob_ev = ob_event_rx.recv() => {
                let mut batch: Vec<onebot::ObEvent> = Vec::new();
                if let Some(e) = ob_ev { batch.push(e); }
                while let Ok(e) = ob_event_rx.try_recv() { batch.push(e); }
                for event in batch {
                    match event {
                        onebot::ObEvent::MessageReceived { self_id: _, user_id, group_id: _, text, .. } => {
                            eprintln!("\n[OneBot] <{}>: {}", user_id, text);
                        }
                        onebot::ObEvent::BotReply { user_id: _, text, .. } => {
                            eprintln!("\n[OneBot] 机器人回复: {}", safe_truncate(&text, 100));
                        }
                        onebot::ObEvent::Error(e) => {
                            eprintln!("\n[OneBot] 错误: {}", e);
                        }
                        onebot::ObEvent::StatusChanged { running } => {
                            ob_running.store(running, std::sync::atomic::Ordering::SeqCst);
                            eprintln!("\n[OneBot] 状态: {}", if running { "运行中" } else { "已停止" });
                        }
                    }
                }
            }

            // Admin commands from bots (now async, not blocked by CLI)
            cmd = admin_rx.recv() => {
                if let Some(cmd) = cmd {
                    if cmd.command == "/onebot stop" || cmd.command.starts_with("/onebot stop") {
                        ob_running.store(false, std::sync::atomic::Ordering::SeqCst);
                        if let Some(tx) = ob_stop_tx.take() {
                            let _ = tx.send(true);
                        }
                        let _ = cmd.reply_tx.send("OneBot 已停止".to_string());
                        continue;
                    }
                    let results = cli::handle_command(&cmd.command, &mut state).await;
                    let reply = results.join("\n");
                    let _ = cmd.reply_tx.send(reply);
                }
            }

            // CLI input from background thread
            input = input_rx.recv() => {
                match input {
                    Some(CliInput::Line(input)) => {
                        if input.is_empty() { continue; }

                        // Handle QQ login
                        if input == "/qq login" || input.starts_with("/qq login") {
                            println!("正在打开 QQ 配置界面...");
                            let current_app_id = state.config.qq_app_id.clone();
                            let current_app_secret = state.config.qq_app_secret.clone();
                            if let Some(qq_cfg) = qq::config_tui::run_config_tui(
                                &current_app_id, &current_app_secret,
                            ) {
                                state.config.qq_app_id = qq_cfg.app_id;
                                state.config.qq_app_secret = qq_cfg.app_secret;
                                let _ = state.config.save("config.json");
                                println!("\nQQ 配置已保存!");
                            } else {
                                println!("\n已取消配置");
                            }
                            continue;
                        }

                        // Handle QQ start
                        if input == "/qq start" || input.starts_with("/qq start") {
                            if state.config.qq_app_id.is_empty() || state.config.qq_app_secret.is_empty() {
                                println!("请先设置 QQ AppID: /config set qq_app_id <id>");
                                continue;
                            }
                            if state.qq_running {
                                println!("QQ 机器人已在运行");
                                continue;
                            }
                            let app_id = state.config.qq_app_id.clone();
                            let app_secret = state.config.qq_app_secret.clone();
                            let s = state.store.clone();
                            let api_key = state.config.api_key();
                            let api_url = state.config.api_url.clone();
                            let model = state.config.model.clone();
                            let max_tokens = state.config.max_tokens;
                            let temperature = state.config.temperature;
                            let top_p = state.config.top_p;
                            let system_prompt = state.system_prompt();
                            let event_tx = qq_event_tx.clone();
                            let tts_config = if state.config.qq_voice_enabled {
                                Some(state.tts.build_config())
                            } else { None };
                            let admins_clone = admins.clone();
                            let blacklist_clone = blacklist.clone();
                            let admin_tx_clone = admin_tx.clone();
                            let plugin_mgr_clone = state.plugin_mgr.clone();
                            let (qq_stop, qq_stop_rx) = tokio::sync::watch::channel(false);
                            qq_stop_tx = Some(qq_stop);
                            state.qq_running = true;
                            tokio::spawn(async move {
                                let mut bot = qq::bot::QqBot::new(app_id, app_secret, s, event_tx);
                                bot.tts_config = tts_config;
                                bot.admins = admins_clone;
                                bot.blacklist = blacklist_clone;
                                bot.admin_tx = Some(admin_tx_clone);
                                bot.plugin_mgr = Some(plugin_mgr_clone);
                                let _ = bot.start(api_key, api_url, model, max_tokens, temperature, top_p, system_prompt, qq_stop_rx).await;
                            });
                            println!("QQ 机器人已启动");
                            continue;
                        }

                        // Handle QQ stop
                        if input == "/qq stop" || input.starts_with("/qq stop") {
                            state.qq_running = false;
                            if let Some(tx) = qq_stop_tx.take() { let _ = tx.send(true); }
                            println!("QQ 机器人已停止");
                            continue;
                        }

                        // Handle WeChat start
                        if input == "/wechat start" || input == "/wx start"
                            || input.starts_with("/wechat start") || input.starts_with("/wx start")
                        {
                            if !state.wechat_logged_in {
                                println!("请先登录微信: /wechat login");
                                continue;
                            }
                            if state.wechat_running {
                                println!("微信机器人已在运行");
                                continue;
                            }
                            let creds = state.wechat_credentials.clone().unwrap();
                            let api_key = state.config.api_key();
                            let api_url = state.config.api_url.clone();
                            let model = state.config.model.clone();
                            let max_tokens = state.config.max_tokens;
                            let temperature = state.config.temperature;
                            let top_p = state.config.top_p;
                            let system_prompt = state.system_prompt();
                            let s = state.store.clone();
                            let event_tx = wechat_event_tx.clone();
                            let admins_wx = admins.clone();
                            let blacklist_wx = blacklist.clone();
                            let admin_tx_wx = admin_tx.clone();
                            let plugin_mgr_wx = state.plugin_mgr.clone();
                            let (wx_stop, wx_stop_rx) = tokio::sync::watch::channel(false);
                            wechat_stop_tx = Some(wx_stop);
                            state.wechat_running = true;
                            tokio::spawn(async move {
                                let mut bot = wechat::bot::WeChatBot::new(creds, s, event_tx);
                                bot.admins = admins_wx;
                                bot.blacklist = blacklist_wx;
                                bot.admin_tx = Some(admin_tx_wx);
                                bot.plugin_mgr = Some(plugin_mgr_wx);
                                bot.start(api_key, api_url, model, max_tokens, temperature, top_p, system_prompt, wx_stop_rx).await;
                            });
                            println!("微信机器人已启动! 输入 /wechat stop 停止");
                            continue;
                        }

                        // Handle WeChat stop
                        if input == "/wechat stop" || input == "/wx stop"
                            || input.starts_with("/wechat stop") || input.starts_with("/wx stop")
                        {
                            state.wechat_running = false;
                            if let Some(tx) = wechat_stop_tx.take() { let _ = tx.send(true); }
                            println!("微信机器人停止信号已发送");
                            continue;
                        }

                        // Handle OneBot start
                        if input == "/onebot start" || input.starts_with("/onebot start") {
                            if ob_running.load(std::sync::atomic::Ordering::SeqCst) {
                                println!("OneBot 已在运行");
                                continue;
                            }
                            let api_key = state.config.api_key();
                            let api_url = state.config.api_url.clone();
                            let model = state.config.model.clone();
                            let max_tokens = state.config.max_tokens;
                            let temperature = state.config.temperature;
                            let top_p = state.config.top_p;
                            let system_prompt = state.system_prompt();
                            let active_persona = state.active_persona.clone();
                            let tts_config = if state.config.qq_voice_enabled {
                                Some(state.tts.build_config())
                            } else { None };
                            let s = state.store.clone();
                            let bind_addr = format!("127.0.0.1:{}", state.config.onebot_ws_port);
                            let ob_at_only = state.config.onebot_at_only;
                            let ob_tx_for_handler = ob_event_tx.clone();
                            let connections = ob_connections.clone();
                            let (ob_stop, mut ob_stop_rx) = tokio::sync::watch::channel(false);
                            ob_stop_tx = Some(ob_stop);
                            ob_running.store(true, std::sync::atomic::Ordering::SeqCst);
                            let ob_admins = admins.clone();
                            let ob_blacklist = blacklist.clone();
                            let ob_admin_tx = admin_tx.clone();
                            let ob_plugin_mgr = state.plugin_mgr.clone();
                            tokio::spawn(async move {
                                let (inner_tx, mut inner_rx) = mpsc::unbounded_channel::<onebot::types::OneBotEvent>();
                                let server_event_tx = inner_tx.clone();
                                let server_connections = connections.clone();
                                let server_stop_rx = ob_stop_rx.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = onebot::server::run_server(bind_addr, server_event_tx, server_connections, server_stop_rx).await {
                                        eprintln!("[onebot] 服务端错误: {}", e);
                                    }
                                });
                                let mut handler: Option<onebot::bot::OneBotHandler> = None;
                                loop {
                                    tokio::select! {
                                        _ = ob_stop_rx.changed() => break,
                                        event = inner_rx.recv() => {
                                            match event {
                                                Some(e) => {
                                                    let self_id = e.self_id.unwrap_or(0);
                                                    if handler.is_none() && self_id != 0 {
                                                        let mut h = onebot::bot::OneBotHandler::new(self_id, connections.clone(), s.clone(), ob_tx_for_handler.clone());
                                                        h.tts_config = tts_config.clone();
                                                        h.at_only = ob_at_only;
                                                        h.admins = ob_admins.clone();
                                                        h.blacklist = ob_blacklist.clone();
                                                        h.admin_tx = Some(ob_admin_tx.clone());
                                                        h.plugin_mgr = Some(ob_plugin_mgr.clone());
                                                        handler = Some(h);
                                                    }
                                                    if let Some(ref h) = handler {
                                                        h.handle_event(e, &api_key, &api_url, &model, max_tokens, temperature, top_p, system_prompt.as_deref()).await;
                                                    }
                                                }
                                                None => break,
                                            }
                                        }
                                    }
                                }
                            });
                            eprintln!("OneBot 已启动! 端口: {}, 角色: {}, 语音: {}",
                                state.config.onebot_ws_port, active_persona,
                                if state.config.qq_voice_enabled { "开启" } else { "关闭" });
                            println!("请配置 OneBot 实现端连接 ws://127.0.0.1:{}/", state.config.onebot_ws_port);
                            continue;
                        }

                        // Handle OneBot stop
                        if input == "/onebot stop" || input.starts_with("/onebot stop") {
                            ob_running.store(false, std::sync::atomic::Ordering::SeqCst);
                            if let Some(tx) = ob_stop_tx.take() { let _ = tx.send(true); }
                            println!("OneBot 已停止");
                            continue;
                        }

                        // Regular command processing
                        let results = cli::handle_command(&input, &mut state).await;
                        for line in results {
                            println!("{}", line);
                        }
                        if !state.running { break; }
                        if input.starts_with("/config set") {
                            let _ = state.config.save("config.json");
                        }
                    }
                    Some(CliInput::Exit) | None => {
                        println!("\n再见!");
                        break;
                    }
                }
            }
        }
    }

    if let Some(tx) = qq_stop_tx { let _ = tx.send(true); }
    if let Some(tx) = wechat_stop_tx { let _ = tx.send(true); }
    state.running = false;
    println!("Character-Chat CLI 已退出");
}
