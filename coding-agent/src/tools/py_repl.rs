//! Python REPL tool: persistent Python subprocess with reverse RPC for orchestration.
//!
//! The LLM writes Python code that runs in a long-lived subprocess. A `tau` object
//! in the Python namespace provides blocking APIs for tool calls, thread spawning,
//! queries, parallel execution, and document sharing. Communication is bidirectional
//! JSON-lines over stdin/stdout.

use std::collections::HashMap;
use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::sync::CancellationToken;

const PY_KERNEL_SOURCE: &str = include_str!("../../prompts/py_kernel.py");

/// Default timeout for py_repl cells (seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Max output lines returned to the LLM.
const MAX_OUTPUT_LINES: usize = 2000;

/// Max output bytes returned to the LLM.
const MAX_OUTPUT_BYTES: usize = 30_000;

type RunningThreadHandle = tokio::task::JoinHandle<Result<Value, String>>;
type RunningThreads = Arc<tokio::sync::Mutex<HashMap<String, RunningThreadHandle>>>;
type CompletedThreads = Arc<tokio::sync::Mutex<HashMap<String, Value>>>;

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
    // Pre-built tool instances for reverse RPC dispatch
    thread_tool: Arc<dyn AgentTool>,
    query_tool: Arc<dyn AgentTool>,
    document_tool: Arc<dyn AgentTool>,
    generic_tools: Arc<HashMap<String, Arc<dyn AgentTool>>>,
    // Long-lived kernel subprocess
    kernel: Arc<tokio::sync::Mutex<Option<KernelProcess>>>,
    // Cell counter for IDs
    cell_counter: std::sync::atomic::AtomicU64,
    // Non-blocking thread orchestration state for tau.launch/poll/wait.
    running_threads: RunningThreads,
    completed_threads: CompletedThreads,
}

impl PyReplTool {
    pub fn new(
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
        generic_tools: HashMap<String, Arc<dyn AgentTool>>,
    ) -> Self {
        Self {
            thread_tool,
            query_tool,
            document_tool,
            generic_tools: Arc::new(generic_tools),
            kernel: Arc::new(tokio::sync::Mutex::new(None)),
            cell_counter: std::sync::atomic::AtomicU64::new(0),
            running_threads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            completed_threads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn arc(
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            thread_tool,
            query_tool,
            document_tool,
            HashMap::new(),
        ))
    }

    pub fn arc_with_tools(
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
        generic_tools: HashMap<String, Arc<dyn AgentTool>>,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
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

        // Clone self references for the async block
        let this_thread_tool = self.thread_tool.clone();
        let this_query_tool = self.query_tool.clone();
        let this_document_tool = self.document_tool.clone();
        let generic_tools = self.generic_tools.clone();
        let running_threads = self.running_threads.clone();
        let completed_threads = self.completed_threads.clone();

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

            // Build a temporary self-like struct for RPC dispatch
            let dispatcher = RpcDispatcher {
                thread_tool: this_thread_tool,
                query_tool: this_query_tool,
                document_tool: this_document_tool,
                generic_tools,
                running_threads,
                completed_threads,
            };

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

                                let rpc_result = dispatcher.dispatch(method, &rpc_params).await;

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

/// Helper struct for dispatching RPCs from inside the execute future.
struct RpcDispatcher {
    thread_tool: Arc<dyn AgentTool>,
    query_tool: Arc<dyn AgentTool>,
    document_tool: Arc<dyn AgentTool>,
    generic_tools: Arc<HashMap<String, Arc<dyn AgentTool>>>,
    running_threads: RunningThreads,
    completed_threads: CompletedThreads,
}

impl RpcDispatcher {
    async fn dispatch(&self, method: &str, params: &Value) -> Result<Value, String> {
        let result = match method {
            "tool" => self.dispatch_tool(params).await,
            "thread" => self.dispatch_thread(params).await,
            "launch" => self.dispatch_launch(params).await,
            "poll" => self.dispatch_poll(params).await,
            "wait" => self.dispatch_wait(params).await,
            "query" => self.dispatch_to_tool(&self.query_tool, params).await,
            "document" => self.dispatch_to_tool(&self.document_tool, params).await,
            "parallel" => self.dispatch_parallel(params).await,
            "diff" => self.dispatch_diff(params).await,
            "merge" => self.dispatch_merge(params).await,
            "branches" => self.dispatch_branches(params).await,
            "log" => {
                if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                    eprintln!("[py_repl:log] {}", msg);
                }
                Ok(Value::Null)
            }
            _ => Err(format!("unknown RPC method: {}", method)),
        };
        result
    }

    async fn dispatch_tool(&self, params: &Value) -> Result<Value, String> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("missing 'name' in tool RPC")?;
        let args = params.get("args").cloned().unwrap_or(json!({}));

        let tool = self
            .generic_tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {}", name))?;

        let result = tool
            .execute(format!("py-rpc-{}", name), args, None)
            .await
            .map_err(|e| e.to_string())?;

        Ok(Value::String(extract_text(&result)))
    }

