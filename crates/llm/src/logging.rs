//! LLM call logging — records every request/response to disk.

use chrono::Utc;
use serde::Serialize;
use std::path::Path;
use tokio::fs;

/// A single logged LLM interaction.
#[derive(Serialize)]
struct LlmLogEntry {
    timestamp: String,
    agent: String,
    model: String,
    messages_sent: usize,
    request_preview: String,
    response_content: String,
    reasoning_content: Option<String>,
}

const LOG_DIR: &str = "logs/llm";

/// Log an LLM call to disk.
/// `agent_name`: who made the call (e.g. "director", "老板娘")
/// `model`: the model used
/// `messages_sent`: number of messages in the request
/// `last_user_msg`: the last user message content (for preview)
/// `response`: the response content
/// `reasoning`: optional reasoning content (thinking mode)
pub async fn log_llm_call(
    agent_name: &str,
    model: &str,
    messages_sent: usize,
    last_user_msg: &str,
    response: &str,
    reasoning: Option<&str>,
) {
    // Best-effort logging: don't crash if it fails
    if let Err(_) = do_log(
        agent_name,
        model,
        messages_sent,
        last_user_msg,
        response,
        reasoning,
    )
    .await
    {
        // Silently ignore logging errors
    }
}

async fn do_log(
    agent_name: &str,
    model: &str,
    messages_sent: usize,
    last_user_msg: &str,
    response: &str,
    reasoning: Option<&str>,
) -> std::io::Result<()> {
    let dir = Path::new(LOG_DIR);
    fs::create_dir_all(dir).await?;

    let now = Utc::now();
    let filename = format!(
        "{}_{}_{}.json",
        now.format("%Y%m%d_%H%M%S"),
        agent_name.replace(' ', "_"),
        now.timestamp_millis() % 10000,
    );

    let entry = LlmLogEntry {
        timestamp: now.to_rfc3339(),
        agent: agent_name.to_string(),
        model: model.to_string(),
        messages_sent,
        request_preview: last_user_msg.chars().take(200).collect(),
        response_content: response.to_string(),
        reasoning_content: reasoning.map(|s| s.to_string()),
    };

    let json = serde_json::to_string_pretty(&entry).unwrap_or_default();
    let path = dir.join(filename);
    fs::write(path, json).await?;

    Ok(())
}
