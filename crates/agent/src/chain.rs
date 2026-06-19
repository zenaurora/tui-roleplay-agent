//! Chain - a sequential processing pipeline.

use async_trait::async_trait;
use rust_agent_core::{Message, Result};

/// A step in a chain that transforms messages.
#[async_trait]
pub trait ChainStep: Send + Sync {
    /// Process a list of messages and return the transformed list.
    async fn process(&self, messages: Vec<Message>) -> Result<Vec<Message>>;
}

/// A sequential chain of processing steps.
pub struct Chain {
    steps: Vec<Box<dyn ChainStep>>,
}

impl Chain {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Add a step to the chain.
    pub fn add_step(mut self, step: Box<dyn ChainStep>) -> Self {
        self.steps.push(step);
        self
    }

    /// Execute all steps in sequence.
    pub async fn run(&self, mut messages: Vec<Message>) -> Result<Vec<Message>> {
        for step in &self.steps {
            messages = step.process(messages).await?;
        }
        Ok(messages)
    }
}

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}
