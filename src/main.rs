//! rust-agent: A multi-AI roleplay chat application.
//!
//! This is the main entry point that wires together all the crates.

use anyhow::{Result, anyhow};
use rust_agent_core::{Agent, AppConfig, Character, Message, Scene, TurnDecision};
use rust_agent_llm::OpenAiClient;
use rust_agent_memory::ConversationMemory;
use rust_agent_roleplay::{CharacterAgent, SceneManager, TurnManager};
use rust_agent_tui::{
    app::{App, AppEvent, CharacterInfo, ChatMessage},
    command::Command,
};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    // Load configuration
    let config = load_config().await?;

    // Initialize LLM client
    let llm_client = OpenAiClient::from_config(&config.llm);

    // Setup scene manager with characters
    let mut scene_manager = SceneManager::new();
    let characters = create_demo_characters();
    for character in &characters {
        scene_manager.add_character(character.clone());
    }

    // Create a demo scene
    let scene = Scene::new("Opening", &config.story.description)
        .with_characters(characters.iter().map(|c| c.id).collect())
        .with_goals(vec![
            "旅人通过对话收集线索，找出偷翡翠坠子的人".to_string(),
            "揭露猎人是真正的小偷".to_string(),
        ])
        .with_context("场景：醉仙居客栈大堂，清晨。老板娘在柜台后面焦急踱步，书生在角落低头看书，猎人靠在门边沉默不语。".to_string());
    scene_manager.add_scene(scene);
    scene_manager.set_scene(0);

    // Setup turn manager (Director gets its own labeled client)
    let director_client = llm_client.clone().with_label("director");
    let mut turn_manager =
        TurnManager::new(config.story.turn_strategy.clone()).with_director(director_client);

    // Setup conversation memory
    let mut memory = ConversationMemory::new();

    // Create character agents (each with a labeled client)
    let character_agents: Vec<CharacterAgent> = characters
        .iter()
        .map(|c| {
            let client = llm_client.clone().with_label(&c.name);
            CharacterAgent::new(c.clone(), client)
        })
        .collect();

    // Setup TUI channels
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>(100);
    let (command_tx, mut command_rx) = mpsc::channel::<Command>(100);

    // Initialize TUI app
    let app = App::new(config.story.title.clone(), "Opening".to_string());

    // Send initial character info to TUI
    let char_infos: Vec<CharacterInfo> = characters
        .iter()
        .map(|c| CharacterInfo {
            name: c.name.clone(),
            short_description: c
                .short_description
                .clone()
                .unwrap_or_else(|| c.personality.chars().take(30).collect()),
            is_active: true,
        })
        .collect();
    let _ = event_tx.send(AppEvent::CharactersUpdated(char_infos)).await;
    let _ = event_tx
        .send(AppEvent::SystemMessage(format!(
            "Welcome to '{}'. You are {}. Type /help for commands.",
            config.story.title, config.story.protagonist_name
        )))
        .await;

    // Spawn the command handler
    let event_tx_clone = event_tx.clone();
    let protagonist_name = config.story.protagonist_name.clone();

    // Capture model info for /model command
    let model_info = format!(
        "Model: {}\n\
         Base URL: {}\n\
         Max tokens: {}\n\
         Temperature: {}\n\
         Thinking mode: {}\n\
         Reasoning effort: {}",
        config.llm.model,
        config.llm.base_url,
        config.llm.max_tokens,
        config.llm.temperature,
        if config.llm.thinking_enabled { "ON" } else { "OFF" },
        config.llm.reasoning_effort,
    );

    // Background task: Director-orchestrated scene loop
    tokio::spawn(async move {
        let active_chars: Vec<Character> = scene_manager
            .active_characters()
            .into_iter()
            .cloned()
            .collect();

        // Safety: track consecutive NPC turns to prevent infinite loops
        let mut consecutive_npc_turns: usize = 0;
        const MAX_NPC_TURNS: usize = 3;

        // Director loop: keeps running until scene ends or quit
        loop {
            // Safety cap: force player turn if too many consecutive NPC turns
            let decision = if consecutive_npc_turns >= MAX_NPC_TURNS {
                consecutive_npc_turns = 0;
                TurnDecision::Player
            } else {
                // Ask Director what happens next
                let _ = event_tx_clone.send(AppEvent::Loading(true)).await;
                let d = turn_manager
                    .next_action(memory.history(), &active_chars)
                    .await
                    .unwrap_or(TurnDecision::Player);
                let _ = event_tx_clone.send(AppEvent::Loading(false)).await;
                d
            };

            match decision {
                TurnDecision::Sequential(names) => {
                    consecutive_npc_turns += 1;
                    let _ = event_tx_clone.send(AppEvent::Loading(true)).await;
                    for name in &names {
                        if let Some(agent) = character_agents
                            .iter()
                            .find(|a| &a.character.name == name)
                        {
                            match agent.run(memory.history()).await {
                                Ok(response) => {
                                    let char_name = response
                                        .character_name
                                        .clone()
                                        .unwrap_or_else(|| name.clone());
                                    memory.add_message(response.clone(), Some(agent.character.id));
                                    let _ = event_tx_clone
                                        .send(AppEvent::NewMessage(ChatMessage {
                                            character_name: char_name,
                                            content: response.content,
                                            is_user: false,
                                            is_system: false,
                                        }))
                                        .await;
                                }
                                Err(e) => {
                                    let _ = event_tx_clone
                                        .send(AppEvent::SystemMessage(format!("Error: {}", e)))
                                        .await;
                                }
                            }
                        }
                    }
                    let _ = event_tx_clone.send(AppEvent::Loading(false)).await;
                }
                TurnDecision::Parallel(names) => {
                    consecutive_npc_turns += 1;
                    let _ = event_tx_clone.send(AppEvent::Loading(true)).await;
                    // Snapshot context before parallel execution
                    let history_snapshot: Vec<Message> = memory.history().to_vec();
                    let mut results = Vec::new();
                    for name in &names {
                        if let Some(agent) = character_agents
                            .iter()
                            .find(|a| &a.character.name == name)
                        {
                            match agent.run(&history_snapshot).await {
                                Ok(response) => results.push((agent.character.id, response)),
                                Err(e) => {
                                    let _ = event_tx_clone
                                        .send(AppEvent::SystemMessage(format!("Error: {}", e)))
                                        .await;
                                }
                            }
                        }
                    }
                    // Add all results to memory and TUI
                    for (char_id, response) in results {
                        let char_name = response
                            .character_name
                            .clone()
                            .unwrap_or_else(|| "Unknown".to_string());
                        memory.add_message(response.clone(), Some(char_id));
                        let _ = event_tx_clone
                            .send(AppEvent::NewMessage(ChatMessage {
                                character_name: char_name,
                                content: response.content,
                                is_user: false,
                                is_system: false,
                            }))
                            .await;
                    }
                    let _ = event_tx_clone.send(AppEvent::Loading(false)).await;
                }
                TurnDecision::Player => {
                    consecutive_npc_turns = 0;
                    // Wait for player input
                    loop {
                        match command_rx.recv().await {
                            Some(Command::SendMessage(text)) => {
                                let user_msg = Message::user(&text).with_character(&protagonist_name);
                                memory.add_message(user_msg, None);
                                let _ = event_tx_clone
                                    .send(AppEvent::NewMessage(ChatMessage {
                                        character_name: protagonist_name.clone(),
                                        content: text,
                                        is_user: true,
                                        is_system: false,
                                    }))
                                    .await;
                                break; // Back to Director loop
                            }
                            Some(Command::Help) => {
                                let _ = event_tx_clone
                                    .send(AppEvent::SystemMessage(Command::help_text().to_string()))
                                    .await;
                            }
                            Some(Command::Clear) => {
                                memory.clear();
                                let _ = event_tx_clone
                                    .send(AppEvent::SystemMessage("Chat cleared.".to_string()))
                                    .await;
                            }
                            Some(Command::Characters) => {
                                let list: String = active_chars
                                    .iter()
                                    .map(|c| format!("- {} ({})", c.name, c.personality))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                let _ = event_tx_clone
                                    .send(AppEvent::SystemMessage(format!("Active characters:\n{}", list)))
                                    .await;
                            }
                            Some(Command::Model) => {
                                let _ = event_tx_clone
                                    .send(AppEvent::SystemMessage(model_info.clone()))
                                    .await;
                            }
                            Some(Command::Quit) => {
                                let _ = event_tx_clone.send(AppEvent::Quit).await;
                                return;
                            }
                            None => return, // channel closed
                            _ => {
                                let _ = event_tx_clone
                                    .send(AppEvent::SystemMessage("Command not yet implemented.".to_string()))
                                    .await;
                            }
                        }
                    }
                }
                TurnDecision::EndScene => {
                    let _ = event_tx_clone
                        .send(AppEvent::SystemMessage(
                            "—— 场景结束 ——\n导演判定本场剧情已完结。感谢游玩！输入 /quit 退出。".to_string(),
                        ))
                        .await;
                    // Wait for quit command
                    while let Some(cmd) = command_rx.recv().await {
                        if matches!(cmd, Command::Quit) {
                            let _ = event_tx_clone.send(AppEvent::Quit).await;
                            return;
                        }
                    }
                    return;
                }
            }
        }
    });

    // Run the TUI (blocks until quit)
    app.run(event_rx, command_tx).await?;

    Ok(())
}

