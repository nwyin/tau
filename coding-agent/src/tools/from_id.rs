//! FromId tool: retrieve a prior thread/query episode by alias.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::orchestration::{EpisodeLookupRequest, OrchestrationRuntime};

pub struct FromIdTool {
    runtime: OrchestrationRuntime,
}

impl FromIdTool {
    pub fn new(orchestrator: Arc<OrchestratorState>) -> Self {
        Self {
            runtime: OrchestrationRuntime::new(orchestrator),
        }
    }

    pub fn arc(orchestrator: Arc<OrchestratorState>) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(orchestrator))
    }
}

impl AgentTool for FromIdTool {
    fn name(&self) -> &str {
        "from_id"
    }

    fn label(&self) -> &str {
        "FromId"
    }

    fn description(&self) -> &str {
        "Retrieve the result of a previously completed thread or query by its alias. \
         Returns the compact trace without re-running the thread."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "alias": {
                        "type": "string",
                        "description": "The alias of a previously completed thread or query."
                    }
                },
                "required": ["alias"]
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

        Box::pin(async move {
            let request = EpisodeLookupRequest::from_params(&params)?;
            Ok(runtime.lookup_episode(request))
        })
    }
}
