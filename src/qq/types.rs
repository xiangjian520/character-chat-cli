use serde::{Deserialize, Serialize};

pub const OP_DISPATCH: u32 = 0;
pub const OP_HEARTBEAT: u32 = 1;
pub const OP_IDENTIFY: u32 = 2;
pub const OP_RECONNECT: u32 = 7;
pub const OP_INVALID_SESSION: u32 = 9;
pub const OP_HELLO: u32 = 10;
pub const OP_HEARTBEAT_ACK: u32 = 11;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPayload {
    #[serde(default)]
    pub op: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentifyData {
    pub token: String,
    pub intents: u32,
    pub shard: [u32; 2],
}

#[derive(Debug, Clone, Deserialize)]
pub struct HelloData {
    pub heartbeat_interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadyData {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub user: Option<ReadyUser>,
    #[serde(default)]
    pub shard: Option<[u32; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadyUser {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub bot: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct C2cMessageEvent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub author: Option<MessageAuthor>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageAuthor {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub user_openid: String,
}

pub const INTENT_GROUP_AND_C2C: u32 = 1 << 25;
pub const INTENT_AUDIO_ACTION: u32 = 1 << 29;

#[derive(Debug, Deserialize)]
pub struct AccessTokenResp {
    pub access_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize)]
pub struct GatewayUrlResp {
    pub url: String,
    #[serde(default)]
    pub shards: u32,
}

#[derive(Debug, Serialize)]
pub struct SendMessageReq {
    pub content: String,
    pub msg_type: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_seq: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct FileUploadReq {
    pub file_type: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileUploadResp {
    pub file_uuid: String,
    pub file_info: String,
    pub ttl: u32,
}

#[derive(Debug, Serialize)]
pub struct MediaSendReq {
    pub msg_type: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_seq: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct MediaInfo {
    pub file_info: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqCredentials {
    pub app_id: String,
    pub app_secret: String,
}

impl QqCredentials {
    const PATH: &str = "data/qq_credentials.json";

    pub fn save(&self) {
        if let Some(parent) = std::path::Path::new(Self::PATH).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::PATH, json);
        }
    }

    pub fn load() -> Option<Self> {
        let data = std::fs::read_to_string(Self::PATH).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn clear() {
        let _ = std::fs::remove_file(Self::PATH);
    }
}
