//! Thread tool: spawn in-process agent threads with episode-based synchronization.
//!
//! Unlike SubagentTool (subprocess), threads run as tokio tasks sharing
//! an OrchestratorState. Named threads support reuse (appending to existing
//! conversation history). Episodes are the primary sync mechanism.

use std::sync::Arc;

use agent::completion_tools::{self, AbortTool, CompleteTool, EscalateTool};
use agent::episode::generate_episode;
use agent::orchestrator::OrchestratorState;
use agent::thread::ThreadOutcome;
use agent::types::{AgentTool, AgentToolResult, BoxFuture, GetApiKeyFn, ToolUpdateFn};
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::{Model, UserBlock};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::tools;

const THREAD_IDENTITY: &str = include_str!("../../prompts/thread_identity.md");

/// Default tools for threads when none specified.
const DEFAULT_THREAD_TOOLS: &[&str] = &["file_read", "grep", "glob"];

pub struct ThreadTool {
    orchestrator: Arc<OrchestratorState>,
    get_api_key: Option<GetApiKeyFn>,
    default_model: Model,
    edit_mode: String,
}

impl ThreadTool {
    pub fn new(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        edit_mode: String,
    ) -> Self {
        Self {
            orchestrator,
            get_api_key,
            default_model,
            edit_mode,
        }
    }

    pub fn arc(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        edit_mode: String,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            orchestrator,
            get_api_key,
            default_model,
            edit_mode,
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
                        "description": "Tool names this thread can use. Defaults to [\"file_read\", \"grep\", \"glob\"]. Available: bash, file_read, file_edit, file_write, glob, grep, web_fetch, web_search."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model override (e.g. 'claude-haiku-4-5' for cheap exploration)."
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
            let tool_names: Vec<String> = params
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_THREAD_TOOLS.iter().map(|s| s.to_string()).collect());
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

            // Resolve model
            let model = if let Some(ref model_id) = model_override {
                ai::models::find_model(model_id)
                    .map(|m| (*m).clone())
                    .unwrap_or_else(|| {
                        eprintln!("[thread] model '{}' not found, using default", model_id);
                        default_model.clone()
                    })
            } else {
                default_model.clone()
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
