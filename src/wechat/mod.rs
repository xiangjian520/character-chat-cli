pub mod types;
pub mod auth;
pub mod api;
pub mod bot;

#[derive(Clone, Debug)]
pub enum WeChatEvent {
    AuthStatus(String),
    QrCode(String),
    QrCodeImage(String),
    LoginSuccess {
        account_id: String,
        user_id: Option<String>,
    },
    LoginError(String),
    MessageReceived {
        from_user: String,
        text: String,
    },
    BotReply {
        to_user: String,
        text: String,
    },
    BotError(String),
    BotStatus {
        running: bool,
    },
}
