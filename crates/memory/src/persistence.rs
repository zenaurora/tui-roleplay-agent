//! Persistence layer for saving/loading conversation state to disk.

use rust_agent_core::{MemoryError, Message};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Serializable save state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveState {
    pub messages: Vec<Message>,
    pub scene_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// File-based memory persistence.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    base_path: PathBuf,
}

impl MemoryStore {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Save conversation state to a JSON file.
    pub async fn save(&self, name: &str, state: &SaveState) -> Result<PathBuf, MemoryError> {
        let path = self.base_path.join(format!("{}.json", name));

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }

        let json = serde_json::to_string_pretty(state)
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;

        tokio::fs::write(&path, json)
            .await
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        Ok(path)
    }

    /// Load conversation state from a JSON file.
    pub async fn load(&self, name: &str) -> Result<SaveState, MemoryError> {
        let path = self.base_path.join(format!("{}.json", name));

        if !path.exists() {
            return Err(MemoryError::NotFound(format!(
                "Save file not found: {}",
                path.display()
            )));
        }

        let json = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        serde_json::from_str(&json).map_err(|e| MemoryError::Serialization(e.to_string()))
    }

    /// List all available save files.
    pub async fn list_saves(&self) -> Result<Vec<String>, MemoryError> {
        let mut saves = Vec::new();

        if !self.base_path.exists() {
            return Ok(saves);
        }

        let mut entries = tokio::fs::read_dir(&self.base_path)
            .await
            .map_err(|e| MemoryError::Storage(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| MemoryError::Storage(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                if let Some(stem) = path.file_stem() {
                    saves.push(stem.to_string_lossy().to_string());
                }
            }
        }

        Ok(saves)
    }

    /// Delete a save file.
    pub async fn delete(&self, name: &str) -> Result<(), MemoryError> {
        let path = self.base_path.join(format!("{}.json", name));
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    /// Get the base path.
    pub fn path(&self) -> &Path {
        &self.base_path
    }
}
