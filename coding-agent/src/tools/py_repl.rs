//! Python REPL tool: persistent Python subprocess with reverse RPC for orchestration.
//!
//! The LLM writes Python code that runs in a long-lived subprocess. A `tau` object
//! in the Python namespace provides blocking APIs for tool calls, thread spawning,
//! queries, parallel execution, and document sharing. Communication is bidirectional
//! JSON-lines over stdin/stdout.

use std::collections::HashMap;
use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::sync::CancellationToken;

use crate::orchestration::{OrchestrationRpcFacade, OrchestrationRuntime};

const PY_KERNEL_SOURCE: &str = include_str!("../../prompts/py_kernel.py");

/// Default timeout for py_repl cells (seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Max output lines returned to the LLM.
const MAX_OUTPUT_LINES: usize = 2000;

/// Max output bytes returned to the LLM.
const MAX_OUTPUT_BYTES: usize = 30_000;

struct KernelProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    kernel_path: std::path::PathBuf,
}

impl Drop for KernelProcess {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.kernel_path);
    }
}

pub struct PyReplTool {
    rpc_facade: OrchestrationRpcFacade,
    // Long-lived kernel subprocess
    kernel: Arc<tokio::sync::Mutex<Option<KernelProcess>>>,
    // Cell counter for IDs
    cell_counter: std::sync::atomic::AtomicU64,
}

impl PyReplTool {
    pub fn new(
        orchestrator: Arc<OrchestratorState>,
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
        generic_tools: HashMap<String, Arc<dyn AgentTool>>,
    ) -> Self {
        Self {
            rpc_facade: OrchestrationRpcFacade::new(
                OrchestrationRuntime::new(orchestrator),
                thread_tool,
                query_tool,
                document_tool,
                generic_tools,
            ),
            kernel: Arc::new(tokio::sync::Mutex::new(None)),
            cell_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn arc(
        orchestrator: Arc<OrchestratorState>,
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            orchestrator,
            thread_tool,
            query_tool,
            document_tool,
            HashMap::new(),
        ))
    }

    pub fn arc_with_tools(
        orchestrator: Arc<OrchestratorState>,
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
        generic_tools: HashMap<String, Arc<dyn AgentTool>>,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            orchestrator,
            thread_tool,
            query_tool,
            document_tool,
            generic_tools,
        ))
    }

    /// Start the Python kernel subprocess. Returns a KernelProcess.
    fn start_kernel() -> anyhow::Result<KernelProcess> {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tau_py_kernel_{}.py", std::process::id()));
        std::fs::write(&path, PY_KERNEL_SOURCE)?;

        let mut child = Command::new("python3")
            .arg("-u") // unbuffered stdout/stderr — critical for pipe communication
            .arg(&path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start Python kernel: {}", e))?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Ok(KernelProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            kernel_path: path,
        })
    }
}

impl AgentTool for PyReplTool {
    fn name(&self) -> &str {
        "py_repl"
    }

    fn label(&self) -> &str {
        "Python"
    }

