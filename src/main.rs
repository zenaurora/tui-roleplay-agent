//! rust-agent: A multi-AI roleplay chat application.
//!
//! This is the main entry point that wires together all the crates.

use anyhow::{anyhow, Result};
use rust_agent_core::{Agent, AppConfig, Character, CharacterConfig, Message, Scene, TurnDecision};
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

    // Setup scene manager with characters from config
    let mut scene_manager = SceneManager::new();
    let characters = characters_from_config(&config.story.characters);
    for character in &characters {
        scene_manager.add_character(character.clone());
    }

    // Create scene from config
    let mut scene = Scene::new("Opening", &config.story.description)
        .with_characters(characters.iter().map(|c| c.id).collect())
        .with_goals(config.story.scene_goals.clone());
    if let Some(ref ctx) = config.story.scene_context {
        scene = scene.with_context(ctx);
    }
    scene_manager.add_scene(scene);
    scene_manager.set_scene(0);

    // Setup turn manager (Director gets its own labeled client)
    let director_client = llm_client.clone().with_label("director");
    let mut turn_manager = TurnManager::new(config.story.turn_strategy.clone())
        .with_director_config(director_client, &config.story);

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
        if config.llm.thinking_enabled {
            "ON"
        } else {
            "OFF"
        },
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
                        if let Some(agent) =
                            character_agents.iter().find(|a| &a.character.name == name)
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
                        if let Some(agent) =
                            character_agents.iter().find(|a| &a.character.name == name)
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
                                let user_msg =
                                    Message::user(&text).with_character(&protagonist_name);
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
                                    .send(AppEvent::SystemMessage(format!(
                                        "Active characters:\n{}",
                                        list
                                    )))
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
                                    .send(AppEvent::SystemMessage(
                                        "Command not yet implemented.".to_string(),
                                    ))
                                    .await;
                            }
                        }
                    }
                }
                TurnDecision::EndScene => {
                    let _ = event_tx_clone
                        .send(AppEvent::SystemMessage(
                            "—— 场景结束 ——\n导演判定本场剧情已完结。感谢游玩！输入 /quit 退出。"
                                .to_string(),
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

/// Load configuration from file.
async fn load_config() -> Result<AppConfig> {
    let config_path = std::path::Path::new("config/story.toml");

    if config_path.exists() {
        let content = tokio::fs::read_to_string(config_path).await?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    } else {
        Err(anyhow!("config not exists"))
    }
}

/// Convert CharacterConfig entries from TOML into Character objects.
fn characters_from_config(configs: &[CharacterConfig]) -> Vec<Character> {
    configs
        .iter()
        .map(|c| {
            let mut character = Character::new(&c.name, &c.personality, &c.backstory)
                .with_system_prompt(&c.system_prompt);

            if let Some(ref desc) = c.short_description {
                character = character.with_description(desc);
            }
            if let Some(ref model) = c.model {
                character = character.with_model(model);
            }

            character
        })
        .collect()
}
