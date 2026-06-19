//! Sliding window context - trims conversation to fit within token limits.

use rust_agent_core::Message;

/// Manages a sliding window over conversation history to keep within token limits.
#[derive(Debug, Clone)]
pub struct SlidingWindowContext {
    /// Maximum number of messages to include.
    pub max_messages: usize,
    /// Maximum estimated characters (as a rough proxy for tokens).
    pub max_chars: usize,
    /// Always include system messages regardless of window.
    pub preserve_system: bool,
}

impl Default for SlidingWindowContext {
    fn default() -> Self {
        Self {
            max_messages: 50,
            max_chars: 32_000, // ~8k tokens rough estimate
            preserve_system: true,
        }
    }
}

impl SlidingWindowContext {
    pub fn new(max_messages: usize, max_chars: usize) -> Self {
        Self {
            max_messages,
            max_chars,
            preserve_system: true,
        }
    }

    /// Apply the sliding window to a message list, returning the trimmed messages.
    pub fn apply(&self, messages: &[Message]) -> Vec<Message> {
        let mut system_messages: Vec<Message> = Vec::new();
        let mut other_messages: Vec<Message> = Vec::new();

        for msg in messages {
            if self.preserve_system && msg.role == rust_agent_core::Role::System {
                system_messages.push(msg.clone());
            } else {
                other_messages.push(msg.clone());
            }
        }

        // Take last N non-system messages
        let windowed: Vec<Message> = if other_messages.len() > self.max_messages {
            other_messages[other_messages.len() - self.max_messages..].to_vec()
        } else {
            other_messages
        };

        // Combine system + windowed messages
        let mut result = system_messages;
        result.extend(windowed);

        // Trim by character count from the back (keeping system messages)
        let mut total_chars: usize = result.iter().map(|m| m.content.len()).sum();
        while total_chars > self.max_chars && result.len() > 1 {
            // Find the first non-system message to remove
            if let Some(pos) = result
                .iter()
                .position(|m| m.role != rust_agent_core::Role::System)
            {
                total_chars -= result[pos].content.len();
                result.remove(pos);
            } else {
                break;
            }
        }

        result
    }
}