    async fn dispatch_thread(&self, params: &Value) -> Result<Value, String> {
        let result = self
            .thread_tool
            .execute(
                format!("py-rpc-{}", self.thread_tool.name()),
                params.clone(),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(build_thread_result(&result))
    }

    async fn dispatch_launch(&self, params: &Value) -> Result<Value, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in launch RPC")?
            .to_string();

        self.collect_finished_aliases(std::slice::from_ref(&alias))
            .await?;

        {
            let running = self.running_threads.lock().await;
            if running.contains_key(&alias) {
                return Err(format!("thread '{}' is already running", alias));
            }
        }

        self.completed_threads.lock().await.remove(&alias);

        let thread_tool = self.thread_tool.clone();
        let params = params.clone();
        let launched_alias = alias.clone();
        let handle = tokio::spawn(async move {
            let result = thread_tool
                .execute(
                    format!("py-launch-{}-{}", thread_tool.name(), launched_alias),
                    params,
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(build_thread_result(&result))
        });

        self.running_threads
            .lock()
            .await
            .insert(alias.clone(), handle);
        Ok(thread_state(&alias, "running", ""))
    }

    async fn dispatch_poll(&self, params: &Value) -> Result<Value, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in poll RPC")?
            .to_string();

        self.collect_finished_aliases(std::slice::from_ref(&alias))
            .await?;
        Ok(self.status_for_alias(&alias).await)
    }

    async fn dispatch_wait(&self, params: &Value) -> Result<Value, String> {
        let aliases = parse_alias_list(params, "aliases")?;
        let timeout = params
            .get("timeout")
            .and_then(|v| v.as_u64())
            .map(std::time::Duration::from_secs);

        if aliases.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }

