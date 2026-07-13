use log::info;
use crate::wechat::types::{LoginCredentials, QrCodeResponse, QrStatusResponse};

const BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const BOT_TYPE: &str = "3";
pub const CREDENTIALS_FILE: &str = "data/wechat_credentials.json";

pub async fn fetch_qrcode() -> Result<QrCodeResponse, String> {
    let url = format!("{}/ilink/bot/get_bot_qrcode?bot_type={}", BASE_URL, BOT_TYPE);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("获取二维码失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("获取二维码失败: HTTP {}", resp.status()));
    }
    resp.json::<QrCodeResponse>()
        .await
        .map_err(|e| format!("解析二维码响应失败: {}", e))
}

pub async fn poll_qr_status(qrcode: &str) -> Result<QrStatusResponse, String> {
    let url = format!(
        "{}/ilink/bot/get_qrcode_status?qrcode={}",
        BASE_URL,
        urlencoding(qrcode)
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(35))
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;
    let resp = client
        .get(&url)
        .header("iLink-App-ClientVersion", "1")
        .send()
        .await
        .map_err(|e| format!("轮询状态失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("轮询状态失败: HTTP {}", resp.status()));
    }
    resp.json::<QrStatusResponse>()
        .await
        .map_err(|e| format!("解析状态失败: {}", e))
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthStatus {
    FetchingQr,
    WaitingScan,
    Scanned,
    Confirmed,
    Expired,
    RefreshingQr,
    Error(String),
    LoadedCredentials,
}

pub fn load_saved_credentials() -> Option<LoginCredentials> {
    LoginCredentials::load_from_file(CREDENTIALS_FILE)
}

pub fn clear_saved_credentials() {
    LoginCredentials::clear_file(CREDENTIALS_FILE);
}

pub struct AuthFlowResult {
    pub qrcode_content: String,
    pub status: AuthStatus,
    pub qrcode_img_base64: Option<String>,
}

/// Run the auth flow with callbacks for each status update.
/// Returns credentials on success.
pub async fn run_auth_flow(
    max_qr_refresh: u32,
    mut on_status: impl FnMut(&AuthStatus, Option<&str>, Option<&LoginCredentials>),
) -> Result<LoginCredentials, String> {
    if let Some(creds) = load_saved_credentials() {
        info!("[wechat] 使用已保存的凭证 (account_id={})", creds.account_id);
        on_status(&AuthStatus::LoadedCredentials, None, Some(&creds));
        return Ok(creds);
    }

    on_status(&AuthStatus::FetchingQr, None, None);
    let mut qr = fetch_qrcode().await?;
    let qr_display = qr
        .qrcode_img_content
        .clone()
        .unwrap_or_else(|| qr.qrcode.clone());
    on_status(&AuthStatus::WaitingScan, Some(&qr_display), None);

    let mut refresh_count = 0u32;

    loop {
        match poll_qr_status(&qr.qrcode).await {
            Ok(status) => match status {
                QrStatusResponse::Wait => {
                    let d = qr
                        .qrcode_img_content
                        .clone()
                        .unwrap_or_else(|| qr.qrcode.clone());
                    on_status(&AuthStatus::WaitingScan, Some(&d), None);
                }
                QrStatusResponse::Scanned => {
                    let d = qr
                        .qrcode_img_content
                        .clone()
                        .unwrap_or_else(|| qr.qrcode.clone());
                    on_status(&AuthStatus::Scanned, Some(&d), None);
                    info!("[wechat] 已扫码，等待确认...");
                }
                QrStatusResponse::Expired => {
                    refresh_count += 1;
                    if refresh_count >= max_qr_refresh {
                        on_status(&AuthStatus::Error("二维码多次过期".into()), None, None);
                        return Err("二维码多次过期".to_string());
                    }
                    on_status(&AuthStatus::RefreshingQr, None, None);
                    info!(
                        "[wechat] 二维码过期，刷新中... ({}/{})",
                        refresh_count, max_qr_refresh
                    );
                    qr = fetch_qrcode().await?;
                    let d = qr
                        .qrcode_img_content
                        .clone()
                        .unwrap_or_else(|| qr.qrcode.clone());
                    on_status(&AuthStatus::WaitingScan, Some(&d), None);
                }
                QrStatusResponse::Confirmed {
                    bot_token,
                    ilink_bot_id,
                    baseurl,
                    ilink_user_id,
                } => {
                    let creds = LoginCredentials {
                        token: bot_token,
                        base_url: baseurl.unwrap_or_else(|| BASE_URL.to_string()),
                        account_id: ilink_bot_id,
                        user_id: ilink_user_id,
                    };
                    creds.save_to_file(CREDENTIALS_FILE);
                    info!("[wechat] 登录成功! account_id={}", creds.account_id);
                    on_status(&AuthStatus::Confirmed, None, Some(&creds));
                    return Ok(creds);
                }
            },
            Err(e) => {
                on_status(&AuthStatus::Error(e.clone()), None, None);
                return Err(e);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
