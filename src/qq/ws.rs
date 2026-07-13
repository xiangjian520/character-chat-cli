use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use tokio::time::{interval, Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use crate::qq::types::*;

pub async fn run_gateway(
    access_token: String,
    token: String,
    intents: u32,
    on_event: impl Fn(String, serde_json::Value) + Send + 'static,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let gateway_url = super::api::get_gateway_url(&access_token).await?;
    info!("[qqbot] 连接网关: {}", gateway_url);

    let (ws_stream, _) = connect_async(&gateway_url)
        .await
        .map_err(|e| format!("WebSocket连接失败: {}", e))?;
    let (mut write, mut read) = ws_stream.split();

    let mut last_seq: u64 = 0;
    let mut heartbeat_interval_ms: u64 = 45000;
    let mut heartbeat_timer = interval(Duration::from_millis(heartbeat_interval_ms));
    let mut identified = false;
    let mut need_heartbeat = false;

    loop {
        tokio::select! {
            biased;
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let payload: GatewayPayload = match serde_json::from_str(&text) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!("[qqbot] 解析消息失败: {}", e);
                                continue;
                            }
                        };
                        if let Some(s) = payload.s {
                            last_seq = s;
                        }

                        match payload.op {
                            OP_HELLO => {
                                need_heartbeat = true;
                                if let Some(d) = &payload.d {
                                    if let Ok(h) = serde_json::from_value::<HelloData>(d.clone()) {
                                        heartbeat_interval_ms = h.heartbeat_interval;
                                        heartbeat_timer = interval(Duration::from_millis(heartbeat_interval_ms));
                                    }
                                }
                                let identify = GatewayPayload {
                                    op: OP_IDENTIFY,
                                    s: None, t: None,
                                    d: Some(serde_json::to_value(IdentifyData {
                                        token: format!("QQBot {}", token),
                                        intents,
                                        shard: [0, 1],
                                    }).unwrap()),
                                };
                                let json = serde_json::to_string(&identify).unwrap();
                                let _ = write.send(Message::Text(json.into())).await;
                                info!("[qqbot] 已发送鉴权");
                            }
                            OP_DISPATCH => {
                                let event_type = payload.t.clone().unwrap_or_default();
                                if let Some(d) = &payload.d {
                                    on_event(event_type, d.clone());
                                }
                            }
                            OP_INVALID_SESSION => {
                                error!("[qqbot] 无效session");
                                return Err("Invalid session".into());
                            }
                            OP_RECONNECT => {
                                warn!("[qqbot] 服务端要求重连");
                                return Err("Server reconnect".into());
                            }
                            _ => {}
                        }

                        if let Some(ref t) = payload.t {
                            if t == "READY" {
                                if let Some(d) = &payload.d {
                                    if let Ok(ready) = serde_json::from_value::<ReadyData>(d.clone()) {
                                        identified = true;
                                        info!("[qqbot] 就绪, user={}",
                                            ready.user.as_ref().map(|u| u.username.as_str()).unwrap_or("?"));
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => return Ok(()),
                    Some(Ok(Message::Ping(d))) => {
                        let _ = write.send(Message::Pong(d)).await;
                    }
                    Some(Err(e)) => return Err(format!("WebSocket错误: {}", e)),
                    None => return Ok(()),
                    _ => {}
                }
            }
            _ = stop_rx.changed() => {
                info!("[qqbot] 收到停止信号");
                let _ = write.send(Message::Close(None)).await;
                return Ok(());
            }
            _ = async {
                if !need_heartbeat {
                    sleep(Duration::from_secs(3600)).await;
                    return;
                }
                heartbeat_timer.tick().await;
            } => {
                if need_heartbeat && identified {
                    let hb = GatewayPayload {
                        op: OP_HEARTBEAT,
                        s: if last_seq > 0 { Some(last_seq) } else { None },
                        t: None,
                        d: None,
                    };
                    let json = serde_json::to_string(&hb).unwrap();
                    let _ = write.send(Message::Text(json.into())).await;
                }
            }
        }
    }
}
