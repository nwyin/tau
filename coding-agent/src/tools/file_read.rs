use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::EditMode;

pub struct FileReadTool {
    mode: EditMode,
    description: String,
    schema: Value,
}

impl FileReadTool {
    pub fn new(mode: EditMode) -> Self {
        let description = match mode {
            EditMode::Replace => "Read the contents of a text file.".to_string(),
            EditMode::Hashline => concat!(
                "Read file contents with hash-tagged line references.\n\n",
                "Output format: each line is prefixed with LINE#HASH:content ",
                "(e.g. \"23#VP:  const x = 1;\").\n",
                "Use the NUM#HASH tags as anchors when calling file_edit.\n\n",
                "- Use offset and limit for large files.\n",
                "- You MUST use file_read instead of bash for ALL file reading ",
                "(cat, head, tail are forbidden).\n",
                "- Re-read a file after editing it — file_edit rejects stale hashes."
            )
            .to_string(),
        };
        let schema = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative path to the file to read" },
                "offset": { "type": "number", "description": "Line number to start reading from (1-indexed)" },
                "limit": { "type": "number", "description": "Maximum number of lines to read (default: 2000)" }
            },
            "required": ["path"]
        });
        Self {
            mode,
            description,
            schema,
        }
    }

    pub fn arc(mode: EditMode) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(mode))
    }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new(EditMode::Replace)
    }
}

impl AgentTool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn label(&self) -> &str {
        "Read File"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> &Value {
        &self.schema
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        let mode = self.mode.clone();
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
                        text: format!("Offset {} exceeds file length ({} lines)", offset, total),
                    }],
                    details: None,
                });
            }

            let start_idx = offset.saturating_sub(1); // convert to 0-indexed
            let end_idx = (start_idx + limit).min(total);

            let selected = &all_lines[start_idx..end_idx];

            // Format output based on edit mode
            let mut output = match mode {
                EditMode::Hashline => {
                    let selected_text = selected.join("\n");
                    let formatted =
                        super::hashline::format_hash_lines(&selected_text, start_idx + 1);
                    formatted + "\n"
                }
                EditMode::Replace => {
                    let mut buf = String::new();
                    for (i, line) in selected.iter().enumerate() {
                        let line_num = start_idx + i + 1; // 1-indexed
                        buf.push_str(&format!("{}\t{}\n", line_num, line));
                    }
                    buf
                }
            };

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
                details: Some(json!({
                    "path": path.display().to_string(),
                    "offset": offset,
                    "limit": limit,
                    "lines_returned": end_idx - start_idx,
                    "total_lines": total,
                })),
            })
        })
    }
}
