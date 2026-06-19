//! Scene manager - handles scene transitions and state.

use rust_agent_core::{Character, Scene};
use std::collections::HashMap;
use uuid::Uuid;

/// Manages scenes and their transitions.
pub struct SceneManager {
    scenes: Vec<Scene>,
    characters: HashMap<Uuid, Character>,
    current_scene_index: Option<usize>,
}

impl SceneManager {
    pub fn new() -> Self {
        Self {
            scenes: Vec::new(),
            characters: HashMap::new(),
            current_scene_index: None,
        }
    }

    /// Add a scene.
    pub fn add_scene(&mut self, scene: Scene) {
        self.scenes.push(scene);
    }

    /// Add a character.
    pub fn add_character(&mut self, character: Character) {
        self.characters.insert(character.id, character);
    }

    /// Set the current scene by index.
    pub fn set_scene(&mut self, index: usize) -> Option<&Scene> {
        if index < self.scenes.len() {
            self.current_scene_index = Some(index);
            Some(&self.scenes[index])
        } else {
            None
        }
    }

    /// Get the current scene.
    pub fn current_scene(&self) -> Option<&Scene> {
        self.current_scene_index.map(|i| &self.scenes[i])
    }

    /// Get characters active in the current scene.
    pub fn active_characters(&self) -> Vec<&Character> {
        match self.current_scene() {
            Some(scene) => scene
                .active_characters
                .iter()
                .filter_map(|id| self.characters.get(id))
                .collect(),
            None => self.characters.values().collect(),
        }
    }

    /// Get a character by ID.
    pub fn get_character(&self, id: &Uuid) -> Option<&Character> {
        self.characters.get(id)
    }

    /// Get all characters.
    pub fn all_characters(&self) -> Vec<&Character> {
        self.characters.values().collect()
    }

    /// Move to the next scene.
    pub fn next_scene(&mut self) -> Option<&Scene> {
        match self.current_scene_index {
            Some(i) if i + 1 < self.scenes.len() => {
                self.current_scene_index = Some(i + 1);
                Some(&self.scenes[i + 1])
            }
            None if !self.scenes.is_empty() => {
                self.current_scene_index = Some(0);
                Some(&self.scenes[0])
            }
            _ => None,
        }
    }

    /// Get all scenes.
    pub fn scenes(&self) -> &[Scene] {
        &self.scenes
    }

    /// Number of scenes.
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}
