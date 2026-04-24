use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent::completion_tools::{self, AbortTool, CompleteTool, EscalateTool};
use agent::episode::{generate_episode, EpisodeWorktreeInfo};
use agent::orchestrator::OrchestratorState;
use agent::thread::{Episode, ThreadOutcome};
use agent::types::{AgentEvent, AgentTool, AgentToolResult, GetApiKeyFn};
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::{Model, UserBlock};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::ModelSlots;
use crate::orchestration::EventForwarderCell;
use crate::tools;
use crate::tools::worktree;

const THREAD_IDENTITY: &str = include_str!("../../prompts/thread_identity.md");

/// Default tools for threads when none specified.
const DEFAULT_THREAD_TOOLS: &[&str] = &["file_read", "grep", "glob"];

type RunningThreadHandle = tokio::task::JoinHandle<Result<Value, String>>;
type RunningThreads = Arc<tokio::sync::Mutex<HashMap<String, RunningThreadHandle>>>;
type CompletedThreads = Arc<tokio::sync::Mutex<HashMap<String, Value>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRequest {
    pub alias: String,
    pub task: String,
    pub raw_tool_names: Vec<String>,
    pub tool_names: Vec<String>,
    pub model_override: Option<String>,
    pub episode_aliases: Vec<String>,
    pub timeout_secs: u64,
    pub max_turns: u32,
    pub use_worktree: bool,
    pub worktree_base: Option<String>,
    pub worktree_include: Vec<String>,
}

