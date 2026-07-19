use redis::{Commands, Connection};
use std::sync::Mutex;
use crate::api::ChatMessage;

pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl MemoryStore {
    pub fn open(redis_url: &str) -> Result<Self, String> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| format!("Redis URL 解析失败: {}", e))?;
        let mut conn = client.get_connection()
            .map_err(|e| format!("连接 Redis 失败: {}", e))?;
        redis::cmd("PING").query::<String>(&mut conn)
            .map_err(|e| format!("Redis 存活检测失败 (PING): {}", e))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn key_chat() -> &'static str { "chat:messages" }
    fn key_bot(platform: &str, user_id: &str) -> String {
        format!("bot:{}:{}", platform, user_id)
    }
    fn key_bot_pattern(platform: &str) -> String {
        format!("bot:{}:*", platform)
    }
    fn key_bot_all() -> &'static str { "bot:*:*" }

    pub fn chat_messages(&self) -> Vec<ChatMessage> {
        let mut conn = self.conn.lock().unwrap();
        let values: Vec<String> = conn.lrange(Self::key_chat(), 0, -1).unwrap_or_default();
        values.iter().filter_map(|v| serde_json::from_str(v).ok()).collect()
    }

    pub fn chat_add(&self, role: &str, content: &str) {
        let mut conn = self.conn.lock().unwrap();
        let msg = ChatMessage { role: role.to_string(), content: content.to_string() };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _: Result<(), _> = conn.rpush(Self::key_chat(), json);
        }
    }

    pub fn chat_clear(&self) {
        let mut conn = self.conn.lock().unwrap();
        let _: Result<(), _> = redis::cmd("DEL").arg(Self::key_chat()).query(&mut *conn);
    }

    pub fn chat_count(&self) -> usize {
        let mut conn = self.conn.lock().unwrap();
        conn.llen(Self::key_chat()).unwrap_or(0)
    }

    pub fn bot_context(&self, platform: &str, user_id: &str, limit: usize) -> Vec<ChatMessage> {
        let mut conn = self.conn.lock().unwrap();
        let key = Self::key_bot(platform, user_id);
        let start = -(limit as isize);
        let values: Vec<String> = conn.lrange(&key, start, -1).unwrap_or_default();
        values.iter().filter_map(|v| serde_json::from_str(v).ok()).collect()
    }

    pub fn bot_add(&self, platform: &str, user_id: &str, role: &str, content: &str) {
        let mut conn = self.conn.lock().unwrap();
        let key = Self::key_bot(platform, user_id);
        let msg = ChatMessage { role: role.to_string(), content: content.to_string() };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _: Result<(), _> = conn.rpush(&key, json);
        }
    }

    pub fn bot_clear(&self, platform: &str, user_id: &str) {
        let mut conn = self.conn.lock().unwrap();
        let key = Self::key_bot(platform, user_id);
        let _: Result<(), _> = redis::cmd("DEL").arg(&key).query(&mut *conn);
    }

    pub fn bot_clear_platform(&self, platform: &str) {
        let mut conn = self.conn.lock().unwrap();
        let pattern = Self::key_bot_pattern(platform);
        let keys: Vec<String> = redis::cmd("KEYS").arg(&pattern).query(&mut *conn).unwrap_or_default();
        if !keys.is_empty() {
            let _: Result<(), _> = redis::cmd("DEL").arg(keys).query(&mut *conn);
        }
    }

    pub fn bot_prune(&self, max_per_user: usize) {
        let mut conn = self.conn.lock().unwrap();
        let keys: Vec<String> = redis::cmd("KEYS").arg(Self::key_bot_all()).query(&mut *conn).unwrap_or_default();
        let start = -(max_per_user as isize);
        for key in keys {
            let _: Result<(), _> = redis::cmd("LTRIM").arg(&key).arg(start).arg(-1).query(&mut *conn);
        }
    }
}
