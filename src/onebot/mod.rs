pub mod types;
pub mod server;
pub mod bot;

#[derive(Clone, Debug)]
pub enum ObEvent {
    MessageReceived {
        self_id: i64,
        user_id: i64,
        group_id: Option<i64>,
        message_id: Option<i64>,
        text: String,
        raw: serde_json::Value,
    },
    BotReply {
        user_id: i64,
        group_id: Option<i64>,
        text: String,
    },
    Error(String),
    StatusChanged {
        running: bool,
    },
}
