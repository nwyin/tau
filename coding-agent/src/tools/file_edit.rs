use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::hashline;
use crate::config::EditMode;

pub struct FileEditTool {
    mode: EditMode,
    description: String,
    schema: Value,
}

struct ParsedEdit {
    op: String,
    pos: Option<hashline::Anchor>,
    end: Option<hashline::Anchor>,
    lines: Vec<String>,
}

impl FileEditTool {
    pub fn new(mode: EditMode) -> Self {
        let (description, schema) = match mode {
            EditMode::Replace => (
                "Replace an exact string in a file with a new string. The old_string must match exactly (including whitespace) and must appear exactly once.".to_string(),
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
                }),
            ),
            EditMode::Hashline => (
                concat!(
                    "Applies precise file edits using LINE#HASH tags from file_read output.\n\n",
                    "WORKFLOW:\n",
                    "1. You MUST issue a file_read call before editing if you have no tagged context.\n",
                    "2. Pick the smallest operation per change site.\n",
                    "3. Submit one file_edit call per file with all operations.\n\n",
                    "OPERATIONS:\n",
                    "- path: the file to edit.\n",
                    "- edits[n].op: \"replace\", \"append\", or \"prepend\".\n",
                    "- edits[n].pos: the anchor line as \"NUM#HASH\". For replace: start of range. For prepend: insert before. For append: insert after. Omit for file boundaries.\n",
                    "- edits[n].end: range replace only — last line (inclusive). Omit for single-line replace.\n",
                    "- edits[n].lines: replacement content as array of strings. Use null or [] to delete lines.\n\n",
                    "RULES:\n",
                    "- Every tag MUST be copied exactly from a fresh file_read result as NUM#HASH.\n",
                    "- Edits are applied bottom-up, so earlier tags stay valid even when later ops add/remove lines.\n",
                    "- lines entries MUST be literal file content — copy indentation exactly from the read output.\n",
                    "- You MUST re-read after each edit call before issuing another on the same file.\n\n",
                    "RECOVERY:\n",
                    "- Tag mismatch error: retry using fresh tags from a new file_read.\n",
                    "- No-op (identical content): do NOT resubmit. Re-read and adjust.\n\n",
                    "EXAMPLE (single-line replace):\n",
                    "If file_read shows: 23#VP:  const timeout = 5000;\n",
                    "To change the value: {edits: [{op: \"replace\", pos: \"23#VP\", lines: [\"  const timeout = 30000;\"]}]}\n\n",
                    "EXAMPLE (range replace):\n",
                    "To replace lines 10-12: {edits: [{op: \"replace\", pos: \"10#ZM\", end: \"12#KT\", lines: [\"  new line 1\", \"  new line 2\"]}]}\n\n",
                    "EXAMPLE (insert after):\n",
                    "To add a line after line 5: {edits: [{op: \"append\", pos: \"5#QR\", lines: [\"  new_line = True\"]}]}\n\n",
                    "EXAMPLE (delete line):\n",
                    "To delete line 8: {edits: [{op: \"replace\", pos: \"8#SN\", lines: []}]}"
                )
                .to_string(),
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
                }),
            ),
        };
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

impl Default for FileEditTool {
    fn default() -> Self {
        Self::new(EditMode::Replace)
    }
}

impl AgentTool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn label(&self) -> &str {
        "Edit File"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> &Value {
        &self.schema
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        match self.mode {
            EditMode::Replace => execute_replace(tool_call_id, params, signal, on_update),
            EditMode::Hashline => execute_hashline(tool_call_id, params, signal, on_update),
        }
    }
}

// ---------------------------------------------------------------------------
// Replace mode execution
// ---------------------------------------------------------------------------

