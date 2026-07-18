use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use log::{error, info, warn};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::api::ChatMessage;
use crate::cli;
use crate::memory::MemoryStore;
use super::server::{send_api, ConnMap};
use super::types::{ApiRequest, OneBotEvent};

fn build_api(action: &str, params: serde_json::Value) -> ApiRequest {
    ApiRequest {
        action: action.to_string(),
        params,
        echo: None,
    }
}

pub struct OneBotHandler {
    pub self_id: i64,
    connections: ConnMap,
    store: Arc<MemoryStore>,
    sender: mpsc::UnboundedSender<super::ObEvent>,
    pub tts_config: Option<crate::tts::TtsConfig>,
    pub admins: Vec<String>,
    pub admin_tx: Option<mpsc::UnboundedSender<cli::AdminCmd>>,
    pub plugin_mgr: Option<Arc<std::sync::Mutex<crate::plugin::PluginManager>>>,
}

impl OneBotHandler {
    pub fn new(
        self_id: i64,
        connections: ConnMap,
        store: Arc<MemoryStore>,
        sender: mpsc::UnboundedSender<super::ObEvent>,
    ) -> Self {
        Self {
            self_id,
            connections,
            store,
            sender,
            tts_config: None,
            admins: Vec::new(),
            admin_tx: None,
            plugin_mgr: None,
        }
    }

    pub async fn handle_event(
        &self,
        event: OneBotEvent,
        api_key: &str,
        api_url: &str,
        model: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        system_prompt: Option<&str>,
    ) {
        let post_type = event.post_type.as_deref().unwrap_or("");
        if post_type != "message" {
            return;
        }

        let message_type = event.message_type.as_deref().unwrap_or("");
        if message_type != "private" && message_type != "group" {
            return;
        }

        let user_id = match event.user_id {
            Some(id) => id,
            None => return,
        };
        let group_id = event.group_id;

        let text = event.message.as_ref()
            .map(|m| super::types::extract_text(m))
            .unwrap_or_default();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        let display_id = format!("ob_{}", user_id);
        info!("[onebot] 收到消息 user={user_id}, group={:?}, text={}",
            group_id,
            text.chars().take(50).collect::<String>());

        let _ = self.sender.send(super::ObEvent::MessageReceived {
            self_id: self.self_id,
            user_id,
            group_id,
            message_id: event.message_id,
            text: text.clone(),
            raw: serde_json::to_value(&event).unwrap_or_default(),
        });

        if text == "/clear" {
            self.store.bot_clear("onebot", &display_id);
            let (action, params) = build_text_msg(user_id, group_id, "对话已重置");
            let _ = send_api(&self.connections, self.self_id, &build_api(&action, params)).await;
            return;
        }

        // Admin commands
        if let Some(ref admin_tx) = self.admin_tx {
            let user_id_str = user_id.to_string();
            if let Some(rx) = cli::check_admin_cmd(&user_id_str, &text, &self.admins, admin_tx) {
                if let Ok(reply) = rx.await {
                    let (action, params) = build_text_msg(user_id, group_id, &reply);
                    let _ = send_api(&self.connections, self.self_id, &build_api(&action, params)).await;
                }
                return;
            }
        }

        // Plugin hook: on_message
        let plugin_reply = {
            if let Some(ref pm) = self.plugin_mgr {
                let ctx = crate::plugin::MessageContext {
                    protocol: "onebot".into(),
                    user_id: user_id.to_string(),
                    group_id: group_id.map(|g| g.to_string()),
                    text: text.clone(),
                    is_admin: self.admins.iter().any(|a| a == &user_id.to_string()),
                };
                pm.lock().unwrap().dispatch_message(&ctx)
            } else {
                None
            }
        };
        if let Some(reply) = plugin_reply {
            let (action, params) = build_text_msg(user_id, group_id, &reply);
            let _ = send_api(&self.connections, self.self_id, &build_api(&action, params)).await;
            return;
        }

        self.store.bot_add("onebot", &display_id, "user", &text);

        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(sp) = system_prompt {
            messages.push(ChatMessage { role: "system".into(), content: sp.to_string() });
        }
        messages.extend(self.store.bot_context("onebot", &display_id, 20));

        match crate::api::send_message_async(
            api_key, api_url, model, max_tokens, temperature, top_p, messages,
        ).await {
            Ok(reply) => {
                info!("[onebot] AI 回复 user={}: {}",
                    user_id,
                    reply.chars().take(50).collect::<String>());
                self.store.bot_add("onebot", &display_id, "assistant", &reply);

                let (action, params) = build_text_msg(user_id, group_id, &reply);
                if let Err(e) = send_api(&self.connections, self.self_id, &build_api(&action, params)).await {
                    error!("[onebot] 发送回复失败: {}", e);
                }

                if let Some(ref pm) = self.plugin_mgr {
                    let ctx = crate::plugin::MessageContext {
                        protocol: "onebot".into(),
                        user_id: user_id.to_string(),
                        group_id: group_id.map(|g| g.to_string()),
                        text: String::new(),
                        is_admin: false,
                    };
                    pm.lock().unwrap().dispatch_reply(&ctx, &reply);
                }

                // TTS voice
                if let Some(ref tts_cfg) = self.tts_config {
                    if !tts_cfg.ref_audio_path.is_empty() {
                        let reply_clone = reply.clone();
                        let connections = self.connections.clone();
                        let self_id = self.self_id;
                        let tts_cfg = tts_cfg.clone();
                        tokio::spawn(async move {
                            match crate::tts::generate_speech(&tts_cfg, &reply_clone).await {
                                Ok(audio) => {
                                    let b64 = BASE64.encode(&audio);
                                    let voice_msg = serde_json::json!([
                                        {"type": "record", "data": {"file": format!("base64://{}", b64)}}
                                    ]);
                                    let (va, vp) = build_voice_msg(user_id, group_id, voice_msg);
                                    if let Err(e) = send_api(&connections, self_id, &build_api(&va, vp)).await {
                                        warn!("[onebot] 发送语音失败: {}", e);
                                    }
                                }
                                Err(e) => warn!("[onebot] TTS 生成失败: {}", e),
                            }
                        });
                    }
                }

                let _ = self.sender.send(super::ObEvent::BotReply {
                    user_id,
                    group_id,
                    text: reply,
                });
            }
            Err(e) => {
                let (action, params) = build_text_msg(user_id, group_id, "抱歉，AI 暂时无法回复。");
                let _ = send_api(&self.connections, self.self_id, &build_api(&action, params)).await;
                let _ = self.sender.send(super::ObEvent::Error(e));
            }
        }
    }
}

fn build_text_msg(user_id: i64, group_id: Option<i64>, text: &str) -> (String, serde_json::Value) {
    let (action, params) = if let Some(gid) = group_id {
        ("send_group_msg".to_string(), serde_json::json!({ "group_id": gid, "message": text }))
    } else {
        ("send_private_msg".to_string(), serde_json::json!({ "user_id": user_id, "message": text }))
    };
    (action, params)
}

fn build_voice_msg(user_id: i64, group_id: Option<i64>, message: serde_json::Value) -> (String, serde_json::Value) {
    let (action, params) = if let Some(gid) = group_id {
        ("send_group_msg".to_string(), serde_json::json!({ "group_id": gid, "message": message }))
    } else {
        ("send_private_msg".to_string(), serde_json::json!({ "user_id": user_id, "message": message }))
    };
    (action, params)
}
