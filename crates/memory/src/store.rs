//! SQLite-backed persistent memory store.

use chrono::Utc;
use rusqlite::{params, Connection};
use rust_agent_core::Message;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteMemoryStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct ArtifactRecord {
    pub id: Uuid,
    pub session_id: String,
    pub source_message_id: Option<Uuid>,
    pub kind: String,
    pub title: String,
    pub mime_type: String,
    pub content: String,
    pub summary: Option<String>,
    pub token_estimate: usize,
}

#[derive(Debug, Clone)]
pub struct SummaryRecord {
    pub id: Uuid,
    pub session_id: String,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub from_sequence: i64,
    pub to_sequence: i64,
    pub summary_json: serde_json::Value,
    pub summary_text: String,
    pub token_estimate: usize,
    pub model: String,
}

impl SqliteMemoryStore {
    pub fn open(path: impl AsRef<Path>) -> rust_agent_core::Result<Self> {
        let conn = Connection::open(path).map_err(store_err)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init()?;
        Ok(store)
    }

    pub fn in_memory() -> rust_agent_core::Result<Self> {
        let conn = Connection::open_in_memory().map_err(store_err)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> rust_agent_core::Result<()> {
        let conn = self.conn.lock().map_err(lock_err)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS messages (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                character_id TEXT,
                character_name TEXT,
                content TEXT NOT NULL,
                metadata_json TEXT,
                tool_calls_json TEXT NOT NULL DEFAULT '[]',
                tool_call_id TEXT,
                reasoning_content TEXT,
                token_estimate INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session_sequence
                ON messages(session_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_messages_session_character
                ON messages(session_id, character_id, sequence);

            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                source_message_id TEXT,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                content TEXT NOT NULL,
                summary TEXT,
                token_estimate INTEGER NOT NULL,
                metadata_json TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_artifacts_session
                ON artifacts(session_id, created_at);

            CREATE TABLE IF NOT EXISTS summaries (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                scope_type TEXT NOT NULL,
                scope_id TEXT,
                from_sequence INTEGER NOT NULL,
                to_sequence INTEGER NOT NULL,
                summary_json TEXT NOT NULL,
                summary_text TEXT NOT NULL,
                token_estimate INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_summaries_scope
                ON summaries(session_id, scope_type, scope_id, to_sequence);
            "#,
        )
        .map_err(store_err)?;
        Ok(())
    }

    pub fn append_message(
        &self,
        session_id: &str,
        message: &Message,
        character_id: Option<Uuid>,
    ) -> rust_agent_core::Result<i64> {
        let conn = self.conn.lock().map_err(lock_err)?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO messages (
                id, session_id, role, character_id, character_name, content,
                metadata_json, tool_calls_json, tool_call_id, reasoning_content,
                token_estimate, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                message.id.to_string(),
                session_id,
                format!("{:?}", message.role).to_lowercase(),
                character_id.map(|id| id.to_string()),
                message.character_name.as_deref(),
                message.content,
                optional_json(&message.metadata)?,
                serde_json::to_string(&message.tool_calls).map_err(store_err)?,
                message.tool_call_id.as_deref(),
                message.reasoning_content.as_deref(),
                estimate_tokens(&message.content) as i64,
                message.timestamp.to_rfc3339(),
            ],
        )
        .map_err(store_err)?;
        Ok(conn.last_insert_rowid())
    }

    pub fn save_artifact(&self, artifact: &ArtifactRecord) -> rust_agent_core::Result<()> {
        let conn = self.conn.lock().map_err(lock_err)?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO artifacts (
                id, session_id, source_message_id, kind, title, mime_type,
                content, summary, token_estimate, metadata_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10)
            "#,
            params![
                artifact.id.to_string(),
                artifact.session_id,
                artifact.source_message_id.map(|id| id.to_string()),
                artifact.kind,
                artifact.title,
                artifact.mime_type,
                artifact.content,
                artifact.summary,
                artifact.token_estimate as i64,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(store_err)?;
        Ok(())
    }

    pub fn save_summary(&self, summary: &SummaryRecord) -> rust_agent_core::Result<()> {
        let conn = self.conn.lock().map_err(lock_err)?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO summaries (
                id, session_id, scope_type, scope_id, from_sequence, to_sequence,
                summary_json, summary_text, token_estimate, model, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                summary.id.to_string(),
                summary.session_id,
                summary.scope_type,
                summary.scope_id,
                summary.from_sequence,
                summary.to_sequence,
                summary.summary_json.to_string(),
                summary.summary_text,
                summary.token_estimate as i64,
                summary.model,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(store_err)?;
        Ok(())
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    chars.saturating_div(3).max(1)
}

fn optional_json(value: &Option<serde_json::Value>) -> rust_agent_core::Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(store_err)
}

fn store_err(err: impl std::fmt::Display) -> rust_agent_core::AgentError {
    rust_agent_core::AgentError::Memory(rust_agent_core::MemoryError::Storage(err.to_string()))
}

fn lock_err<T>(err: std::sync::PoisonError<T>) -> rust_agent_core::AgentError {
    rust_agent_core::AgentError::Memory(rust_agent_core::MemoryError::Storage(format!(
        "sqlite memory store lock poisoned: {err}"
    )))
}
