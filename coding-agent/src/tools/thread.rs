//! Thread tool: spawn in-process agent threads with episode-based synchronization.
//!
//! Unlike SubagentTool (subprocess), threads run as tokio tasks sharing
//! an OrchestratorState. Named threads support reuse (appending to existing
//! conversation history). Episodes are the primary sync mechanism.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent::completion_tools::{self, AbortTool, CompleteTool, EscalateTool};
use agent::episode::{generate_episode, EpisodeWorktreeInfo};
use agent::orchestrator::OrchestratorState;
use agent::thread::ThreadOutcome;
use agent::types::{AgentEvent, AgentTool, AgentToolResult, BoxFuture, GetApiKeyFn};
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::{Model, UserBlock};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::ModelSlots;
use crate::tools;
use crate::tools::worktree;

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
    event_forwarder: EventForwarderCell,
    model_slots: ModelSlots,
}

impl ThreadTool {
    pub fn new(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        event_forwarder: EventForwarderCell,
        model_slots: ModelSlots,
    ) -> Self {
        Self {
            orchestrator,
            get_api_key,
            default_model,
            event_forwarder,
            model_slots,
        }
    }

    pub fn arc(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        event_forwarder: EventForwarderCell,
        model_slots: ModelSlots,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            orchestrator,
            get_api_key,
            default_model,
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
                        "description": "Timeout in seconds (default: 300)."
                    },
                    "worktree": {
                        "type": "boolean",
                        "description": "If true, run in an isolated git worktree on its own branch. Use for write-heavy threads to prevent conflicts with other parallel threads. Default: false."
                    },
                    "worktree_base": {
                        "type": "string",
                        "description": "Alias of a prior worktree thread whose branch to use as the base for this thread's worktree. E.g. worktree_base='worker-1' bases this thread on branch tau/worker-1. Default: branch from HEAD."
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
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let orchestrator = self.orchestrator.clone();
        let get_api_key = self.get_api_key.clone();
        let default_model = self.default_model.clone();
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
                .unwrap_or(300);
            let use_worktree = params
                .get("worktree")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let worktree_base = params
                .get("worktree_base")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Resolve model: slot name → slot config → find_model, or raw ID → find_model.
            // Important: when the resolved ID matches the default model, use default_model
            // directly to preserve OAuth base_url/header modifications from agent_builder.
            let default_model_id = &default_model.id;
            let resolve_model = |resolved_id: &str| -> Model {
                if resolved_id == default_model_id.as_str() {
                    default_model.clone()
                } else {
                    ai::models::find_model(resolved_id)
                        .map(|m| (*m).clone())
                        .unwrap_or_else(|| {
                            eprintln!("[thread] model '{}' not found, using default", resolved_id);
                            default_model.clone()
                        })
                }
            };
            let model = if let Some(ref model_param) = model_override {
                let resolved_id = if ModelSlots::is_slot(model_param) {
                    model_slots.resolve(model_param, default_model_id)
                } else {
                    model_param.clone()
                };
                resolve_model(&resolved_id)
            } else {
                // No override — use subagent slot
                let subagent_id = model_slots.resolve("subagent", default_model_id);
                resolve_model(&subagent_id)
            };

            // Generate thread ID
            let thread_id = orchestrator.next_thread_id();

            // Resolve the main working directory
            let main_cwd =
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

            // Create worktree if requested
            let worktree_info: Option<worktree::WorktreeInfo> = if use_worktree {
                match worktree::find_repo_root(&main_cwd) {
                    Ok(repo_root) => {
                        match worktree::create_worktree(
                            &repo_root,
                            &alias,
                            &thread_id,
                            worktree_base.as_deref(),
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

            // Resolve effective CWD (worktree path or main cwd)
            let effective_cwd = worktree_info
                .as_ref()
                .map(|wt| wt.path.clone())
                .unwrap_or_else(|| main_cwd.clone());
            let cwd = effective_cwd.to_string_lossy().to_string();

            // Build thread system prompt
            let mut system_prompt =
                build_thread_system_prompt(&tool_names, &cwd, worktree_info.as_ref());

            // Resolve event forwarder early so we can emit EpisodeInject
            let forward_fn = event_forwarder.lock().ok().and_then(|g| g.clone());

            // Inject prior episodes if specified
            if let Some(prior_section) = orchestrator.format_prior_episodes(&episode_aliases) {
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&prior_section);

                // Emit EpisodeInject event
                if let Some(ref fwd) = forward_fn {
                    fwd(AgentEvent::EpisodeInject {
                        source_aliases: episode_aliases.clone(),
                        target_alias: alias.clone(),
                        target_thread_id: thread_id.clone(),
                    });
                }
            }

            // Get or create thread state
            let lookup = orchestrator.get_or_create_thread(&alias, &system_prompt);

            // On reuse with new episodes, update the stored system prompt
            if lookup.is_reuse && !episode_aliases.is_empty() {
                orchestrator.update_system_prompt(&alias, system_prompt.clone());
            }

            // Build tool list: requested tools + completion tools
            // Use cwd-overridden tools when running in a worktree
            let (outcome_signal, mut outcome_rx) = completion_tools::outcome_channel();
            let mut thread_tools: Vec<Arc<dyn AgentTool>> = if worktree_info.is_some() {
                tools::tools_from_allowlist_with_cwd(&tool_names, effective_cwd.clone())
            } else {
                tools::tools_from_allowlist(&tool_names)
            };
            thread_tools.push(CompleteTool::arc(outcome_signal.clone()));
            thread_tools.push(AbortTool::arc(outcome_signal.clone()));
            thread_tools.push(EscalateTool::arc(outcome_signal));
            thread_tools.push(tools::DocumentTool::arc_for_thread(
                orchestrator.clone(),
                event_forwarder.clone(),
                alias.clone(),
            ));
            thread_tools.push(tools::LogTool::arc(orchestrator.clone()));
            thread_tools.push(tools::FromIdTool::arc(orchestrator.clone()));

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
                max_turns: Some(25),
            });

            // Restore conversation history for reused threads
            if lookup.is_reuse {
                agent.replace_messages(lookup.messages);
            }

            // Subscribe to inner agent events and forward to parent
            if let Some(ref fwd) = forward_fn {
                let fwd = fwd.clone();
                let tid = thread_id.clone();
                let a = alias.clone();
                let _unsub = agent.subscribe(move |event| {
                    // Forward tool events with thread identity baked in (for main chat display)
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

                    // Also forward ALL events wrapped in ThreadEvent for the inspector modal
                    fwd(AgentEvent::ThreadEvent {
                        thread_id: tid.clone(),
                        alias: a.clone(),
                        event: Box::new(event.clone()),
                    });
                });
            }

            // Emit ThreadQueued if semaphore is full
            if orchestrator.thread_semaphore_available() == 0 {
                if let Some(ref fwd) = forward_fn {
                    fwd(AgentEvent::ThreadQueued {
                        thread_id: thread_id.clone(),
                        alias: alias.clone(),
                    });
                }
            }

            // Acquire permit (blocks if at capacity)
            let _permit = orchestrator.acquire_thread_permit().await;

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
                // Grace period: completion tool may still be sending outcome
                match tokio::time::timeout(std::time::Duration::from_millis(100), outcome_rx).await
                {
                    Ok(Ok(outcome)) => outcome,
                    _ => ThreadOutcome::TimedOut,
                }
            } else {
                match outcome_rx.try_recv() {
                    Ok(outcome) => outcome,
                    Err(_) => ThreadOutcome::TimedOut, // loop ended without completion tool
                }
            };

            // Emit EvidenceCite if thread completed with evidence
            if let ThreadOutcome::Completed { ref evidence, .. } = outcome {
                if !evidence.is_empty() {
                    if let Some(ref fwd) = forward_fn {
                        fwd(AgentEvent::EvidenceCite {
                            thread_alias: alias.clone(),
                            thread_id: thread_id.clone(),
                            tool_call_ids: evidence.clone(),
                        });
                    }
                }
            }

            // Emit ThreadEnd
            if let Some(ref fwd) = forward_fn {
                fwd(AgentEvent::ThreadEnd {
                    thread_id: thread_id.clone(),
                    alias: alias.clone(),
                    outcome: outcome.clone(),
                    duration_ms,
                });
            }

            // Worktree cleanup: auto-commit, capture diff, remove worktree
            let mut worktree_branch: Option<String> = None;
            let mut worktree_diff_stat: Option<String> = None;

            if let Some(ref wt) = worktree_info {
                worktree_branch = Some(wt.branch.clone());

                if let Ok(repo_root) = worktree::find_repo_root(&main_cwd) {
                    // Auto-commit any changes made by the thread
                    match worktree::auto_commit(&wt.path, &alias, &thread_id) {
                        Ok(true) => {
                            worktree_diff_stat =
                                worktree::diff_stat(&repo_root, &wt.branch).ok();
                        }
                        Ok(false) => {} // no changes
                        Err(e) => eprintln!("[thread] auto-commit failed: {}", e),
                    }
                    // Remove the worktree directory (keep the branch)
                    worktree::remove_worktree(&repo_root, &wt.path);
                }
            }

            // Extract messages and generate episode
            let final_messages = agent.with_state(|s| s.messages.clone());
            let ep_worktree = worktree_branch.as_ref().map(|branch| EpisodeWorktreeInfo {
                branch: branch.clone(),
                diff_summary: worktree_diff_stat.clone(),
            });
            let episode = generate_episode(
                thread_id.clone(),
                &alias,
                &task,
                &final_messages,
                &outcome,
                duration_ms,
                ep_worktree,
            );

            // Record in orchestrator state
            orchestrator.record_episode(episode.clone(), final_messages);

            // Build tool result
            let mut details = json!({
                "thread_id": thread_id,
                "alias": alias,
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
                details["diff_stat"] =
                    json!(worktree_diff_stat.as_deref().unwrap_or("(no changes)"));
            }

            Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: episode.full_trace,
                }],
                details: Some(details),
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
fn build_thread_system_prompt(
    tool_names: &[String],
    cwd: &str,
    wt: Option<&worktree::WorktreeInfo>,
) -> String {
    let mut parts = Vec::new();

    parts.push(THREAD_IDENTITY.to_string());

    // Worktree isolation notice
    if let Some(wt) = wt {
        parts.push(format!(
            "# Worktree isolation\n\
             You are working in an isolated git worktree on branch `{}`.\n\
             Your changes are isolated from other threads — there is no risk of conflict.\n\
             Changes are auto-committed when you call complete.",
            wt.branch
        ));
    }

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
