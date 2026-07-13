#![allow(dead_code, unused_variables, unused_mut)]
mod api;
mod cli;
mod config;
mod memory;
mod persona;
mod qq;
mod tts;
mod wechat;

use cli::AppState;
use config::Config;
use memory::MemoryStore;
use reedline::{Prompt, PromptEditMode, PromptHistorySearch, Reedline, Signal};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;

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

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    println!("╔══════════════════════════════════════╗");
    println!("║     Character-Chat CLI v0.1.0        ║");
    println!("║  基于 DeepSeek 的 AI 对话客户端      ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("输入 /help 查看命令列表");

    let config = Config::load("config.json");
    let store = Arc::new(MemoryStore::open("data/deepseek_chat.db").unwrap_or_else(|e| {
        eprintln!("警告: 数据库初始化失败: {}", e);
        MemoryStore::open("data/deepseek_chat.db").expect("无法初始化数据库")
    }));

    let mut state = AppState::new(config, store);

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

    // Reedline setup
    let mut line_editor = Reedline::create();
    let prompt = SimplePrompt {
        text: "character-chat> ".to_string(),
    };

    loop {
        // Process QQ events (non-blocking)
        while let Ok(event) = qq_event_rx.try_recv() {
            match event {
                qq::QqEvent::MessageReceived { from_user, text } => {
                    let short = if from_user.len() > 16 {
                        format!("{}...", &from_user[..16])
                    } else {
                        from_user.clone()
                    };
                    println!("\n[QQ] <{}>: {}", short, text);
                }
                qq::QqEvent::BotReply { to_user: _, text } => {
                    let preview = if text.len() > 100 {
                        format!("{}...", &text[..100])
                    } else {
                        text
                    };
                    println!("\n[QQ] 机器人回复: {}", preview);
                }
                qq::QqEvent::Error(e) => {
                    eprintln!("\n[QQ] 错误: {}", e);
                }
                qq::QqEvent::Connected { username } => {
                    println!("\n[QQ] 已连接, 用户: {}", username);
                    qq_running.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                qq::QqEvent::Disconnected => {
                    println!("\n[QQ] 已断开");
                    qq_running.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                qq::QqEvent::StatusChanged { running } => {
                    state.qq_running = running;
                    println!("\n[QQ] 状态: {}", if running { "运行中" } else { "已停止" });
                }
                _ => {}
            }
        }

        // Process WeChat events (non-blocking)
        while let Ok(event) = wechat_event_rx.try_recv() {
            match event {
                wechat::WeChatEvent::MessageReceived { from_user, text } => {
                    let short = if from_user.len() > 16 {
                        format!("{}...", &from_user[..16])
                    } else {
                        from_user.clone()
                    };
                    println!("\n[微信] <{}>: {}", short, text);
                }
                wechat::WeChatEvent::BotReply { to_user: _, text } => {
                    let preview = if text.len() > 100 {
                        format!("{}...", &text[..100])
                    } else {
                        text
                    };
                    println!("\n[微信] 机器人回复: {}", preview);
                }
                wechat::WeChatEvent::BotError(e) => {
                    eprintln!("\n[微信] 错误: {}", e);
                }
                wechat::WeChatEvent::BotStatus { running } => {
                    state.wechat_running = running;
                    println!("\n[微信] 状态: {}", if running { "运行中" } else { "已停止" });
                }
                _ => {}
            }
        }

        // Read input
        let sig = line_editor.read_line(&prompt);
        match sig {
            Ok(Signal::Success(buffer)) => {
                let input = buffer.trim().to_string();
                if input.is_empty() {
                    continue;
                }

                // Handle QQ login
                if input == "/qq login" || input.starts_with("/qq login") {
                    println!("正在打开 QQ 配置界面...");

                    let current_app_id = state.config.qq_app_id.clone();
                    let current_app_secret = state.config.qq_app_secret.clone();

                    if let Some(qq_cfg) = qq::config_tui::run_config_tui(
                        &current_app_id,
                        &current_app_secret,
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
                    let store = state.store.clone();
                    let api_key = state.config.api_key.clone();
                    let api_url = state.config.api_url.clone();
                    let model = state.config.model.clone();
                    let max_tokens = state.config.max_tokens;
                    let temperature = state.config.temperature;
                    let top_p = state.config.top_p;
                    let system_prompt = state.system_prompt();
                    let event_tx = qq_event_tx.clone();

                    let (qq_stop, qq_stop_rx) = tokio::sync::watch::channel(false);
                    qq_stop_tx = Some(qq_stop);

                    state.qq_running = true;

                    tokio::spawn(async move {
                        let mut bot = qq::bot::QqBot::new(app_id, app_secret, store, event_tx);
                        let _ = bot.start(
                            api_key, api_url, model, max_tokens, temperature, top_p,
                            system_prompt, qq_stop_rx,
                        ).await;
                    });

                    println!("QQ 机器人已启动");
                    continue;
                }

                // Handle QQ stop
                if input == "/qq stop" || input.starts_with("/qq stop") {
                    state.qq_running = false;
                    if let Some(tx) = qq_stop_tx.take() {
                        let _ = tx.send(true);
                    }
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
                    let api_key = state.config.api_key.clone();
                    let api_url = state.config.api_url.clone();
                    let model = state.config.model.clone();
                    let max_tokens = state.config.max_tokens;
                    let temperature = state.config.temperature;
                    let top_p = state.config.top_p;
                    let system_prompt = state.system_prompt();
                    let store = state.store.clone();
                    let event_tx = wechat_event_tx.clone();

                    let (wx_stop, wx_stop_rx) = tokio::sync::watch::channel(false);
                    wechat_stop_tx = Some(wx_stop);

                    state.wechat_running = true;

                    tokio::spawn(async move {
                        let mut bot = wechat::bot::WeChatBot::new(creds, store, event_tx);
                        bot.start(
                            api_key, api_url, model, max_tokens, temperature, top_p,
                            system_prompt, wx_stop_rx,
                        )
                        .await;
                    });

                    println!("微信机器人已启动! 输入 /wechat stop 停止");
                    continue;
                }

                // Handle WeChat stop
                if input == "/wechat stop" || input == "/wx stop"
                    || input.starts_with("/wechat stop") || input.starts_with("/wx stop")
                {
                    state.wechat_running = false;
                    if let Some(tx) = wechat_stop_tx.take() {
                        let _ = tx.send(true);
                    }
                    println!("微信机器人停止信号已发送");
                    continue;
                }

                // Regular command processing
                let results = cli::handle_command(&input, &mut state).await;
                for line in results {
                    println!("{}", line);
                }

                if !state.running {
                    break;
                }

                if input.starts_with("/config set") {
                    let _ = state.config.save("config.json");
                }
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                println!("\n再见!");
                break;
            }
            Err(e) => {
                eprintln!("输入错误: {}", e);
                continue;
            }
        }
    }

    if let Some(tx) = qq_stop_tx {
        let _ = tx.send(true);
    }
    if let Some(tx) = wechat_stop_tx {
        let _ = tx.send(true);
    }
    state.running = false;

    println!("Character-Chat CLI 已退出");
}