/// Load configuration from file or create a default.
async fn load_config() -> Result<AppConfig> {
    let config_path = std::path::Path::new("config/story.toml");

    if config_path.exists() {
        let content = tokio::fs::read_to_string(config_path).await?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    } else {
        return Err(anyhow!("config not exists"));
    }
}

/// Create demo characters for the default story.
fn create_demo_characters() -> Vec<Character> {
    vec![
        Character::new(
            "老板娘",
            "热情、爱八卦、说话带着市井气。会主动提供线索但有时会夸大其词。",
            "醉仙居的老板娘，经营这家客栈十五年了。翡翠坠子是她亡母留下的遗物，价值连城。她昨晚喝了点酒，睡得很沉，醒来发现坠子不见了。她怀疑是书生偷的，因为书生欠了赌债。",
        )
        .with_system_prompt(
            "你是「醉仙居」客栈的老板娘。你的翡翠坠子昨夜被偷了。\n\
             性格：热情、爱说话、有点八卦、偶尔夸大其词。\n\
             你知道的信息：\n\
             - 坠子放在柜台后面的暗格里，只有你知道位置\n\
             - 昨晚你喝多了酒，睡得很沉\n\
             - 书生最近赌博输了很多钱，经常唉声叹气\n\
             - 猎人昨晚很晚才回来，身上带着泥\n\
             - 其实你半夜模模糊糊听到了脚步声，但看不清是谁\n\
             秘密：你昨晚确实把暗格打开给自己看了一眼坠子才睡的，所以暗格可能没锁好。\n\
             用中文回复，保持角色，每次回复2-3句话。只扮演你自己（老板娘），不要替其他角色说话。"
        )
        .with_description("客栈老板娘"),
        Character::new(
            "书生",
            "紧张、书卷气、说话文绉绉。有隐情但不是小偷。",
            "赶考的书生，因为赌博输了盘缠被困在客栈。他昨晚确实半夜起来过，但只是去院子里背书解压。他看到猎人从后门悄悄进来。",
        )
        .with_system_prompt(
            "你是一个赶考的书生，被困在醉仙居客栈。\n\
             性格：紧张、文雅、有点胆小、说话文绉绉。\n\
             你知道的信息：\n\
             - 你确实欠了赌债，但你绝对没有偷东西\n\
             - 昨晚你睡不着，半夜去院子里背书\n\
             - 你在院子里看到猎人从客栈后门悄悄进来，手里拿着什么东西\n\
             - 你很害怕被冤枉，所以一开始不敢说看到猎人的事\n\
             秘密：你之所以紧张，是因为赌债的事怕被人知道，不是因为偷窃。如果旅人持续追问或表示信任你，你会透露看到猎人的事。\n\
             用中文回复，保持角色，每次回复2-3句话。只扮演你自己（书生），不要替其他角色说话。"
        )
        .with_description("落魄书生"),
        Character::new(
            "猎人",
            "沉默、直接、不善言辞。是真正的小偷。",
            "猎人表面上以打猎为生，实际上偶尔会做些偷鸡摸狗的事。他昨晚趁老板娘醉酒，发现暗格没锁好，偷走了翡翠坠子藏在客栈后院的枯井里。",
        )
        .with_system_prompt(
            "你是一个猎人，暂住在醉仙居客栈。你是偷翡翠坠子的人。\n\
             性格：沉默寡言、回答简短、有点凶、不喜欢被追问。\n\
             你做了什么：\n\
             - 昨晚你发现老板娘喝醉了，暗格没锁好\n\
             - 你偷走了翡翠坠子，藏在客栈后院的枯井里\n\
             - 你从后门溜回来时以为没人看到\n\
             你的策略：\n\
             - 尽量少说话，装作不关心\n\
             - 如果被直接质问，会反过来指责书生\n\
             - 如果旅人提到「后门」「枯井」「半夜」等关键词，你会开始慌张，说话出现矛盾\n\
             - 如果证据确凿（旅人提到有人看到你从后门进来+手里有东西），你会认罪\n\
             用中文回复，保持角色，每次回复1-2句话，尽量简短。只扮演你自己（猎人），不要替其他角色说话。"
        )
        .with_description("沉默猎人"),
    ]
}
