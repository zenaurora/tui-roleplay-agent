//! Conversation memory - stores full message history per character/session.

use crate::store::{estimate_tokens, SqliteMemoryStore, SummaryRecord};
use rust_agent_core::{Message, Role};
use std::collections::HashMap;
use uuid::Uuid;

/// Stores conversation history, organized by session or character.
#[derive(Debug, Clone, Default)]
pub struct ConversationMemory {
    /// Global conversation history (current context view, not necessarily full persisted log).
    pub global_history: Vec<Message>,
    /// Per-character message history (current context view for messages involving that character).
    pub character_history: HashMap<Uuid, Vec<Message>>,
    /// Stable session id for persistent storage.
    pub session_id: String,
    /// Optional SQLite store that keeps the complete un-compacted history.
    pub store: Option<SqliteMemoryStore>,
}

impl ConversationMemory {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            ..Self::default()
        }
    }

    pub fn with_store(session_id: impl Into<String>, store: SqliteMemoryStore) -> Self {
        Self {
            session_id: session_id.into(),
            store: Some(store),
            ..Self::default()
        }
    }

    /// Add a message to the global history and optionally to a character's history.
    pub fn add_message(&mut self, message: Message, character_id: Option<Uuid>) {
        let _ = self.add_message_result(message, character_id);
    }

    /// Add a message and persist it if a SQLite store is configured.
    pub fn add_message_result(
        &mut self,
        message: Message,
        character_id: Option<Uuid>,
    ) -> rust_agent_core::Result<()> {
        if let Some(store) = &self.store {
            store.append_message(&self.session_id, &message, character_id)?;
        }
        if let Some(id) = character_id {
            self.character_history
                .entry(id)
                .or_default()
                .push(message.clone());
        }
        self.global_history.push(message);
        Ok(())
    }

    /// Get the full global history.
    pub fn history(&self) -> &[Message] {
        &self.global_history
    }

    /// Get history for a specific character.
    pub fn character_messages(&self, character_id: &Uuid) -> &[Message] {
        self.character_history
            .get(character_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the last N messages from global history.
    pub fn last_n(&self, n: usize) -> &[Message] {
        let len = self.global_history.len();
        if n >= len {
            &self.global_history
        } else {
            &self.global_history[len - n..]
        }
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.global_history.clear();
        self.character_history.clear();
    }

    /// Total message count.
    pub fn len(&self) -> usize {
        self.global_history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.global_history.is_empty()
    }

    pub fn compact_with_summary(
        &mut self,
        summary_text: String,
        summary_json: serde_json::Value,
        keep_recent_messages: usize,
        model: impl Into<String>,
    ) -> rust_agent_core::Result<CompactStats> {
        let before_messages = self.global_history.len();
        let before_tokens = self.estimated_tokens();
        if before_messages <= keep_recent_messages {
            return Ok(CompactStats {
                before_messages,
                after_messages: before_messages,
                before_tokens,
                after_tokens: before_tokens,
            });
        }

        let compacted_count = before_messages - keep_recent_messages;
        let recent = self.global_history.split_off(compacted_count);
        let summary_message = Message::system(format!(
            "Conversation summary for compacted older context:\n{}",
            summary_text
        ))
        .with_metadata(serde_json::json!({
            "kind": "structured_compact_summary",
            "summary": summary_json,
            "compacted_message_count": compacted_count,
        }));

        let mut compacted_history = Vec::with_capacity(recent.len() + 1);
        compacted_history.push(summary_message.clone());
        compacted_history.extend(recent);
        self.global_history = compacted_history;
        self.rebuild_character_history();

        if let Some(store) = &self.store {
            store.save_summary(&SummaryRecord {
                id: Uuid::new_v4(),
                session_id: self.session_id.clone(),
                scope_type: "global".to_string(),
                scope_id: None,
                from_sequence: 1,
                to_sequence: compacted_count as i64,
                summary_json,
                summary_text,
                token_estimate: estimate_tokens(&summary_message.content),
                model: model.into(),
            })?;
        }

        Ok(CompactStats {
            before_messages,
            after_messages: self.global_history.len(),
            before_tokens,
            after_tokens: self.estimated_tokens(),
        })
    }

    pub fn compact(&mut self) {
        if self.global_history.len() <= 8 {
            return;
        }
        let compacted_count = self.global_history.len() - 8;
        let summary_text = format!(
            "Earlier context was compacted without an LLM summary. Compacted messages: {}.",
            compacted_count
        );
        let _ = self.compact_with_summary(
            summary_text,
            serde_json::json!({"fallback": true}),
            8,
            "fallback",
        );
    }

    pub fn estimated_tokens(&self) -> usize {
        self.global_history
            .iter()
            .map(|msg| estimate_tokens(&msg.content))
            .sum()
    }

    fn rebuild_character_history(&mut self) {
        self.character_history.clear();
        for message in &self.global_history {
            if message.role == Role::System {
                continue;
            }
            if let Some(name) = &message.character_name {
                let key = stable_character_scope_id(name);
                self.character_history
                    .entry(key)
                    .or_default()
                    .push(message.clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompactStats {
    pub before_messages: usize,
    pub after_messages: usize,
    pub before_tokens: usize,
    pub after_tokens: usize,
}

fn stable_character_scope_id(name: &str) -> Uuid {
    let mut bytes = [0_u8; 16];
    for (idx, byte) in name.as_bytes().iter().enumerate() {
        bytes[idx % 16] = bytes[idx % 16].wrapping_add(*byte).rotate_left(1);
    }
    Uuid::from_bytes(bytes)
}
