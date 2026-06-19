//! Turn manager - decides turn order for character responses.

use rust_agent_core::{Character, Message, Result, TurnStrategy};
use rust_agent_llm::OpenAiClient;
use uuid::Uuid;

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

    /// Create a TurnManager with a director for DirectorControlled strategy.
    pub fn with_director(mut self, client: OpenAiClient) -> Self {
        self.director = Some(Director::new(client));
        self
    }

    /// Decide who speaks next based on the strategy.
    pub async fn next_speakers(
        &mut self,
        conversation: &[Message],
        available_characters: &[Character],
    ) -> Result<Vec<Uuid>> {
        if available_characters.is_empty() {
            return Ok(Vec::new());
        }

        match &self.strategy {
            TurnStrategy::RoundRobin => {
                let idx = self.round_robin_index % available_characters.len();
                self.round_robin_index += 1;
                Ok(vec![available_characters[idx].id])
            }
            TurnStrategy::DirectorControlled => {
                if let Some(director) = &self.director {
                    director
                        .decide_next_speaker(conversation, available_characters)
                        .await
                } else {
                    // Fallback to round-robin if no director is set
                    let idx = self.round_robin_index % available_characters.len();
                    self.round_robin_index += 1;
                    Ok(vec![available_characters[idx].id])
                }
            }
            TurnStrategy::Random => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as usize;
                let idx = seed % available_characters.len();
                Ok(vec![available_characters[idx].id])
            }
        }
    }

    /// Ask the director whether the scene should end.
    /// Returns false if no director is configured.
    pub async fn should_end_scene(&self, conversation: &[Message]) -> Result<bool> {
        if let Some(director) = &self.director {
            director.should_end_scene(conversation).await
        } else {
            Ok(false)
        }
    }

    /// Reset the turn counter.
    pub fn reset(&mut self) {
        self.round_robin_index = 0;
    }
}
