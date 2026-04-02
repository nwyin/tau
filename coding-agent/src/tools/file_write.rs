use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct FileWriteTool;

impl AgentTool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn label(&self) -> &str {
        "Write File"
    }

    fn description(&self) -> &str {
        "Write content to a file, creating it if it doesn't exist"
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative path to write to" },
                    "content": { "type": "string", "description": "Content to write to the file" }
                },
                "required": ["path", "content"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let path_str = params["path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;
            let content = params["content"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'content' parameter"))?;

            let path = if std::path::Path::new(path_str).is_absolute() {
                std::path::PathBuf::from(path_str)
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                    .join(path_str)
            };

            let path_existed_before = path.exists();

            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: format!("Failed to create directories: {}", e),
                        }],
                        details: None,
                    });
                }
            }

            match std::fs::write(&path, content) {
                Ok(()) => {
                    let created = !path_existed_before;
                    let mut details = json!({
                        "path": path.display().to_string(),
                        "bytes_written": content.len(),
                        "created": created,
                    });
                    // Include file content for new-file diff rendering (truncated)
                    if created {
                        let lines: Vec<&str> = content.lines().collect();
                        let total = lines.len();
                        let preview: Vec<&str> = lines.into_iter().take(50).collect();
                        details["new_content"] = json!(preview.join("\n"));
                        details["total_lines"] = json!(total);
                    }
                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: format!("Wrote {} bytes to {}", content.len(), path.display()),
                        }],
                        details: Some(details),
                    })
                }
                Err(e) => Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: e.to_string(),
                    }],
                    details: None,
                }),
            }
        })
    }
}

impl FileWriteTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(FileWriteTool)
    }
}
