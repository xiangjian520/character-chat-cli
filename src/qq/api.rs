use log::info;
use crate::qq::types::*;

pub async fn get_access_token(app_id: &str, app_secret: &str) -> Result<AccessTokenResp, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "appId": app_id,
        "clientSecret": app_secret
    });
    info!("[qqbot] 请求AccessToken: appId={}", app_id);

    let resp = client
        .post("https://bots.qq.com/app/getAppAccessToken")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求AccessToken失败: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("AccessToken HTTP {}: {}", status, text));
    }

    if let Ok(r) = serde_json::from_str::<serde_json::Value>(&text) {
        let token = r.get("access_token").and_then(|v| v.as_str()).unwrap_or("");
        let expires = r
            .get("expires_in")
            .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0);
        if !token.is_empty() {
            return Ok(AccessTokenResp {
                access_token: token.to_string(),
                expires_in: expires,
            });
        }
    }

    if let Ok(w) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(data) = w.get("data") {
            if let (Some(tok), Some(exp)) = (
                data.get("access_token").and_then(|v| v.as_str()),
                data.get("expires_in").and_then(|v| v.as_i64()),
            ) {
                return Ok(AccessTokenResp {
                    access_token: tok.to_string(),
                    expires_in: exp,
                });
            }
        }
        if let Some(err) = w.get("message").or_else(|| w.get("error")).and_then(|v| v.as_str()) {
            return Err(format!("AccessToken API错误: {}", err));
        }
    }

    Err(format!("无法解析AccessToken响应: {}", text))
}

pub async fn get_gateway_url(access_token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let text = client
        .get("https://api.sgroup.qq.com/gateway/bot")
        .header("Authorization", format!("QQBot {}", access_token))
        .send()
        .await
        .map_err(|e| format!("获取网关地址失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取网关响应失败: {}", e))?;
    let resp: GatewayUrlResp = serde_json::from_str(&text).map_err(|e| {
        format!(
            "解析网关地址失败: {} - 原始响应: {}",
            e,
            &text[..text.len().min(200)]
        )
    })?;
    Ok(resp.url)
}

pub async fn send_c2c_message(
    access_token: &str,
    openid: &str,
    content: &str,
    msg_id: Option<&str>,
    msg_seq: Option<i32>,
) -> Result<(), String> {
    let url = format!("https://api.sgroup.qq.com/v2/users/{}/messages", openid);
    send_message(access_token, &url, content, msg_id, msg_seq, "C2C").await
}

async fn send_message(
    access_token: &str,
    url: &str,
    content: &str,
    msg_id: Option<&str>,
    msg_seq: Option<i32>,
    label: &str,
) -> Result<(), String> {
    let body = SendMessageReq {
        content: content.to_string(),
        msg_type: 0,
        msg_id: msg_id.map(|s| s.to_string()),
        msg_seq,
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Authorization", format!("QQBot {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("发送{}消息失败: {}", label, e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("发送{}消息 HTTP {}: {}", label, status, text));
    }
    Ok(())
}

pub async fn upload_file(
    access_token: &str,
    openid: &str,
    file_type: u32,
    file_data: &str,
) -> Result<FileUploadResp, String> {
    let url = format!("https://api.sgroup.qq.com/v2/users/{}/files", openid);
    let body = FileUploadReq {
        file_type,
        url: None,
        file_data: Some(file_data.to_string()),
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("QQBot {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("上传文件失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("上传文件 HTTP {}: {}", status, text));
    }
    let text = resp.text().await.unwrap_or_default();
    serde_json::from_str::<FileUploadResp>(&text).map_err(|e| {
        format!(
            "解析上传响应失败: {} - raw: {}",
            e,
            &text[..text.len().min(300)]
        )
    })
}

pub async fn send_media_message(
    access_token: &str,
    openid: &str,
    file_info: &str,
    msg_id: Option<&str>,
    msg_seq: Option<i32>,
) -> Result<(), String> {
    let url = format!("https://api.sgroup.qq.com/v2/users/{}/messages", openid);
    let body = MediaSendReq {
        msg_type: 7,
        content: None,
        media: Some(MediaInfo {
            file_info: file_info.to_string(),
        }),
        msg_id: msg_id.map(|s| s.to_string()),
        msg_seq,
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("QQBot {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("发送富媒体消息失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("发送富媒体 HTTP {}: {}", status, text));
    }
    Ok(())
}

pub async fn send_c2c_voice(
    access_token: &str,
    openid: &str,
    audio_base64: &str,
    msg_id: Option<&str>,
    msg_seq: Option<i32>,
) -> Result<(), String> {
    let upload = upload_file(access_token, openid, 3, audio_base64).await?;
    info!(
        "[qqbot] 语音上传成功, file_info={}",
        &upload.file_info[..upload.file_info.len().min(50)]
    );
    send_media_message(access_token, openid, &upload.file_info, msg_id, msg_seq).await
}
