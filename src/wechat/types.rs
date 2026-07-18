use serde::{Deserialize, Serialize};

pub const MESSAGE_TYPE_USER: i32 = 1;
pub const MESSAGE_TYPE_BOT: i32 = 2;
pub const ITEM_TYPE_TEXT: i32 = 1;
pub const ITEM_TYPE_IMAGE: i32 = 2;
pub const ITEM_TYPE_FILE: i32 = 4;
pub const ITEM_TYPE_VIDEO: i32 = 5;
pub const MESSAGE_STATE_FINISH: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefMsg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_item: Option<Box<MessageItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<Box<RefMsg>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    pub from_user_id: Option<String>,
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetUpdatesResp {
    #[serde(default)]
    pub ret: Option<i32>,
    #[serde(default)]
    pub errcode: Option<i32>,
    #[serde(default)]
    pub errmsg: Option<String>,
    #[serde(default)]
    pub msgs: Option<Vec<WeixinMessage>>,
    #[serde(default)]
    pub get_updates_buf: Option<String>,
    #[serde(default)]
    pub longpolling_timeout_ms: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QrCodeResponse {
    pub qrcode: String,
    pub qrcode_img_content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status")]
pub enum QrStatusResponse {
    #[serde(rename = "wait")]
    Wait,
    #[serde(rename = "scaned")]
    Scanned,
    #[serde(rename = "confirmed")]
    Confirmed {
        bot_token: String,
        ilink_bot_id: String,
        #[serde(default)]
        baseurl: Option<String>,
        #[serde(default)]
        ilink_user_id: Option<String>,
    },
    #[serde(rename = "expired")]
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginCredentials {
    pub token: String,
    pub base_url: String,
    pub account_id: String,
    pub user_id: Option<String>,
}

impl LoginCredentials {
    pub fn save_to_file(&self, path: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load_from_file(path: &str) -> Option<Self> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn clear_file(path: &str) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn extract_text(msg: &WeixinMessage) -> String {
    let items = match &msg.item_list {
        Some(v) => v,
        None => return String::new(),
    };
    for item in items {
        if item.item_type == Some(ITEM_TYPE_TEXT) {
            if let Some(ref ti) = item.text_item {
                if let Some(ref text) = ti.text {
                    let mut out = text.clone();
                    if let Some(ref ref_msg) = item.ref_msg {
                        let mut parts: Vec<String> = Vec::new();
                        if let Some(ref title) = ref_msg.title {
                            parts.push(title.clone());
                        }
                        if !parts.is_empty() {
                            out = format!("[引用: {}]\n{}", parts.join(" | "), out);
                        }
                    }
                    return out;
                }
            }
        }
    }
    String::new()
}
