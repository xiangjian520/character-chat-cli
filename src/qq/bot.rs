use log::{error, info, warn};
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, mpsc};
use crate::api::ChatMessage;
use crate::memory::MemoryStore;
use crate::qq::{api, types::*, QqEvent};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub struct QqBot {
    pub app_id: String,
    pub app_secret: String,
    pub access_token: String,
    pub token_expires_at: i64,
    pub running: bool,
    sender: mpsc::UnboundedSender<QqEvent>,
    store: Arc<MemoryStore>,
    stop_tx: Option<tokio::sync::watch::Sender<bool>>,
    pub tts_config: Option<crate::tts::TtsConfig>,
    pub admins: Vec<String>,
    pub blacklist: Vec<String>,
    pub admin_tx: Option<mpsc::UnboundedSender<crate::cli::AdminCmd>>,
    pub plugin_mgr: Option<Arc<std::sync::Mutex<crate::plugin::PluginManager>>>,
}

impl QqBot {
    pub fn new(
        app_id: String,
        app_secret: String,
        store: Arc<MemoryStore>,
        sender: mpsc::UnboundedSender<QqEvent>,
    ) -> Self {
        Self {
            app_id,
            app_secret,
            access_token: String::new(),
            token_expires_at: 0,
            running: false,
            sender,
            store,
            stop_tx: None,
            tts_config: None,
            admins: Vec::new(),
            blacklist: Vec::new(),
            admin_tx: None,
            plugin_mgr: None,
        }
    }

