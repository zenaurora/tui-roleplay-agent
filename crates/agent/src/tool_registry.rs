//! Tool registry - manages available tools for agents.

use rust_agent_core::{Result, Tool};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry that holds available tools and provides lookup/execution.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// Execute a tool by name with the given arguments.
    pub async fn execute(&self, name: &str, args: Value) -> Result<String> {
        let tool = self.tools.get(name).ok_or_else(|| {
            rust_agent_core::AgentError::Other(format!("Tool not found: {}", name))
        })?;
        tool.execute(args).await
    }

    /// Get descriptions of all registered tools (for LLM prompts).
    pub fn tool_descriptions(&self) -> Vec<ToolDescription> {
        self.tools
            .values()
            .map(|t| ToolDescription {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    /// List all tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Description of a tool for use in prompts.
#[derive(Debug, Clone)]
pub struct ToolDescription {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
