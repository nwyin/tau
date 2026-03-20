use std::sync::Arc;
use std::time::Instant;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub struct RunTestsTool {
    command: Option<String>,
}

impl RunTestsTool {
    pub fn new(command: Option<String>) -> Self {
        RunTestsTool { command }
    }

    pub fn arc(command: Option<String>) -> Arc<dyn AgentTool> {
        Arc::new(RunTestsTool::new(command))
    }
}

impl AgentTool for RunTestsTool {
    fn name(&self) -> &str {
        "run_tests"
    }

    fn label(&self) -> &str {
        "Run Tests"
    }

    fn description(&self) -> &str {
        "Run the configured test command. The command is set by the harness, not the model."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {}
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        let command = self.command.clone();
        Box::pin(async move {
            let command = command.ok_or_else(|| {
                anyhow::anyhow!(
                    "No test command configured. Set TAU_BENCHMARK_TEST_CMD or use --test-command."
                )
            })?;

            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));

            let start = Instant::now();

            let mut child = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(&cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to spawn test command: {}", e))?;

            let mut stdout_pipe = child.stdout.take().expect("stdout piped");
            let mut stderr_pipe = child.stderr.take().expect("stderr piped");

            let stdout_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                stdout_pipe.read_to_end(&mut buf).await.ok();
                buf
            });
            let stderr_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                stderr_pipe.read_to_end(&mut buf).await.ok();
                buf
            });

            let status = child.wait().await?;
            let duration = start.elapsed();

            let stdout_bytes = stdout_task.await.unwrap_or_default();
            let stderr_bytes = stderr_task.await.unwrap_or_default();

            let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
            let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
            let exit_code = status.code().unwrap_or(-1);

            // Build combined output (stdout then stderr)
            let mut combined = String::new();
            combined.push_str(&stdout);
            if !stderr.is_empty() {
                combined.push_str(&stderr);
            }

            // Truncate: 2000 lines OR 30KB, whichever is hit first (same as BashTool)
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

            text.push_str(&format!("\nExit code: {}", exit_code));

            let stdout_lines = stdout.lines().count();
            let stderr_lines = stderr.lines().count();

            let details = Some(json!({
                "command": command,
                "exit_code": exit_code,
                "duration_ms": duration.as_millis(),
                "stdout_lines": stdout_lines,
                "stderr_lines": stderr_lines,
                "passed": exit_code == 0,
            }));

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details,
            })
        })
    }
}
