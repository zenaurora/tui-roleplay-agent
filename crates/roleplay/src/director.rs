//! Director - a meta-agent that controls narrative flow via structured JSON decisions.

use rust_agent_core::{Character, Message, Result, StoryConfig, TurnDecision};
use rust_agent_llm::OpenAiClient;
use serde::Deserialize;

/// The Director decides what happens next in the scene.
pub struct Director {
    client: OpenAiClient,
    system_prompt: String,
    protagonist_name: String,
    story_title: String,
    story_description: String,
    scene_goals: Vec<String>,
    scene_context: Option<String>,
    recent_message_count: usize,
}

/// Raw JSON structure returned by the Director LLM.
#[derive(Debug, Deserialize)]
struct DirectorOutput {
    #[serde(rename = "type")]
    decision_type: String,
    speakers: Option<Vec<String>>,
}

impl Director {
    pub fn from_story_config(client: OpenAiClient, story: &StoryConfig) -> Self {
        Self {
            client,
            system_prompt: story.director.system_prompt.clone(),
            protagonist_name: story.protagonist_name.clone(),
            story_title: story.title.clone(),
            story_description: story.description.clone(),
            scene_goals: story.scene_goals.clone(),
            scene_context: story.scene_context.clone(),
            recent_message_count: story.director.recent_message_count.max(1),
        }
    }

    /// Decide what happens next in the scene.
    pub async fn decide_next_action(
        &self,
        conversation: &[Message],
        available_characters: &[Character],
    ) -> Result<TurnDecision> {
        let char_names: Vec<String> = available_characters
            .iter()
            .map(|c| c.name.clone())
            .collect();

        let context = self.build_context(&char_names);

        let mut messages = vec![
            Message::system(&self.system_prompt),
            Message::system(&context),
        ];

        // Add last few messages for context
        let recent: Vec<Message> = conversation
            .iter()
            .rev()
            .take(self.recent_message_count)
            .rev()
            .cloned()
            .collect();
        messages.extend(recent);
        messages.push(Message::user("输出JSON决定接下来发生什么。"));

        let response = self.client.chat_completion(&messages).await?;
        let response_text = response.content.trim();

        // Try to parse JSON from the response
        self.parse_decision(response_text, available_characters)
    }

    fn build_context(&self, character_names: &[String]) -> String {
        let goals = if self.scene_goals.is_empty() {
            "无".to_string()
        } else {
            self.scene_goals
                .iter()
                .map(|goal| format!("- {}", goal))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let scene_context = self.scene_context.as_deref().unwrap_or("无");

        format!(
            "故事标题: {}\n故事描述: {}\n玩家角色名: {}\n可用NPC角色: {}\n场景目标:\n{}\n场景上下文: {}\n\n根据对话历史，决定接下来发生什么。只输出一个JSON对象。",
            self.story_title,
            self.story_description,
            self.protagonist_name,
            character_names.join("、"),
            goals,
            scene_context
        )
    }

    /// Parse the Director's JSON response into a TurnDecision.
    fn parse_decision(
        &self,
        response_text: &str,
        available_characters: &[Character],
    ) -> Result<TurnDecision> {
        // Extract JSON object from response (LLM may output text around it)
        let json_str = Self::extract_json(response_text);

        // Try to parse the JSON
        if let Ok(output) = serde_json::from_str::<DirectorOutput>(&json_str) {
            match output.decision_type.as_str() {
                "player" => return Ok(TurnDecision::Player),
                "end_scene" => return Ok(TurnDecision::EndScene),
                "sequential" => {
                    if let Some(speakers) = output.speakers {
                        let valid = self.filter_valid_names(&speakers, available_characters);
                        if !valid.is_empty() {
                            return Ok(TurnDecision::Sequential(valid));
                        }
                    }
                }
                "parallel" => {
                    if let Some(speakers) = output.speakers {
                        let valid = self.filter_valid_names(&speakers, available_characters);
                        if !valid.is_empty() {
                            return Ok(TurnDecision::Parallel(valid));
                        }
                    }
                }
                _ => {}
            }
        }

        // Fallback: try to find character names in raw text
        let lower = response_text.to_lowercase();
        if lower.contains("player") || lower.contains("玩家") {
            return Ok(TurnDecision::Player);
        }
        if lower.contains("end_scene") || lower.contains("结束") {
            return Ok(TurnDecision::EndScene);
        }

        // Last resort: pick first available character
        let fallback = available_characters
            .first()
            .map(|c| c.name.clone())
            .unwrap_or_default();
        Ok(TurnDecision::Sequential(vec![fallback]))
    }

    /// Extract a JSON object from potentially noisy LLM output.
    fn extract_json(text: &str) -> String {
        // Strip markdown code fences
        let stripped = text
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Try to find a JSON object by locating first '{' and last '}'
        if let Some(start) = stripped.find('{') {
            if let Some(end) = stripped.rfind('}') {
                if end > start {
                    return stripped[start..=end].to_string();
                }
            }
        }

        stripped.to_string()
    }

    /// Filter speaker names to only those that match available characters.
    fn filter_valid_names(
        &self,
        names: &[String],
        available_characters: &[Character],
    ) -> Vec<String> {
        names
            .iter()
            .filter(|name| available_characters.iter().any(|c| c.name == **name))
            .cloned()
            .collect()
    }
}
