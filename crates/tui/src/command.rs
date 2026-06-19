//! Command parsing for TUI slash commands.

/// Commands that can be issued from the TUI input.
#[derive(Debug, Clone)]
pub enum Command {
    /// Send a chat message.
    SendMessage(String),
    /// Quit the application.
    Quit,
    /// Switch to a different scene.
    Scene(String),
    /// List or show characters.
    Characters,
    /// Show current model configuration.
    Model,
    /// Save the current session.
    Save(String),
    /// Load a saved session.
    Load(String),
    /// Show help.
    Help,
    /// Clear chat history.
    Clear,
}

impl Command {
    /// Try to parse a slash command from input text.
    /// Returns None if it's not a command (regular message).
    pub fn parse(input: &str) -> Option<Command> {
        let input = input.trim();
        if !input.starts_with('/') {
            return None;
        }

        let parts: Vec<&str> = input[1..].splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

        match cmd.as_str() {
            "quit" | "q" | "exit" => Some(Command::Quit),
            "scene" => Some(Command::Scene(arg)),
            "characters" | "chars" => Some(Command::Characters),
            "model" => Some(Command::Model),
            "save" => Some(Command::Save(if arg.is_empty() {
                "autosave".to_string()
            } else {
                arg
            })),
            "load" => Some(Command::Load(if arg.is_empty() {
                "autosave".to_string()
            } else {
                arg
            })),
            "help" | "h" => Some(Command::Help),
            "clear" => Some(Command::Clear),
            _ => None,
        }
    }

    /// Get help text for all available commands.
    pub fn help_text() -> &'static str {
        "/scene <name>  - Switch to a scene\n\
         /characters    - List active characters\n\
         /model         - Show current model configuration\n\
         /save [name]   - Save session\n\
         /load [name]   - Load session\n\
         /clear         - Clear chat history\n\
         /help          - Show this help\n\
         /quit          - Exit the application"
    }
}
