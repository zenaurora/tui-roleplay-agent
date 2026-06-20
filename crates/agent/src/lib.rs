//! Agent framework inspired by LangChain/LangGraph patterns.
//!
//! Provides:
//! - StateGraph: A graph-based state machine for orchestrating agent flows
//! - Chain: Sequential processing pipeline
//! - Tool infrastructure for agent capabilities

pub mod basic_tools;
pub mod chain;
pub mod graph;
pub mod tool_registry;

pub use basic_tools::{basic_tool_registry, ReadFileTool, ToolSandbox, WriteFileTool, ZshTool};
pub use chain::Chain;
pub use graph::{Edge, GraphState, Node, StateGraph};
pub use tool_registry::ToolRegistry;
