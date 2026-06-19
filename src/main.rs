//! rust-agent: A multi-AI roleplay chat application.
//!
//! This is the main entry point that wires together all the crates.

use anyhow::{Result, anyhow};
use rust_agent_core::{Agent, AppConfig, Character, Message, Scene};
use rust_agent_llm::OpenAiClient;
use rust_agent_memory::ConversationMemory;
use rust_agent_roleplay::{CharacterAgent, SceneManager, TurnManager};
use rust_agent_tui::{
    app::{App, AppEvent, CharacterInfo, ChatMessage},
    command::Command,
};
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, fmt::format};

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
        .with_characters(characters.iter().map(|c| c.id).collect());
    scene_manager.add_scene(scene);
    scene_manager.set_scene(0);

    // Setup turn manager
    let mut turn_manager =
        TurnManager::new(config.story.turn_strategy.clone()).with_director(llm_client.clone());

    // Setup conversation memory
    let mut memory = ConversationMemory::new();

    // Create character agents
    let character_agents: Vec<CharacterAgent> = characters
        .iter()
        .map(|c| CharacterAgent::new(c.clone(), llm_client.clone()))
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

    // 后台启动一个线程处理 整个进程周期内 通过tx发送的消息
    tokio::spawn(async move {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                Command::SendMessage(text) => {
                    // Add user message
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

                    // Get next speaker(s)
                    let _ = event_tx_clone.send(AppEvent::Loading(true)).await;

                    let active_chars: Vec<Character> = scene_manager
                        .active_characters()
                        .into_iter()
                        .cloned()
                        .collect();

                    let speakers = turn_manager
                        .next_speakers(memory.history(), &active_chars)
                        .await
                        .unwrap_or_else(|_| {
                            if active_chars.is_empty() {
                                vec![]
                            } else {
                                vec![active_chars[0].id]
                            }
                        });

                    // Generate responses from each speaker
                    for speaker_id in speakers {
                        if let Some(agent) = character_agents
                            .iter()
                            .find(|a| a.character.id == speaker_id)
                        {
                            match agent.run(memory.history()).await {
                                Ok(response) => {
                                    let char_name = response
                                        .character_name
                                        .clone()
                                        .unwrap_or_else(|| "Unknown".to_string());

                                    memory.add_message(response.clone(), Some(speaker_id));

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
                                        .send(AppEvent::SystemMessage(format!(
                                            "Error: {}",
                                            e
                                        )))
                                        .await;
                                }
                            }
                        }
                    }

                    let _ = event_tx_clone.send(AppEvent::Loading(false)).await;
                }
                Command::Help => {
                    let _ = event_tx_clone
                        .send(AppEvent::SystemMessage(Command::help_text().to_string()))
                        .await;
                }
                Command::Clear => {
                    memory.clear();
                    let _ = event_tx_clone
                        .send(AppEvent::SystemMessage("Chat cleared.".to_string()))
                        .await;
                }
                Command::Characters => {
                    let list: String = scene_manager
                        .active_characters()
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
                Command::Quit => {
                    let _ = event_tx_clone.send(AppEvent::Quit).await;
                    break;
                }
                _ => {
                    let _ = event_tx_clone
                        .send(AppEvent::SystemMessage(
                            "Command not yet implemented.".to_string(),
                        ))
                        .await;
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
            "Elena",
            "Wise, mysterious, speaks in riddles. Former court mage.",
            "Once the most powerful mage in the kingdom, Elena left the court after a mysterious incident. She now runs a small apothecary and offers cryptic advice to travelers.",
        )
        .with_description("Mysterious mage"),
        Character::new(
            "Theron",
            "Brave, loyal, sometimes reckless. A wandering knight.",
            "Theron is a knight who lost his lord in battle. He wanders the land seeking purpose, always ready to draw his sword for a just cause.",
        )
        .with_description("Wandering knight"),
        Character::new(
            "Pip",
            "Cheerful, clever, mischievous. A young thief with a heart of gold.",
            "Pip grew up on the streets and learned to survive by wit and nimble fingers. Despite the rough exterior, Pip dreams of becoming a legitimate merchant someday.",
        )
        .with_description("Cheerful thief"),
    ]
}
