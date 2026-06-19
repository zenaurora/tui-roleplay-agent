//! Types for the LLM API requests and responses.

use serde::{Deserialize, Serialize};

/// A chat completion request.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// DeepSeek thinking mode configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Reasoning effort level: "high" or "max".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// Thinking mode configuration for DeepSeek models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    /// "enabled" or "disabled"
    #[serde(rename = "type")]
    pub thinking_type: String,
}

/// An API-level message (simplified for the OpenAI format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A chat completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// A choice in the completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: ResponseMessage,
    pub finish_reason: Option<String>,
}

/// The message in a response choice.
#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    /// Thinking/reasoning content (DeepSeek thinking mode).
    pub reasoning_content: Option<String>,
}

/// Token usage information.
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// A streaming chunk (SSE delta).
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChatCompletionChunk {
    pub id: String,
    pub choices: Vec<StreamChoice>,
}

/// A choice in a streaming chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    pub index: usize,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

/// Delta content in a streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct Delta {
    pub role: Option<String>,
    pub content: Option<String>,
    /// Thinking/reasoning content delta (DeepSeek thinking mode).
    pub reasoning_content: Option<String>,
}

/// Configuration for the LLM client.
#[derive(Debug, Clone)]
pub struct LlmClientConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
    /// Whether thinking/reasoning mode is enabled.
    pub thinking_enabled: bool,
    /// Reasoning effort level: "high" or "max".
    pub reasoning_effort: String,
}

impl LlmClientConfig {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            max_tokens: 4096,
            temperature: 0.8,
            thinking_enabled: false,
            reasoning_effort: "high".to_string(),
        }
    }
}
