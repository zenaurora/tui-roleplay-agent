//! Error types for the agent framework.

use thiserror::Error;

/// Top-level error type for the rust-agent framework.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Memory error: {0}")]
    Memory(#[from] MemoryError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Roleplay error: {0}")]
    Roleplay(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

/// Errors specific to LLM operations.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Request(String),

    #[error("API error (status {status}): {message}")]
    Api { status: u16, message: String },

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Token limit exceeded: used {used}, limit {limit}")]
    TokenLimit { used: usize, limit: usize },

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Errors specific to memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// A Result type alias using AgentError.
pub type Result<T> = std::result::Result<T, AgentError>;
