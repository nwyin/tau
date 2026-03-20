use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct FileEditTool;

impl AgentTool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn label(&self) -> &str {
        "Edit File"
    }

    fn description(&self) -> &str {
        "Replace an exact string in a file with a new string. The old_string must match exactly (including whitespace) and must appear exactly once."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact string to find and replace (must match exactly including whitespace)"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Replacement string (can be empty to delete the matched text)"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let path_str = params["path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;
            let old_string = params["old_string"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'old_string' parameter"))?;
            let new_string = params["new_string"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'new_string' parameter"))?;

            // Empty old_string is ambiguous — reject it
            if old_string.is_empty() {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: "old_string must not be empty".to_string(),
                    }],
                    details: None,
                });
            }

            let path = if std::path::Path::new(path_str).is_absolute() {
                std::path::PathBuf::from(path_str)
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                    .join(path_str)
            };

            if !path.exists() {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("File not found: {}", path.display()),
                    }],
                    details: None,
                });
            }

            let raw = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: e.to_string(),
                        }],
                        details: None,
                    });
                }
            };

            let content = match String::from_utf8(raw) {
                Ok(s) => s,
                Err(_) => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "File appears to be binary".to_string(),
                        }],
                        details: None,
                    });
                }
            };

            let count = content.matches(old_string).count();

            if count == 0 {
                // Provide surrounding context to help the caller diagnose stale context
                let context = build_not_found_context(&content);
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!(
                            "old_string not found in {}.\n\nFile context (first ~10 lines):\n{}",
                            path.display(),
                            context
                        ),
                    }],
                    details: Some(json!({
                        "path": path.display().to_string(),
                        "success": false,
                        "replacements": 0,
                    })),
                });
            }

            if count > 1 {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!(
                            "Found {} occurrences of old_string; must be exactly 1",
                            count
                        ),
                    }],
                    details: Some(json!({
                        "path": path.display().to_string(),
                        "success": false,
                        "replacements": 0,
                    })),
                });
            }

            // Exactly one match — perform the replacement
            let old_bytes = content.len();
            let new_content = content.replacen(old_string, new_string, 1);
            let new_bytes = new_content.len();

            match std::fs::write(&path, &new_content) {
                Ok(()) => Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!(
                            "Replaced 1 occurrence in {}. {} → {} bytes",
                            path.display(),
                            old_bytes,
                            new_bytes
                        ),
                    }],
                    details: Some(json!({
                        "path": path.display().to_string(),
                        "success": true,
                        "replacements": 1,
                    })),
                }),
                Err(e) => Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: e.to_string(),
                    }],
                    details: Some(json!({
                        "path": path.display().to_string(),
                        "success": false,
                        "replacements": 0,
                    })),
                }),
            }
        })
    }
}

impl FileEditTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(FileEditTool)
    }
}

/// Build a short context snippet from the beginning of the file for error messages.
fn build_not_found_context(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let take = lines.len().min(10);
    lines[..take]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{}\t{}", i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}
