use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct FileEditTool {
    schema: Value,
}

impl FileEditTool {
    pub fn new() -> Self {
        Self {
            schema: json!({
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
        }
    }

    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self::new())
    }
}

impl Default for FileEditTool {
    fn default() -> Self {
        Self::new()
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
        "Replace an exact string in a file with a new string. The old_string must match exactly (including whitespace) and must appear exactly once."
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
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201c}', '\u{201d}'], "\"")
        .replace('\u{2013}', "-")
        .replace('\u{2014}', "--")
        .replace('\u{2026}', "...")
        .replace(['\u{00a0}', '\u{2002}', '\u{2003}', '\u{2009}'], " ")
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
type NormPass = (&'static str, fn(&str) -> String);

fn fuzzy_find_unique(content: &str, old_string: &str) -> Option<FuzzyMatch> {
    let passes: &[NormPass] = &[
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
            continue;
        }

        let norm_pos = match norm_content.find(&norm_old) {
            Some(p) => p,
            None => continue,
        };

        let start_line = norm_content[..norm_pos].matches('\n').count();
        let old_line_count = norm_old.matches('\n').count() + 1;
        let end_line = start_line + old_line_count;

        if end_line > original_lines.len() {
            continue;
        }

        let orig_start_offset: usize = if start_line == 0 {
            0
        } else {
            original_lines[..start_line]
                .iter()
                .map(|l| l.len() + 1)
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
