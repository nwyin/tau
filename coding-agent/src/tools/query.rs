//! Query tool: single-shot LLM call without tools.
//!
//! For quick classification, decision, or extraction tasks where a full
//! agent loop with tools is unnecessary.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentTool, AgentToolResult, BoxFuture, GetApiKeyFn};
use ai::types::Model;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::ModelSlots;
use crate::orchestration::{
    AgentRuntimeConfig, EventForwarderCell, OrchestrationRuntime, QueryRequest,
};

pub struct QueryTool {
    runtime: OrchestrationRuntime,
    config: AgentRuntimeConfig,
}

impl QueryTool {
    pub fn new(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
        model_slots: ModelSlots,
        event_forwarder: EventForwarderCell,
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
        model_slots: ModelSlots,
        event_forwarder: EventForwarderCell,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(
            orchestrator,
            get_api_key,
            default_model,
            model_slots,
            event_forwarder,
        ))
    }
}

impl AgentTool for QueryTool {
    fn name(&self) -> &str {
        "query"
    }

    fn label(&self) -> &str {
        "Query"
    }

    fn description(&self) -> &str {
        "Single-shot LLM call without tools. Use for quick classification, decisions, \
         summarization, or extraction tasks that don't need tool access. Faster and cheaper \
         than spawning a thread."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "alias": {
                        "type": "string",
                        "description": "Optional name for this query, so its result can be referenced by threads via the episodes parameter."
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The question or instruction for the LLM."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model slot name (search, reasoning) or raw model ID. Defaults to search slot."
                    }
                },
                "required": ["prompt"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let runtime = self.runtime.clone();
        let config = self.config.clone();

        Box::pin(async move {
            let request = QueryRequest::from_params(&params)?;
            let result = runtime.run_query(&config, request).await?;
            Ok(result.to_agent_tool_result())
        })
    }
}
