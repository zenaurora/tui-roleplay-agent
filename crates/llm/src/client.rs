//! OpenAI-compatible HTTP client with streaming support.

use futures::StreamExt;
use reqwest::Client;
use rust_agent_core::{Message, Role, StreamChunk, ToolSpec};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error};

use crate::types::*;

/// An OpenAI-compatible LLM client.
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    http: Client,
    config: LlmClientConfig,
    /// Label for logging (e.g. "director", "老板娘").
    label: String,
}

impl OpenAiClient {
    /// Create a new client with the given configuration.
    pub fn new(config: LlmClientConfig) -> Self {
        let http = Client::new();
        Self {
            http,
            config,
            label: "unknown".to_string(),
        }
    }

    /// Set the logging label for this client.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Create from core LlmConfig, resolving API key from env if needed.
    pub fn from_config(config: &rust_agent_core::LlmConfig) -> Self {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();

        Self::new(LlmClientConfig {
            base_url: config.base_url.clone(),
            api_key,
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            thinking_enabled: config.thinking_enabled,
            reasoning_effort: config.reasoning_effort.clone(),
        })
    }

    /// Convert internal Messages to API messages.
    fn to_api_messages(messages: &[Message]) -> Vec<ApiMessage> {
        messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "tool".to_string(),
                };
                let tool_call_id = m.tool_call_id.clone();
                let tool_calls = m
                    .tool_calls
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect::<Vec<ApiToolCall>>();
                let tool_calls = (!tool_calls.is_empty()).then_some(tool_calls);

