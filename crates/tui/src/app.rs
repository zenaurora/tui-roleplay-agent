//! Application state and main loop for the TUI.

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;

use crate::command::Command;
use crate::ui;

/// The main application state.
pub struct App {
    /// Chat messages to display.
    pub messages: Vec<ChatMessage>,
    /// Current input buffer.
    pub input: String,
    /// Input history for up/down navigation.
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Scroll position for the chat panel.
    pub scroll_offset: u16,
    /// Currently active characters (for sidebar display).
    pub characters: Vec<CharacterInfo>,
    /// Current scene name.
    pub scene_name: String,
    /// Story title.
    pub story_title: String,
    /// Whether we're waiting for a response.
    pub is_loading: bool,
    /// Cursor position in the input.
    pub cursor_position: usize,
}

/// A message displayed in the chat.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub character_name: String,
    pub content: String,
    pub is_user: bool,
    pub is_system: bool,
}

/// Character info for sidebar display.
#[derive(Debug, Clone)]
pub struct CharacterInfo {
    pub name: String,
    pub short_description: String,
    pub is_active: bool,
}

impl App {
    pub fn new(story_title: String, scene_name: String) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            input_history: Vec::new(),
            history_index: None,
            should_quit: false,
            scroll_offset: 0,
            characters: Vec::new(),
            scene_name,
            story_title,
            is_loading: false,
            cursor_position: 0,
        }
    }

    /// Add a chat message to the display.
    pub fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        // Auto-scroll to bottom
        self.scroll_offset = 0;
    }

    /// Add a system notification.
    pub fn add_system_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            character_name: "System".to_string(),
            content,
            is_user: false,
            is_system: true,
        });
    }

    /// Submit the current input, returning the text if non-empty.
    pub fn submit_input(&mut self) -> Option<String> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input_history.push(text.clone());
        self.history_index = None;
        self.input.clear();
        self.cursor_position = 0;
        Some(text)
    }

    /// Convert char index (cursor_position) to byte index in self.input.
    fn char_to_byte_index(&self, char_idx: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_idx)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.input.len())
    }

    /// Get the number of characters in the input (not bytes).
    fn input_char_count(&self) -> usize {
        self.input.chars().count()
    }

    /// Handle a key event.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<Command> {
        match (code, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                Some(Command::Quit)
            }
            (KeyCode::Enter, _) => {
                if let Some(text) = self.submit_input() {
                    if let Some(cmd) = Command::parse(&text) {
                        Some(cmd)
                    } else {
                        Some(Command::SendMessage(text))
                    }
                } else {
                    None
                }
            }
            (KeyCode::Backspace, _) => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    let byte_idx = self.char_to_byte_index(self.cursor_position);
                    // Find the byte range of the char at this position and remove it
                    let ch = self.input[byte_idx..].chars().next().unwrap();
                    self.input.drain(byte_idx..byte_idx + ch.len_utf8());
                }
                None
            }
            (KeyCode::Left, _) => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
                None
            }
            (KeyCode::Right, _) => {
                if self.cursor_position < self.input_char_count() {
                    self.cursor_position += 1;
                }
                None
            }
            (KeyCode::Up, _) => {
                if !self.input_history.is_empty() {
                    let idx = match self.history_index {
                        Some(i) if i > 0 => i - 1,
                        Some(i) => i,
                        None => self.input_history.len() - 1,
                    };
                    self.history_index = Some(idx);
                    self.input = self.input_history[idx].clone();
                    self.cursor_position = self.input_char_count();
                }
                None
            }
            (KeyCode::Down, _) => {
                if let Some(idx) = self.history_index {
                    if idx + 1 < self.input_history.len() {
                        self.history_index = Some(idx + 1);
                        self.input = self.input_history[idx + 1].clone();
                    } else {
                        self.history_index = None;
                        self.input.clear();
                    }
                    self.cursor_position = self.input_char_count();
                }
                None
            }
            (KeyCode::PageUp, _) => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                None
            }
            (KeyCode::PageDown, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                None
            }
            (KeyCode::Char(c), _) => {
                let byte_idx = self.char_to_byte_index(self.cursor_position);
                self.input.insert(byte_idx, c);
                self.cursor_position += 1;
                None
            }
            _ => None,
        }
    }

    /// Run the TUI event loop.
    pub async fn run(
        mut self,
        mut event_rx: mpsc::Receiver<AppEvent>,
        command_tx: mpsc::Sender<Command>,
    ) -> io::Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            // Draw UI
            terminal.draw(|f| ui::draw(f, &self))?;

            // Handle events with a timeout for async messages
            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if let Some(cmd) = self.handle_key(key.code, key.modifiers) {
                        if matches!(cmd, Command::Quit) {
                            break;
                        }
                        let _ = command_tx.send(cmd).await;
                    }
                }
            }

            // Process incoming events from the roleplay engine
            while let Ok(event) = event_rx.try_recv() {
                match event {
                    AppEvent::NewMessage(msg) => self.add_message(msg),
                    AppEvent::SystemMessage(text) => self.add_system_message(text),
                    AppEvent::Loading(loading) => self.is_loading = loading,
                    AppEvent::SceneChanged(name) => self.scene_name = name,
                    AppEvent::CharactersUpdated(chars) => self.characters = chars,
                    AppEvent::StreamDelta { character, delta } => {
                        // Append delta to the last message from this character
                        if let Some(last) = self.messages.last_mut() {
                            if last.character_name == character {
                                last.content.push_str(&delta);
                            }
                        }
                    }
                    AppEvent::Quit => {
                        self.should_quit = true;
                        break;
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }
}

/// Events sent to the TUI from the roleplay engine.
#[derive(Debug, Clone)]
pub enum AppEvent {
    NewMessage(ChatMessage),
    SystemMessage(String),
    Loading(bool),
    SceneChanged(String),
    CharactersUpdated(Vec<CharacterInfo>),
    StreamDelta { character: String, delta: String },
    Quit,
}
