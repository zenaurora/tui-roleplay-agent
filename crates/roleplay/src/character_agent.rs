//! CharacterAgent - wraps an LLM call with character-specific prompting.

use async_trait::async_trait;
use rust_agent_core::{Agent, Character, Message, Result};
use rust_agent_llm::OpenAiClient;
use rust_agent_memory::SlidingWindowContext;

/// An agent that plays a specific character in the roleplay.
pub struct CharacterAgent {
    pub character: Character,
    client: OpenAiClient,
    context_window: SlidingWindowContext,
}

impl CharacterAgent {
    pub fn new(character: Character, client: OpenAiClient) -> Self {
        Self {
            character,
            client,
            context_window: SlidingWindowContext::default(),
        }
    }

    pub fn with_context_window(mut self, window: SlidingWindowContext) -> Self {
        self.context_window = window;
        self
    }

    /// Build the messages to send to the LLM, including the character's system prompt.
    fn build_messages(&self, conversation: &[Message]) -> Vec<Message> {
        let system = Message::system(&self.character.system_prompt);
        let mut messages = vec![system];
        let windowed = self.context_window.apply(conversation);
        messages.extend(windowed);
        messages
    }
}

#[async_trait]
impl Agent for CharacterAgent {
    fn name(&self) -> &str {
        &self.character.name
    }

    async fn run(&self, messages: &[Message]) -> Result<Message> {
        let prepared = self.build_messages(messages);

        let model = self.character.model.as_deref();
        let response = if let Some(model) = model {
            self.client
                .chat_completion_with_model(&prepared, model)
                .await?
        } else {
            self.client.chat_completion(&prepared).await?
        };

        Ok(response.with_character(&self.character.name))
    }
}
