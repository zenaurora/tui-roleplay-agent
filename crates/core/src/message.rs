//! Message types for conversation history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of a message sender.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: String,
    /// The character name who sent this message (None for system/tool).
    pub character_name: Option<String>,
    pub timestamp: DateTime<Utc>,
    /// Optional metadata (tool call id, function name, etc.)
    pub metadata: Option<serde_json::Value>,
    /// Reasoning/thinking content (from models with thinking mode, e.g. DeepSeek R1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: content.into(),
            character_name: None,
            timestamp: Utc::now(),
            metadata: None,
            reasoning_content: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    pub fn with_character(mut self, name: impl Into<String>) -> Self {
        self.character_name = Some(name.into());
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}
