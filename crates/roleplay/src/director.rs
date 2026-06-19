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
        let system_prompt = r#"你是一个角色扮演故事的叙事导演。你的职责是：
1. 根据对话流向决定下一个应该说话的角色
2. 维持叙事节奏和张力
3. 确保所有角色获得适当的出场机会
4. 引导故事朝目标推进

重要规则：
- 如果玩家直接称呼某个角色或对某角色提问（例如"老板娘，你..."），那个被称呼的角色必须优先回应
- 只回复角色的名字，不要附带任何解释
- 如果多个角色需要回应，用逗号分隔名字，顺序就是说话顺序（先写的先说）
- 后面的角色会看到前面角色的回复再发言，请考虑对话的因果关系来排序
- 大多数情况下只需要1-2个角色回应即可，不要让所有人都说话
- 如果你认为场景应该结束，回复 "END_SCENE"
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
            "可用角色: {}\n\n根据对话内容，接下来谁应该说话？用逗号分隔名字，顺序代表说话先后。只输出名字。",
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
            .take(10)
            .rev()
            .cloned()
            .collect();
        messages.extend(recent);
        messages.push(Message::user("接下来谁应该说话？"));

        let response = self.client.chat_completion(&messages).await?;
        let response_text = response.content.trim().to_lowercase();

        // Parse the response to find character IDs, ordered by position in response
        let mut found: Vec<(usize, Uuid)> = available_characters
            .iter()
            .filter_map(|c| {
                response_text
                    .find(&c.name.to_lowercase())
                    .map(|pos| (pos, c.id))
            })
            .collect();
        found.sort_by_key(|(pos, _)| *pos);
        let mut speakers: Vec<Uuid> = found.into_iter().map(|(_, id)| id).collect();

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
            "这个场景应该结束了吗？只回复 YES 或 NO。",
        ));

        let response = self.client.chat_completion(&messages).await?;
        Ok(response.content.trim().to_uppercase().contains("YES"))
    }
}
