//! Conversation memory - stores full message history per character/session.

use rust_agent_core::Message;
use std::collections::HashMap;
use uuid::Uuid;

/// Stores conversation history, organized by session or character.
#[derive(Debug, Clone, Default)]
pub struct ConversationMemory {
    /// Global conversation history (all messages in order).
    pub global_history: Vec<Message>,
    /// Per-character message history (only messages involving that character).
    pub character_history: HashMap<Uuid, Vec<Message>>,
}

impl ConversationMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a message to the global history and optionally to a character's history.
    pub fn add_message(&mut self, message: Message, character_id: Option<Uuid>) {
        if let Some(id) = character_id {
            self.character_history
                .entry(id)
                .or_default()
                .push(message.clone());
        }
        self.global_history.push(message);
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
}
