use std::path::PathBuf;
use std::sync::Arc;

use agent::completion_tools::{self, AbortTool, CompleteTool, EscalateTool};
use agent::episode::{generate_episode, EpisodeWorktreeInfo};
use agent::orchestrator::{OrchestratorState, ThreadLookup};
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
type EventForwarder = Arc<dyn Fn(AgentEvent) + Send + Sync>;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRequest {
    pub message: String,
}

impl LogRequest {
    pub fn from_params(params: &Value) -> anyhow::Result<Self> {
        Ok(Self {
            message: params
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'message' parameter"))?
                .to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeLookupRequest {
    pub alias: String,
}

impl EpisodeLookupRequest {
    pub fn from_params(params: &Value) -> anyhow::Result<Self> {
        Ok(Self {
            alias: params
                .get("alias")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'alias' parameter"))?
                .to_string(),
        })
    }
}

#[derive(Clone)]
pub struct AgentRuntimeConfig {
    get_api_key: Option<GetApiKeyFn>,
    default_model: Model,
    model_slots: ModelSlots,
}

impl AgentRuntimeConfig {
    pub fn new(
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        model_slots: ModelSlots,
    ) -> Self {
        Self {
            get_api_key,
            default_model,
            model_slots,
        }
    }

    fn get_api_key(&self) -> Option<GetApiKeyFn> {
        self.get_api_key.clone()
    }

    fn resolve_model(
        &self,
        override_or_slot: Option<&str>,
        default_slot: &str,
    ) -> anyhow::Result<Model> {
        let default_model = self.default_model.clone();
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
}

struct PreparedThreadContext {
    thread_id: String,
    main_cwd: PathBuf,
    worktree_info: Option<worktree::WorktreeInfo>,
    effective_cwd: PathBuf,
    cwd: String,
}

struct PreparedThreadInvocation {
    lookup: ThreadLookup,
    effective_system_prompt: String,
}

struct FinalizedWorktree {
    branch: Option<String>,
    diff_stat: Option<String>,
}

#[derive(Clone)]
pub struct OrchestrationRuntime {
    orchestrator: Arc<OrchestratorState>,
    event_forwarder: Option<EventForwarderCell>,
}

impl OrchestrationRuntime {
    pub fn new(orchestrator: Arc<OrchestratorState>) -> Self {
        Self {
            orchestrator,
            event_forwarder: None,
        }
    }

    pub fn with_event_forwarder(
        orchestrator: Arc<OrchestratorState>,
        event_forwarder: EventForwarderCell,
    ) -> Self {
        Self {
            orchestrator,
            event_forwarder: Some(event_forwarder),
        }
    }

    fn current_forwarder(&self) -> Option<EventForwarder> {
        self.event_forwarder
            .as_ref()
            .and_then(|cell| cell.lock().ok().and_then(|g| g.clone()))
    }

    fn prepare_thread_context(&self, request: &ThreadRequest) -> PreparedThreadContext {
        let thread_id = self.orchestrator.next_thread_id();
        let main_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let worktree_info = if request.use_worktree {
            create_thread_worktree(request, &main_cwd, &thread_id)
        } else {
            None
        };
        let effective_cwd = worktree_info
            .as_ref()
            .map(|wt| wt.path.clone())
            .unwrap_or_else(|| main_cwd.clone());
        let cwd = effective_cwd.to_string_lossy().to_string();

        PreparedThreadContext {
            thread_id,
            main_cwd,
            worktree_info,
            effective_cwd,
            cwd,
        }
    }

    fn prepare_thread_invocation(
        &self,
        request: &ThreadRequest,
        context: &PreparedThreadContext,
        forward_fn: Option<&EventForwarder>,
    ) -> PreparedThreadInvocation {
        let mut system_prompt = build_thread_system_prompt(
            &request.tool_names,
            &context.cwd,
            context.worktree_info.as_ref(),
        );

        let mut injected_episodes = false;
        if let Some(prior_section) = self
            .orchestrator
            .format_prior_episodes(&request.episode_aliases)
        {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&prior_section);
            injected_episodes = true;

            if let Some(fwd) = forward_fn {
                fwd(AgentEvent::EpisodeInject {
                    source_aliases: request.episode_aliases.clone(),
                    target_alias: request.alias.clone(),
                    target_thread_id: context.thread_id.clone(),
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

        PreparedThreadInvocation {
            lookup,
            effective_system_prompt,
        }
    }

    fn build_thread_tools(
        &self,
        request: &ThreadRequest,
        context: &PreparedThreadContext,
        outcome_signal: completion_tools::OutcomeSignal,
    ) -> Vec<Arc<dyn AgentTool>> {
        let mut thread_tools: Vec<Arc<dyn AgentTool>> = if context.worktree_info.is_some() {
            tools::tools_from_allowlist_with_cwd(&request.tool_names, context.effective_cwd.clone())
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
        thread_tools
    }

    fn build_thread_agent(
        &self,
        config: &AgentRuntimeConfig,
        model: Model,
        effective_system_prompt: String,
        thread_tools: Vec<Arc<dyn AgentTool>>,
        max_turns: u32,
    ) -> Agent {
        let model_for_compact = model.clone();
        let transform_context: agent::types::TransformContextFn =
            Arc::new(move |messages, _cancel| {
                let m = model_for_compact.clone();
                Box::pin(async move { agent::context::compact_messages(messages, &m) })
            });

        Agent::new(AgentOptions {
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
            get_api_key: config.get_api_key(),
            thinking_budgets: None,
            max_turns: Some(max_turns),
        })
    }

    fn subscribe_thread_events(
        &self,
        agent: &Agent,
        forward_fn: Option<&EventForwarder>,
        thread_id: &str,
        alias: &str,
    ) {
        let Some(fwd) = forward_fn.cloned() else {
            return;
        };
        let tid = thread_id.to_string();
        let a = alias.to_string();
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

    fn finalize_thread_worktree(
        &self,
        request: &ThreadRequest,
        context: &PreparedThreadContext,
    ) -> FinalizedWorktree {
        let mut branch = None;
        let mut diff_stat = None;

        if let Some(ref wt) = context.worktree_info {
            branch = Some(wt.branch.clone());

            if let Ok(repo_root) = worktree::find_repo_root(&context.main_cwd) {
                match worktree::auto_commit(&wt.path, &request.alias, &context.thread_id) {
                    Ok(true) => {
                        diff_stat = worktree::diff_stat(&repo_root, &wt.branch).ok();
                    }
                    Ok(false) => {}
                    Err(e) => eprintln!("[thread] auto-commit failed: {}", e),
                }
                worktree::remove_worktree(&repo_root, &wt.path);
            }
        }

        FinalizedWorktree { branch, diff_stat }
    }

    fn record_thread_episode(
        &self,
        request: &ThreadRequest,
        context: &PreparedThreadContext,
        outcome: &ThreadOutcome,
        duration_ms: u64,
        worktree: &FinalizedWorktree,
        final_messages: Vec<agent::types::AgentMessage>,
    ) -> Episode {
        let ep_worktree = worktree.branch.as_ref().map(|branch| EpisodeWorktreeInfo {
            branch: branch.clone(),
            diff_summary: worktree.diff_stat.clone(),
        });
        let episode = generate_episode(
            context.thread_id.clone(),
            &request.alias,
            &request.task,
            &final_messages,
            outcome,
            duration_ms,
            ep_worktree,
        );

        self.orchestrator
            .record_episode(episode.clone(), final_messages);
        episode
    }

    fn record_query_episode(
        &self,
        alias: &str,
        prompt: &str,
        response_text: &str,
        duration_ms: u64,
    ) -> String {
        let thread_id = self.orchestrator.next_thread_id();
        self.orchestrator.get_or_create_thread(alias, "");
        let trace = format!(
            "--- Query: {} ---\nPROMPT: {}\nOUTPUT: {}\n",
            alias, prompt, response_text
        );
        let episode = Episode {
            thread_id: thread_id.clone(),
            alias: alias.to_string(),
            task: prompt.to_string(),
            outcome: ThreadOutcome::Completed {
                result: response_text.to_string(),
                evidence: vec![],
            },
            full_trace: trace.clone(),
            compact_trace: trace,
            duration_ms,
            turn_count: 1,
            branch: None,
            diff_summary: None,
        };
        self.orchestrator.record_episode(episode, vec![]);
        thread_id
    }

    pub async fn execute_thread(
        &self,
        config: &AgentRuntimeConfig,
        request: ThreadRequest,
        signal: Option<CancellationToken>,
    ) -> anyhow::Result<ThreadRunResult> {
        let model = config.resolve_model(request.model_override.as_deref(), "subagent")?;
        let resolved_model_id = model.id.clone();
        let context = self.prepare_thread_context(&request);
        let forward_fn = self.current_forwarder();
        let invocation = self.prepare_thread_invocation(&request, &context, forward_fn.as_ref());
        let is_reuse = invocation.lookup.is_reuse;

        let (outcome_signal, mut outcome_rx) = completion_tools::outcome_channel();
        let thread_tools = self.build_thread_tools(&request, &context, outcome_signal);
        let agent = self.build_thread_agent(
            config,
            model,
            invocation.effective_system_prompt,
            thread_tools,
            request.max_turns,
        );

        if is_reuse {
            agent.replace_messages(invocation.lookup.messages);
        }

        self.subscribe_thread_events(
            &agent,
            forward_fn.as_ref(),
            &context.thread_id,
            &request.alias,
        );

        if self.orchestrator.thread_semaphore_available() == 0 {
            if let Some(ref fwd) = forward_fn {
                fwd(AgentEvent::ThreadQueued {
                    thread_id: context.thread_id.clone(),
                    alias: request.alias.clone(),
                });
            }
        }

        let _permit = self.orchestrator.acquire_thread_permit().await;

        if let Some(ref fwd) = forward_fn {
            fwd(AgentEvent::ThreadStart {
                thread_id: context.thread_id.clone(),
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
                        thread_id: context.thread_id.clone(),
                        tool_call_ids: evidence.clone(),
                    });
                }
            }
        }

        if let Some(ref fwd) = forward_fn {
            fwd(AgentEvent::ThreadEnd {
                thread_id: context.thread_id.clone(),
                alias: request.alias.clone(),
                outcome: outcome.clone(),
                duration_ms,
            });
        }

        let worktree = self.finalize_thread_worktree(&request, &context);
        let final_messages = agent.with_state(|s| s.messages.clone());
        let episode = self.record_thread_episode(
            &request,
            &context,
            &outcome,
            duration_ms,
            &worktree,
            final_messages,
        );

        let mut details = json!({
            "thread_id": context.thread_id,
            "alias": request.alias,
            "outcome": {
                "kind": outcome.status_str(),
                "text": outcome.result_text(),
            },
            "duration_ms": duration_ms,
            "turns": episode.turn_count,
            "is_reuse": is_reuse,
        });
        if let Some(ref branch) = worktree.branch {
            details["branch"] = json!(branch);
            details["diff_stat"] = json!(worktree.diff_stat.as_deref().unwrap_or("(no changes)"));
        }

        Ok(ThreadRunResult {
            trace: episode.full_trace,
            details,
        })
    }

    pub async fn run_query(
        &self,
        config: &AgentRuntimeConfig,
        request: QueryRequest,
    ) -> anyhow::Result<QueryResult> {
        let alias = request
            .alias
            .unwrap_or_else(|| format!("query-{}", self.orchestrator.next_thread_id()));
        let model = config.resolve_model(request.model_override.as_deref(), "search")?;

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

        let api_key = if let Some(ref get_key) = config.get_api_key {
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

        let thread_id =
            self.record_query_episode(&alias, &request.prompt, &response_text, duration_ms);

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
        self.document_op_for_thread(None, request)
    }

    pub fn document_op_for_thread(
        &self,
        thread_alias: Option<&str>,
        request: DocumentRequest,
    ) -> AgentToolResult {
        match request {
            DocumentRequest::List => {
                let names = self.orchestrator.list_documents();
                let text = if names.is_empty() {
                    "(no documents)".to_string()
                } else {
                    names.join("\n")
                };
                self.emit_document_op(thread_alias, "list", "", &text);
                AgentToolResult {
                    content: vec![UserBlock::Text { text }],
                    details: Some(json!({"operation": "list", "count": names.len()})),
                }
            }
            DocumentRequest::Read { name } => match self.orchestrator.read_document(&name) {
                Some(text) => {
                    let bytes = text.len();
                    self.emit_document_op(thread_alias, "read", &name, &text);
                    AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({"operation": "read", "name": name, "bytes": bytes})),
                    }
                }
                None => {
                    self.emit_document_op(thread_alias, "read", &name, "");
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
                self.emit_document_op(thread_alias, "write", &name, &content);
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
                self.emit_document_op(thread_alias, "append", &name, &content);
                AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("Appended {} bytes to '{}'.", bytes, name),
                    }],
                    details: Some(json!({"operation": "append", "name": name, "bytes": bytes})),
                }
            }
        }
    }

    pub fn log_message(&self, request: LogRequest) -> AgentToolResult {
        let entry = format!("[log] {}\n", request.message);
        self.orchestrator
            .append_document("_orchestration_log", &entry);

        AgentToolResult {
            content: vec![UserBlock::Text {
                text: format!("Logged: {}", request.message),
            }],
            details: Some(json!({"message": request.message})),
        }
    }

    pub fn lookup_episode(&self, request: EpisodeLookupRequest) -> AgentToolResult {
        match self.orchestrator.get_episode(&request.alias) {
            Some(episode) => AgentToolResult {
                content: vec![UserBlock::Text {
                    text: episode.compact_trace,
                }],
                details: Some(json!({
                    "alias": request.alias,
                    "outcome": episode.outcome.status_str(),
                    "duration_ms": episode.duration_ms,
                    "turn_count": episode.turn_count,
                })),
            },
            None => AgentToolResult {
                content: vec![UserBlock::Text {
                    text: format!("No episode found for alias '{}'.", request.alias),
                }],
                details: Some(json!({"alias": request.alias, "error": true})),
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

    fn emit_document_op(&self, thread_alias: Option<&str>, op: &str, name: &str, content: &str) {
        let Some(event_forwarder) = &self.event_forwarder else {
            return;
        };
        if let Some(forward) = event_forwarder.lock().ok().and_then(|guard| guard.clone()) {
            forward(AgentEvent::DocumentOp {
                thread_alias: thread_alias.map(String::from),
                op: op.to_string(),
                name: name.to_string(),
                content: content.to_string(),
            });
        }
    }
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

fn create_thread_worktree(
    request: &ThreadRequest,
    main_cwd: &std::path::Path,
    thread_id: &str,
) -> Option<worktree::WorktreeInfo> {
    match worktree::find_repo_root(main_cwd) {
        Ok(repo_root) => match worktree::create_worktree(
            &repo_root,
            &request.alias,
            thread_id,
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
        },
        Err(_) => {
            eprintln!("[thread] not in a git repo, skipping worktree isolation");
            None
        }
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
    fn log_and_lookup_requests_validate_required_fields() {
        let log = LogRequest::from_params(&json!({"message": "checkpoint"})).unwrap();
        assert_eq!(log.message, "checkpoint");
        assert!(LogRequest::from_params(&json!({}))
            .unwrap_err()
            .to_string()
            .contains("missing 'message' parameter"));

        let lookup = EpisodeLookupRequest::from_params(&json!({"alias": "scanner"})).unwrap();
        assert_eq!(lookup.alias, "scanner");
        assert!(EpisodeLookupRequest::from_params(&json!({}))
            .unwrap_err()
            .to_string()
            .contains("missing 'alias' parameter"));
    }

    #[test]
    fn model_slot_resolution_preserves_modified_default_model() {
        let model = test_model();
        let config = AgentRuntimeConfig::new(
            None,
            model.clone(),
            ModelSlots {
                search: Some(model.id.clone()),
                ..Default::default()
            },
        );

        let resolved = config.resolve_model(Some("search"), "search").unwrap();
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
            crate::orchestration::rpc::build_thread_result_json(&result.to_agent_tool_result()),
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
        let runtime = OrchestrationRuntime::with_event_forwarder(OrchestratorState::new(), cell);

        runtime.document_op_for_thread(
            Some("worker"),
            DocumentRequest::Append {
                name: "notes".to_string(),
                content: "line".to_string(),
            },
        );

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

        let result = runtime.log_message(LogRequest {
            message: "decided".to_string(),
        });

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

        let result = runtime.lookup_episode(EpisodeLookupRequest {
            alias: "scanner".to_string(),
        });

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
