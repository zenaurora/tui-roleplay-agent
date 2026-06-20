//! Types for the LLM API requests and responses.

use rust_agent_core::{ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};

/// A chat completion request.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ApiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ApiToolCall>>,
}

/// A tool exposed to an OpenAI-compatible chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ApiToolFunction,
}

/// Function metadata for a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl From<ToolSpec> for ApiTool {
    fn from(spec: ToolSpec) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: ApiToolFunction {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            },
        }
    }
}

/// A model-requested tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ApiToolCallFunction,
}

/// Function call payload inside a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCallFunction {
    pub name: String,
    pub arguments: String,
}

impl From<ToolCall> for ApiToolCall {
    fn from(call: ToolCall) -> Self {
        Self {
            id: call.id,
            tool_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: call.name,
                arguments: call.arguments,
            },
        }
    }
}

impl From<ApiToolCall> for ToolCall {
    fn from(call: ApiToolCall) -> Self {
        Self {
            id: call.id,
            name: call.function.name,
            arguments: call.function.arguments,
        }
    }
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
    pub tool_calls: Option<Vec<ApiToolCall>>,
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
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
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
