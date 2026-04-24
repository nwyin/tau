use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentEvent, AgentToolResult};
use ai::types::UserBlock;
use serde_json::{json, Value};

use crate::orchestration::EventForwarderCell;

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

#[derive(Clone)]
pub struct OrchestrationRuntime {
    orchestrator: Arc<OrchestratorState>,
    event_forwarder: Option<EventForwarderCell>,
    thread_alias: Option<String>,
}

impl OrchestrationRuntime {
    pub fn new(orchestrator: Arc<OrchestratorState>) -> Self {
        Self {
            orchestrator,
            event_forwarder: None,
            thread_alias: None,
        }
    }

    pub fn with_event_forwarder(
        orchestrator: Arc<OrchestratorState>,
        event_forwarder: EventForwarderCell,
    ) -> Self {
        Self {
            orchestrator,
            event_forwarder: Some(event_forwarder),
            thread_alias: None,
        }
    }

    pub fn for_thread(&self, alias: String) -> Self {
        Self {
            orchestrator: self.orchestrator.clone(),
            event_forwarder: self.event_forwarder.clone(),
            thread_alias: Some(alias),
        }
    }

    pub fn document_op(&self, request: DocumentRequest) -> AgentToolResult {
        match request {
            DocumentRequest::List => {
                let names = self.orchestrator.list_documents();
                let text = if names.is_empty() {
                    "(no documents)".to_string()
                } else {
                    names.join("\n")
                };
                self.emit_document_op("list", "", &text);
                AgentToolResult {
                    content: vec![UserBlock::Text { text }],
                    details: Some(json!({"operation": "list", "count": names.len()})),
                }
            }
            DocumentRequest::Read { name } => match self.orchestrator.read_document(&name) {
                Some(text) => {
                    let bytes = text.len();
                    self.emit_document_op("read", &name, &text);
                    AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({"operation": "read", "name": name, "bytes": bytes})),
                    }
                }
                None => {
                    self.emit_document_op("read", &name, "");
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
                self.emit_document_op("write", &name, &content);
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
                self.emit_document_op("append", &name, &content);
                AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("Appended {} bytes to '{}'.", bytes, name),
                    }],
                    details: Some(json!({"operation": "append", "name": name, "bytes": bytes})),
                }
            }
        }
    }

    pub fn log_message(&self, message: &str) -> AgentToolResult {
        let entry = format!("[log] {}\n", message);
        self.orchestrator
            .append_document("_orchestration_log", &entry);

        AgentToolResult {
            content: vec![UserBlock::Text {
                text: format!("Logged: {}", message),
            }],
            details: Some(json!({"message": message})),
        }
    }

    pub fn lookup_episode(&self, alias: &str) -> AgentToolResult {
        match self.orchestrator.get_episode(alias) {
            Some(episode) => AgentToolResult {
                content: vec![UserBlock::Text {
                    text: episode.compact_trace,
                }],
                details: Some(json!({
                    "alias": alias,
                    "outcome": episode.outcome.status_str(),
                    "duration_ms": episode.duration_ms,
                    "turn_count": episode.turn_count,
                })),
            },
            None => AgentToolResult {
                content: vec![UserBlock::Text {
                    text: format!("No episode found for alias '{}'.", alias),
                }],
                details: Some(json!({"alias": alias, "error": true})),
            },
        }
    }

    fn emit_document_op(&self, op: &str, name: &str, content: &str) {
        let Some(event_forwarder) = &self.event_forwarder else {
            return;
        };
        if let Some(forward) = event_forwarder.lock().ok().and_then(|guard| guard.clone()) {
            forward(AgentEvent::DocumentOp {
                thread_alias: self.thread_alias.clone(),
                op: op.to_string(),
                name: name.to_string(),
                content: content.to_string(),
            });
        }
    }
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
        let runtime = OrchestrationRuntime::with_event_forwarder(OrchestratorState::new(), cell)
            .for_thread("worker".to_string());

        runtime.document_op(DocumentRequest::Append {
            name: "notes".to_string(),
            content: "line".to_string(),
        });

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

        let result = runtime.log_message("decided");

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

        let result = runtime.lookup_episode("scanner");

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
