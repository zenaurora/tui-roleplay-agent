//! Director - a meta-agent that controls narrative flow via structured JSON decisions.

use rust_agent_core::{Character, Message, Result, TurnDecision};
use rust_agent_llm::OpenAiClient;
use serde::Deserialize;

/// The Director decides what happens next in the scene.
pub struct Director {
    client: OpenAiClient,
    system_prompt: String,
}

/// Raw JSON structure returned by the Director LLM.
#[derive(Debug, Deserialize)]
struct DirectorOutput {
    #[serde(rename = "type")]
    decision_type: String,
    speakers: Option<Vec<String>>,
}

impl Director {
    pub fn new(client: OpenAiClient) -> Self {
        let system_prompt = r#"你是叙事导演。每次调用时，你决定接下来发生什么。

输出一个 JSON 对象（不要输出任何其他内容，不要用 markdown 代码块包裹）：

- 让角色依次说话（后面的角色能看到前面角色的回复）：{"type":"sequential","speakers":["角色名"]}
- 让多个角色同时说话（他们看不到彼此的回复）：{"type":"parallel","speakers":["角色A","角色B"]}
- 让玩家说话：{"type":"player"}
- 结束场景：{"type":"end_scene"}

规则：
- 如果上一条消息是玩家直接称呼某角色，该角色必须下一个回应
- 考虑对话因果关系决定顺序
- 不要让所有人都说话，通常1-2个角色即可
- 不要连续选择同一个角色说话，一个角色说完后应该换人或给玩家机会
- 只有谜题解决/冲突完全结束时才 end_scene
- 场景刚开始时，先让NPC开场欢迎玩家，再给玩家机会说话
- 玩家说完话之后，通常应该有1-2个NPC回应，然后给玩家机会说话
- 只输出 JSON，不要有任何额外文字
"#
        .to_string();

        Self {
            client,
            system_prompt,
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

        let context = format!(
            "可用角色: {}\n玩家角色名: 旅人\n\n根据对话历史，决定接下来发生什么。只输出一个JSON对象。",
            char_names.join("、")
        );

        let mut messages = vec![
            Message::system(&self.system_prompt),
            Message::system(&context),
        ];

        // Add last few messages for context
        let recent: Vec<Message> = conversation
            .iter()
            .rev()
            .take(15)
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
            .filter(|name| {
                available_characters
                    .iter()
                    .any(|c| c.name == **name)
            })
            .cloned()
            .collect()
    }
}