                ApiMessage {
                    role,
                    content: Self::api_message_content(m, tool_calls.as_ref()),
                    tool_call_id,
                    tool_calls,
                }
            })
            .collect()
    }

    fn format_message_content(message: &Message) -> String {
        match &message.character_name {
            Some(name) if !name.trim().is_empty() => format!("{}: {}", name, message.content),
            _ => message.content.clone(),
        }
    }

    fn api_message_content(
        message: &Message,
        tool_calls: Option<&Vec<ApiToolCall>>,
    ) -> Option<String> {
        if message.role == Role::Assistant && tool_calls.is_some() && message.content.is_empty() {
            None
        } else {
            Some(Self::format_message_content(message))
        }
    }

    /// Build thinking config if enabled.
    fn thinking_config(&self) -> (Option<ThinkingConfig>, Option<String>) {
        if self.config.thinking_enabled {
            (
                Some(ThinkingConfig {
                    thinking_type: "enabled".to_string(),
                }),
                Some(self.config.reasoning_effort.clone()),
            )
        } else {
            (None, None)
        }
    }

    /// Send a non-streaming chat completion request.
    pub async fn chat_completion(&self, messages: &[Message]) -> rust_agent_core::Result<Message> {
        self.chat_completion_with_tools(messages, &[]).await
    }

    /// Send a non-streaming chat completion request with optional tools.
    pub async fn chat_completion_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> rust_agent_core::Result<Message> {
        self.chat_completion_inner(&self.config.model, messages, tools)
            .await
    }

    /// Send a streaming chat completion request, returning a stream of chunks.
    pub async fn stream_completion(
        &self,
        messages: &[Message],
    ) -> rust_agent_core::Result<ReceiverStream<StreamChunk>> {
        let (thinking, reasoning_effort) = self.thinking_config();
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: Self::to_api_messages(messages),
            tools: None,
            tool_choice: None,
            max_tokens: Some(self.config.max_tokens),
            temperature: if self.config.thinking_enabled {
                None
            } else {
                Some(self.config.temperature)
            },
            stream: Some(true),
            thinking,
            reasoning_effort,
        };

        let url = format!("{}/chat/completions", self.config.base_url);
        debug!("Sending streaming request to {}", url);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| rust_agent_core::LlmError::Request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(rust_agent_core::LlmError::Api {
                status: status.as_u16(),
                message: body,
            }
            .into());
        }

        let (tx, rx) = mpsc::channel(100);
        let mut byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut accumulated = String::new();
            let mut buffer = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete SSE lines
                        while let Some(line_end) = buffer.find('\n') {
                            let line = buffer[..line_end].trim().to_string();
                            buffer = buffer[line_end + 1..].to_string();

                            if line.is_empty() || line.starts_with(':') {
                                continue;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    let _ = tx
                                        .send(StreamChunk::Done {
                                            content: accumulated.clone(),
                                            usage: None,
                                        })
                                        .await;
                                    return;
                                }

                                match serde_json::from_str::<StreamChatCompletionChunk>(data) {
                                    Ok(chunk) => {
                                        if let Some(choice) = chunk.choices.first() {
                                            if let Some(content) = &choice.delta.content {
                                                accumulated.push_str(content);
                                                let _ = tx
                                                    .send(StreamChunk::Delta(content.clone()))
                                                    .await;
                                            }
                                            if choice.finish_reason.is_some() {
                                                let _ = tx
                                                    .send(StreamChunk::Done {
                                                        content: accumulated.clone(),
                                                        usage: None,
                                                    })
                                                    .await;
                                                return;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse SSE chunk: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                        return;
                    }
                }
            }

            // Stream ended without [DONE]
            if !accumulated.is_empty() {
                let _ = tx
                    .send(StreamChunk::Done {
                        content: accumulated,
                        usage: None,
                    })
                    .await;
            }
        });

        Ok(ReceiverStream::new(rx))
    }

    /// Get a completion with a specific model override.
    pub async fn chat_completion_with_model(
        &self,
        messages: &[Message],
        model: &str,
    ) -> rust_agent_core::Result<Message> {
        self.chat_completion_with_model_and_tools(messages, model, &[])
            .await
    }

    /// Get a completion with a specific model override and optional tools.
    pub async fn chat_completion_with_model_and_tools(
        &self,
        messages: &[Message],
        model: &str,
        tools: &[ToolSpec],
    ) -> rust_agent_core::Result<Message> {
        self.chat_completion_inner(model, messages, tools).await
    }

    async fn chat_completion_inner(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> rust_agent_core::Result<Message> {
        let (thinking, reasoning_effort) = self.thinking_config();
        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages: Self::to_api_messages(messages),
            tools: Self::api_tools(tools),
            tool_choice: Self::tool_choice(tools),
            max_tokens: Some(self.config.max_tokens),
            temperature: if self.config.thinking_enabled {
                None
            } else {
                Some(self.config.temperature)
            },
            stream: Some(false),
            thinking,
            reasoning_effort,
        };

        let url = format!("{}/chat/completions", self.config.base_url);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| rust_agent_core::LlmError::Request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(rust_agent_core::LlmError::Api {
                status: status.as_u16(),
                message: body,
            }
            .into());
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| rust_agent_core::LlmError::InvalidResponse(e.to_string()))?;

        let choice = completion.choices.first();
        let content = choice
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();
        let reasoning = choice.and_then(|c| c.message.reasoning_content.clone());
        let tool_calls = choice.and_then(|c| c.message.tool_calls.clone());

        // Log the LLM call
        let last_msg_preview = messages.last().map(|m| m.content.as_str()).unwrap_or("");
        crate::logging::log_llm_call(
            &self.label,
            model,
            messages.len(),
            last_msg_preview,
            &content,
            reasoning.as_deref(),
        )
        .await;

        let mut msg = Message::assistant(content);
        msg.reasoning_content = reasoning;
        msg.tool_calls = tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(msg)
    }

    fn api_tools(tools: &[ToolSpec]) -> Option<Vec<ApiTool>> {
        if tools.is_empty() {
            None
        } else {
            Some(tools.iter().cloned().map(Into::into).collect())
        }
    }

    fn tool_choice(tools: &[ToolSpec]) -> Option<String> {
        if tools.is_empty() {
            None
        } else {
            Some("auto".to_string())
        }
    }
}
