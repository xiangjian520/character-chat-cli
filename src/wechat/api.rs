use log::warn;
use rand::Rng;
use crate::wechat::types::*;

const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;

fn random_wechat_uin() -> String {
    let n: u32 = rand::thread_rng().gen();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, n.to_be_bytes())
}

fn build_headers(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    headers.insert(
        "AuthorizationType",
        "ilink_bot_token".parse().unwrap(),
    );
    headers.insert(
        "X-WECHAT-UIN",
        random_wechat_uin().parse().unwrap(),
    );
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", token).parse().unwrap(),
    );
    headers
}

async fn api_post<T: serde::de::DeserializeOwned>(
    base_url: &str,
    endpoint: &str,
    body: &serde_json::Value,
    token: &str,
    timeout_secs: u64,
) -> Result<T, String> {
    let url = if base_url.ends_with('/') {
        format!("{}{}", base_url, endpoint)
    } else {
        format!("{}/{}", base_url, endpoint)
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;

    let resp = client
        .post(&url)
        .headers(build_headers(token))
        .json(body)
        .send()
        .await
        .map_err(|e| format!("API 请求失败: {}", e))?;

    let text = resp.text().await.unwrap_or_default();
    if text.is_empty() {
        return Err("API 返回空响应".to_string());
    }
    serde_json::from_str::<T>(&text).map_err(|e| format!("解析响应失败: {} - {}", e, text))
}

pub async fn get_updates(
    base_url: &str,
    token: &str,
    buf: &str,
) -> Result<GetUpdatesResp, String> {
    let body = serde_json::json!({ "get_updates_buf": buf });
    match api_post::<GetUpdatesResp>(
        base_url,
        "ilink/bot/getupdates",
        &body,
        token,
        DEFAULT_LONG_POLL_TIMEOUT_MS / 1000 + 10,
    )
    .await
    {
        Ok(resp) => Ok(resp),
        Err(e) => {
            warn!("[wechat] getUpdates 错误: {}", e);
            Err(e)
        }
    }
}

pub async fn send_text_message(
    base_url: &str,
    token: &str,
    to: &str,
    text: &str,
    context_token: Option<&str>,
) -> Result<(), String> {
    send_message_inner(base_url, token, to, text, None, context_token).await
}

pub async fn send_voice_message(
    base_url: &str,
    token: &str,
    to: &str,
    voice_base64: &str,
    duration_ms: i64,
    context_token: Option<&str>,
) -> Result<(), String> {
    send_message_inner(base_url, token, to, "", Some((voice_base64, duration_ms)), context_token)
        .await
}

async fn send_message_inner(
    base_url: &str,
    token: &str,
    to: &str,
    text: &str,
    voice: Option<(&str, i64)>,
    context_token: Option<&str>,
) -> Result<(), String> {
    let client_id = format!(
        "bot-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        rand::thread_rng().gen_range(100000_u32..999999)
    );

    let mut items: Vec<serde_json::Value> = Vec::new();
    if !text.is_empty() {
        items.push(serde_json::json!({
            "type": ITEM_TYPE_TEXT,
            "text_item": { "text": text }
        }));
    }
    if let Some((voice_b64, duration)) = voice {
        items.push(serde_json::json!({
            "type": ITEM_TYPE_VOICE,
            "voice": {
                "voice_data": voice_b64,
                "duration": duration
            }
        }));
    }

    let mut msg = serde_json::json!({
        "msg": {
            "from_user_id": "",
            "to_user_id": to,
            "client_id": client_id,
            "message_type": MESSAGE_TYPE_BOT,
            "message_state": MESSAGE_STATE_FINISH,
        }
    });

    if !items.is_empty() {
        msg["msg"]["item_list"] = serde_json::json!(items);
    }
    if let Some(ct) = context_token {
        msg["msg"]["context_token"] = serde_json::json!(ct);
    }

    let _: serde_json::Value = api_post(base_url, "ilink/bot/sendmessage", &msg, token, 15).await?;
    Ok(())
}
