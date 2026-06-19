//! Director - a meta-agent that controls narrative flow.

use rust_agent_core::{Character, Message, Result};
use rust_agent_llm::OpenAiClient;
use uuid::Uuid;

/// The Director decides who speaks next and manages narrative pacing.
pub struct Director {
    client: OpenAiClient,
    system_prompt: String,
}

impl Director {
    pub fn new(client: OpenAiClient) -> Self {
        let system_prompt = r#"You are a narrative director for a roleplay story. Your job is to:
1. Decide which character should speak next based on the conversation flow
2. Maintain narrative pacing and tension
3. Ensure all characters get appropriate screen time
4. Guide the story toward its goals

When asked who should speak next, respond with ONLY the character name.
If you think multiple characters should react, list them separated by commas.
If you think the scene should end, respond with "END_SCENE".
"#
        .to_string();

        Self {
            client,
            system_prompt,
        }
    }

    /// Decide which character should speak next.
    pub async fn decide_next_speaker(
        &self,
        conversation: &[Message],
        available_characters: &[Character],
    ) -> Result<Vec<Uuid>> {
        let char_names: Vec<String> = available_characters
            .iter()
            .map(|c| c.name.clone())
            .collect();

        let context = format!(
            "Available characters: {}\n\nBased on the conversation, who should speak next?",
            char_names.join(", ")
        );

        let mut messages = vec![
            Message::system(&self.system_prompt),
            Message::system(&context),
        ];

        // Add last few messages for context
        let recent: Vec<Message> = conversation
            .iter()
            .rev()
            .take(10)
            .rev()
            .cloned()
            .collect();
        messages.extend(recent);
        messages.push(Message::user("Who should speak next?"));

        let response = self.client.chat_completion(&messages).await?;
        let response_text = response.content.trim().to_lowercase();

        // Parse the response to find character IDs
        let mut speakers = Vec::new();
        for character in available_characters {
            if response_text.contains(&character.name.to_lowercase()) {
                speakers.push(character.id);
            }
        }

        // Fallback: if no character matched, pick the first available
        if speakers.is_empty() && !available_characters.is_empty() {
            speakers.push(available_characters[0].id);
        }

        Ok(speakers)
    }

    /// Check if the scene should end based on conversation flow.
    pub async fn should_end_scene(&self, conversation: &[Message]) -> Result<bool> {
        let mut messages = vec![Message::system(&self.system_prompt)];

        let recent: Vec<Message> = conversation
            .iter()
            .rev()
            .take(10)
            .rev()
            .cloned()
            .collect();
        messages.extend(recent);
        messages.push(Message::user(
            "Should this scene end? Reply YES or NO only.",
        ));

        let response = self.client.chat_completion(&messages).await?;
        Ok(response.content.trim().to_uppercase().contains("YES"))
    }
}
