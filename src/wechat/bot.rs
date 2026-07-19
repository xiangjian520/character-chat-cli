use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::memory::MemoryStore;
use crate::wechat::{api, types::*, WeChatEvent};

pub struct WeChatBot {
    pub credentials: LoginCredentials,
    pub running: bool,
    get_updates_buf: String,
    context_tokens: HashMap<String, String>,
    store: Arc<MemoryStore>,
    sender: mpsc::UnboundedSender<WeChatEvent>,
    stop_tx: Option<tokio::sync::watch::Sender<bool>>,
    pub admins: Vec<String>,
    pub blacklist: Vec<String>,
    pub admin_tx: Option<mpsc::UnboundedSender<crate::cli::AdminCmd>>,
    pub plugin_mgr: Option<Arc<std::sync::Mutex<crate::plugin::PluginManager>>>,
}

impl WeChatBot {
    pub fn new(
        credentials: LoginCredentials,
        store: Arc<MemoryStore>,
        sender: mpsc::UnboundedSender<WeChatEvent>,
    ) -> Self {
        store.bot_prune(50);
        Self {
            credentials,
            running: false,
            get_updates_buf: String::new(),
            context_tokens: HashMap::new(),
            store,
            sender,
            stop_tx: None,
            admins: Vec::new(),
            blacklist: Vec::new(),
            admin_tx: None,
            plugin_mgr: None,
        }
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<WeChatEvent> {
        self.sender.clone()
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
    ) {
        if self.running {
            let _ = self.sender.send(WeChatEvent::BotStatus { running: true });
            return;
        }

        self.running = true;

        info!("[wechat] 机器人启动");
        let _ = self
            .sender
            .send(WeChatEvent::BotStatus { running: true });

        self.run_loop(
            api_key, api_url, model, max_tokens, temperature, top_p, system_prompt, stop_rx,
        )
        .await;
    }

    async fn run_loop(
        &mut self,
        api_key: String,
        api_url: String,
        model: String,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        system_prompt: Option<String>,
        mut stop_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut failures: u32 = 0;
        const MAX_FAILURES: u32 = 5;
        const BACKOFF_MS: u64 = 30_000;
        const RETRY_MS: u64 = 2_000;

        while self.running {
            tokio::select! {
                _ = stop_rx.changed() => {
                    self.running = false;
                    break;
                }
                result = api::get_updates(
                    &self.credentials.base_url,
                    &self.credentials.token,
                    &self.get_updates_buf,
                ) => {
                    match result {
                        Ok(resp) => {
                            if resp.ret.unwrap_or(0) != 0 {
                                failures += 1;
                                error!(
                                    "[wechat] getUpdates 错误: ret={:?} errcode={:?}",
                                    resp.ret, resp.errcode
                                );
                                if failures >= MAX_FAILURES {
                                    let _ = self.sender.send(WeChatEvent::BotError(
                                        format!("连续 {} 次失败", failures),
                                    ));
                                    tokio::time::sleep(std::time::Duration::from_millis(BACKOFF_MS)).await;
                                    failures = 0;
                                } else {
                                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_MS)).await;
                                }
                                continue;
                            }
                            failures = 0;
                            if let Some(ref buf) = resp.get_updates_buf {
                                self.get_updates_buf = buf.clone();
                            }
                            if let Some(ref msgs) = resp.msgs {
                                for msg in msgs {
                                    self.handle_message(
                                        msg, &api_key, &api_url, &model,
                                        max_tokens, temperature, top_p,
                                        system_prompt.as_deref(),
                                    ).await;
                                }
                            }
                        }
                        Err(e) => {
                            failures += 1;
                            error!("[wechat] 轮询异常: {}", e);
                            if failures >= MAX_FAILURES {
                                let _ = self.sender.send(WeChatEvent::BotError(e));
                                tokio::time::sleep(std::time::Duration::from_millis(BACKOFF_MS)).await;
                                failures = 0;
                            } else {
                                tokio::time::sleep(std::time::Duration::from_millis(RETRY_MS)).await;
                            }
                        }
                    }
                }
            }
        }
        let _ = self
            .sender
            .send(WeChatEvent::BotStatus { running: false });
        info!("[wechat] 机器人已停止");
    }

    pub fn stop(&mut self) {
        self.running = false;
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(true);
        }
    }

    async fn handle_message(
        &mut self,
        msg: &WeixinMessage,
        api_key: &str,
        api_url: &str,
        model: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        system_prompt: Option<&str>,
    ) {
        if msg.message_type != Some(MESSAGE_TYPE_USER) {
            return;
        }
        let from_user = match &msg.from_user_id {
            Some(u) if !u.is_empty() => u.clone(),
            _ => return,
        };

        if self.blacklist.iter().any(|b| b == &from_user) {
            return;
        }

        if let Some(ref ct) = msg.context_token {
            self.context_tokens.insert(from_user.clone(), ct.clone());
        }

        let text = extract_text(msg);
        if text.trim().is_empty() {
            return;
        }

        info!(
            "[wechat] 收到消息 from={}: {}",
            &from_user[..from_user.len().min(20)],
            text.chars().take(50).collect::<String>()
        );
        let _ = self.sender.send(WeChatEvent::MessageReceived {
            from_user: from_user.clone(),
            text: text.clone(),
        });

        if text.trim() == "/clear" || text.trim() == "/close" {
            self.store.bot_clear("wechat", &from_user);
            let _ = self.reply(&from_user, "对话已重置").await;
            return;
        }

        if let Some(ref admin_tx) = self.admin_tx {
            if let Some(rx) = crate::cli::check_admin_cmd(&from_user, &text, &self.admins, admin_tx) {
                if let Ok(reply) = rx.await {
                    let _ = self.reply(&from_user, &reply).await;
                }
                return;
            }
        }

        // Plugin hook: on_message
        let plugin_reply = {
            if let Some(ref pm) = self.plugin_mgr {
                let ctx = crate::plugin::MessageContext {
                    protocol: "wechat".into(),
                    user_id: from_user.clone(),
                    group_id: None,
                    text: text.clone(),
                    is_admin: self.admins.iter().any(|a| a == &from_user),
                };
                pm.lock().unwrap().dispatch_message(&ctx)
            } else {
                None
            }
        };
        if let Some(reply) = plugin_reply {
            let _ = self.reply(&from_user, &reply).await;
            return;
        }

        self.store.bot_add("wechat", &from_user, "user", &text);

        let mut messages: Vec<crate::api::ChatMessage> = Vec::new();
        if let Some(sp) = system_prompt {
            messages.push(crate::api::ChatMessage {
                role: "system".into(),
                content: sp.to_string(),
            });
        }
        messages.extend(self.store.bot_context("wechat", &from_user, 20));

        match crate::api::send_message_async(
            api_key, api_url, model, max_tokens, temperature, top_p, messages,
        )
        .await
        {
            Ok(reply) => {
                info!(
                    "[wechat] AI 回复 to={}: {}",
                    &from_user[..from_user.len().min(20)],
                    reply.chars().take(50).collect::<String>()
                );
                self.store
                    .bot_add("wechat", &from_user, "assistant", &reply);
                let _ = self.reply(&from_user, &reply).await;

                if let Some(ref pm) = self.plugin_mgr {
                    let ctx = crate::plugin::MessageContext {
                        protocol: "wechat".into(),
                        user_id: from_user.clone(),
                        group_id: None,
                        text: String::new(),
                        is_admin: false,
                    };
                    pm.lock().unwrap().dispatch_reply(&ctx, &reply);
                }
                let _ = self.sender.send(WeChatEvent::BotReply {
                    to_user: from_user,
                    text: reply,
                });
            }
            Err(e) => {
                let _ = self
                    .reply(&from_user, "抱歉，AI 暂时无法回复，请稍后再试。")
                    .await;
                let _ = self.sender.send(WeChatEvent::BotError(e));
            }
        }
    }

    async fn reply(&self, to: &str, text: &str) -> Result<(), ()> {
        let ct = self.context_tokens.get(to);
        match api::send_text_message(
            &self.credentials.base_url,
            &self.credentials.token,
            to,
            text,
            ct.map(|s| s.as_str()),
        )
        .await
        {
            Ok(()) => Ok(()),
            Err(e) => {
                error!("[wechat] 发送消息失败 to={}: {}", to, e);
                let _ = self
                    .sender
                    .send(WeChatEvent::BotError(format!("发送失败: {}", e)));
                Err(())
            }
        }
    }
}