    fn description(&self) -> &str {
        "Execute Python code in a persistent REPL with the tau orchestration API. \
         The namespace persists across calls. Use tau.tool(name, **args) to call \
         any tau tool, tau.thread(alias, task) for blocking threads, tau.launch()/tau.poll()/tau.wait() \
         for reactive coordination, tau.query(prompt) for LLM queries, tau.parallel(...) for \
         concurrent execution, and tau.document(op, name, content) for shared documents. Use for: \
         programmatic orchestration with control flow, loops, data processing, \
         phased dependencies, and reactive fan-out/gather patterns."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python code to execute. Has access to the `tau` object for orchestration. Namespace persists across calls."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 300)."
                    }
                },
                "required": ["code"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let kernel = self.kernel.clone();
        let cell_counter = &self.cell_counter;
        let cell_id = format!(
            "c-{}",
            cell_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );

        let rpc_facade = self.rpc_facade.clone();

        Box::pin(async move {
            let code = params
                .get("code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'code' parameter"))?
                .to_string();
            let timeout_secs = params
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_TIMEOUT_SECS);

            // Lock kernel — held across await points (tokio::sync::Mutex)
            let mut kernel_guard = kernel.lock().await;

            // Ensure kernel is alive
            if let Some(ref mut kp) = *kernel_guard {
                // Check if child is still running
                match kp.child.try_wait() {
                    Ok(Some(_)) => {
                        // Process exited, need restart
                        *kernel_guard = None;
                    }
                    Ok(None) => {} // still running
                    Err(_) => {
                        *kernel_guard = None;
                    }
                }
            }
            if kernel_guard.is_none() {
                *kernel_guard = Some(PyReplTool::start_kernel()?);
            }

            let kp = kernel_guard.as_mut().unwrap();

            // Send exec message
            let exec_msg = json!({
                "type": "exec",
                "cell_id": cell_id,
                "code": code,
            });
            let msg_line = format!("{}\n", serde_json::to_string(&exec_msg)?);
            kp.stdin.write_all(msg_line.as_bytes()).await?;
            kp.stdin.flush().await?;

            // Response loop: read lines, dispatch RPCs, wait for cell result
            let start = std::time::Instant::now();
            let timeout_dur = std::time::Duration::from_secs(timeout_secs);
            let mut line_buf = String::new();

            let cell_result: Option<Value> = loop {
                line_buf.clear();
                let remaining = timeout_dur.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break None; // timeout
                }

                let read_result = tokio::select! {
                    r = kp.stdout.read_line(&mut line_buf) => r,
                    _ = tokio::time::sleep(remaining) => {
                        break None; // timeout
                    }
                    _ = async {
                        if let Some(ref sig) = signal {
                            sig.cancelled().await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        break None; // cancelled
                    }
                };

                match read_result {
                    Ok(0) => {
                        // EOF — kernel died
                        break Some(json!({
                            "type": "result",
                            "cell_id": cell_id,
                            "output": null,
                            "error": "Python kernel process exited unexpectedly",
                            "stdout": "",
                            "stderr": "",
                        }));
                    }
                    Ok(_) => {
                        let parsed: Value = match serde_json::from_str(line_buf.trim()) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        match parsed.get("type").and_then(|v| v.as_str()) {
                            Some("result")
                                if parsed.get("cell_id").and_then(|v| v.as_str())
                                    == Some(&cell_id) =>
                            {
                                break Some(parsed);
                            }
                            Some("rpc") => {
                                let rpc_id = parsed
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let method =
                                    parsed.get("method").and_then(|v| v.as_str()).unwrap_or("");
                                let rpc_params = parsed.get("params").cloned().unwrap_or(json!({}));

                                let rpc_result = rpc_facade.dispatch(method, &rpc_params).await;

                                let response = match rpc_result {
                                    Ok(val) => json!({
                                        "type": "rpc_result",
                                        "id": rpc_id,
                                        "result": val,
                                        "error": null,
                                    }),
                                    Err(e) => json!({
                                        "type": "rpc_result",
                                        "id": rpc_id,
                                        "result": null,
                                        "error": e,
                                    }),
                                };

                                let resp_line = format!("{}\n", serde_json::to_string(&response)?);
                                kp.stdin.write_all(resp_line.as_bytes()).await?;
                                kp.stdin.flush().await?;
                            }
                            _ => {
                                // Ignore unexpected messages
                            }
                        }
                    }
                    Err(e) => {
                        break Some(json!({
                            "type": "result",
                            "cell_id": cell_id,
                            "output": null,
                            "error": format!("IO error reading from kernel: {}", e),
                            "stdout": "",
                            "stderr": "",
                        }));
                    }
                }
            };

            let duration_ms = start.elapsed().as_millis() as u64;

            // Handle timeout — kill kernel
            if cell_result.is_none() {
                if let Some(mut kp) = kernel_guard.take() {
                    let _ = kp.child.kill().await;
                }
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!(
                            "[py_repl timed out after {}s — kernel killed]",
                            timeout_secs
                        ),
                    }],
                    details: Some(json!({"timeout": true, "duration_ms": duration_ms})),
                });
            }

            let result = cell_result.unwrap();
            let output_text = format_cell_result(&result);

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: output_text }],
                details: Some(json!({"cell_id": cell_id, "duration_ms": duration_ms})),
            })
        })
    }
}

