//! LangGraph-inspired state graph for orchestrating agent flows.

use async_trait::async_trait;
use rust_agent_core::Result;
use serde_json::Value;
use std::collections::HashMap;

/// The state passed through the graph.
#[derive(Debug, Clone, Default)]
pub struct GraphState {
    /// Arbitrary key-value state.
    pub data: HashMap<String, Value>,
}

impl GraphState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.data.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.data
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

/// A node in the state graph that processes state.
#[async_trait]
pub trait Node: Send + Sync {
    /// The unique name of this node.
    fn name(&self) -> &str;

    /// Process the state and return the updated state.
    async fn execute(&self, state: GraphState) -> Result<GraphState>;
}

/// An edge condition that determines the next node.
pub enum Edge {
    /// Always go to the specified node.
    Direct(String),
    /// Conditionally choose the next node based on state.
    Conditional(Box<dyn Fn(&GraphState) -> String + Send + Sync>),
    /// End the graph execution.
    End,
}

/// A state graph that orchestrates node execution.
pub struct StateGraph {
    nodes: HashMap<String, Box<dyn Node>>,
    edges: HashMap<String, Edge>,
    entry_point: String,
}

impl StateGraph {
    /// Create a new state graph with the given entry point.
    pub fn new(entry_point: impl Into<String>) -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_point: entry_point.into(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: Box<dyn Node>) {
        let name = node.name().to_string();
        self.nodes.insert(name, node);
    }

    /// Add a direct edge from one node to another.
    pub fn add_edge(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.edges.insert(from.into(), Edge::Direct(to.into()));
    }

    /// Add a conditional edge from a node.
    pub fn add_conditional_edge(
        &mut self,
        from: impl Into<String>,
        condition: impl Fn(&GraphState) -> String + Send + Sync + 'static,
    ) {
        self.edges
            .insert(from.into(), Edge::Conditional(Box::new(condition)));
    }

    /// Set a node as an end point.
    pub fn set_end(&mut self, node: impl Into<String>) {
        self.edges.insert(node.into(), Edge::End);
    }

    /// Execute the graph starting from the entry point.
    pub async fn run(&self, mut state: GraphState) -> Result<GraphState> {
        let mut current = self.entry_point.clone();

        loop {
            // Execute current node
            let node = self.nodes.get(&current).ok_or_else(|| {
                rust_agent_core::AgentError::Other(format!("Node not found: {}", current))
            })?;

            state = node.execute(state).await?;

            // Determine next node
            match self.edges.get(&current) {
                Some(Edge::Direct(next)) => {
                    current = next.clone();
                }
                Some(Edge::Conditional(condition)) => {
                    current = condition(&state);
                }
                Some(Edge::End) | None => {
                    break;
                }
            }
        }

        Ok(state)
    }
}