        let deadline = timeout.map(|dur| std::time::Instant::now() + dur);
        loop {
            self.collect_finished_aliases(&aliases).await?;

            let statuses = self.statuses_for_aliases(&aliases).await;
            let all_terminal = statuses.iter().all(is_terminal_thread_state);
            if all_terminal {
                return Ok(Value::Array(statuses));
            }

            if let Some(deadline) = deadline {
                if std::time::Instant::now() >= deadline {
                    return Ok(Value::Array(statuses));
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    async fn dispatch_to_tool(
        &self,
        tool: &Arc<dyn AgentTool>,
        params: &Value,
    ) -> Result<Value, String> {
        let result = tool
            .execute(format!("py-rpc-{}", tool.name()), params.clone(), None)
            .await
            .map_err(|e| e.to_string())?;

        Ok(Value::String(extract_text(&result)))
    }

    async fn dispatch_parallel(&self, params: &Value) -> Result<Value, String> {
        let specs = params
            .get("specs")
            .and_then(|v| v.as_array())
            .ok_or("missing 'specs' array in parallel RPC")?;

        let mut handles = Vec::with_capacity(specs.len());

        for spec in specs {
            let method = spec
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            let spec = spec.clone();
            let thread_tool = self.thread_tool.clone();
            let query_tool = self.query_tool.clone();
            let document_tool = self.document_tool.clone();
            let generic_tools = self.generic_tools.clone();
            handles.push(tokio::spawn(async move {
                match method.as_str() {
                    "thread" => dispatch_single_thread(&thread_tool, &spec).await,
                    "query" => dispatch_single(&query_tool, &spec).await,
                    "document" => dispatch_single(&document_tool, &spec).await,
                    "tool" => {
                        let name = spec
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let args = spec.get("args").cloned().unwrap_or(json!({}));
                        let tool = generic_tools
                            .get(name)
                            .ok_or_else(|| format!("unknown tool: {}", name))?;
                        let result = tool
                            .execute(format!("py-parallel-{}", name), args, None)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(Value::String(extract_text(&result)))
                    }
                    _ => Err(format!("unknown parallel method: {}", method)),
                }
            }));
        }

        let results = futures::future::join_all(handles).await;
        let mut values = Vec::with_capacity(results.len());
        for result in results {
            match result {
                Ok(Ok(val)) => values.push(val),
                Ok(Err(e)) => values.push(Value::String(format!("error: {}", e))),
                Err(e) => values.push(Value::String(format!("task error: {}", e))),
            }
        }

        Ok(Value::Array(values))
    }

    async fn dispatch_diff(&self, params: &Value) -> Result<Value, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in diff RPC")?;

        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let repo_root = crate::tools::worktree::find_repo_root(&cwd).map_err(|e| e.to_string())?;

        let sanitized = alias
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let branch = format!("tau/{}", sanitized);

        let stat =
            crate::tools::worktree::diff_stat(&repo_root, &branch).map_err(|e| e.to_string())?;
        let diff = crate::tools::worktree::diff_full(&repo_root, &branch, 50_000)
            .map_err(|e| e.to_string())?;
        let (files_changed, insertions, deletions) =
            crate::tools::worktree::parse_stat_summary(&stat);

        Ok(json!({
            "branch": branch,
            "stat": stat,
            "diff": diff,
            "files_changed": files_changed,
            "insertions": insertions,
            "deletions": deletions,
        }))
    }

    async fn dispatch_merge(&self, params: &Value) -> Result<Value, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in merge RPC")?;

        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let repo_root = crate::tools::worktree::find_repo_root(&cwd).map_err(|e| e.to_string())?;

        let sanitized = alias
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let branch = format!("tau/{}", sanitized);

        let (success, conflicts, message) =
            crate::tools::worktree::merge_branch(&repo_root, &branch).map_err(|e| e.to_string())?;

        Ok(json!({
            "success": success,
            "conflicts": conflicts,
            "message": message,
            "branch": branch,
        }))
    }

    async fn dispatch_branches(&self, _params: &Value) -> Result<Value, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let repo_root = crate::tools::worktree::find_repo_root(&cwd).map_err(|e| e.to_string())?;
        let branches =
            crate::tools::worktree::list_branches(&repo_root).map_err(|e| e.to_string())?;
        Ok(json!(branches))
    }

    async fn collect_finished_aliases(&self, aliases: &[String]) -> Result<(), String> {
        let ready = {
            let mut running = self.running_threads.lock().await;
            let mut ready = Vec::new();
            for alias in aliases {
                let is_finished = running
                    .get(alias)
                    .map(tokio::task::JoinHandle::is_finished)
                    .unwrap_or(false);
                if is_finished {
                    if let Some(handle) = running.remove(alias) {
                        ready.push((alias.clone(), handle));
                    }
                }
            }
            ready
        };

        if ready.is_empty() {
            return Ok(());
        }

        let mut completed = self.completed_threads.lock().await;
        for (alias, handle) in ready {
            let value = match handle.await {
                Ok(Ok(result)) => canonicalize_thread_state(result, Some(alias.as_str())),
                Ok(Err(err)) => thread_state(&alias, "error", &err),
                Err(err) => thread_state(&alias, "error", &format!("task error: {}", err)),
            };
            completed.insert(alias, value);
        }
        Ok(())
    }

    async fn status_for_alias(&self, alias: &str) -> Value {
        if let Some(value) = self.completed_threads.lock().await.get(alias).cloned() {
            return canonicalize_thread_state(value, Some(alias));
        }

        if self.running_threads.lock().await.contains_key(alias) {
            return thread_state(alias, "running", "");
        }

        thread_state(alias, "unknown", "thread not found")
    }

