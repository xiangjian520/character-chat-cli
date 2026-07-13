use rusqlite::{params, Connection};
use std::sync::Mutex;
use crate::api::ChatMessage;

pub struct MemoryStore {
    db: Mutex<Connection>,
}

impl MemoryStore {
    pub fn open(path: &str) -> Result<Self, String> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建 data 目录失败: {}", e))?;
        }
        let db = Connection::open(path).map_err(|e| format!("打开数据库失败: {}", e))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| format!("设置 WAL 模式失败: {}", e))?;
        let store = Self { db: Mutex::new(db) };
        store.init_tables()?;
        Ok(store)
    }

    pub fn init_tables(&self) -> Result<(), String> {
        let db = self.db.lock().unwrap();
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE TABLE IF NOT EXISTS bot_sessions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                platform    TEXT NOT NULL,
                user_id     TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE INDEX IF NOT EXISTS idx_bot_user
                ON bot_sessions(platform, user_id, created_at);",
        )
        .map_err(|e| format!("初始化数据表失败: {}", e))?;
        Ok(())
    }

    pub fn chat_messages(&self) -> Vec<ChatMessage> {
        let db = self.db.lock().unwrap();
        let mut stmt = match db.prepare("SELECT role, content FROM chat_messages ORDER BY id ASC") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |row| {
            Ok(ChatMessage {
                role: row.get(0)?,
                content: row.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn chat_add(&self, role: &str, content: &str) {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO chat_messages (role, content) VALUES (?1, ?2)",
            params![role, content],
        )
        .ok();
    }

    pub fn chat_clear(&self) {
        let db = self.db.lock().unwrap();
        db.execute("DELETE FROM chat_messages", []).ok();
    }

    pub fn chat_count(&self) -> usize {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT COUNT(*) FROM chat_messages", [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn bot_context(&self, platform: &str, user_id: &str, limit: usize) -> Vec<ChatMessage> {
        let db = self.db.lock().unwrap();
        let mut stmt = match db.prepare(
            "SELECT role, content FROM (
                SELECT id, role, content FROM bot_sessions
                WHERE platform = ?1 AND user_id = ?2
                ORDER BY id DESC LIMIT ?3
            ) ORDER BY id ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![platform, user_id, limit as i64], |row| {
            Ok(ChatMessage {
                role: row.get(0)?,
                content: row.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn bot_add(&self, platform: &str, user_id: &str, role: &str, content: &str) {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO bot_sessions (platform, user_id, role, content) VALUES (?1, ?2, ?3, ?4)",
            params![platform, user_id, role, content],
        )
        .ok();
    }

    pub fn bot_clear(&self, platform: &str, user_id: &str) {
        let db = self.db.lock().unwrap();
        db.execute(
            "DELETE FROM bot_sessions WHERE platform = ?1 AND user_id = ?2",
            params![platform, user_id],
        )
        .ok();
    }

    pub fn bot_clear_platform(&self, platform: &str) {
        let db = self.db.lock().unwrap();
        db.execute(
            "DELETE FROM bot_sessions WHERE platform = ?1",
            params![platform],
        )
        .ok();
    }

    pub fn bot_prune(&self, max_per_user: usize) {
        let db = self.db.lock().unwrap();
        db.execute(
            "DELETE FROM bot_sessions WHERE id NOT IN (
                SELECT id FROM bot_sessions AS b1
                WHERE b1.id IN (
                    SELECT b2.id FROM bot_sessions AS b2
                    WHERE b2.platform = b1.platform AND b2.user_id = b1.user_id
                    ORDER BY b2.created_at DESC
                    LIMIT ?1
                )
            )",
            params![max_per_user as i64],
        )
        .ok();
    }
}
