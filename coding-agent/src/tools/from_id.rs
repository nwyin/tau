//! FromId tool: retrieve a prior thread/query episode by alias.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct FromIdTool {
    orchestrator: Arc<OrchestratorState>,
}

impl FromIdTool {
    pub fn new(orchestrator: Arc<OrchestratorState>) -> Self {
        Self { orchestrator }
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
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let orchestrator = self.orchestrator.clone();

        Box::pin(async move {
            let alias = params
                .get("alias")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'alias' parameter"))?;

            match orchestrator.get_episode(alias) {
                Some(episode) => Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: episode.compact_trace,
                    }],
                    details: Some(json!({
                        "alias": alias,
                        "outcome": episode.outcome.status_str(),
                        "duration_ms": episode.duration_ms,
                        "turn_count": episode.turn_count,
                    })),
                }),
                None => Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("No episode found for alias '{}'.", alias),
                    }],
                    details: Some(json!({"alias": alias, "error": true})),
                }),
            }
        })
    }
}
