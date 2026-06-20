//! Turn manager - decides turn order for character responses.

use rust_agent_core::{Character, Message, Result, StoryConfig, TurnDecision, TurnStrategy};
use rust_agent_llm::OpenAiClient;

use crate::director::Director;

/// Manages turn order for characters in the roleplay.
pub struct TurnManager {
    strategy: TurnStrategy,
    director: Option<Director>,
    /// Index for round-robin tracking.
    round_robin_index: usize,
}

impl TurnManager {
    pub fn new(strategy: TurnStrategy) -> Self {
        Self {
            strategy,
            director: None,
            round_robin_index: 0,
        }
    }

    /// Create a TurnManager with a director configured from story settings.
    pub fn with_director_config(mut self, client: OpenAiClient, story: &StoryConfig) -> Self {
        self.director = Some(Director::from_story_config(client, story));
        self
    }

    /// Decide what happens next based on the strategy.
    pub async fn next_action(
        &mut self,
        conversation: &[Message],
        available_characters: &[Character],
    ) -> Result<TurnDecision> {
        if available_characters.is_empty() {
            return Ok(TurnDecision::Player);
        }

        match &self.strategy {
            TurnStrategy::RoundRobin => {
                let idx = self.round_robin_index % available_characters.len();
                self.round_robin_index += 1;
                Ok(TurnDecision::Sequential(vec![available_characters[idx]
                    .name
                    .clone()]))
            }
            TurnStrategy::DirectorControlled => {
                if let Some(director) = &self.director {
                    director
                        .decide_next_action(conversation, available_characters)
                        .await
                } else {
                    Err(rust_agent_core::AgentError::Config(
                        "DirectorControlled strategy requires story.director configuration"
                            .to_string(),
                    ))
                }
            }
            TurnStrategy::Random => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as usize;
                let idx = seed % available_characters.len();
                Ok(TurnDecision::Sequential(vec![available_characters[idx]
                    .name
                    .clone()]))
            }
        }
    }

    /// Reset the turn counter.
    pub fn reset(&mut self) {
        self.round_robin_index = 0;
    }
}
