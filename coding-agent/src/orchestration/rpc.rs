use std::collections::HashMap;
use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult};
use ai::types::UserBlock;
use serde_json::{json, Value};

use super::runtime::{BranchDiffResult, BranchMergeResult, LogRequest, OrchestrationRuntime};

type RunningThreadHandle = tokio::task::JoinHandle<Result<Value, String>>;
type RunningThreads = Arc<tokio::sync::Mutex<HashMap<String, RunningThreadHandle>>>;
type CompletedThreads = Arc<tokio::sync::Mutex<HashMap<String, Value>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadState {
    pub value: Value,
}

impl ThreadState {
    pub fn running(alias: &str) -> Self {
        Self {
            value: thread_state_json(alias, "running", ""),
        }
    }

    pub fn unknown(alias: &str) -> Self {
        Self {
            value: thread_state_json(alias, "unknown", "thread not found"),
        }
    }
}

#[derive(Clone)]
pub struct OrchestrationRpcFacade {
    runtime: OrchestrationRuntime,
    thread_tool: Arc<dyn AgentTool>,
    query_tool: Arc<dyn AgentTool>,
    document_tool: Arc<dyn AgentTool>,
    generic_tools: Arc<HashMap<String, Arc<dyn AgentTool>>>,
    running_threads: RunningThreads,
    completed_threads: CompletedThreads,
}

impl OrchestrationRpcFacade {
    pub fn new(
        runtime: OrchestrationRuntime,
        thread_tool: Arc<dyn AgentTool>,
        query_tool: Arc<dyn AgentTool>,
        document_tool: Arc<dyn AgentTool>,
        generic_tools: HashMap<String, Arc<dyn AgentTool>>,
    ) -> Self {
        Self {
            runtime,
            thread_tool,
            query_tool,
            document_tool,
            generic_tools: Arc::new(generic_tools),
            running_threads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            completed_threads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub async fn dispatch(&self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "tool" => self.dispatch_tool(params).await,
            "thread" => self.dispatch_thread(params).await,
            "launch" => self.dispatch_launch(params).await,
            "poll" => self.dispatch_poll(params).await,
            "wait" => self.dispatch_wait(params).await,
            "query" => self.dispatch_to_tool(&self.query_tool, params).await,
            "document" => self.dispatch_to_tool(&self.document_tool, params).await,
            "parallel" => self.dispatch_parallel(params).await,
            "diff" => self
                .dispatch_diff(params)
                .await
                .map(|result| result.to_json()),
            "merge" => self
                .dispatch_merge(params)
                .await
                .map(|result| result.to_json()),
            "branches" => self
                .runtime
                .list_branches()
                .map(|branches| json!(branches))
                .map_err(|e| e.to_string()),
            "log" => {
                let request = LogRequest::from_params(params).map_err(|e| e.to_string())?;
                self.runtime.log_message(request);
                Ok(Value::Null)
            }
            _ => Err(format!("unknown RPC method: {}", method)),
        }
    }

    pub async fn dispatch_tool(&self, params: &Value) -> Result<Value, String> {
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

    pub async fn dispatch_thread(&self, params: &Value) -> Result<Value, String> {
        let result = self
            .thread_tool
            .execute(
                format!("py-rpc-{}", self.thread_tool.name()),
                params.clone(),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(build_thread_result_json(&result))
    }

    pub async fn dispatch_launch(&self, params: &Value) -> Result<Value, String> {
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
            Ok(build_thread_result_json(&result))
        });

        self.running_threads
            .lock()
            .await
            .insert(alias.clone(), handle);
        Ok(thread_state_json(&alias, "running", ""))
    }

    pub async fn dispatch_poll(&self, params: &Value) -> Result<Value, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in poll RPC")?
            .to_string();

        self.collect_finished_aliases(std::slice::from_ref(&alias))
            .await?;
        Ok(self.status_for_alias(&alias).await)
    }

    pub async fn dispatch_wait(&self, params: &Value) -> Result<Value, String> {
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
            let all_terminal = statuses.iter().all(is_terminal_thread_state_json);
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

    pub async fn dispatch_parallel(&self, params: &Value) -> Result<Value, String> {
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

    pub async fn dispatch_diff(&self, params: &Value) -> Result<BranchDiffResult, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in diff RPC")?;
        self.runtime.diff_branch(alias).map_err(|e| e.to_string())
    }

    pub async fn dispatch_merge(&self, params: &Value) -> Result<BranchMergeResult, String> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or("missing 'alias' in merge RPC")?;
        self.runtime.merge_branch(alias).map_err(|e| e.to_string())
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
                Ok(Ok(result)) => canonicalize_thread_state_json(result, Some(alias.as_str())),
                Ok(Err(err)) => thread_state_json(&alias, "error", &err),
                Err(err) => thread_state_json(&alias, "error", &format!("task error: {}", err)),
            };
            completed.insert(alias, value);
        }
        Ok(())
    }

    async fn status_for_alias(&self, alias: &str) -> Value {
        if let Some(value) = self.completed_threads.lock().await.get(alias).cloned() {
            return canonicalize_thread_state_json(value, Some(alias));
        }

        if self.running_threads.lock().await.contains_key(alias) {
            return thread_state_json(alias, "running", "");
        }

        thread_state_json(alias, "unknown", "thread not found")
    }

    async fn statuses_for_aliases(&self, aliases: &[String]) -> Vec<Value> {
        let completed = self.completed_threads.lock().await.clone();
        let running = self.running_threads.lock().await;

        aliases
            .iter()
            .map(|alias| {
                if let Some(value) = completed.get(alias).cloned() {
                    return canonicalize_thread_state_json(value, Some(alias));
                }
                if running.contains_key(alias) {
                    return thread_state_json(alias, "running", "");
                }
                thread_state_json(alias, "unknown", "thread not found")
            })
            .collect()
    }
}

async fn dispatch_single_thread(
    tool: &Arc<dyn AgentTool>,
    params: &Value,
) -> Result<Value, String> {
    let result = tool
        .execute(format!("py-parallel-{}", tool.name()), params.clone(), None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(build_thread_result_json(&result))
}

async fn dispatch_single(tool: &Arc<dyn AgentTool>, params: &Value) -> Result<Value, String> {
    let result = tool
        .execute(format!("py-parallel-{}", tool.name()), params.clone(), None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Value::String(extract_text(&result)))
}

pub fn build_thread_result_json(result: &AgentToolResult) -> Value {
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
    canonicalize_thread_state_json(structured, None)
}

pub fn extract_text(result: &AgentToolResult) -> String {
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

pub fn thread_state_json(alias: &str, status: &str, output: &str) -> Value {
    json!({
        "alias": alias,
        "status": status,
        "output": output,
        "reason": output,
        "completed": status == "completed",
    })
}

pub fn canonicalize_thread_state_json(value: Value, fallback_alias: Option<&str>) -> Value {
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
        other => thread_state_json(fallback_alias.unwrap_or(""), "error", &other.to_string()),
    }
}

fn is_terminal_thread_state_json(value: &Value) -> bool {
    value
        .get("status")
        .and_then(|v| v.as_str())
        .map(|status| status != "running")
        .unwrap_or(true)
}
