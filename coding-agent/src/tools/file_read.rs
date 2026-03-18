use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct FileReadTool;

impl AgentTool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn label(&self) -> &str {
        "Read File"
    }

    fn description(&self) -> &str {
        "Read the contents of a text file"
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative path to the file to read" },
                    "offset": { "type": "number", "description": "Line number to start reading from (1-indexed)" },
                    "limit": { "type": "number", "description": "Maximum number of lines to read (default: 2000)" }
                },
                "required": ["path"]
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

            if content.is_empty() {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: "(empty file)".to_string(),
                    }],
                    details: None,
                });
            }

            let all_lines: Vec<&str> = content.lines().collect();
            let total = all_lines.len();

            let offset = params["offset"].as_u64().unwrap_or(1) as usize;
            let limit = params["limit"].as_u64().unwrap_or(2000) as usize;

            // offset is 1-indexed
            if offset > total {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!(
                            "Offset {} exceeds file length ({} lines)",
                            offset, total
                        ),
                    }],
                    details: None,
                });
            }

            let start_idx = offset.saturating_sub(1); // convert to 0-indexed
            let end_idx = (start_idx + limit).min(total);

            let selected = &all_lines[start_idx..end_idx];
            let mut output = String::new();

            for (i, line) in selected.iter().enumerate() {
                let line_num = start_idx + i + 1; // 1-indexed
                output.push_str(&format!("{}\t{}\n", line_num, line));
            }

            if end_idx < total {
                output.push_str(&format!(
                    "\n[Showing lines {}-{} of {}. Use offset={} to continue.]",
                    start_idx + 1,
                    end_idx,
                    total,
                    end_idx + 1
                ));
            }

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: output }],
                details: None,
            })
        })
    }
}

impl FileReadTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(FileReadTool)
    }
}
