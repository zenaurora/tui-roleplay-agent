//! OpenAI-compatible HTTP client with streaming support.

use futures::StreamExt;
use reqwest::Client;
use rust_agent_core::{Message, Role, StreamChunk};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error};

use crate::types::*;

/// An OpenAI-compatible LLM client.
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    http: Client,
    config: LlmClientConfig,
}

impl OpenAiClient {
    /// Create a new client with the given configuration.
    pub fn new(config: LlmClientConfig) -> Self {
        let http = Client::new();
        Self { http, config }
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
        })
    }

    /// Convert internal Messages to API messages.
    fn to_api_messages(messages: &[Message]) -> Vec<ApiMessage> {
        messages
            .iter()
            .map(|m| ApiMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
                name: m.character_name.clone(),
            })
            .collect()
    }

    /// Send a non-streaming chat completion request.
    pub async fn chat_completion(
        &self,
        messages: &[Message],
    ) -> rust_agent_core::Result<Message> {
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: Self::to_api_messages(messages),
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
            stream: Some(false),
        };

        let url = format!("{}/chat/completions", self.config.base_url);
        debug!("Sending chat completion request to {}", url);

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

        let content = completion
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(Message::assistant(content))
    }

    /// Send a streaming chat completion request, returning a stream of chunks.
    pub async fn stream_completion(
        &self,
        messages: &[Message],
    ) -> rust_agent_core::Result<ReceiverStream<StreamChunk>> {
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: Self::to_api_messages(messages),
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
            stream: Some(true),
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
        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages: Self::to_api_messages(messages),
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
            stream: Some(false),
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

        let content = completion
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(Message::assistant(content))
    }
}
