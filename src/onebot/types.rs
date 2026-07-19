use serde::{Deserialize, Serialize};

// ─── API request (from us to OneBot client) ───

#[derive(Serialize, Debug)]
pub struct ApiRequest {
    pub action: String,
    pub params: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<serde_json::Value>,
}

// ─── API response (from OneBot client to us) ───

#[derive(Deserialize, Debug)]
pub struct ApiResponse {
    pub status: String,
    pub retcode: i64,
    pub data: serde_json::Value,
    #[serde(default)]
    pub echo: Option<serde_json::Value>,
}

// ─── Event (from OneBot client to us) ───

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OneBotEvent {
    pub time: Option<i64>,
    pub self_id: Option<i64>,
    pub post_type: Option<String>,
    #[serde(rename = "message_type")]
    pub message_type: Option<String>,
    pub sub_type: Option<String>,
    pub message_id: Option<i64>,
    pub group_id: Option<i64>,
    pub user_id: Option<i64>,
    pub message: Option<serde_json::Value>,
    pub raw_message: Option<String>,
    pub font: Option<i64>,
    pub sender: Option<Sender>,
    pub notice_type: Option<String>,
    pub request_type: Option<String>,
    pub comment: Option<String>,
    pub flag: Option<String>,
    pub meta_event_type: Option<String>,
    pub status: Option<serde_json::Value>,
    #[serde(default)]
    pub echo: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sender {
    pub user_id: Option<i64>,
    pub nickname: Option<String>,
    pub sex: Option<String>,
    pub age: Option<i32>,
    #[serde(default)]
    pub card: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub area: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
}

// ─── Incoming message (can be event OR api response) ───

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum IncomingMessage {
    Response(ApiResponse),
    Event(OneBotEvent),
}

// ─── Helpers to extract text from message field ───

pub fn extract_text(message: &serde_json::Value) -> String {
    match message {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(segs) => {
            segs.iter()
                .filter_map(|seg| {
                    seg.get("type")
                        .and_then(|t| t.as_str())
                        .filter(|t| *t == "text")
                        .and_then(|_| {
                            seg.get("data")
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                        })
                })
                .collect::<Vec<_>>()
                .join("")
        }
        _ => String::new(),
    }
}

/// 检查群消息是否 @了机器人 (self_id)
pub fn is_at_bot(message: &serde_json::Value, self_id: i64) -> bool {
    match message {
        serde_json::Value::Array(segs) => {
            segs.iter().any(|seg| {
                let is_at = seg.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "at")
                    .unwrap_or(false);
                if !is_at {
                    return false;
                }
                let target = seg.get("data").and_then(|d| d.get("qq"));
                match target {
                    Some(serde_json::Value::String(s)) => {
                        *s == self_id.to_string() || *s == "all"
                    }
                    Some(serde_json::Value::Number(n)) => {
                        n.as_i64().map(|i| i.to_string() == self_id.to_string()).unwrap_or(false)
                    }
                    _ => false,
                }
            })
        }
        _ => false,
    }
}

// ─── Helpers to build message segments ───

pub fn text_segment(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "text",
        "data": { "text": text }
    })
}

pub fn text_message(text: &str) -> serde_json::Value {
    serde_json::json!([text_segment(text)])
}

pub fn voice_segment(file: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "record",
        "data": { "file": file }
    })
}

pub fn reply_segment(message_id: i64) -> serde_json::Value {
    serde_json::json!({
        "type": "reply",
        "data": { "id": message_id.to_string() }
    })
}
