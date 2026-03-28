//! Thread tool: spawn in-process agent threads with episode-based synchronization.
//!
//! Unlike SubagentTool (subprocess), threads run as tokio tasks sharing
//! an OrchestratorState. Named threads support reuse (appending to existing
//! conversation history). Episodes are the primary sync mechanism.

use std::sync::{Arc, Mutex};

use agent::completion_tools::{self, AbortTool, CompleteTool, EscalateTool};
use agent::episode::generate_episode;
use agent::orchestrator::OrchestratorState;
use agent::thread::ThreadOutcome;
use agent::types::{AgentEvent, AgentTool, AgentToolResult, BoxFuture, GetApiKeyFn, ToolUpdateFn};
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::{Model, UserBlock};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::ModelSlots;
use crate::tools;

const THREAD_IDENTITY: &str = include_str!("../../prompts/thread_identity.md");

/// Default tools for threads when none specified.
const DEFAULT_THREAD_TOOLS: &[&str] = &["file_read", "grep", "glob"];

/// Shared cell for event forwarding. Populated after agent creation.
pub type EventForwarderCell = Arc<Mutex<Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>>>;

/// Create an empty event forwarder cell.
pub fn event_forwarder_cell() -> EventForwarderCell {
    Arc::new(Mutex::new(None))
}

pub struct ThreadTool {
    orchestrator: Arc<OrchestratorState>,
    get_api_key: Option<GetApiKeyFn>,
    default_model: Model,
    edit_mode: String,
    event_forwarder: EventForwarderCell,
    model_slots: ModelSlots,
}

impl ThreadTool {
    pub fn new(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        edit_mode: String,
        event_forwarder: EventForwarderCell,
        model_slots: ModelSlots,
    ) -> Self {
        Self {
            orchestrator,
            get_api_key,
            default_model,
            edit_mode,
            event_forwarder,
            model_slots,
        }
    }

    pub fn arc(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        edit_mode: String,
        event_forwarder: EventForwarderCell,
        model_slots: ModelSlots,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            orchestrator,
            get_api_key,
            default_model,
            edit_mode,
            event_forwarder,
            model_slots,
        ))
    }
}

impl AgentTool for ThreadTool {
    fn name(&self) -> &str {
        "thread"
    }

    fn label(&self) -> &str {
        "Thread"
    }

