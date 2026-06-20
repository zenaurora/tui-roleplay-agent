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
        // 这里额外拼接一个“单角色输出”护栏。
        //
        // 背景：即使 Director 已经决定了当前只该某个角色说话，LLM 仍可能把上下文当成
        // 一段剧本续写，输出类似：
        //
        //   NiKo: ...
        //   s1mple: ...
        //
        // 这不一定是业务调度错了，更可能是模型把历史里的 “角色名: 内容” 格式学走了。
        // 如果后面确认模型足够稳定，或者改成结构化输出，可以优先删除
        // `single_speaker_guardrail` 和 `clean_response_content` 这两段防护。
        let system = Message::system(format!(
            "{}\n\n{}",
            self.character.system_prompt,
            self.single_speaker_guardrail(conversation)
        ));
        let mut messages = vec![system];
        let windowed = self.context_window.apply(conversation);
        messages.extend(windowed);
        messages
    }

    // 临时/防御性提示词：要求当前模型调用只输出当前角色自己的台词。
    //
    // 这不是 Director 的职责。Director 只决定“谁该说话”；这里约束“被选中的角色
    // 不要顺手替别人说话”。它用最近上下文里出现过的角色名构造禁止列表，例如
    // “Do not write lines for s1mple, donk”，避免 NiKo 回复里夹出 s1mple 台词。
    //
    // 删除影响：
    // - 删除后功能仍能运行；
    // - 但模型可能再次输出 “NiKo: ...\n\ns1mple: ...” 这种多角色剧本。
    fn single_speaker_guardrail(&self, conversation: &[Message]) -> String {
        // 从历史消息中收集“其他角色名”。这里只看已有历史，而不是全量角色列表，
        // 是为了让这个 crate 不依赖场景管理器，也避免把无关角色名塞进 prompt。
        let mut other_names = conversation
            .iter()
            .filter_map(|message| message.character_name.as_deref())
            .filter(|name| *name != self.character.name)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        other_names.sort();
        other_names.dedup();

        // 如果历史里暂时没有其他角色，也仍然保留一个泛化规则，避免模型续写任意
        // “another speaker”。
        let other_rule = if other_names.is_empty() {
            "Do not write lines for any other character.".to_string()
        } else {
            format!(
                "Do not write lines for any other character, including: {}.",
                other_names.join(", ")
            )
        };

        format!(
            "Output rules:\n\
             - You are speaking as {name} only.\n\
             - Return only {name}'s own dialogue.\n\
             - Do not prefix your answer with \"{name}:\".\n\
             - {other_rule}\n\
             - Do not continue the transcript or invent another speaker's reply.",
            name = self.character.name,
            other_rule = other_rule
        )
    }

    // 响应后清洗：这是 prompt 之外的第二层保险。
    //
    // prompt 只能降低串台概率，不能保证模型一定听话。因此在把消息写入 memory 和 TUI
    // 之前，再做一次保守清洗：
    // 1. 去掉当前角色自己的名字前缀，比如 “NiKo: 感谢支持。” -> “感谢支持。”
    // 2. 如果后面出现其他角色行，比如 “s1mple: ...”，从那一行开始截断。
    //
    // 这个函数只处理非常明确的 “角色名:” / “角色名：” 行，避免误删正文里自然提到
    // 其他角色名字的情况。也就是说，“我同意 s1mple 的看法”不会被截断。
    //
    // 删除影响：
    // - 删除后日志、UI、memory 都会保留模型原始输出；
    // - 如果模型串台，其他角色台词会被存入历史，后续更容易继续串台。
    fn clean_response_content(&self, content: &str, conversation: &[Message]) -> String {
        // 先去掉自己的 speaker prefix。UI 已经会用 ChatMessage.character_name 渲染角色名，
        // 如果内容里再带 “NiKo:”，就会出现重复。
        let mut cleaned = strip_speaker_prefix(content.trim(), &self.character.name);

        // 和 guardrail 一样，只从历史里收集已出现过的其他角色名，保持逻辑局部。
        let mut speaker_names = conversation
            .iter()
            .filter_map(|message| message.character_name.as_deref())
            .filter(|name| *name != self.character.name)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        speaker_names.sort();
        speaker_names.dedup();

        // 一旦发现 “其他角色名:” 开头的行，就认为模型开始续写下一个 speaker，
        // 直接截断。顺序遍历所有已知角色名，任何一个命中都截。
        for name in speaker_names {
            if let Some(index) = find_speaker_line_start(&cleaned, &name) {
                cleaned = cleaned[..index].trim_end().to_string();
            }
        }

        cleaned
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

            let mut response = response.with_character(&self.character.name);
            // 最后一道防线：在消息进入 memory/TUI 之前清洗串台内容。
            response.content = self.clean_response_content(&response.content, messages);
            return Ok(response);
        }

        Ok(Message::assistant("工具调用轮次过多，已停止继续调用工具。")
            .with_character(&self.character.name))
    }
}

// 去掉模型可能自己加上的当前角色名前缀。
//
// 只处理字符串开头精确匹配的 “speaker:” 或 “speaker：”。这样比较保守：
// - “NiKo: 感谢支持。” 会变成 “感谢支持。”
// - “我觉得 NiKo: 这个标签...” 不会被动到，因为它不在开头
fn strip_speaker_prefix(content: &str, speaker: &str) -> String {
    let Some(rest) = content.strip_prefix(speaker) else {
        return content.to_string();
    };

    rest.strip_prefix(':')
        .or_else(|| rest.strip_prefix('：'))
        .map(str::trim_start)
        .unwrap_or(content)
        .to_string()
}

// 查找某个 speaker 是否作为“新台词行”出现。
//
// 这个函数刻意要求 speaker 标签出现在某一行的开头（允许前面有空格），并且后面
// 必须紧跟英文冒号或中文冒号。这样能识别：
//
//   s1mple: practice 才帮。
//
// 但不会误伤：
//
//   我同意 s1mple 的说法。
//
// 返回值是该 speaker 标签在原始字符串里的字节 offset，调用方用它截断内容。
fn find_speaker_line_start(content: &str, speaker: &str) -> Option<usize> {
    let colon_prefix = format!("{speaker}:");
    let full_width_colon_prefix = format!("{speaker}：");
    let mut offset = 0;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&colon_prefix) || trimmed.starts_with(&full_width_colon_prefix) {
            // `line` 可能有前导空格；这里返回 trim 后标签真正开始的位置。
            return Some(offset + line.len() - line.trim_start().len());
        }
        offset += line.len();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_own_speaker_prefix() {
        assert_eq!(
            strip_speaker_prefix("NiKo: 感谢支持。", "NiKo"),
            "感谢支持。"
        );
        assert_eq!(
            strip_speaker_prefix("NiKo：感谢支持。", "NiKo"),
            "感谢支持。"
        );
    }

    #[test]
    fn finds_other_speaker_line() {
        let content = "感谢支持。\n\ns1mple: practice 才帮。";

        assert_eq!(find_speaker_line_start(content, "s1mple"), Some(17));
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
