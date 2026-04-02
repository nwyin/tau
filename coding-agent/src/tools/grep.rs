use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub struct GrepTool;

impl AgentTool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn label(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using ripgrep. Returns matching lines with file paths and line numbers."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "Directory or file to search in (default: cwd)" },
                    "glob": { "type": "string", "description": "File glob filter, e.g. '*.rs' or '*.{ts,tsx}'" },
                    "ignore_case": { "type": "boolean", "description": "Case-insensitive search (default: false)" },
                    "context": { "type": "number", "description": "Lines of context around each match (default: 0)" },
                    "limit": { "type": "number", "description": "Maximum number of matching lines to return (default: 100)" }
                },
                "required": ["pattern"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let pattern = params["pattern"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?
                .to_string();

            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));

            let search_path = if let Some(path_str) = params["path"].as_str() {
                if std::path::Path::new(path_str).is_absolute() {
                    std::path::PathBuf::from(path_str)
                } else {
                    cwd.join(path_str)
                }
            } else {
                cwd.clone()
            };

            let ignore_case = params["ignore_case"].as_bool().unwrap_or(false);
            let context_lines = params["context"].as_u64().unwrap_or(0) as usize;
            let limit = params["limit"].as_u64().unwrap_or(100) as usize;

            let mut args: Vec<String> = vec![
                "-n".to_string(),
                "--color=never".to_string(),
                "--no-heading".to_string(),
            ];

            if ignore_case {
                args.push("-i".to_string());
            }

            if context_lines > 0 {
                args.push(format!("-C{}", context_lines));
            }

            if let Some(glob_str) = params["glob"].as_str() {
                args.push("--glob".to_string());
                args.push(glob_str.to_string());
            }

            // Use limit * 2 as a buffer to handle context lines; truncate post-hoc
            let max_count = (limit * 2).max(limit + 100);
            args.push(format!("--max-count={}", max_count));

            args.push(pattern);
            args.push(search_path.to_string_lossy().to_string());

            let mut child = tokio::process::Command::new("rg")
                .args(&args)
                .current_dir(&cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let mut stdout = child.stdout.take().expect("stdout piped");
            let mut stderr = child.stderr.take().expect("stderr piped");

            let stdout_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await.ok();
                buf
            });
            let stderr_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                stderr.read_to_end(&mut buf).await.ok();
                buf
            });

            enum Outcome {
                Done(std::process::ExitStatus),
                TimedOut,
                Aborted,
            }

            let outcome = tokio::select! {
                status = child.wait() => {
                    match status {
                        Ok(s) => Outcome::Done(s),
                        Err(_) => Outcome::TimedOut,
                    }
                },
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => Outcome::TimedOut,
                _ = async {
                    if let Some(sig) = &signal {
                        sig.cancelled().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => Outcome::Aborted,
            };

            match outcome {
                Outcome::TimedOut => {
                    let _ = child.kill().await;
                    stdout_task.abort();
                    stderr_task.abort();
                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "Search timed out after 30s".to_string(),
                        }],
                        details: None,
                    })
                }
                Outcome::Aborted => {
                    let _ = child.kill().await;
                    stdout_task.abort();
                    stderr_task.abort();
                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "Search aborted".to_string(),
                        }],
                        details: None,
                    })
                }
                Outcome::Done(status) => {
                    let exit_code = status.code().unwrap_or(-1);

                    // rg exit code 1 = no matches, exit code 2+ = error
                    if exit_code == 1 {
                        stdout_task.abort();
                        stderr_task.abort();
                        return Ok(AgentToolResult {
                            content: vec![UserBlock::Text {
                                text: "No matches found.".to_string(),
                            }],
                            details: Some(json!({
                                "pattern": params["pattern"].as_str().unwrap_or(""),
                                "path": search_path.display().to_string(),
                                "glob": params["glob"].as_str(),
                                "match_count": 0usize,
                                "files_with_matches": 0usize,
                                "truncated": false,
                            })),
                        });
                    }

                    let stdout_bytes = stdout_task.await.unwrap_or_default();
                    let stderr_bytes = stderr_task.await.unwrap_or_default();

                    if exit_code >= 2 {
                        let err_msg = String::from_utf8_lossy(&stderr_bytes).to_string();
                        return Ok(AgentToolResult {
                            content: vec![UserBlock::Text { text: err_msg }],
                            details: None,
                        });
                    }

                    let output = String::from_utf8_lossy(&stdout_bytes);
                    let lines: Vec<&str> = output.lines().collect();
                    let total = lines.len();

                    let files_with_matches_count = {
                        let mut files = std::collections::HashSet::new();
                        for line in &lines {
                            if let Some(colon_pos) = line.find(':') {
                                files.insert(&line[..colon_pos]);
                            }
                        }
                        files.len()
                    };
                    let was_truncated = total > limit;

                    let text = if was_truncated {
                        let shown = lines[..limit].join("\n");
                        format!("{}\n[{} matches, showing first {}]", shown, total, limit)
                    } else {
                        lines.join("\n")
                    };

                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({
                            "pattern": params["pattern"].as_str().unwrap_or(""),
                            "path": search_path.display().to_string(),
                            "glob": params["glob"].as_str(),
                            "match_count": total,
                            "files_with_matches": files_with_matches_count,
                            "truncated": was_truncated,
                        })),
                    })
                }
            }
        })
    }
}

impl GrepTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(GrepTool)
    }
}
