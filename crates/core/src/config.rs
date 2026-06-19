//! Configuration types.

use serde::{Deserialize, Serialize};

use crate::types::TurnStrategy;

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
