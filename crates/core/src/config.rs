//! Configuration types.

use serde::{Deserialize, Serialize};

use crate::types::TurnStrategy;

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
    #[serde(default)]
    pub tools: ToolConfig,
    pub story: StoryConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Base URL for the API (e.g., "https://api.openai.com/v1").
    pub base_url: String,
    /// API key (can also be set via OPENAI_API_KEY env var).
    pub api_key: Option<String>,
    /// Default model to use.
    pub model: String,
    /// Maximum tokens for completion.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Temperature for generation.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Enable thinking/reasoning mode (DeepSeek R1, etc.)
    #[serde(default)]
    pub thinking_enabled: bool,
    /// Reasoning effort level: "high" or "max". Only used when thinking_enabled is true.
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
}

/// Local tool availability configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolConfig {
    /// Whether local tools can be exposed to character agents.
    #[serde(default)]
    pub enabled: bool,
    /// Global allowlist for tool names. Empty means no tools are globally allowed.
    #[serde(default)]
    pub allowed: Vec<String>,
}

/// Story/roleplay configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryConfig {
    pub title: String,
    pub description: String,
    /// The user's character name in the story.
    pub protagonist_name: String,
    /// Turn strategy for the roleplay.
    #[serde(default)]
    pub turn_strategy: TurnStrategy,
    /// Character definition files (paths relative to config).
    #[serde(default)]
    pub character_files: Vec<String>,
    /// Scene definition files.
    #[serde(default)]
    pub scene_files: Vec<String>,
    #[serde(default)]
    pub characters: Vec<CharacterConfig>,
    /// Scene goals/objectives.
    #[serde(default)]
    pub scene_goals: Vec<String>,
    /// Scene context description.
    pub scene_context: Option<String>,
    /// Director behavior configuration.
    pub director: DirectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorConfig {
    /// System prompt for the director.
    pub system_prompt: String,
    /// Number of recent conversation messages sent to the director.
    pub recent_message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterConfig {
    pub name: String,
    pub personality: String,
    pub backstory: String,
    pub system_prompt: String,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}
/// Runtime behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Maximum consecutive NPC turns before forcing a player turn.
    #[serde(default = "default_max_npc_turns")]
    pub max_npc_turns: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_npc_turns: default_max_npc_turns(),
        }
    }
}

fn default_max_tokens() -> usize {
    4096
}

fn default_temperature() -> f32 {
    0.8
}

fn default_max_npc_turns() -> usize {
    3
}

fn default_reasoning_effort() -> String {
    "high".to_string()
}
