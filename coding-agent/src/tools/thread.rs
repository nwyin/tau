//! Thread tool: spawn in-process agent threads with episode-based synchronization.
//!
//! Unlike SubagentTool (subprocess), threads run as tokio tasks sharing
//! an OrchestratorState. Named threads support reuse (appending to existing
//! conversation history). Episodes are the primary sync mechanism.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentTool, AgentToolResult, BoxFuture, GetApiKeyFn};
use ai::types::Model;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::ModelSlots;
use crate::orchestration::{
    AgentRuntimeConfig, EventForwarderCell, OrchestrationRuntime, ThreadRequest,
};

pub struct ThreadTool {
    runtime: OrchestrationRuntime,
    config: AgentRuntimeConfig,
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
            runtime: OrchestrationRuntime::with_event_forwarder(orchestrator, event_forwarder),
            config: AgentRuntimeConfig::new(get_api_key, default_model, model_slots),
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
                    "max_turns": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Maximum agent turns before the thread is forced to stop (default: 25). Increase this for long-running reactive or research threads."
                    },
                    "worktree": {
                        "type": "boolean",
                        "description": "If true, run in an isolated git worktree on its own branch. Use for write-heavy threads to prevent conflicts with other parallel threads. Default: false."
                    },
                    "worktree_base": {
                        "type": "string",
                        "description": "Alias of a prior worktree thread whose branch to use as the base for this thread's worktree. E.g. worktree_base='worker-1' bases this thread on branch tau/worker-1. Default: branch from HEAD."
                    },
                    "worktree_include": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Paths (relative to repo root) to copy into the worktree. Use for untracked directories like test suites that aren't in git. E.g. ['_reference/test', 'fixtures']."
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
        let runtime = self.runtime.clone();
        let config = self.config.clone();

        Box::pin(async move {
            let request = ThreadRequest::from_params(&params)?;
            let result = runtime.execute_thread(&config, request, signal).await?;
            Ok(result.to_agent_tool_result())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::orchestration::{event_forwarder_cell, runtime::expand_capabilities};

    fn test_model() -> Model {
        Model {
            id: "mock".into(),
            name: "mock".into(),
            api: "openai-responses".into(),
            provider: "openai".into(),
            base_url: "https://example.invalid".into(),
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
            headers: None,
        }
    }

    #[test]
    fn thread_schema_exposes_max_turns() {
        let tool = ThreadTool::new(
            OrchestratorState::new(),
            None,
            test_model(),
            event_forwarder_cell(),
            ModelSlots::default(),
        );

        let max_turns = tool
            .parameters()
            .get("properties")
            .and_then(|v| v.get("max_turns"))
            .expect("thread schema should expose max_turns");

        assert_eq!(
            max_turns.get("type").and_then(|v| v.as_str()),
            Some("integer")
        );
    }

    #[test]
    fn expand_capabilities_deduplicates_and_expands() {
        let expanded = expand_capabilities(&[
            "read".to_string(),
            "write".to_string(),
            "file_read".to_string(),
        ]);

        assert_eq!(
            expanded,
            vec![
                "file_edit".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
                "glob".to_string(),
                "grep".to_string(),
            ]
        );
    }
}
