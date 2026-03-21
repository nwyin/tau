use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::hashline;

pub struct HashFileEditTool;

struct ParsedEdit {
    op: String,
    pos: Option<hashline::Anchor>,
    end: Option<hashline::Anchor>,
    lines: Vec<String>,
}

impl AgentTool for HashFileEditTool {
    fn name(&self) -> &str {
        "hash_file_edit"
    }

    fn label(&self) -> &str {
        "Edit File (Hash)"
    }

    fn description(&self) -> &str {
        concat!(
            "Applies precise file edits using LINE#HASH tags from hash_file_read output.\n\n",
            "WORKFLOW:\n",
            "1. You MUST issue a hash_file_read call before editing if you have no tagged context.\n",
            "2. Pick the smallest operation per change site.\n",
            "3. Submit one hash_file_edit call per file with all operations.\n\n",
            "OPERATIONS:\n",
            "- path: the file to edit.\n",
            "- edits[n].op: \"replace\", \"append\", or \"prepend\".\n",
            "- edits[n].pos: the anchor line as \"NUM#HASH\". For replace: start of range. For prepend: insert before. For append: insert after. Omit for file boundaries.\n",
            "- edits[n].end: range replace only — last line (inclusive). Omit for single-line replace.\n",
            "- edits[n].lines: replacement content as array of strings. Use null or [] to delete lines.\n\n",
            "RULES:\n",
            "- Every tag MUST be copied exactly from a fresh hash_file_read result as NUM#HASH.\n",
            "- Edits are applied bottom-up, so earlier tags stay valid even when later ops add/remove lines.\n",
            "- lines entries MUST be literal file content — copy indentation exactly from the read output.\n",
            "- You MUST re-read after each edit call before issuing another on the same file.\n\n",
            "RECOVERY:\n",
            "- Tag mismatch error: retry using fresh tags from a new hash_file_read.\n",
            "- No-op (identical content): do NOT resubmit. Re-read and adjust.\n\n",
            "EXAMPLE (single-line replace):\n",
            "If hash_file_read shows: 23#VP:  const timeout = 5000;\n",
            "To change the value: {edits: [{op: \"replace\", pos: \"23#VP\", lines: [\"  const timeout = 30000;\"]}]}\n\n",
            "EXAMPLE (range replace):\n",
            "To replace lines 10-12: {edits: [{op: \"replace\", pos: \"10#ZM\", end: \"12#KT\", lines: [\"  new line 1\", \"  new line 2\"]}]}\n\n",
            "EXAMPLE (insert after):\n",
            "To add a line after line 5: {edits: [{op: \"append\", pos: \"5#QR\", lines: [\"  new_line = True\"]}]}\n\n",
            "EXAMPLE (delete line):\n",
            "To delete line 8: {edits: [{op: \"replace\", pos: \"8#SN\", lines: []}]}"
        )
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
                    "edits": {
                        "type": "array",
                        "description": "List of edit operations to apply",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": {
                                    "type": "string",
                                    "enum": ["replace", "append", "prepend"],
                                    "description": "Operation type"
                                },
                                "pos": {
                                    "type": "string",
                                    "description": "NUM#HASH anchor for the target line"
                                },
                                "end": {
                                    "type": "string",
                                    "description": "NUM#HASH anchor for end of range (replace only)"
                                },
                                "lines": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Replacement/insertion lines"
                                }
                            },
                            "required": ["op", "lines"]
                        }
                    }
                },
                "required": ["path", "edits"]
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
            // 1. Parse path parameter
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

            // 2. Parse edits array
            let edits_val = params["edits"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing 'edits' parameter"))?;

            // 3. Read file, check existence and UTF-8
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

            // 4. Split into lines
            let mut file_lines: Vec<String> = content.lines().map(String::from).collect();

            // 5. Early return if no edits
            if edits_val.is_empty() {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: "No edits to apply".to_string(),
                    }],
                    details: None,
                });
            }

            // 6. Parse each edit
            let mut parsed_edits: Vec<ParsedEdit> = Vec::with_capacity(edits_val.len());
            for edit_val in edits_val {
                let op = match edit_val["op"].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        return Ok(AgentToolResult {
                            content: vec![UserBlock::Text {
                                text: "Each edit must have an 'op' field".to_string(),
                            }],
                            details: None,
                        });
                    }
                };

                let pos = match edit_val["pos"].as_str() {
                    Some(s) => Some(hashline::parse_tag(s)?),
                    None => None,
                };

                let end = match edit_val["end"].as_str() {
                    Some(s) => Some(hashline::parse_tag(s)?),
                    None => None,
                };

                let lines_arr = match edit_val["lines"].as_array() {
                    Some(arr) => arr
                        .iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect::<Vec<String>>(),
                    None => {
                        return Ok(AgentToolResult {
                            content: vec![UserBlock::Text {
                                text: "Each edit must have a 'lines' array".to_string(),
                            }],
                            details: None,
                        });
                    }
                };

                parsed_edits.push(ParsedEdit {
                    op,
                    pos,
                    end,
                    lines: lines_arr,
                });
            }

            // 7. Collect all anchors and validate them
            let file_line_refs: Vec<&str> = file_lines.iter().map(|s| s.as_str()).collect();
            let mut all_anchors: Vec<&hashline::Anchor> = Vec::new();
            for edit in &parsed_edits {
                if let Some(ref a) = edit.pos {
                    all_anchors.push(a);
                }
                if let Some(ref a) = edit.end {
                    all_anchors.push(a);
                }
            }

            // 8. If validation fails, return error with no mutations
            if let Err(mismatches) = hashline::validate_all_refs(&all_anchors, &file_line_refs) {
                let err_msg = hashline::format_mismatch_error(&mismatches, &file_line_refs);
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text { text: err_msg }],
                    details: None,
                });
            }

            // 9. Strip hash prefixes from replacement lines
            for edit in &mut parsed_edits {
                edit.lines = hashline::strip_hash_prefixes(&edit.lines);
            }

            // 10. Sort edits by primary anchor line number descending (bottom-up)
            parsed_edits.sort_by(|a, b| {
                let a_line = a.pos.as_ref().map(|p| p.line).unwrap_or(0);
                let b_line = b.pos.as_ref().map(|p| p.line).unwrap_or(0);
                b_line.cmp(&a_line)
            });

            // 11. Apply each edit
            let edit_count = parsed_edits.len();
            for edit in &parsed_edits {
                let new_lines = edit.lines.clone();

                match edit.op.as_str() {
                    "replace" => {
                        if let Some(ref pos_anchor) = edit.pos {
                            let pos_line = pos_anchor.line;
                            if let Some(ref end_anchor) = edit.end {
                                // Range replace: pos..=end
                                let end_line = end_anchor.line;
                                if end_line < pos_line {
                                    return Ok(AgentToolResult {
                                        content: vec![UserBlock::Text {
                                            text: format!(
                                                "Invalid range: end line {} is before pos line {}",
                                                end_line, pos_line
                                            ),
                                        }],
                                        details: None,
                                    });
                                }
                                file_lines.splice(pos_line - 1..=end_line - 1, new_lines);
                            } else {
                                // Single line replace
                                file_lines.splice(pos_line - 1..pos_line, new_lines);
                            }
                        } else {
                            return Ok(AgentToolResult {
                                content: vec![UserBlock::Text {
                                    text: "replace op requires a 'pos' anchor".to_string(),
                                }],
                                details: None,
                            });
                        }
                    }
                    "append" => {
                        if let Some(ref pos_anchor) = edit.pos {
                            let pos_line = pos_anchor.line;
                            file_lines.splice(pos_line..pos_line, new_lines);
                        } else {
                            // Append at end of file
                            file_lines.extend(new_lines);
                        }
                    }
                    "prepend" => {
                        if let Some(ref pos_anchor) = edit.pos {
                            let pos_line = pos_anchor.line;
                            file_lines.splice(pos_line - 1..pos_line - 1, new_lines);
                        } else {
                            // Prepend at beginning of file
                            file_lines.splice(0..0, new_lines);
                        }
                    }
                    other => {
                        return Ok(AgentToolResult {
                            content: vec![UserBlock::Text {
                                text: format!(
                                    "Unknown op '{}'. Must be replace, append, or prepend",
                                    other
                                ),
                            }],
                            details: None,
                        });
                    }
                }
            }

            // 12. Join lines, preserve trailing newline
            let has_trailing_newline = content.ends_with('\n');
            let mut result = file_lines.join("\n");
            if has_trailing_newline {
                result.push('\n');
            }

            // 13. Write file
            if let Err(e) = std::fs::write(&path, &result) {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: e.to_string(),
                    }],
                    details: Some(json!({
                        "path": path.display().to_string(),
                        "success": false,
                        "replacements": 0,
                    })),
                });
            }

            // 14. Return success
            Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: format!("Applied {} edit(s) to {}", edit_count, path.display()),
                }],
                details: Some(json!({
                    "path": path.display().to_string(),
                    "success": true,
                    "replacements": edit_count,
                })),
            })
        })
    }
}

impl HashFileEditTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(HashFileEditTool)
    }
}
