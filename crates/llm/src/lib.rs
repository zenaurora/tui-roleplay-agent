//! OpenAI-compatible LLM client with streaming support.

pub mod client;
pub mod types;

pub use client::OpenAiClient;
pub use types::*;
