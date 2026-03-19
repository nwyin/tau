use std::sync::Arc;
use std::time::SystemTime;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct GlobTool;

impl AgentTool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn label(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files by glob pattern. Returns matching file paths sorted by modification time (newest first). Respects .gitignore."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern, e.g. '**/*.rs' or 'src/**/*.ts'" },
                    "path": { "type": "string", "description": "Root directory to search from (default: cwd)" }
                },
                "required": ["pattern"]
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
            let pattern = params["pattern"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?
                .to_string();

            let path_str = params["path"].as_str().unwrap_or(".").to_string();
            let path = if std::path::Path::new(&path_str).is_absolute() {
                std::path::PathBuf::from(&path_str)
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                    .join(&path_str)
            };

            let matcher = globset::Glob::new(&pattern)
                .map_err(|e| anyhow::anyhow!("invalid glob pattern '{}': {}", pattern, e))?
                .compile_matcher();

            let root = path.clone();
            let pattern_clone = pattern.clone();

            let matches = tokio::task::spawn_blocking(move || {
                let mut found: Vec<(std::path::PathBuf, SystemTime)> = Vec::new();
                for entry in ignore::WalkBuilder::new(&root).require_git(false).build() {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    // Skip directories
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(true) {
                        continue;
                    }

                    let full_path = entry.path().to_path_buf();
                    // Match against path relative to root
                    let rel = full_path.strip_prefix(&root).unwrap_or(&full_path);
                    if !matcher.is_match(rel) {
                        continue;
                    }

                    let mtime = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(SystemTime::UNIX_EPOCH);

                    found.push((full_path, mtime));
                }
                found
            })
            .await?;

            let total = matches.len();
            if total == 0 {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("No files matched pattern '{}'.", pattern_clone),
                    }],
                    details: None,
                });
            }

            let mut sorted = matches;
            sorted.sort_by(|a, b| b.1.cmp(&a.1)); // newest first

            const MAX_RESULTS: usize = 1000;
            let capped = sorted.len() > MAX_RESULTS;
            let shown = &sorted[..MAX_RESULTS.min(sorted.len())];

            let mut output = String::new();
            for (p, _) in shown {
                let display = p.strip_prefix(&path).unwrap_or(p).display().to_string();
                output.push_str(&display);
                output.push('\n');
            }

            if capped {
                output.push_str(&format!("\n[1000 of {} matches shown]", total));
            }

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: output }],
                details: None,
            })
        })
    }
}

impl GlobTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(GlobTool)
    }
}
