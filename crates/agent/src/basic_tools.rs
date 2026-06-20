//! Basic local tools for file IO and shell execution.

use async_trait::async_trait;
use rust_agent_core::{AgentError, Result, Tool};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const MAX_READ_BYTES: u64 = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const ZSH_TIMEOUT_SECS: u64 = 10;

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn path_property(description: &str) -> Value {
    json!({
        "type": "string",
        "description": description
    })
}

#[derive(Debug, Clone)]
pub struct ToolSandbox {
    root: PathBuf,
}

impl ToolSandbox {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let root = std::fs::canonicalize(&root).map_err(|e| {
            AgentError::Config(format!(
                "Failed to canonicalize tool sandbox root {}: {}",
                root.display(),
                e
            ))
        })?;
        Ok(Self { root })
    }

    fn resolve_existing(&self, path: &str) -> Result<PathBuf> {
        let path = self.join_root(path);
        let canonical = std::fs::canonicalize(&path).map_err(|e| {
            AgentError::Other(format!("Failed to resolve path {}: {}", path.display(), e))
        })?;
        self.ensure_in_root(&canonical)?;
        Ok(canonical)
    }

    fn resolve_for_write(&self, path: &str) -> Result<PathBuf> {
        let path = self.join_root(path);
        let parent = path.parent().ok_or_else(|| {
            AgentError::Other(format!("Path has no parent directory: {}", path.display()))
        })?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|e| {
            AgentError::Other(format!(
                "Failed to resolve parent directory {}: {}",
                parent.display(),
                e
            ))
        })?;
        self.ensure_in_root(&canonical_parent)?;

        let resolved = canonical_parent.join(path.file_name().ok_or_else(|| {
            AgentError::Other(format!("Path has no filename: {}", path.display()))
        })?);
        if let Ok(metadata) = std::fs::symlink_metadata(&resolved) {
            if metadata.file_type().is_symlink() {
                return Err(AgentError::Other(format!(
                    "Refusing to write through symlink: {}",
                    resolved.display()
                )));
            }
        }

        Ok(resolved)
    }

    fn join_root(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    fn ensure_in_root(&self, path: &Path) -> Result<()> {
        if path.starts_with(&self.root) {
            Ok(())
        } else {
            Err(AgentError::Other(format!(
                "Path escapes tool sandbox root: {}",
                path.display()
            )))
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone)]
pub struct ReadFileTool {
    sandbox: ToolSandbox,
}

impl ReadFileTool {
    pub fn new(sandbox: ToolSandbox) -> Self {
        Self { sandbox }
    }
}

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    path: String,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a UTF-8 text file from the project workspace."
    }

    fn parameters_schema(&self) -> Value {
        object_schema(
            json!({
                "path": {
                    "type": "string",
                    "description": "File path to read. Relative paths are resolved from the project root."
                }
            }),
            &["path"],
        )
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let args: ReadFileArgs = serde_json::from_value(args)?;
        let path = self.sandbox.resolve_existing(&args.path)?;
        let metadata = tokio::fs::metadata(&path).await?;
        if metadata.len() > MAX_READ_BYTES {
            return Err(AgentError::Other(format!(
                "File is too large to read via tool: {} bytes",
                metadata.len()
            )));
        }

        tokio::fs::read_to_string(&path).await.map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
pub struct WriteFileTool {
    sandbox: ToolSandbox,
}

impl WriteFileTool {
    pub fn new(sandbox: ToolSandbox) -> Self {
        Self { sandbox }
    }
}

#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    path: String,
    content: String,
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write UTF-8 text content to a file inside the project workspace."
    }

    fn parameters_schema(&self) -> Value {
        object_schema(
            json!({
                "path": path_property("File path to write. Relative paths are resolved from the project root."),
                "content": {
                    "type": "string",
                    "description": "Full file content to write."
                }
            }),
            &["path", "content"],
        )
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let args: WriteFileArgs = serde_json::from_value(args)?;
        let path = self.sandbox.resolve_for_write(&args.path)?;
        tokio::fs::write(&path, args.content).await?;
        Ok(format!("Wrote {}", path.display()))
    }
}

#[derive(Debug, Clone)]
pub struct ZshTool {
    sandbox: ToolSandbox,
}

impl ZshTool {
    pub fn new(sandbox: ToolSandbox) -> Self {
        Self { sandbox }
    }
}

#[derive(Debug, Deserialize)]
struct ZshArgs {
    command: String,
}

#[async_trait]
impl Tool for ZshTool {
    fn name(&self) -> &str {
        "zsh"
    }

    fn description(&self) -> &str {
        "Execute a zsh command in the project workspace and return stdout/stderr."
    }

    fn parameters_schema(&self) -> Value {
        object_schema(
            json!({
                "command": {
                    "type": "string",
                    "description": "Command to execute with zsh -lc from the project root."
                }
            }),
            &["command"],
        )
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let args: ZshArgs = serde_json::from_value(args)?;
        let mut command = Command::new("zsh");
        command
            .arg("-lc")
            .arg(&args.command)
            .current_dir(self.sandbox.root())
            .kill_on_drop(true);
        let output = timeout(Duration::from_secs(ZSH_TIMEOUT_SECS), command.output())
            .await
            .map_err(|_| {
                AgentError::Other(format!(
                    "zsh command timed out after {} seconds",
                    ZSH_TIMEOUT_SECS
                ))
            })??;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!(
            "status: {}\nstdout:\n{}\nstderr:\n{}",
            output.status, stdout, stderr
        );

        Ok(truncate_output(&combined))
    }
}

fn truncate_output(output: &str) -> String {
    if output.len() <= MAX_OUTPUT_BYTES {
        output.to_string()
    } else {
        let mut end = MAX_OUTPUT_BYTES;
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        format!(
            "{}\n[truncated: output exceeded {} bytes]",
            &output[..end],
            MAX_OUTPUT_BYTES
        )
    }
}

pub fn basic_tool_registry(root: impl Into<PathBuf>) -> Result<crate::ToolRegistry> {
    let sandbox = ToolSandbox::new(root)?;
    let mut registry = crate::ToolRegistry::new();
    registry.register(Arc::new(ReadFileTool::new(sandbox.clone())));
    registry.register(Arc::new(WriteFileTool::new(sandbox.clone())));
    registry.register(Arc::new(ZshTool::new(sandbox)));
    Ok(registry)
}