    async fn statuses_for_aliases(&self, aliases: &[String]) -> Vec<Value> {
        let completed = self.completed_threads.lock().await.clone();
        let running = self.running_threads.lock().await;

        aliases
            .iter()
            .map(|alias| {
                if let Some(value) = completed.get(alias).cloned() {
                    return canonicalize_thread_state(value, Some(alias));
                }
                if running.contains_key(alias) {
                    return thread_state(alias, "running", "");
                }
                thread_state(alias, "unknown", "thread not found")
            })
            .collect()
    }
}

/// Helper: dispatch a single thread spec, returning structured ThreadResult.
async fn dispatch_single_thread(
    tool: &Arc<dyn AgentTool>,
    params: &Value,
) -> Result<Value, String> {
    let result = tool
        .execute(format!("py-parallel-{}", tool.name()), params.clone(), None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(build_thread_result(&result))
}

/// Helper: dispatch a single spec to a tool.
async fn dispatch_single(tool: &Arc<dyn AgentTool>, params: &Value) -> Result<Value, String> {
    let result = tool
        .execute(format!("py-parallel-{}", tool.name()), params.clone(), None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Value::String(extract_text(&result)))
}

/// Build a structured result from a thread's AgentToolResult.
/// Flattens outcome.kind → status, outcome.text → output, and includes the full trace.
fn build_thread_result(result: &AgentToolResult) -> Value {
    let text = extract_text(result);
    let mut structured = result.details.clone().unwrap_or(json!({}));
    if let Value::Object(ref mut map) = structured {
        map.insert("trace".to_string(), Value::String(text));
        if let Some(outcome) = map.remove("outcome") {
            if let Some(kind) = outcome.get("kind") {
                map.insert("status".to_string(), kind.clone());
            }
            if let Some(text) = outcome.get("text") {
                map.insert("output".to_string(), text.clone());
            }
        }
    }
    canonicalize_thread_state(structured, None)
}

/// Extract text content from an AgentToolResult.
fn extract_text(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|b| match b {
            UserBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_alias_list(params: &Value, field: &str) -> Result<Vec<String>, String> {
    let aliases = params
        .get(field)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("missing '{}' array in wait RPC", field))?;

    aliases
        .iter()
        .map(|value| match value {
            Value::String(alias) => Ok(alias.clone()),
            Value::Object(map) => map
                .get("alias")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| {
                    format!(
                        "'{}' entries must be strings or objects with 'alias'",
                        field
                    )
                }),
            _ => Err(format!(
                "'{}' entries must be strings or objects with 'alias'",
                field
            )),
        })
        .collect()
}

fn thread_state(alias: &str, status: &str, output: &str) -> Value {
    json!({
        "alias": alias,
        "status": status,
        "output": output,
        "reason": output,
        "completed": status == "completed",
    })
}

fn canonicalize_thread_state(value: Value, fallback_alias: Option<&str>) -> Value {
    match value {
        Value::Object(mut map) => {
            if let Some(alias) = fallback_alias {
                map.entry("alias".to_string())
                    .or_insert_with(|| Value::String(alias.to_string()));
            }

            let status = map
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("completed")
                .to_string();
            let output = map
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            map.entry("status".to_string())
                .or_insert_with(|| Value::String(status.clone()));
            map.entry("output".to_string())
                .or_insert_with(|| Value::String(output.clone()));
            map.insert("reason".to_string(), Value::String(output));
            map.insert("completed".to_string(), Value::Bool(status == "completed"));
            Value::Object(map)
        }
        other => thread_state(fallback_alias.unwrap_or(""), "error", &other.to_string()),
    }
}

fn is_terminal_thread_state(value: &Value) -> bool {
    value
        .get("status")
        .and_then(|v| v.as_str())
        .map(|status| status != "running")
        .unwrap_or(true)
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
    ) -> RpcDispatcher {
        RpcDispatcher {
            thread_tool,
            query_tool: Arc::new(FakeTextTool),
            document_tool: Arc::new(FakeTextTool),
            generic_tools: Arc::new(generic_tools),
            running_threads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            completed_threads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    fn make_dispatcher(thread_tool: Arc<FakeThreadTool>) -> RpcDispatcher {
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