    pub async fn start(
        &mut self,
        api_key: String,
        api_url: String,
        model: String,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        system_prompt: Option<String>,
        stop_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), String> {
        if self.running {
            return Ok(());
        }
        self.running = true;

        if let Err(e) = self.refresh_token().await {
            self.running = false;
            let _ = self.sender.send(QqEvent::StatusChanged { running: false });
            let _ = self.sender.send(QqEvent::Error(format!("获取token失败: {}", e)));
            return Err(format!("获取token失败: {}", e));
        }
        let _ = self.sender.send(QqEvent::Token {
            access_token: self.access_token.clone(),
        });

        let (internal_tx, _) = broadcast::channel::<QqEvent>(256);

        let shared_token = Arc::new(RwLock::new(self.access_token.clone()));

        let intents = INTENT_GROUP_AND_C2C | INTENT_AUDIO_ACTION;
        let sender_spawn = self.sender.clone();
        let gateway_stop_rx = stop_rx.clone();
        let internal_tx_clone = internal_tx.clone();

        let api_key_for_handler = api_key.clone();
        let api_url_for_handler = api_url.clone();
        let model_for_handler = model.clone();
        let sp_for_handler = system_prompt.clone();
        let store_for_handler = self.store.clone();
        let sender_for_handler = self.sender.clone();
        let mut handler_stop_rx = stop_rx.clone();
        let mut internal_rx = internal_tx.subscribe();
        let tts_cfg_for_handler = self.tts_config.clone();
        let admins_for_handler = self.admins.clone();
        let blacklist_for_handler = self.blacklist.clone();
        let admin_tx_for_handler = self.admin_tx.clone();
        let plugin_mgr_for_handler = self.plugin_mgr.clone();

        let gateway_token = shared_token.clone();
        tokio::spawn(async move {
            loop {
                if *gateway_stop_rx.borrow() {
                    break;
                }
                let sender_c = sender_spawn.clone();
                let itx = internal_tx_clone.clone();
                let current_token = gateway_token.read().unwrap().clone();
                let result = super::ws::run_gateway(
                    current_token.clone(),
                    current_token,
                    intents,
                    move |event_type, data| {
                        let event = QqEvent::Raw { event_type, data };
                        let _ = sender_c.send(event.clone());
                        let _ = itx.send(event);
                    },
                    gateway_stop_rx.clone(),
                )
                .await;
                match result {
                    Ok(()) => break,
                    Err(e) => {
                        error!("[qqbot] 网关断开: {}，3秒后重连", e);
                        if *gateway_stop_rx.borrow() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                }
            }
        });

        let _ = self.sender.send(QqEvent::StatusChanged { running: true });

        let handler_token = shared_token.clone();
        tokio::spawn(async move {
            let api_key = api_key_for_handler;
            let api_url = api_url_for_handler;
            let model = model_for_handler;
            let system_prompt = sp_for_handler;
            let handler_store = store_for_handler;
            let handler_sender = sender_for_handler;
            let tts_config = tts_cfg_for_handler;
            let handler_admins = admins_for_handler;
            let handler_blacklist = blacklist_for_handler;
            let handler_admin_tx = admin_tx_for_handler;
            let plugin_mgr = plugin_mgr_for_handler;
            loop {
                tokio::select! {
                    _ = handler_stop_rx.changed() => break,
                    event = internal_rx.recv() => {
                        match event {
                            Ok(QqEvent::Raw { event_type, data }) => {
                                if event_type == "C2C_MESSAGE_CREATE" {
                                    let access = handler_token.read().unwrap().clone();
                                    if let Ok(msg) = serde_json::from_value::<C2cMessageEvent>(data) {
                                        let from_user = msg.author.as_ref()
                                            .and_then(|a| {
                                                if !a.user_openid.is_empty() { Some(a.user_openid.clone()) }
                                                else if !a.id.is_empty() { Some(a.id.clone()) }
                                                else { None }
                                            })
                                            .unwrap_or_default();

                                        if from_user.is_empty() { continue; }
                                        if handler_blacklist.iter().any(|b| b == &from_user) {
                                            continue;
                                        }
                                        let text = msg.content.clone();
                                        info!("[qqbot] 收到消息 from={}: {}",
                                            &from_user[..from_user.len().min(20)],
                                            text.chars().take(50).collect::<String>());

                                        let _ = handler_sender.send(QqEvent::MessageReceived {
                                            from_user: from_user.clone(),
                                            text: text.clone(),
                                        });

                                        if text.trim() == "/clear" || text.trim() == "/close" {
                                            handler_store.bot_clear("qq", &from_user);
                                            let _ = api::send_c2c_message(
                                                &access, &from_user, "对话已重置", None, None,
                                            ).await;
                                            continue;
                                        }

                                        if let Some(ref admin_tx) = handler_admin_tx {
                                            if let Some(rx) = crate::cli::check_admin_cmd(
                                                &from_user, &text, &handler_admins, admin_tx,
                                            ) {
                                                if let Ok(reply) = rx.await {
                                                let _ = api::send_c2c_message(
                                                    &access, &from_user, &reply, None, None,
                                                ).await;

                                                if let Some(ref pm) = plugin_mgr {
                                                    let ctx = crate::plugin::MessageContext {
                                                        protocol: "qq".into(),
                                                        user_id: from_user.clone(),
                                                        group_id: None,
                                                        text: String::new(),
                                                        is_admin: false,
                                                    };
                                                    pm.lock().unwrap().dispatch_reply(&ctx, &reply);
                                                }
                                                }
                                                continue;
                                            }
                                        }

                                        // Plugin hook: on_message
                                        let plugin_reply = {
                                            if let Some(ref pm) = plugin_mgr {
                                                let ctx = crate::plugin::MessageContext {
                                                    protocol: "qq".into(),
                                                    user_id: from_user.clone(),
                                                    group_id: None,
                                                    text: text.clone(),
                                                    is_admin: handler_admins.iter().any(|a| a == &from_user),
                                                };
                                                pm.lock().unwrap().dispatch_message(&ctx)
                                            } else {
                                                None
                                            }
                                        };
                                        if let Some(reply) = plugin_reply {
                                            let _ = api::send_c2c_message(
                                                &access, &from_user, &reply, None, None,
                                            ).await;
                                            continue;
                                        }

                                        handler_store.bot_add("qq", &from_user, "user", &text);

                                        let mut messages: Vec<ChatMessage> = Vec::new();
                                        if let Some(ref sp) = system_prompt {
                                            messages.push(ChatMessage {
                                                role: "system".into(),
                                                content: sp.clone(),
                                            });
                                        }
                                        messages.extend(handler_store.bot_context("qq", &from_user, 20));

                                        if messages.is_empty() {
                                            warn!("[qqbot] messages 为空，添加默认 system prompt");
                                            messages.push(ChatMessage {
                                                role: "system".into(),
                                                content: "你是一个友好的AI助手，简洁明了地回答问题。".to_string(),
                                            });
                                        }

                                        match crate::api::send_message_streaming(
                                            &api_key, &api_url, &model,
                                            max_tokens, temperature, top_p, messages,
                                            |_| {},
                                        ).await {
                                            Ok(reply) => {
                                                info!("[qqbot] AI 回复 to={}: {}",
                                                    &from_user[..from_user.len().min(20)],
                                                    reply.chars().take(50).collect::<String>());
                                                handler_store.bot_add("qq", &from_user, "assistant", &reply);
                                                let _ = api::send_c2c_message(
                                                    &access, &from_user, &reply, None, None,
                                                ).await;

                                                if let Some(ref tts_cfg) = tts_config {
                                                    if !tts_cfg.ref_audio_path.is_empty() {
                                                        let reply_clone = reply.clone();
                                                        let access_clone = access.clone();
                                                        let from_user_clone = from_user.clone();
                                                        let tts_cfg_clone = tts_cfg.clone();
                                                        tokio::spawn(async move {
                                                            match crate::tts::generate_speech(&tts_cfg_clone, &reply_clone).await {
                                                                Ok(audio) => {
                                                                    let b64 = BASE64.encode(&audio);
                                                                    if let Err(e) = api::send_c2c_voice(
                                                                        &access_clone, &from_user_clone, &b64, None, None,
                                                                    ).await {
                                                                        warn!("[qqbot] 发送语音失败: {}", e);
                                                                    } else {
                                                                        info!("[qqbot] 语音消息已发送 to={}", &from_user_clone[..from_user_clone.len().min(20)]);
                                                                    }
                                                                }
                                                                Err(e) => warn!("[qqbot] TTS 生成失败: {}", e),
                                                            }
                                                        });
                                                    }
                                                }

                                                let _ = handler_sender.send(QqEvent::BotReply {
                                                    to_user: from_user,
                                                    text: reply,
                                                });
                                            }
                                            Err(e) => {
                                                let _ = api::send_c2c_message(
                                                    &access, &from_user,
                                                    "抱歉，AI 暂时无法回复。", None, None,
                                                ).await;
                                                let _ = handler_sender.send(QqEvent::Error(e));
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        // Token refresh loop
        let app_id = self.app_id.clone();
        let app_secret = self.app_secret.clone();
        let mut self_expires = self.token_expires_at;
        let mut stop_rx_token = stop_rx.clone();
        let refresh_token_arc = shared_token.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
                    _ = stop_rx_token.changed() => break,
                }
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                if now >= self_expires {
                    match api::get_access_token(&app_id, &app_secret).await {
                        Ok(resp) => {
                            *refresh_token_arc.write().unwrap() = resp.access_token;
                            self_expires = now + resp.expires_in - 300;
                            info!("[qqbot] AccessToken 已刷新");
                        }
                        Err(e) => {
                            error!("[qqbot] 刷新token失败: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(true);
        }
        let _ = self.sender.send(QqEvent::StatusChanged { running: false });
    }

    async fn refresh_token(&mut self) -> Result<(), String> {
        let resp = api::get_access_token(&self.app_id, &self.app_secret).await?;
        self.access_token = resp.access_token;
        self.token_expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + resp.expires_in
            - 300;
        info!("[qqbot] AccessToken 已获取");
        Ok(())
    }
}
