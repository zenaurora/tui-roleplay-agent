//! Tool registry - manages available tools for agents.

use rust_agent_core::{Result, Tool, ToolSpec};
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

    /// Create a registry containing only the selected tools.
    pub fn subset(&self, names: &[String]) -> Self {
        let tools = names
            .iter()
            .filter_map(|name| {
                self.tools
                    .get(name)
                    .map(|tool| (name.clone(), tool.clone()))
            })
            .collect();
        Self { tools }
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    fn sorted_tools(&self) -> Vec<&Arc<dyn Tool>> {
        let mut tools = self.tools.values().collect::<Vec<_>>();
        tools.sort_by_key(|tool| tool.name());
        tools
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

    /// Get stable provider-neutral tool specs for prompts or model tool schemas.
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.sorted_tools()
            .into_iter()
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    /// List all tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.sorted_tools()
            .into_iter()
            .map(|tool| tool.name())
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
