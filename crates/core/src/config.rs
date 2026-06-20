//! Configuration types.

use serde::{Deserialize, Serialize};

use crate::{types::TurnStrategy};

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub story: StoryConfig,
    #[serde(default)]
    pub tui: TuiConfig,
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
}
/// TUI display configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    /// Whether to show the character sidebar.
    #[serde(default = "default_true")]
    pub show_sidebar: bool,
    /// Whether to show the story info bar.
    #[serde(default = "default_true")]
    pub show_story_bar: bool,
    /// Typing speed for streaming display (chars per second, 0 = instant).
    #[serde(default)]
    pub typing_speed: u32,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            show_sidebar: true,
            show_story_bar: true,
            typing_speed: 0,
        }
    }
}

fn default_max_tokens() -> usize {
    4096
}

fn default_temperature() -> f32 {
    0.8
}

fn default_true() -> bool {
    true
}

fn default_reasoning_effort() -> String {
    "high".to_string()
}
