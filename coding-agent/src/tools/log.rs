//! Log tool: record progress messages in the orchestration trace.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::orchestration::{LogRequest, OrchestrationRuntime};

pub struct LogTool {
    runtime: OrchestrationRuntime,
}

impl LogTool {
    pub fn new(orchestrator: Arc<OrchestratorState>) -> Self {
        Self {
            runtime: OrchestrationRuntime::new(orchestrator),
        }
    }

    pub fn arc(orchestrator: Arc<OrchestratorState>) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(orchestrator))
    }
}

impl AgentTool for LogTool {
    fn name(&self) -> &str {
        "log"
    }

    fn label(&self) -> &str {
        "Log"
    }

    fn description(&self) -> &str {
        "Record a progress message or decision note in the orchestration trace. \
         Use to track what you're doing and why between thread/query calls."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Progress message or decision note to record."
                    }
                },
                "required": ["message"]
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
            let request = LogRequest::from_params(&params)?;
            Ok(runtime.log_message(request))
        })
    }
}