    fn description(&self) -> &str {
        "Spawn a worker thread to execute a bounded task. Threads run in-process with their own \
         context window and restricted tools. Reusing an alias appends to the existing thread's \
         conversation, giving it memory of previous actions. Multiple thread calls in the same \
         turn execute in parallel. Use for: decomposing work into focused subtasks, parallel \
         exploration, iterative refinement via thread reuse."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "alias": {
                        "type": "string",
                        "description": "Name for this thread. Reusing an alias appends to the existing thread's conversation, giving it memory of previous actions."
                    },
                    "task": {
                        "type": "string",
                        "description": "Complete, self-contained task description."
                    },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Capabilities or tool names. Capabilities: read (file_read+grep+glob), write (file_read+file_edit+file_write), terminal (bash), web (web_fetch+web_search), full (all). Raw tool names also accepted. Defaults to read. document/complete/abort/escalate are always available."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model slot name (search, subagent, reasoning) or raw model ID. Defaults to subagent slot."
                    },
                    "episodes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Aliases of prior threads whose episodes should be injected as context for this thread."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 120)."
                    }
                },
                "required": ["alias", "task"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let orchestrator = self.orchestrator.clone();
        let get_api_key = self.get_api_key.clone();
        let default_model = self.default_model.clone();
        let edit_mode = self.edit_mode.clone();
        let event_forwarder = self.event_forwarder.clone();
        let model_slots = self.model_slots.clone();

        Box::pin(async move {
            // Parse parameters
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
            // Names of tools that are always injected — filter from allowlist lookup
            // to avoid "unknown tool" warnings.
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
            let model_override = params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);
            let episode_aliases: Vec<String> = params
                .get("episodes")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let timeout_secs = params
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(120);

            // Resolve model: slot name → slot config → find_model, or raw ID → find_model
            let default_model_id = &default_model.id;
            let model = if let Some(ref model_param) = model_override {
                let resolved_id = if ModelSlots::is_slot(model_param) {
                    model_slots.resolve(model_param, default_model_id)
                } else {
                    model_param.clone()
                };
                ai::models::find_model(&resolved_id)
                    .map(|m| (*m).clone())
                    .unwrap_or_else(|| {
                        eprintln!("[thread] model '{}' not found, using default", resolved_id);
                        default_model.clone()
                    })
            } else {
                // No override — use subagent slot
                let subagent_id = model_slots.resolve("subagent", default_model_id);
                ai::models::find_model(&subagent_id)
                    .map(|m| (*m).clone())
                    .unwrap_or(default_model.clone())
            };

            // Generate thread ID
            let thread_id = orchestrator.next_thread_id();

            // Build thread system prompt
            let cwd = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                .to_string_lossy()
                .to_string();
            let mut system_prompt = build_thread_system_prompt(&tool_names, &cwd);

            // Inject prior episodes if specified
            if let Some(prior_section) = orchestrator.format_prior_episodes(&episode_aliases) {
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&prior_section);
            }

            // Get or create thread state
            let lookup = orchestrator.get_or_create_thread(&alias, &system_prompt);

            // Build tool list: requested tools + completion tools
            let (outcome_signal, mut outcome_rx) = completion_tools::outcome_channel();
            let mut thread_tools: Vec<Arc<dyn AgentTool>> =
                tools::tools_from_allowlist(&tool_names, &edit_mode);
            thread_tools.push(CompleteTool::arc(outcome_signal.clone()));
            thread_tools.push(AbortTool::arc(outcome_signal.clone()));
            thread_tools.push(EscalateTool::arc(outcome_signal));
            thread_tools.push(tools::DocumentTool::arc(orchestrator.clone()));

            // Capture model ID before model is moved into Agent
            let resolved_model_id = model.id.clone();

            // Build compaction callback
            let model_for_compact = model.clone();
            let transform_context: agent::types::TransformContextFn =
                Arc::new(move |messages, _cancel| {
                    let m = model_for_compact.clone();
                    Box::pin(async move { agent::context::compact_messages(messages, &m) })
                });

            // Create the thread agent
            let agent = Agent::new(AgentOptions {
                initial_state: Some(AgentStateInit {
                    model: Some(model),
                    system_prompt: Some(if lookup.is_reuse {
                        lookup.system_prompt
                    } else {
                        system_prompt
                    }),
                    tools: Some(thread_tools),
                    thinking_level: Some(agent::types::ThinkingLevel::Off),
                }),
                convert_to_llm: None,
                transform_context: Some(transform_context),
                stream_fn: None,
                steering_mode: None,
                follow_up_mode: None,
                session_id: None,
                get_api_key,
                thinking_budgets: None,
                transport: None,
                max_retry_delay_ms: None,
                max_turns: Some(25),
            });

            // Restore conversation history for reused threads
            if lookup.is_reuse {
                agent.replace_messages(lookup.messages);
            }

            // Subscribe to inner agent events and forward to parent
            let forward_fn = event_forwarder.lock().unwrap().clone();
            if let Some(ref fwd) = forward_fn {
                let fwd = fwd.clone();
                let tid = thread_id.clone();
                let a = alias.clone();
                let _unsub = agent.subscribe(move |event| {
                    // Forward inner tool execution events with thread context
                    match event {
                        AgentEvent::ToolExecutionStart { .. }
                        | AgentEvent::ToolExecutionEnd { .. } => {
                            // Wrap as a thread-scoped event by re-emitting directly
                            fwd(event.clone());
                        }
                        _ => {
                            // Skip agent lifecycle events from inner agent to avoid
                            // confusion with the outer agent's lifecycle
                            let _ = (&tid, &a);
                        }
                    }
                });
            }

            // Emit ThreadStart
            if let Some(ref fwd) = forward_fn {
                fwd(AgentEvent::ThreadStart {
                    thread_id: thread_id.clone(),
                    alias: alias.clone(),
                    task: task.clone(),
                    model: resolved_model_id.clone(),
                });
            }

            // Run the thread with timeout
            let start = std::time::Instant::now();
            let run_result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                run_thread(&agent, &task, signal),
            )
            .await;
            let duration_ms = start.elapsed().as_millis() as u64;

            // Determine outcome
            let timed_out = run_result.is_err();
            if timed_out {
                agent.abort();
            }

            let outcome = if timed_out {
                ThreadOutcome::TimedOut
            } else {
                match outcome_rx.try_recv() {
                    Ok(outcome) => outcome,
                    Err(_) => ThreadOutcome::TimedOut, // loop ended without completion tool
                }
            };

            // Emit ThreadEnd
            if let Some(ref fwd) = forward_fn {
                fwd(AgentEvent::ThreadEnd {
                    thread_id: thread_id.clone(),
                    alias: alias.clone(),
                    outcome: outcome.clone(),
                    duration_ms,
                });
            }

            // Extract messages and generate episode
            let final_messages = agent.with_state(|s| s.messages.clone());
            let episode = generate_episode(
                thread_id.clone(),
                &alias,
                &task,
                &final_messages,
                &outcome,
                duration_ms,
            );

            // Record in orchestrator state
            orchestrator.record_episode(episode.clone(), final_messages);

            // Build tool result
            Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: episode.full_trace,
                }],
                details: Some(json!({
                    "thread_id": thread_id,
                    "alias": alias,
                    "outcome": {
                        "kind": outcome.status_str(),
                        "text": outcome.result_text(),
                    },
                    "duration_ms": duration_ms,
                    "turns": episode.turn_count,
                    "is_reuse": lookup.is_reuse,
                })),
            })
        })
    }
}

/// Run the thread agent, respecting parent cancellation.
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

/// Build the thread's system prompt from identity + tool descriptions + env.
fn build_thread_system_prompt(tool_names: &[String], cwd: &str) -> String {
    let mut parts = Vec::new();

    parts.push(THREAD_IDENTITY.to_string());

    // Tool usage hints
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

/// Expand capability aliases into concrete tool names.
///
/// Capabilities: read, write, terminal, web, full.
/// Raw tool names pass through unchanged. Duplicates are removed.
fn expand_capabilities(names: &[String]) -> Vec<String> {
    let mut tools = Vec::new();
    for name in names {
        match name.as_str() {
            "read" => tools.extend(["file_read", "grep", "glob"].map(String::from)),
            "write" => tools.extend(["file_read", "file_edit", "file_write"].map(String::from)),
            "terminal" => tools.push("bash".to_string()),
            "web" => tools.extend(["web_fetch", "web_search"].map(String::from)),
            "full" => tools.extend(
                [
                    "bash",
                    "file_read",
                    "file_edit",
                    "file_write",
                    "glob",
                    "grep",
                    "web_fetch",
                    "web_search",
                ]
                .map(String::from),
            ),
            other => tools.push(other.to_string()),
        }
    }
    tools.sort();
    tools.dedup();
    tools
}
