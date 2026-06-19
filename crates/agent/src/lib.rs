//! Agent framework inspired by LangChain/LangGraph patterns.
//!
//! Provides:
//! - StateGraph: A graph-based state machine for orchestrating agent flows
//! - Chain: Sequential processing pipeline
//! - Tool infrastructure for agent capabilities

pub mod chain;
pub mod graph;
pub mod tool_registry;

pub use chain::Chain;
pub use graph::{StateGraph, Node, Edge, GraphState};
pub use tool_registry::ToolRegistry;
