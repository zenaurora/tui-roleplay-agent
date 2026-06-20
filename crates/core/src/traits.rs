//! Core traits: Agent, Tool, and LLM abstractions.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::Result;
use crate::message::Message;

/// Provider-neutral tool metadata exposed to model clients.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// A tool that an agent can invoke.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The unique name of this tool.
    fn name(&self) -> &str;

    /// A description of what this tool does (used in prompts).
    fn description(&self) -> &str;

    /// JSON schema for the tool's parameters.
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with the given arguments.
    async fn execute(&self, args: Value) -> Result<String>;
}

/// An agent that can process messages and produce responses.
#[async_trait]
pub trait Agent: Send + Sync {
    /// The name of this agent.
    fn name(&self) -> &str;

    /// Process a list of messages and produce a response.
    async fn run(&self, messages: &[Message]) -> Result<Message>;
}

/// Streaming chunk from an LLM response.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A text delta.
    Delta(String),
    /// The response is complete.
    Done {
        /// Full accumulated content.
        content: String,
        /// Token usage info if available.
        usage: Option<TokenUsage>,
    },
    /// An error occurred during streaming.
    Error(String),
}

/// Token usage statistics.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}
