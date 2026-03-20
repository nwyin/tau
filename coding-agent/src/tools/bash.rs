use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub struct BashTool;

impl AgentTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn label(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Run a bash command and return its output"
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The bash command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds (default: 120)" }
                },
                "required": ["command"]
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
            let command = params["command"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'command' parameter"))?
                .to_string();

            let timeout_secs = params["timeout"].as_f64().unwrap_or(120.0) as u64;
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            let start = std::time::Instant::now();

            let mut child = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(&cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            // Take stdout/stderr before borrowing child for wait
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
                        Err(_) => Outcome::Done(
                            // Treat error as non-zero exit
                            std::process::Command::new("false")
                                .status()
                                .unwrap_or_else(|_| {
                                    // fallback: use a dummy status
                                    // We'll handle this as exit code -1 below
                                    unreachable!()
                                })
                        ),
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
                            text: format!("Command timed out after {}s", timeout_secs),
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
                            text: "Command aborted".to_string(),
                        }],
                        details: None,
                    })
                }
                Outcome::Done(status) => {
                    let duration = start.elapsed();
                    let stdout_bytes = stdout_task.await.unwrap_or_default();
                    let stderr_bytes = stderr_task.await.unwrap_or_default();

                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_lines_count = if stdout_str.is_empty() {
                        0
                    } else {
                        stdout_str.lines().count()
                    };
                    let stderr_lines_count = if stderr_str.is_empty() {
                        0
                    } else {
                        stderr_str.lines().count()
                    };

                    let mut combined = format!("{}{}", stdout_str, stderr_str);

                    let exit_code = status.code().unwrap_or(-1);

                    // Truncate: 2000 lines OR 30KB, whichever is hit first
                    let total_lines = combined.lines().count();
                    const MAX_LINES: usize = 2000;
                    const MAX_BYTES: usize = 30 * 1024;

                    let truncated_by_bytes = combined.len() > MAX_BYTES;
                    if truncated_by_bytes {
                        let slice = &combined[..MAX_BYTES.min(combined.len())];
                        let cut = slice.rfind('\n').unwrap_or(slice.len());
                        combined.truncate(cut);
                    }

                    let shown_lines: Vec<&str> = combined.lines().collect();
                    let truncated_by_lines = shown_lines.len() > MAX_LINES;

                    let display_lines: Vec<&str> = if truncated_by_lines {
                        shown_lines[shown_lines.len() - MAX_LINES..].to_vec()
                    } else {
                        shown_lines
                    };

                    let mut text = display_lines.join("\n");
                    if !text.is_empty() {
                        text.push('\n');
                    }

                    if truncated_by_lines || truncated_by_bytes {
                        text.push_str(&format!(
                            "\n[Output truncated: showing last {} lines of {}]",
                            display_lines.len(),
                            total_lines
                        ));
                    }

                    if exit_code != 0 {
                        text.push_str(&format!("\nExit code: {}", exit_code));
                    }

                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({
                            "command": command,
                            "exit_code": exit_code,
                            "duration_ms": duration.as_millis(),
                            "stdout_lines": stdout_lines_count,
                            "stderr_lines": stderr_lines_count,
                        })),
                    })
                }
            }
        })
    }
}

impl BashTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(BashTool)
    }
}
