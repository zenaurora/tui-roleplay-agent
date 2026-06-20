//! CharacterAgent - wraps an LLM call with character-specific prompting.

use async_trait::async_trait;
use rust_agent_agent::ToolRegistry;
use rust_agent_core::{Agent, Character, Message, Result};
use rust_agent_llm::OpenAiClient;
use rust_agent_memory::SlidingWindowContext;
use std::sync::Arc;

const MAX_TOOL_ROUNDS: usize = 4;

/// An agent that plays a specific character in the roleplay.
pub struct CharacterAgent {
    pub character: Character,
    client: OpenAiClient,
    context_window: SlidingWindowContext,
    tools: Option<Arc<ToolRegistry>>,
}

impl CharacterAgent {
    pub fn new(character: Character, client: OpenAiClient) -> Self {
        Self {
            character,
            client,
            context_window: SlidingWindowContext::default(),
            tools: None,
        }
    }

    pub fn with_context_window(mut self, window: SlidingWindowContext) -> Self {
        self.context_window = window;
        self
    }

    pub fn with_tools(mut self, tools: Arc<ToolRegistry>) -> Self {
        self.tools = Some(tools);
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
        let mut prepared = self.build_messages(messages);

        let model = self.character.model.as_deref();
        for _ in 0..MAX_TOOL_ROUNDS {
            let tool_specs = self
                .tools
                .as_ref()
                .map(|tools| tools.tool_specs())
                .unwrap_or_default();
            let response = if let Some(model) = model {
                self.client
                    .chat_completion_with_model_and_tools(&prepared, model, &tool_specs)
                    .await?
            } else if !tool_specs.is_empty() {
                self.client
                    .chat_completion_with_tools(&prepared, &tool_specs)
                    .await?
            } else {
                self.client.chat_completion(&prepared).await?
            };

            if let Some(tools) = &self.tools {
                if let Some(tool_messages) = execute_tool_calls(&response, tools).await? {
                    prepared.push(response);
                    prepared.extend(tool_messages);
                    continue;
                }
            }

            return Ok(response.with_character(&self.character.name));
        }

        Ok(Message::assistant("工具调用轮次过多，已停止继续调用工具。")
            .with_character(&self.character.name))
    }
}

async fn execute_tool_calls(
    response: &Message,
    tools: &ToolRegistry,
) -> Result<Option<Vec<Message>>> {
    if response.tool_calls.is_empty() {
        return Ok(None);
    }

    let mut messages = Vec::new();
    for tool_call in &response.tool_calls {
        let content = match serde_json::from_str(&tool_call.arguments) {
            Ok(args) => match tools.execute(&tool_call.name, args).await {
                Ok(result) => result,
                Err(error) => format!("Tool error: {}", error),
            },
            Err(error) => format!("Tool error: invalid JSON arguments: {}", error),
        };

        messages.push(
            Message::new(rust_agent_core::Role::Tool, content)
                .with_tool_call_id(tool_call.id.clone()),
        );
    }

    Ok(Some(messages))
}
