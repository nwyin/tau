//! Sub-agent tool: delegate a task to an independent tau subprocess.
//!
//! Spawns `tau -p "<task>" --yolo --no-skills --no-session` as a child process,
//! captures stdout, and returns the result. Each sub-agent starts with a fresh
//! context window and the default tool set (minus the subagent tool itself, to
//! prevent unbounded recursion).

use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub struct SubagentTool;

impl SubagentTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn label(&self) -> &str {
        "Sub-Agent"
    }

    fn description(&self) -> &str {
        "Delegate a task to a sub-agent running in a separate context. The sub-agent is a fresh tau instance with the same tools (except subagent) but no shared conversation history. Use for: (1) exploratory research that would clutter your context, (2) well-defined subtasks like analyzing a file or searching a codebase, (3) tasks that benefit from a clean context window. Give clear, self-contained task descriptions since the sub-agent cannot see your conversation."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Complete task description for the sub-agent. Must be self-contained — the sub-agent has no access to your conversation history."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model override for the sub-agent (e.g. 'gpt-4o-mini' for cheap exploration). Defaults to the same model as the parent."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 300)"
                    }
                },
                "required": ["task"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let task = params["task"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'task' parameter"))?
                .to_string();

            let model = params["model"].as_str().map(|s| s.to_string());
            let timeout_secs = params["timeout"].as_u64().unwrap_or(300);

            // Find the current executable
            let exe = std::env::current_exe()
                .map_err(|e| anyhow::anyhow!("cannot determine tau executable path: {}", e))?;

            // Build the command
            let mut cmd = tokio::process::Command::new(&exe);
            cmd.arg("-p").arg(&task);
            cmd.arg("--yolo");
            cmd.arg("--no-skills");
            cmd.arg("--no-session");

            // Exclude the subagent tool to prevent unbounded recursion
            let default_tools =
                "bash,file_read,file_edit,file_write,glob,grep,web_fetch,web_search";
            cmd.arg("--tools").arg(default_tools);

            if let Some(ref m) = model {
                cmd.arg("-m").arg(m);
            }

            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            cmd.current_dir(&cwd);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let start = std::time::Instant::now();
            let mut child = cmd
                .spawn()
                .map_err(|e| anyhow::anyhow!("failed to spawn sub-agent: {}", e))?;

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

            let timeout_dur = tokio::time::Duration::from_secs(timeout_secs);

            enum Outcome {
                Done(std::process::ExitStatus),
                TimedOut,
                Aborted,
            }

            let outcome = tokio::select! {
                status = child.wait() => {
                    match status {
                        Ok(s) => Outcome::Done(s),
                        Err(e) => {
                            return Ok(AgentToolResult {
                                content: vec![UserBlock::Text {
                                    text: format!("Sub-agent process error: {}", e),
                                }],
                                details: None,
                            });
                        }
                    }
                },
                _ = tokio::time::sleep(timeout_dur) => Outcome::TimedOut,
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
                            text: format!("Sub-agent timed out after {}s", timeout_secs),
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
                            text: "Sub-agent aborted".to_string(),
                        }],
                        details: None,
                    })
                }
                Outcome::Done(status) => {
                    let duration = start.elapsed();
                    let stdout_bytes = stdout_task.await.unwrap_or_default();
                    let stderr_bytes = stderr_task.await.unwrap_or_default();

                    let output = String::from_utf8_lossy(&stdout_bytes);
                    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
                    let exit_code = status.code().unwrap_or(-1);

                    // Truncate output: 2000 lines or 50KB
                    let mut text = output.to_string();
                    const MAX_LINES: usize = 2000;
                    const MAX_BYTES: usize = 50 * 1024;

                    let original_len = text.len();
                    let truncated_by_bytes = text.len() > MAX_BYTES;
                    if truncated_by_bytes {
                        let slice = &text[..MAX_BYTES.min(text.len())];
                        let cut = slice.rfind('\n').unwrap_or(slice.len());
                        text.truncate(cut);
                    }

                    let lines: Vec<&str> = text.lines().collect();
                    let total_lines = lines.len();
                    let truncated_by_lines = total_lines > MAX_LINES;
                    if truncated_by_lines {
                        text = lines[..MAX_LINES].join("\n");
                    }

                    if truncated_by_bytes || truncated_by_lines {
                        text.push_str(&format!(
                            "\n\n[Sub-agent output truncated: {} bytes, {} lines shown]",
                            original_len.min(MAX_BYTES),
                            total_lines.min(MAX_LINES),
                        ));
                    }

                    if text.trim().is_empty() {
                        text = "(sub-agent produced no output)".to_string();
                        // Include stderr if available for debugging
                        let stderr_trimmed = stderr_str.trim();
                        if !stderr_trimmed.is_empty() {
                            let stderr_preview: String = stderr_trimmed
                                .lines()
                                .take(20)
                                .collect::<Vec<_>>()
                                .join("\n");
                            text.push_str(&format!("\n\nStderr:\n{}", stderr_preview));
                        }
                    }

                    if exit_code != 0 {
                        text.push_str(&format!("\n\n[Sub-agent exited with code {}]", exit_code));
                    }

                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({
                            "exit_code": exit_code,
                            "duration_ms": duration.as_millis() as u64,
                            "model": model,
                        })),
                    })
                }
            }
        })
    }
}
