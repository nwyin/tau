//! Completion tools for thread termination signaling.
//!
//! These tools are available ONLY inside threads. They signal how the thread
//! terminated by sending a `ThreadOutcome` through a shared oneshot channel.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex as TokioMutex;

use crate::thread::ThreadOutcome;
use crate::types::{AgentTool, AgentToolResult, BoxFuture};

/// Shared channel for a thread to signal its outcome.
/// The `Mutex<Option<>>` pattern allows exactly one send.
pub type OutcomeSignal = Arc<TokioMutex<Option<tokio::sync::oneshot::Sender<ThreadOutcome>>>>;

/// Create a new outcome signal pair: (signal for tools, receiver for thread spawner).
pub fn outcome_channel() -> (OutcomeSignal, tokio::sync::oneshot::Receiver<ThreadOutcome>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (Arc::new(TokioMutex::new(Some(tx))), rx)
}

// ---------------------------------------------------------------------------
// CompleteTool
// ---------------------------------------------------------------------------

pub struct CompleteTool {
    signal: OutcomeSignal,
}

impl CompleteTool {
    pub fn new(signal: OutcomeSignal) -> Self {
        Self { signal }
    }

    pub fn arc(signal: OutcomeSignal) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(signal))
    }
}

impl AgentTool for CompleteTool {
    fn name(&self) -> &str {
        "complete"
    }
    fn label(&self) -> &str {
        "Complete"
    }
    fn description(&self) -> &str {
        "Signal that the thread's task is done. Call this when you have completed the assigned task. Do not call any other tools in the same turn."
    }
    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "result": {
                        "type": "string",
                        "description": "A concise summary of what was accomplished and the key findings."
                    },
                    "evidence": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tool_call_ids that support the conclusion."
                    }
                },
                "required": ["result"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let outcome_signal = self.signal.clone();
        let result = params
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let evidence: Vec<String> = params
            .get("evidence")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Box::pin(async move {
            let outcome = ThreadOutcome::Completed { result, evidence };
            send_outcome(&outcome_signal, outcome).await;
            Ok(AgentToolResult {
                content: vec![ai::types::UserBlock::Text {
                    text: "[thread completed]".to_string(),
                }],
                details: None,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// AbortTool
// ---------------------------------------------------------------------------

pub struct AbortTool {
    signal: OutcomeSignal,
}

impl AbortTool {
    pub fn new(signal: OutcomeSignal) -> Self {
        Self { signal }
    }

    pub fn arc(signal: OutcomeSignal) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(signal))
    }
}

impl AgentTool for AbortTool {
    fn name(&self) -> &str {
        "abort"
    }
    fn label(&self) -> &str {
        "Abort"
    }
    fn description(&self) -> &str {
        "Signal that the thread cannot complete its task. Call this when you encounter an unrecoverable problem. Do not call any other tools in the same turn."
    }
    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Why the task cannot be completed."
                    }
                },
                "required": ["reason"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let outcome_signal = self.signal.clone();
        let reason = params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Box::pin(async move {
            let outcome = ThreadOutcome::Aborted { reason };
            send_outcome(&outcome_signal, outcome).await;
            Ok(AgentToolResult {
                content: vec![ai::types::UserBlock::Text {
                    text: "[thread aborted]".to_string(),
                }],
                details: None,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// EscalateTool
// ---------------------------------------------------------------------------

pub struct EscalateTool {
    signal: OutcomeSignal,
}

impl EscalateTool {
    pub fn new(signal: OutcomeSignal) -> Self {
        Self { signal }
    }

    pub fn arc(signal: OutcomeSignal) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(signal))
    }
}

impl AgentTool for EscalateTool {
    fn name(&self) -> &str {
        "escalate"
    }
    fn label(&self) -> &str {
        "Escalate"
    }
    fn description(&self) -> &str {
        "Signal that the thread needs human input to proceed. Call this when you cannot resolve an ambiguity or need a decision. Do not call any other tools in the same turn."
    }
    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "problem": {
                        "type": "string",
                        "description": "What needs human input or decision."
                    }
                },
                "required": ["problem"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let outcome_signal = self.signal.clone();
        let problem = params
            .get("problem")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Box::pin(async move {
            let outcome = ThreadOutcome::Escalated { problem };
            send_outcome(&outcome_signal, outcome).await;
            Ok(AgentToolResult {
                content: vec![ai::types::UserBlock::Text {
                    text: "[thread escalated]".to_string(),
                }],
                details: None,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn send_outcome(signal: &OutcomeSignal, outcome: ThreadOutcome) {
    let mut guard = signal.lock().await;
    if let Some(tx) = guard.take() {
        let _ = tx.send(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_sends_outcome() {
        let (signal, rx) = outcome_channel();
        let tool = CompleteTool::new(signal);
        let params = json!({"result": "Found 3 endpoints", "evidence": ["tc1", "tc2"]});
        let result = tool
            .execute("call1".into(), params, None)
            .await
            .unwrap();

        assert_eq!(result.content.len(), 1);
        let outcome = rx.await.unwrap();
        match outcome {
            ThreadOutcome::Completed { result, evidence } => {
                assert_eq!(result, "Found 3 endpoints");
                assert_eq!(evidence, vec!["tc1", "tc2"]);
            }
            _ => panic!("expected Completed"),
        }
    }

    #[tokio::test]
    async fn test_abort_sends_outcome() {
        let (signal, rx) = outcome_channel();
        let tool = AbortTool::new(signal);
        let params = json!({"reason": "No access"});
        tool.execute("call1".into(), params, None)
            .await
            .unwrap();

        let outcome = rx.await.unwrap();
        match outcome {
            ThreadOutcome::Aborted { reason } => assert_eq!(reason, "No access"),
            _ => panic!("expected Aborted"),
        }
    }

    #[tokio::test]
    async fn test_escalate_sends_outcome() {
        let (signal, rx) = outcome_channel();
        let tool = EscalateTool::new(signal);
        let params = json!({"problem": "Which database?"});
        tool.execute("call1".into(), params, None)
            .await
            .unwrap();

        let outcome = rx.await.unwrap();
        match outcome {
            ThreadOutcome::Escalated { problem } => assert_eq!(problem, "Which database?"),
            _ => panic!("expected Escalated"),
        }
    }

    #[tokio::test]
    async fn test_double_send_is_safe() {
        let (signal, rx) = outcome_channel();
        let tool1 = CompleteTool::new(signal.clone());
        let tool2 = AbortTool::new(signal);

        tool1
            .execute("c1".into(), json!({"result": "first"}), None)
            .await
            .unwrap();
        // Second send is a no-op (sender already taken)
        tool2
            .execute("c2".into(), json!({"reason": "second"}), None)
            .await
            .unwrap();

        let outcome = rx.await.unwrap();
        // First one wins
        assert!(matches!(outcome, ThreadOutcome::Completed { .. }));
    }
}