impl ThreadRequest {
    pub fn from_params(params: &Value) -> anyhow::Result<Self> {
        let alias = params
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'alias' parameter"))?
            .to_string();
        let task = params
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'task' parameter"))?
            .to_string();

        const AUTO_INJECTED: &[&str] = &["document", "complete", "abort", "escalate"];
        let raw_tool_names: Vec<String> = params
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .filter(|name| !AUTO_INJECTED.contains(&name.as_str()))
                    .collect()
            })
            .unwrap_or_else(|| DEFAULT_THREAD_TOOLS.iter().map(|s| s.to_string()).collect());
        let tool_names = expand_capabilities(&raw_tool_names);

        Ok(Self {
            alias,
            task,
            raw_tool_names,
            tool_names,
            model_override: params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from),
            episode_aliases: params
                .get("episodes")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            timeout_secs: params
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(300),
            max_turns: params
                .get("max_turns")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(25),
            use_worktree: params
                .get("worktree")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            worktree_base: params
                .get("worktree_base")
                .and_then(|v| v.as_str())
                .map(String::from),
            worktree_include: params
                .get("worktree_include")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ThreadRunResult {
    pub trace: String,
    pub details: Value,
}

impl ThreadRunResult {
    pub fn to_agent_tool_result(&self) -> AgentToolResult {
        AgentToolResult {
            content: vec![UserBlock::Text {
                text: self.trace.clone(),
            }],
            details: Some(self.details.clone()),
        }
    }

    pub fn to_thread_state_json(&self) -> Value {
        let tool_result = self.to_agent_tool_result();
        build_thread_result_json(&tool_result)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRequest {
    pub alias: Option<String>,
    pub prompt: String,
    pub model_override: Option<String>,
}

impl QueryRequest {
    pub fn from_params(params: &Value) -> anyhow::Result<Self> {
        Ok(Self {
            alias: params
                .get("alias")
                .and_then(|v| v.as_str())
                .map(String::from),
            prompt: params
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'prompt' parameter"))?
                .to_string(),
            model_override: params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub output: String,
    pub thread_id: String,
    pub alias: String,
    pub duration_ms: u64,
    pub details: Option<Value>,
}

impl QueryResult {
    pub fn to_agent_tool_result(&self) -> AgentToolResult {
        AgentToolResult {
            content: vec![UserBlock::Text {
                text: self.output.clone(),
            }],
            details: self.details.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentResult {
    pub result: AgentToolResult,
}

#[derive(Debug, Clone)]
pub struct EpisodeLookupResult {
    pub result: AgentToolResult,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchDiffResult {
    pub branch: String,
    pub stat: String,
    pub diff: String,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

impl BranchDiffResult {
    pub fn to_json(&self) -> Value {
        json!({
            "branch": self.branch,
            "stat": self.stat,
            "diff": self.diff,
            "files_changed": self.files_changed,
            "insertions": self.insertions,
            "deletions": self.deletions,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchMergeResult {
    pub success: bool,
    pub conflicts: Vec<String>,
    pub message: String,
    pub branch: String,
}

impl BranchMergeResult {
    pub fn to_json(&self) -> Value {
        json!({
            "success": self.success,
            "conflicts": self.conflicts,
            "message": self.message,
            "branch": self.branch,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentRequest {
    List,
    Read { name: String },
    Write { name: String, content: String },
    Append { name: String, content: String },
}

impl DocumentRequest {
    pub fn from_params(params: &Value) -> anyhow::Result<Self> {
        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'operation' parameter"))?;
        let name = params.get("name").and_then(|v| v.as_str());
        let content = params.get("content").and_then(|v| v.as_str());

        match operation {
            "list" => Ok(Self::List),
            "read" => Ok(Self::Read {
                name: name
                    .ok_or_else(|| anyhow::anyhow!("'name' is required for read operation"))?
                    .to_string(),
            }),
            "write" => Ok(Self::Write {
                name: name
                    .ok_or_else(|| anyhow::anyhow!("'name' is required for write operation"))?
                    .to_string(),
                content: content
                    .ok_or_else(|| anyhow::anyhow!("'content' is required for write operation"))?
                    .to_string(),
            }),
            "append" => Ok(Self::Append {
                name: name
                    .ok_or_else(|| anyhow::anyhow!("'name' is required for append operation"))?
                    .to_string(),
                content: content
                    .ok_or_else(|| anyhow::anyhow!("'content' is required for append operation"))?
                    .to_string(),
            }),
            _ => Err(anyhow::anyhow!(
                "Unknown operation '{}'. Use: read, write, append, list.",
                operation
            )),
        }
    }
}

#[derive(Clone)]
pub struct OrchestrationRuntime {
    orchestrator: Arc<OrchestratorState>,
    event_forwarder: Option<EventForwarderCell>,
    thread_alias: Option<String>,
    get_api_key: Option<GetApiKeyFn>,
    default_model: Option<Model>,
    model_slots: ModelSlots,
}

impl OrchestrationRuntime {
    pub fn new(orchestrator: Arc<OrchestratorState>) -> Self {
        Self {
            orchestrator,
            event_forwarder: None,
            thread_alias: None,
            get_api_key: None,
            default_model: None,
            model_slots: ModelSlots::default(),
        }
    }

    pub fn with_event_forwarder(
        orchestrator: Arc<OrchestratorState>,
        event_forwarder: EventForwarderCell,
    ) -> Self {
        Self {
            orchestrator,
            event_forwarder: Some(event_forwarder),
            thread_alias: None,
            get_api_key: None,
            default_model: None,
            model_slots: ModelSlots::default(),
        }
    }

    pub fn with_agent_config(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        model_slots: ModelSlots,
        event_forwarder: EventForwarderCell,
    ) -> Self {
        Self {
            orchestrator,
            event_forwarder: Some(event_forwarder),
            thread_alias: None,
            get_api_key,
            default_model: Some(default_model),
            model_slots,
        }
    }

    pub fn for_thread(&self, alias: String) -> Self {
        Self {
            orchestrator: self.orchestrator.clone(),
            event_forwarder: self.event_forwarder.clone(),
            thread_alias: Some(alias),
            get_api_key: self.get_api_key.clone(),
            default_model: self.default_model.clone(),
            model_slots: self.model_slots.clone(),
        }
    }

    fn default_model(&self) -> anyhow::Result<Model> {
        self.default_model
            .clone()
            .ok_or_else(|| anyhow::anyhow!("orchestration runtime missing model configuration"))
    }

    fn resolve_model(
        &self,
        override_or_slot: Option<&str>,
        default_slot: &str,
    ) -> anyhow::Result<Model> {
        let default_model = self.default_model()?;
        let default_model_id = default_model.id.clone();
        let requested = override_or_slot
            .map(|value| {
                if ModelSlots::is_slot(value) {
                    self.model_slots.resolve(value, &default_model_id)
                } else {
                    value.to_string()
                }
            })
            .unwrap_or_else(|| self.model_slots.resolve(default_slot, &default_model_id));

        if requested == default_model_id {
            Ok(default_model)
        } else {
            Ok(ai::models::find_model(&requested)
                .map(|m| (*m).clone())
                .unwrap_or_else(|| {
                    eprintln!(
                        "[orchestration] model '{}' not found, using default",
                        requested
                    );
                    default_model
                }))
        }
    }

    pub async fn execute_thread(
        &self,
        request: ThreadRequest,
        signal: Option<CancellationToken>,
    ) -> anyhow::Result<ThreadRunResult> {
        let model = self.resolve_model(request.model_override.as_deref(), "subagent")?;
        let thread_id = self.orchestrator.next_thread_id();
        let main_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

        let worktree_info: Option<worktree::WorktreeInfo> = if request.use_worktree {
            match worktree::find_repo_root(&main_cwd) {
                Ok(repo_root) => {
                    match worktree::create_worktree(
                        &repo_root,
                        &request.alias,
                        &thread_id,
                        request.worktree_base.as_deref(),
                        &request.worktree_include,
                    ) {
                        Ok(info) => Some(info),
                        Err(e) => {
                            eprintln!(
                                "[thread] worktree creation failed: {}, running without isolation",
                                e
                            );
                            None
                        }
                    }
                }
                Err(_) => {
                    eprintln!("[thread] not in a git repo, skipping worktree isolation");
                    None
                }
            }
        } else {
            None
        };

        let effective_cwd = worktree_info
            .as_ref()
            .map(|wt| wt.path.clone())
            .unwrap_or_else(|| main_cwd.clone());
        let cwd = effective_cwd.to_string_lossy().to_string();
        let mut system_prompt =
            build_thread_system_prompt(&request.tool_names, &cwd, worktree_info.as_ref());
        let forward_fn = self
            .event_forwarder
            .as_ref()
            .and_then(|cell| cell.lock().ok().and_then(|g| g.clone()));

        let mut injected_episodes = false;
        if let Some(prior_section) = self
            .orchestrator
            .format_prior_episodes(&request.episode_aliases)
        {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&prior_section);
            injected_episodes = true;

            if let Some(ref fwd) = forward_fn {
                fwd(AgentEvent::EpisodeInject {
                    source_aliases: request.episode_aliases.clone(),
                    target_alias: request.alias.clone(),
                    target_thread_id: thread_id.clone(),
                });
            }
        }

        let lookup = self
            .orchestrator
            .get_or_create_thread(&request.alias, &system_prompt);
        if lookup.is_reuse && injected_episodes {
            self.orchestrator
                .update_system_prompt(&request.alias, system_prompt.clone());
        }
        let effective_system_prompt = effective_system_prompt_for_invocation(
            lookup.is_reuse,
            lookup.system_prompt.clone(),
            system_prompt,
            injected_episodes,
        );

        let (outcome_signal, mut outcome_rx) = completion_tools::outcome_channel();
        let mut thread_tools: Vec<Arc<dyn AgentTool>> = if worktree_info.is_some() {
            tools::tools_from_allowlist_with_cwd(&request.tool_names, effective_cwd.clone())
        } else {
            tools::tools_from_allowlist(&request.tool_names)
        };
        thread_tools.push(CompleteTool::arc(outcome_signal.clone()));
        thread_tools.push(AbortTool::arc(outcome_signal.clone()));
        thread_tools.push(EscalateTool::arc(outcome_signal));
        if let Some(event_forwarder) = self.event_forwarder.clone() {
            thread_tools.push(tools::DocumentTool::arc_for_thread(
                self.orchestrator.clone(),
                event_forwarder,
                request.alias.clone(),
            ));
        }
        thread_tools.push(tools::LogTool::arc(self.orchestrator.clone()));
        thread_tools.push(tools::FromIdTool::arc(self.orchestrator.clone()));

        let resolved_model_id = model.id.clone();
        let model_for_compact = model.clone();
        let transform_context: agent::types::TransformContextFn =
            Arc::new(move |messages, _cancel| {
                let m = model_for_compact.clone();
                Box::pin(async move { agent::context::compact_messages(messages, &m) })
            });

        let agent = Agent::new(AgentOptions {
            initial_state: Some(AgentStateInit {
                model: Some(model),
                system_prompt: Some(effective_system_prompt),
                tools: Some(thread_tools),
                thinking_level: Some(agent::types::ThinkingLevel::Off),
            }),
            convert_to_llm: None,
            transform_context: Some(transform_context),
            stream_fn: None,
            steering_mode: None,
            follow_up_mode: None,
            session_id: None,
            get_api_key: self.get_api_key.clone(),
            thinking_budgets: None,
            max_turns: Some(request.max_turns),
        });

        if lookup.is_reuse {
            agent.replace_messages(lookup.messages);
        }

        if let Some(ref fwd) = forward_fn {
            let fwd = fwd.clone();
            let tid = thread_id.clone();
            let a = request.alias.clone();
            let _unsub = agent.subscribe(move |event| {
                match event {
                    AgentEvent::ToolExecutionStart {
                        tool_call_id,
                        tool_name,
                        args,
                        ..
                    } => {
                        fwd(AgentEvent::ToolExecutionStart {
                            tool_call_id: tool_call_id.clone(),
                            tool_name: tool_name.clone(),
                            args: args.clone(),
                            thread_id: Some(tid.clone()),
                            thread_alias: Some(a.clone()),
                        });
                    }
                    AgentEvent::ToolExecutionEnd {
                        tool_call_id,
                        tool_name,
                        result,
                        is_error,
                        ..
                    } => {
                        fwd(AgentEvent::ToolExecutionEnd {
                            tool_call_id: tool_call_id.clone(),
                            tool_name: tool_name.clone(),
                            result: result.clone(),
                            is_error: *is_error,
                            thread_id: Some(tid.clone()),
                            thread_alias: Some(a.clone()),
                        });
                    }
                    _ => {}
                }

                fwd(AgentEvent::ThreadEvent {
                    thread_id: tid.clone(),
                    alias: a.clone(),
                    event: Box::new(event.clone()),
                });
            });
        }

        if self.orchestrator.thread_semaphore_available() == 0 {
            if let Some(ref fwd) = forward_fn {
                fwd(AgentEvent::ThreadQueued {
                    thread_id: thread_id.clone(),
                    alias: request.alias.clone(),
                });
            }
        }

        let _permit = self.orchestrator.acquire_thread_permit().await;

        if let Some(ref fwd) = forward_fn {
            fwd(AgentEvent::ThreadStart {
                thread_id: thread_id.clone(),
                alias: request.alias.clone(),
                task: request.task.clone(),
                model: resolved_model_id.clone(),
            });
        }

        let start = std::time::Instant::now();
        let run_result = tokio::time::timeout(
            std::time::Duration::from_secs(request.timeout_secs),
            run_thread(&agent, &request.task, signal),
        )
        .await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let timed_out = run_result.is_err();
        if timed_out {
            agent.abort();
        }

        let outcome = if timed_out {
            match tokio::time::timeout(std::time::Duration::from_millis(100), outcome_rx).await {
                Ok(Ok(outcome)) => outcome,
                _ => ThreadOutcome::TimedOut,
            }
        } else {
            match outcome_rx.try_recv() {
                Ok(outcome) => outcome,
                Err(_) => ThreadOutcome::TimedOut,
            }
        };

        if let ThreadOutcome::Completed { ref evidence, .. } = outcome {
            if !evidence.is_empty() {
                if let Some(ref fwd) = forward_fn {
                    fwd(AgentEvent::EvidenceCite {
                        thread_alias: request.alias.clone(),
                        thread_id: thread_id.clone(),
                        tool_call_ids: evidence.clone(),
                    });
                }
            }
        }

        if let Some(ref fwd) = forward_fn {
            fwd(AgentEvent::ThreadEnd {
                thread_id: thread_id.clone(),
                alias: request.alias.clone(),
                outcome: outcome.clone(),
                duration_ms,
            });
        }

        let mut worktree_branch: Option<String> = None;
        let mut worktree_diff_stat: Option<String> = None;

        if let Some(ref wt) = worktree_info {
            worktree_branch = Some(wt.branch.clone());

            if let Ok(repo_root) = worktree::find_repo_root(&main_cwd) {
                match worktree::auto_commit(&wt.path, &request.alias, &thread_id) {
                    Ok(true) => {
                        worktree_diff_stat = worktree::diff_stat(&repo_root, &wt.branch).ok();
                    }
                    Ok(false) => {}
                    Err(e) => eprintln!("[thread] auto-commit failed: {}", e),
                }
                worktree::remove_worktree(&repo_root, &wt.path);
            }
        }

        let final_messages = agent.with_state(|s| s.messages.clone());
        let ep_worktree = worktree_branch.as_ref().map(|branch| EpisodeWorktreeInfo {
            branch: branch.clone(),
            diff_summary: worktree_diff_stat.clone(),
        });
        let episode = generate_episode(
            thread_id.clone(),
            &request.alias,
            &request.task,
            &final_messages,
            &outcome,
            duration_ms,
            ep_worktree,
        );

        self.orchestrator
            .record_episode(episode.clone(), final_messages);

        let mut details = json!({
            "thread_id": thread_id,
            "alias": request.alias,
            "outcome": {
                "kind": outcome.status_str(),
                "text": outcome.result_text(),
            },
            "duration_ms": duration_ms,
            "turns": episode.turn_count,
            "is_reuse": lookup.is_reuse,
        });
        if let Some(ref branch) = worktree_branch {
            details["branch"] = json!(branch);
            details["diff_stat"] = json!(worktree_diff_stat.as_deref().unwrap_or("(no changes)"));
        }

        Ok(ThreadRunResult {
            trace: episode.full_trace,
            details,
        })
    }

    pub async fn run_query(&self, request: QueryRequest) -> anyhow::Result<QueryResult> {
        let alias = request
            .alias
            .unwrap_or_else(|| format!("query-{}", self.orchestrator.next_thread_id()));
        let model = self.resolve_model(request.model_override.as_deref(), "search")?;

        if let Some(fwd) = self
            .event_forwarder
            .as_ref()
            .and_then(|cell| cell.lock().ok().and_then(|g| g.clone()))
        {
            fwd(AgentEvent::QueryStart {
                query_id: alias.clone(),
                prompt: request.prompt.clone(),
                model: model.id.clone(),
            });
        }

        let api_key = if let Some(ref get_key) = self.get_api_key {
            (get_key)(model.provider.clone()).await
        } else {
            None
        };

        let context = ai::types::Context {
            system_prompt: Some(
                "You are a helpful assistant. Answer concisely and directly.".to_string(),
            ),
            messages: vec![ai::types::Message::User(ai::types::UserMessage::new(
                &request.prompt,
            ))],
            tools: None,
        };

        let opts = ai::types::SimpleStreamOptions {
            reasoning: None,
            thinking_budgets: None,
            base: ai::types::StreamOptions {
                api_key,
                ..Default::default()
            },
        };

        let start = std::time::Instant::now();
        let event_stream = ai::stream_simple(&model, &context, Some(&opts))?;
        let mut pinned = Box::pin(event_stream);
        let mut response_text = String::new();

        while let Some(event) = pinned.next().await {
            match event {
                ai::types::AssistantMessageEvent::Done { message, .. } => {
                    for block in &message.content {
                        if let ai::types::ContentBlock::Text { text, .. } = block {
                            response_text.push_str(text);
                        }
                    }
                    break;
                }
                ai::types::AssistantMessageEvent::Error { error, .. } => {
                    if let Some(err) = &error.error_message {
                        return Ok(QueryResult {
                            output: format!("Query error: {}", err),
                            thread_id: String::new(),
                            alias,
                            duration_ms: start.elapsed().as_millis() as u64,
                            details: None,
                        });
                    }
                    break;
                }
                _ => {}
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        if let Some(fwd) = self
            .event_forwarder
            .as_ref()
            .and_then(|cell| cell.lock().ok().and_then(|g| g.clone()))
        {
            fwd(AgentEvent::QueryEnd {
                query_id: alias.clone(),
                output: response_text.clone(),
                duration_ms,
            });
        }

        let thread_id = self.orchestrator.next_thread_id();
        self.orchestrator.get_or_create_thread(&alias, "");
        let episode = Episode {
            thread_id: thread_id.clone(),
            alias: alias.clone(),
            task: request.prompt.clone(),
            outcome: ThreadOutcome::Completed {
                result: response_text.clone(),
                evidence: vec![],
            },
            full_trace: format!(
                "--- Query: {} ---\nPROMPT: {}\nOUTPUT: {}\n",
                alias, request.prompt, response_text
            ),
            compact_trace: format!(
                "--- Query: {} ---\nPROMPT: {}\nOUTPUT: {}\n",
                alias, request.prompt, response_text
            ),
            duration_ms,
            turn_count: 1,
            branch: None,
            diff_summary: None,
        };
        self.orchestrator.record_episode(episode, vec![]);

        Ok(QueryResult {
            output: response_text,
            thread_id: thread_id.clone(),
            alias: alias.clone(),
            duration_ms,
            details: Some(json!({
                "thread_id": thread_id,
                "alias": alias,
                "duration_ms": duration_ms,
            })),
        })
    }

    pub fn document_op(&self, request: DocumentRequest) -> AgentToolResult {
        match request {
            DocumentRequest::List => {
                let names = self.orchestrator.list_documents();
                let text = if names.is_empty() {
                    "(no documents)".to_string()
                } else {
                    names.join("\n")
                };
                self.emit_document_op("list", "", &text);
                AgentToolResult {
                    content: vec![UserBlock::Text { text }],
                    details: Some(json!({"operation": "list", "count": names.len()})),
                }
            }
            DocumentRequest::Read { name } => match self.orchestrator.read_document(&name) {
                Some(text) => {
                    let bytes = text.len();
                    self.emit_document_op("read", &name, &text);
                    AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({"operation": "read", "name": name, "bytes": bytes})),
                    }
                }
                None => {
                    self.emit_document_op("read", &name, "");
                    AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: format!("Document '{}' not found.", name),
                        }],
                        details: Some(json!({"operation": "read", "name": name, "error": true})),
                    }
                }
            },
            DocumentRequest::Write { name, content } => {
                let bytes = content.len();
                self.orchestrator.write_document(&name, content.clone());
                self.emit_document_op("write", &name, &content);
                AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("Wrote {} bytes to '{}'.", bytes, name),
                    }],
                    details: Some(json!({"operation": "write", "name": name, "bytes": bytes})),
                }
            }
            DocumentRequest::Append { name, content } => {
                let bytes = content.len();
                self.orchestrator.append_document(&name, &content);
                self.emit_document_op("append", &name, &content);
                AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("Appended {} bytes to '{}'.", bytes, name),
                    }],
                    details: Some(json!({"operation": "append", "name": name, "bytes": bytes})),
                }
            }
        }
    }

    pub fn log_message(&self, message: &str) -> AgentToolResult {
        let entry = format!("[log] {}\n", message);
        self.orchestrator
            .append_document("_orchestration_log", &entry);

        AgentToolResult {
            content: vec![UserBlock::Text {
                text: format!("Logged: {}", message),
            }],
            details: Some(json!({"message": message})),
        }
    }

    pub fn lookup_episode(&self, alias: &str) -> AgentToolResult {
        match self.orchestrator.get_episode(alias) {
            Some(episode) => AgentToolResult {
                content: vec![UserBlock::Text {
                    text: episode.compact_trace,
                }],
                details: Some(json!({
                    "alias": alias,
                    "outcome": episode.outcome.status_str(),
                    "duration_ms": episode.duration_ms,
                    "turn_count": episode.turn_count,
                })),
            },
            None => AgentToolResult {
                content: vec![UserBlock::Text {
                    text: format!("No episode found for alias '{}'.", alias),
                }],
                details: Some(json!({"alias": alias, "error": true})),
            },
        }
    }

    pub fn diff_branch(&self, alias: &str) -> anyhow::Result<BranchDiffResult> {
        let cwd = std::env::current_dir()?;
        self.diff_branch_from_repo(&worktree::find_repo_root(&cwd)?, alias)
    }

    pub fn diff_branch_from_repo(
        &self,
        repo_root: &std::path::Path,
        alias: &str,
    ) -> anyhow::Result<BranchDiffResult> {
        let branch = branch_for_alias(alias);
        let stat = worktree::diff_stat(repo_root, &branch)?;
        let diff = worktree::diff_full(repo_root, &branch, 50_000)?;
        let (files_changed, insertions, deletions) = worktree::parse_stat_summary(&stat);
        Ok(BranchDiffResult {
            branch,
            stat,
            diff,
            files_changed,
            insertions,
            deletions,
        })
    }

    pub fn merge_branch(&self, alias: &str) -> anyhow::Result<BranchMergeResult> {
        let cwd = std::env::current_dir()?;
        self.merge_branch_from_repo(&worktree::find_repo_root(&cwd)?, alias)
    }

    pub fn merge_branch_from_repo(
        &self,
        repo_root: &std::path::Path,
        alias: &str,
    ) -> anyhow::Result<BranchMergeResult> {
        let branch = branch_for_alias(alias);
        let (success, conflicts, message) = worktree::merge_branch(repo_root, &branch)?;
        Ok(BranchMergeResult {
            success,
            conflicts,
            message,
            branch,
        })
    }

    pub fn list_branches(&self) -> anyhow::Result<Vec<String>> {
        let cwd = std::env::current_dir()?;
        self.list_branches_from_repo(&worktree::find_repo_root(&cwd)?)
    }

    pub fn list_branches_from_repo(
        &self,
        repo_root: &std::path::Path,
    ) -> anyhow::Result<Vec<String>> {
        worktree::list_branches(repo_root)
    }

    fn emit_document_op(&self, op: &str, name: &str, content: &str) {
        let Some(event_forwarder) = &self.event_forwarder else {
            return;
        };
        if let Some(forward) = event_forwarder.lock().ok().and_then(|guard| guard.clone()) {
            forward(AgentEvent::DocumentOp {
                thread_alias: self.thread_alias.clone(),
                op: op.to_string(),
                name: name.to_string(),
                content: content.to_string(),
            });
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
                if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                    self.runtime.log_message(msg);
                }
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

async fn run_thread(
    agent: &Agent,
    task: &str,
    parent_signal: Option<CancellationToken>,
) -> anyhow::Result<()> {
    if let Some(sig) = parent_signal {
        tokio::select! {
            result = agent.prompt(task) => result,
            _ = sig.cancelled() => {
                agent.abort();
                Ok(())
            }
        }
    } else {
        agent.prompt(task).await
    }
}

fn build_thread_system_prompt(
    tool_names: &[String],
    cwd: &str,
    wt: Option<&worktree::WorktreeInfo>,
) -> String {
    let mut parts = Vec::new();

    parts.push(THREAD_IDENTITY.to_string());

    if let Some(wt) = wt {
        parts.push(format!(
            "# Worktree isolation\n\
             You are working in an isolated git worktree on branch `{}`.\n\
             Your changes are isolated from other threads — there is no risk of conflict.\n\
             Changes are auto-committed when you call complete.",
            wt.branch
        ));
    }

    let has = |name: &str| tool_names.iter().any(|n| n == name);
    let mut guidelines = Vec::new();
    if has("bash") {
        guidelines.push(
            "Use bash for commands that require shell execution. Prefer dedicated tools when available."
                .to_string(),
        );
    }
    if has("file_read") && has("file_edit") {
        guidelines
            .push("Read files before editing them. Follow the edit format precisely.".to_string());
    }
    if !guidelines.is_empty() {
        let mut section = "# Tool guidelines".to_string();
        for g in &guidelines {
            section.push_str(&format!("\n- {}", g));
        }
        parts.push(section);
    }

    parts.push(format!("# Environment\nCurrent working directory: {}", cwd));

    parts.join("\n\n")
}

pub fn expand_capabilities(names: &[String]) -> Vec<String> {
    let mut tools = Vec::new();
    let registry = tools::ToolRegistry::new();
    for name in names {
        if let Some(expanded) = registry.capability_tools(name.as_str()) {
            tools.extend(expanded);
        } else {
            tools.push(name.to_string());
        }
    }
    tools.sort();
    tools.dedup();
    tools
}

fn effective_system_prompt_for_invocation(
    is_reuse: bool,
    stored_prompt: String,
    new_prompt: String,
    injected_episodes: bool,
) -> String {
    if is_reuse && !injected_episodes {
        stored_prompt
    } else {
        new_prompt
    }
}

fn branch_for_alias(alias: &str) -> String {
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
    format!("tau/{}", sanitized)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agent::thread::{Episode, ThreadOutcome};

    use super::*;
    use crate::orchestration::event_forwarder_cell;

    fn text_of(result: &AgentToolResult) -> &str {
        match &result.content[0] {
            UserBlock::Text { text } => text,
            _ => panic!("expected text"),
        }
    }

    fn make_episode(alias: &str) -> Episode {
        Episode {
            thread_id: "t-0001".to_string(),
            alias: alias.to_string(),
            task: "scan".to_string(),
            outcome: ThreadOutcome::Completed {
                result: "done".to_string(),
                evidence: vec![],
            },
            full_trace: "full".to_string(),
            compact_trace: "compact".to_string(),
            duration_ms: 42,
            turn_count: 3,
            branch: None,
            diff_summary: None,
        }
    }

    fn test_model() -> Model {
        Model {
            id: "mock".into(),
            name: "mock".into(),
            api: "openai-responses".into(),
            provider: "openai".into(),
            base_url: "https://oauth.example.invalid".into(),
            reasoning: false,
            input: vec!["text".into()],
            cost: ai::types::ModelCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 8192,
            max_tokens: 2048,
            headers: Some(std::collections::HashMap::from([(
                "X-Test".to_string(),
                "preserve".to_string(),
            )])),
        }
    }

    #[test]
    fn thread_request_parses_defaults_and_filters_auto_tools() {
        let request = ThreadRequest::from_params(&json!({
            "alias": "worker",
            "task": "scan",
            "tools": ["read", "complete", "document"],
            "episodes": ["prior"],
            "max_turns": 9,
            "worktree": true,
            "worktree_base": "base",
            "worktree_include": ["fixtures"],
        }))
        .unwrap();

        assert_eq!(request.alias, "worker");
        assert_eq!(request.task, "scan");
        assert_eq!(request.raw_tool_names, vec!["read"]);
        assert_eq!(
            request.tool_names,
            vec!["file_read", "glob", "grep"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(request.episode_aliases, vec!["prior"]);
        assert_eq!(request.timeout_secs, 300);
        assert_eq!(request.max_turns, 9);
        assert!(request.use_worktree);
        assert_eq!(request.worktree_base.as_deref(), Some("base"));
        assert_eq!(request.worktree_include, vec!["fixtures"]);
    }

    #[test]
    fn thread_request_validates_required_fields() {
        let err = ThreadRequest::from_params(&json!({"task": "scan"})).unwrap_err();
        assert!(err.to_string().contains("missing 'alias' parameter"));

        let err = ThreadRequest::from_params(&json!({"alias": "worker"})).unwrap_err();
        assert!(err.to_string().contains("missing 'task' parameter"));
    }

    #[test]
    fn query_request_parses_defaults_and_validation() {
        let request = QueryRequest::from_params(&json!({
            "prompt": "classify",
            "alias": "q",
            "model": "search",
        }))
        .unwrap();

        assert_eq!(request.prompt, "classify");
        assert_eq!(request.alias.as_deref(), Some("q"));
        assert_eq!(request.model_override.as_deref(), Some("search"));

        let err = QueryRequest::from_params(&json!({"alias": "q"})).unwrap_err();
        assert!(err.to_string().contains("missing 'prompt' parameter"));
    }

    #[test]
    fn model_slot_resolution_preserves_modified_default_model() {
        let model = test_model();
        let runtime = OrchestrationRuntime::with_agent_config(
            OrchestratorState::new(),
            None,
            model.clone(),
            ModelSlots {
                search: Some(model.id.clone()),
                ..Default::default()
            },
            event_forwarder_cell(),
        );

        let resolved = runtime.resolve_model(Some("search"), "search").unwrap();
        assert_eq!(resolved.id, model.id);
        assert_eq!(resolved.base_url, "https://oauth.example.invalid");
        assert_eq!(
            resolved.headers.as_ref().and_then(|h| h.get("X-Test")),
            Some(&"preserve".to_string())
        );
    }

    #[test]
    fn thread_run_result_converts_to_py_thread_state_json() {
        let result = ThreadRunResult {
            trace: "trace".to_string(),
            details: json!({
                "thread_id": "t-1",
                "alias": "worker",
                "outcome": {"kind": "completed", "text": "done"},
                "duration_ms": 5,
            }),
        };

        assert_eq!(
            result.to_thread_state_json(),
            json!({
                "thread_id": "t-1",
                "alias": "worker",
                "status": "completed",
                "output": "done",
                "reason": "done",
                "completed": true,
                "duration_ms": 5,
                "trace": "trace",
            })
        );
    }

    #[test]
    fn branch_results_shape_python_json() {
        assert_eq!(branch_for_alias("feature/one two"), "tau/feature-one-two");

        let diff = BranchDiffResult {
            branch: "tau/worker".to_string(),
            stat: " 1 file changed, 2 insertions(+), 1 deletion(-)".to_string(),
            diff: "diff --git a/x b/x".to_string(),
            files_changed: 1,
            insertions: 2,
            deletions: 1,
        };
        assert_eq!(
            diff.to_json(),
            json!({
                "branch": "tau/worker",
                "stat": " 1 file changed, 2 insertions(+), 1 deletion(-)",
                "diff": "diff --git a/x b/x",
                "files_changed": 1,
                "insertions": 2,
                "deletions": 1,
            })
        );

        let merge = BranchMergeResult {
            success: false,
            conflicts: vec!["src/lib.rs".to_string()],
            message: "conflict".to_string(),
            branch: "tau/worker".to_string(),
        };
        assert_eq!(
            merge.to_json(),
            json!({
                "success": false,
                "conflicts": ["src/lib.rs"],
                "message": "conflict",
                "branch": "tau/worker",
            })
        );
    }

    #[test]
    fn document_op_preserves_text_and_details() {
        let runtime = OrchestrationRuntime::new(OrchestratorState::new());

        let result = runtime.document_op(DocumentRequest::Write {
            name: "notes".to_string(),
            content: "hello".to_string(),
        });
        assert_eq!(text_of(&result), "Wrote 5 bytes to 'notes'.");
        assert_eq!(
            result.details,
            Some(json!({"operation": "write", "name": "notes", "bytes": 5}))
        );

        let result = runtime.document_op(DocumentRequest::Read {
            name: "notes".to_string(),
        });
        assert_eq!(text_of(&result), "hello");
        assert_eq!(
            result.details,
            Some(json!({"operation": "read", "name": "notes", "bytes": 5}))
        );
    }

    #[test]
    fn document_op_emits_thread_scoped_events() {
        let cell = event_forwarder_cell();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        *cell.lock().unwrap() = Some(Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        }));
        let runtime = OrchestrationRuntime::with_event_forwarder(OrchestratorState::new(), cell)
            .for_thread("worker".to_string());

        runtime.document_op(DocumentRequest::Append {
            name: "notes".to_string(),
            content: "line".to_string(),
        });

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::DocumentOp {
                thread_alias,
                op,
                name,
                content,
            } => {
                assert_eq!(thread_alias.as_deref(), Some("worker"));
                assert_eq!(op, "append");
                assert_eq!(name, "notes");
                assert_eq!(content, "line");
            }
            _ => panic!("expected document event"),
        }
    }

    #[test]
    fn log_message_appends_to_orchestration_log() {
        let orchestrator = OrchestratorState::new();
        let runtime = OrchestrationRuntime::new(orchestrator.clone());

        let result = runtime.log_message("decided");

        assert_eq!(text_of(&result), "Logged: decided");
        assert_eq!(
            orchestrator.read_document("_orchestration_log"),
            Some("[log] decided\n".to_string())
        );
    }

    #[test]
    fn lookup_episode_returns_compact_trace_and_metadata() {
        let orchestrator = OrchestratorState::new();
        orchestrator.get_or_create_thread("scanner", "prompt");
        orchestrator.record_episode(make_episode("scanner"), vec![]);
        let runtime = OrchestrationRuntime::new(orchestrator);

        let result = runtime.lookup_episode("scanner");

        assert_eq!(text_of(&result), "compact");
        assert_eq!(
            result.details,
            Some(json!({
                "alias": "scanner",
                "outcome": "completed",
                "duration_ms": 42,
                "turn_count": 3,
            }))
        );
    }
}
