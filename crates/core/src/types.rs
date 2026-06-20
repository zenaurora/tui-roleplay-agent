//! Core domain types: Character, Scene, etc.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A character in the roleplay scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: Uuid,
    pub name: String,
    pub personality: String,
    pub backstory: String,
    pub system_prompt: String,
    /// Short description for display in the TUI sidebar.
    pub short_description: Option<String>,
    /// Model override for this specific character (if different from default).
    pub model: Option<String>,
    /// Local tools this character is allowed to use.
    pub allowed_tools: Vec<String>,
}

impl Character {
    pub fn new(
        name: impl Into<String>,
        personality: impl Into<String>,
        backstory: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let personality = personality.into();
        let backstory = backstory.into();
        let system_prompt = format!(
            "You are {name}. Your personality: {personality}\n\nBackstory: {backstory}\n\n\
             Stay in character at all times. Respond naturally as this character would."
        );
        Self {
            id: Uuid::new_v4(),
            name,
            personality,
            backstory,
            system_prompt,
            short_description: None,
            model: None,
            allowed_tools: Vec::new(),
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.short_description = Some(desc.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }
}

/// A scene in the roleplay story.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// Characters active in this scene (by character id).
    pub active_characters: Vec<Uuid>,
    /// Scene goals or objectives.
    pub goals: Vec<String>,
    /// Additional context injected into prompts during this scene.
    pub context: Option<String>,
}

impl Scene {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            active_characters: Vec::new(),
            goals: Vec::new(),
            context: None,
        }
    }

    pub fn with_characters(mut self, characters: Vec<Uuid>) -> Self {
        self.active_characters = characters;
        self
    }

    pub fn with_goals(mut self, goals: Vec<String>) -> Self {
        self.goals = goals;
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// Turn order strategy for the roleplay.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TurnStrategy {
    /// Characters speak in a fixed round-robin order.
    RoundRobin,
    /// A director agent decides who speaks next.
    #[default]
    DirectorControlled,
    /// Random selection among active characters.
    Random,
}

/// A single decision from the Director about what happens next.
#[derive(Debug, Clone)]
pub enum TurnDecision {
    /// Characters speak one after another; each sees the previous response.
    Sequential(Vec<String>),
    /// Characters speak concurrently; all see the same context snapshot.
    Parallel(Vec<String>),
    /// Wait for the player to provide input.
    Player,
    /// The scene should end.
    EndScene,
}
