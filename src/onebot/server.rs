use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::types::{ApiRequest, IncomingMessage, OneBotEvent};

pub type ConnMap = Arc<RwLock<HashMap<i64, mpsc::UnboundedSender<String>>>>;

pub async fn run_server(
    bind_addr: String,
    event_tx: mpsc::UnboundedSender<OneBotEvent>,
    connections: ConnMap,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("OneBot WebSocket 绑定失败 ({bind_addr}): {e}"))?;

    info!("[onebot] WebSocket 服务已启动: {}", bind_addr);

    loop {
        tokio::select! {
            _ = stop_rx.changed() => break,
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        info!("[onebot] 新连接: {}", addr);
                        let event_tx = event_tx.clone();
                        let conns = connections.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, addr, event_tx, conns).await {
                                error!("[onebot] 连接 {} 错误: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => error!("[onebot] 接受连接失败: {}", e),
                }
            }
        }
    }

    {
        let mut map = connections.write().await;
        map.clear();
    }

    info!("[onebot] WebSocket 服务已停止");
    Ok(())
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    event_tx: mpsc::UnboundedSender<OneBotEvent>,
    connections: ConnMap,
) -> Result<(), String> {
    let ws = accept_async(stream)
        .await
        .map_err(|e| format!("WebSocket 握手失败: {e}"))?;

    let (mut write, mut read) = ws.split();

    let (send_tx, mut send_rx) = mpsc::unbounded_channel::<String>();
    let mut self_id: i64 = 0;
    let mut identified = false;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        if msg.is_ping() || msg.is_pong() {
                            continue;
                        }
                        if msg.is_close() {
                            break;
                        }
                        let text = msg.to_text()
                            .map_err(|e| format!("消息不是文本: {e}"))?
                            .to_string();

                        match serde_json::from_str::<IncomingMessage>(&text) {
                            Ok(IncomingMessage::Event(event)) => {
                                if !identified {
                                    if let Some(id) = event.self_id {
                                        self_id = id;
                                        identified = true;
                                        connections.write().await.insert(self_id, send_tx.clone());
                                        info!("[onebot] 识别机器人 QQ: {}, addr={}", self_id, addr);
                                    }
                                }
                                let _ = event_tx.send(event);
                            }
                            Ok(IncomingMessage::Response(_resp)) => {
                                // API 响应暂不处理
                            }
                            Err(e) => {
                                warn!("[onebot] 无法解析消息: {} raw={}", e, &text[..text.len().min(200)]);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        error!("[onebot] 读取错误: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            data = send_rx.recv() => {
                match data {
                    Some(text) => {
                        if let Err(e) = write.send(Message::Text(text.into())).await {
                            error!("[onebot] 发送失败: {}", e);
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    if self_id != 0 {
        connections.write().await.remove(&self_id);
        info!("[onebot] 连接断开, self_id={}", self_id);
    }
    Ok(())
}

pub async fn send_api(to: &ConnMap, self_id: i64, request: &ApiRequest) -> Result<(), String> {
    let json = serde_json::to_string(request).map_err(|e| format!("序列化失败: {e}"))?;
    let map = to.read().await;
    if let Some(tx) = map.get(&self_id) {
        tx.send(json).map_err(|e| format!("发送API失败: {e}"))
    } else {
        Err(format!("未找到 self_id={} 的连接", self_id))
    }
}
