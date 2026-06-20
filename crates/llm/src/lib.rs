//! OpenAI-compatible LLM client with streaming support.

pub mod client;
pub mod logging;
pub mod types;

pub use client::OpenAiClient;
pub use logging::log_llm_call;
pub use types::*;
