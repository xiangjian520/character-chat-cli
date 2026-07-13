pub mod types;
pub mod api;
pub mod ws;
pub mod bot;
pub mod config_tui;

#[derive(Clone, Debug)]
pub enum QqEvent {
    Connected { username: String },
    Disconnected,
    Raw { event_type: String, data: serde_json::Value },
    MessageReceived { from_user: String, text: String },
    BotReply { to_user: String, text: String },
    Error(String),
    Token { access_token: String },
    StatusChanged { running: bool },
}