/// Format a cell result JSON into a human-readable string for the LLM.
fn format_cell_result(result: &Value) -> String {
    let mut parts = Vec::new();

    if let Some(stdout) = result.get("stdout").and_then(|v| v.as_str()) {
        if !stdout.is_empty() {
            parts.push(stdout.to_string());
        }
    }
    if let Some(output) = result.get("output").and_then(|v| v.as_str()) {
        parts.push(output.to_string());
    }
    if let Some(stderr) = result.get("stderr").and_then(|v| v.as_str()) {
        if !stderr.is_empty() {
            parts.push(format!("[stderr]\n{}", stderr));
        }
    }
    if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        parts.push(format!("[error]\n{}", error));
    }

    let text = if parts.is_empty() {
        "(no output)".to_string()
    } else {
        parts.join("\n")
    };

    truncate_output(&text)
}

/// Truncate output to max lines/bytes.
fn truncate_output(text: &str) -> String {
    if text.len() <= MAX_OUTPUT_BYTES && text.lines().count() <= MAX_OUTPUT_LINES {
        return text.to_string();
    }

    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > MAX_OUTPUT_LINES {
        let half = MAX_OUTPUT_LINES / 2;
        let mut out = String::new();
        for line in &lines[..half] {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!(
            "[... {} lines omitted ...]\n",
            lines.len() - MAX_OUTPUT_LINES
        ));
        for line in &lines[lines.len() - half..] {
            out.push_str(line);
            out.push('\n');
        }
        return out;
    }

    // Over byte limit but under line limit — truncate bytes
    text[..MAX_OUTPUT_BYTES].to_string() + "\n[... truncated ...]"
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeThreadTool {
        calls: Arc<Mutex<Vec<Value>>>,
    }

    impl FakeThreadTool {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl AgentTool for FakeThreadTool {
        fn name(&self) -> &str {
            "thread"
        }

        fn label(&self) -> &str {
            "Thread"
        }

        fn description(&self) -> &str {
            "fake thread tool"
        }

        fn parameters(&self) -> &Value {
            static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            SCHEMA.get_or_init(|| json!({"type": "object"}))
        }

        fn execute(
            &self,
            _tool_call_id: String,
            params: Value,
            _signal: Option<CancellationToken>,
        ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
            self.calls.lock().unwrap().push(params.clone());
            Box::pin(async move {
                let alias = params
                    .get("alias")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let delay_ms = params.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }

                Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("trace:{alias}"),
                    }],
                    details: Some(json!({
                        "alias": alias,
                        "outcome": {
                            "kind": "completed",
                            "text": format!("done:{alias}"),
                        },
                        "duration_ms": delay_ms,
                    })),
                })
            })
        }
    }

    struct FakeTextTool;

    impl AgentTool for FakeTextTool {
        fn name(&self) -> &str {
            "fake"
        }

        fn label(&self) -> &str {
            "Fake"
        }

        fn description(&self) -> &str {
            "fake text tool"
        }

        fn parameters(&self) -> &Value {
            static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            SCHEMA.get_or_init(|| json!({"type": "object"}))
        }

        fn execute(
            &self,
            _tool_call_id: String,
            _params: Value,
            _signal: Option<CancellationToken>,
        ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
            Box::pin(async move {
                Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: "ok".to_string(),
                    }],
                    details: None,
                })
            })
        }
    }

    fn make_dispatcher_with_tools(
        thread_tool: Arc<FakeThreadTool>,
        generic_tools: HashMap<String, Arc<dyn AgentTool>>,
    ) -> OrchestrationRpcFacade {
        OrchestrationRpcFacade::new(
            OrchestrationRuntime::new(OrchestratorState::new()),
            thread_tool,
            Arc::new(FakeTextTool),
            Arc::new(FakeTextTool),
            generic_tools,
        )
    }

    fn make_dispatcher(thread_tool: Arc<FakeThreadTool>) -> OrchestrationRpcFacade {
        make_dispatcher_with_tools(
            thread_tool,
            HashMap::from([(
                "fake".to_string(),
                Arc::new(FakeTextTool) as Arc<dyn AgentTool>,
            )]),
        )
    }

    #[tokio::test]
    async fn dispatch_tool_uses_configured_generic_tools() {
        let dispatcher = make_dispatcher(Arc::new(FakeThreadTool::new()));

        let result = dispatcher
            .dispatch_tool(&json!({"name": "fake", "args": {"x": 1}}))
            .await
            .unwrap();
        assert_eq!(result, Value::String("ok".to_string()));
    }

    #[tokio::test]
    async fn dispatch_tool_rejects_tools_outside_configured_surface() {
        let dispatcher =
            make_dispatcher_with_tools(Arc::new(FakeThreadTool::new()), HashMap::new());

        let err = dispatcher
            .dispatch_tool(&json!({"name": "bash", "args": {"command": "echo bypass"}}))
            .await
            .unwrap_err();
        assert_eq!(err, "unknown tool: bash");
    }

    #[tokio::test]
    async fn dispatch_parallel_tool_uses_configured_generic_tools() {
        let dispatcher = make_dispatcher(Arc::new(FakeThreadTool::new()));

        let result = dispatcher
            .dispatch_parallel(
                &json!({"specs": [ {"method": "tool", "name": "fake", "args": {}} ]}),
            )
            .await
            .unwrap();

        assert_eq!(result, json!(["ok"]));
    }

    #[tokio::test]
    async fn dispatch_tool_preserves_permission_wrappers() {
        let mut config = HashMap::new();
        config.insert("fake".to_string(), "deny".to_string());
        let service = Arc::new(crate::permissions::PermissionService::new(&config, false));
        let denied_tool =
            crate::permissions::PermissionWrapper::arc(Arc::new(FakeTextTool), service);
        let dispatcher = make_dispatcher_with_tools(
            Arc::new(FakeThreadTool::new()),
            HashMap::from([("fake".to_string(), denied_tool)]),
        );

        let result = dispatcher
            .dispatch_tool(&json!({"name": "fake", "args": {}}))
            .await
            .unwrap();
        assert_eq!(
            result,
            Value::String("Tool 'fake' is denied by permission policy.".to_string())
        );
    }

    #[tokio::test]
    async fn launch_poll_wait_supports_partial_collection_and_stable_statuses() {
        let thread_tool = Arc::new(FakeThreadTool::new());
        let dispatcher = make_dispatcher(thread_tool);

        let launched = dispatcher
            .dispatch_launch(&json!({"alias": "fast", "task": "fast", "delay_ms": 50}))
            .await
            .unwrap();
        assert_eq!(
            launched.get("status").and_then(|v| v.as_str()),
            Some("running")
        );

        dispatcher
            .dispatch_launch(&json!({"alias": "slow", "task": "slow", "delay_ms": 1500}))
            .await
            .unwrap();

        let waited = dispatcher
            .dispatch_wait(&json!({"aliases": ["fast", "slow"], "timeout": 1}))
            .await
            .unwrap();
        let waited = waited.as_array().unwrap();
        assert_eq!(
            waited[0].get("status").and_then(|v| v.as_str()),
            Some("completed")
        );
        assert_eq!(
            waited[1].get("status").and_then(|v| v.as_str()),
            Some("running")
        );

        let fast_poll = dispatcher
            .dispatch_poll(&json!({"alias": "fast"}))
            .await
            .unwrap();
        assert_eq!(
            fast_poll.get("status").and_then(|v| v.as_str()),
            Some("completed")
        );
        assert_eq!(
            fast_poll.get("reason").and_then(|v| v.as_str()),
            Some("done:fast")
        );

        tokio::time::sleep(Duration::from_millis(650)).await;

        let slow_poll = dispatcher
            .dispatch_poll(&json!({"alias": "slow"}))
            .await
            .unwrap();
        assert_eq!(
            slow_poll.get("status").and_then(|v| v.as_str()),
            Some("completed")
        );
        assert_eq!(
            slow_poll.get("completed").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            slow_poll.get("reason").and_then(|v| v.as_str()),
            Some("done:slow")
        );
    }

    #[tokio::test]
    async fn dispatch_thread_forwards_max_turns() {
        let thread_tool = Arc::new(FakeThreadTool::new());
        let dispatcher = make_dispatcher(thread_tool.clone());

        let result = dispatcher
            .dispatch_thread(&json!({"alias": "researcher", "task": "scan", "max_turns": 77}))
            .await
            .unwrap();

        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("completed")
        );

        let calls = thread_tool.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].get("max_turns").and_then(|v| v.as_u64()), Some(77));
    }
}