fn execute_replace(
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

        let path = resolve_path(path_str);

        if !path.exists() {
            return Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: format!("File not found: {}", path.display()),
                }],
                details: None,
            });
        }

        let content = match read_utf8(&path) {
            Ok(s) => s,
            Err(e) => {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: e.to_string(),
                    }],
                    details: None,
                });
            }
        };

        let count = content.matches(old_string).count();

        if count == 0 {
            // Fuzzy fallback: try normalized matching cascade
            match fuzzy_find_unique(&content, old_string) {
                Some(fuzzy) => {
                    let old_bytes = content.len();
                    let new_content = format!(
                        "{}{}{}",
                        &content[..fuzzy.offset],
                        new_string,
                        &content[fuzzy.offset + fuzzy.length..]
                    );
                    let new_bytes = new_content.len();

                    return match std::fs::write(&path, &new_content) {
                        Ok(()) => Ok(AgentToolResult {
                            content: vec![UserBlock::Text {
                                text: format!(
                                    "Replaced 1 occurrence in {} (matched via {}). {} → {} bytes",
                                    path.display(),
                                    fuzzy.strategy,
                                    old_bytes,
                                    new_bytes
                                ),
                            }],
                            details: Some(json!({
                                "path": path.display().to_string(),
                                "success": true,
                                "replacements": 1,
                                "match_strategy": fuzzy.strategy,
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
                    };
                }
                None => {
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
            }
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

        // Exactly one exact match — perform the replacement
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

// ---------------------------------------------------------------------------
// Hashline mode execution
// ---------------------------------------------------------------------------

fn execute_hashline(
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

        let path = resolve_path(path_str);

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

        let content = match read_utf8(&path) {
            Ok(s) => s,
            Err(e) => {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: e.to_string(),
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

// ---------------------------------------------------------------------------
// Fuzzy matching cascade (trimmed-cascade strategy)
// ---------------------------------------------------------------------------

/// Strip trailing whitespace from each line.
fn normalize_trim_end(text: &str) -> String {
    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip leading + trailing whitespace from each line.
fn normalize_trim_both(text: &str) -> String {
    text.lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip trailing whitespace + replace unicode punctuation with ASCII equivalents.
fn normalize_unicode(text: &str) -> String {
    let trimmed = normalize_trim_end(text);
    trimmed
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201c}', "\"")
        .replace('\u{201d}', "\"")
        .replace('\u{2013}', "-")
        .replace('\u{2014}', "--")
        .replace('\u{2026}', "...")
        .replace('\u{00a0}', " ")
        .replace('\u{2002}', " ")
        .replace('\u{2003}', " ")
        .replace('\u{2009}', " ")
}

/// Result of a successful fuzzy match.
struct FuzzyMatch {
    /// Byte offset in the original content where the match starts.
    offset: usize,
    /// Length of the matched span in the original content.
    length: usize,
    /// Name of the normalization strategy that found the match.
    strategy: &'static str,
}

/// Try progressively looser normalizations to find `old_string` in `content`.
///
/// Returns a `FuzzyMatch` if exactly one match is found by any normalization
/// pass. Each pass normalizes both content and old_string symmetrically, finds
/// the match position in normalized space, then maps it back to the original
/// content by line index.
fn fuzzy_find_unique(content: &str, old_string: &str) -> Option<FuzzyMatch> {
    let passes: &[(&'static str, fn(&str) -> String)] = &[
        ("trim_end", normalize_trim_end),
        ("trim_both", normalize_trim_both),
        ("unicode", normalize_unicode),
    ];

    let original_lines: Vec<&str> = content.lines().collect();

    for &(strategy_name, normalize_fn) in passes {
        let norm_content = normalize_fn(content);
        let norm_old = normalize_fn(old_string);

        if norm_old.is_empty() {
            continue;
        }

        let match_count = norm_content.matches(&norm_old).count();

        if match_count != 1 {
            // 0 = not found, >1 = ambiguous — try next pass
            continue;
        }

        // Exactly one match in normalized space. Map back to original by line index.
        let norm_pos = match norm_content.find(&norm_old) {
            Some(p) => p,
            None => continue,
        };

        // Count which line the normalized match starts on
        let start_line = norm_content[..norm_pos].matches('\n').count();
        let old_line_count = norm_old.matches('\n').count() + 1;
        let end_line = start_line + old_line_count;

        if end_line > original_lines.len() {
            continue;
        }

        // Extract the corresponding span from the original content
        let orig_start_offset: usize = if start_line == 0 {
            0
        } else {
            // Sum of bytes for lines 0..start_line, plus their newline separators
            original_lines[..start_line]
                .iter()
                .map(|l| l.len() + 1) // +1 for '\n'
                .sum()
        };

        let matched_original_text = original_lines[start_line..end_line].join("\n");

        return Some(FuzzyMatch {
            offset: orig_start_offset,
            length: matched_original_text.len(),
            strategy: strategy_name,
        });
    }

    None
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn resolve_path(path_str: &str) -> std::path::PathBuf {
    if std::path::Path::new(path_str).is_absolute() {
        std::path::PathBuf::from(path_str)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .join(path_str)
    }
}

fn read_utf8(path: &std::path::Path) -> std::result::Result<String, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    String::from_utf8(raw).map_err(|_| "File appears to be binary".to_string())
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
