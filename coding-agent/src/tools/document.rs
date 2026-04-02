//! Document tool: read, write, append, and list shared virtual documents.
//!
//! Virtual documents live in OrchestratorState and provide inter-thread
//! data sharing without touching the real filesystem. Always injected
//! into threads alongside completion tools.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::types::{AgentEvent, AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::thread::EventForwarderCell;

pub struct DocumentTool {
    orchestrator: Arc<OrchestratorState>,
    event_forwarder: EventForwarderCell,
    thread_alias: Option<String>,
}

impl DocumentTool {
    pub fn new(orchestrator: Arc<OrchestratorState>, event_forwarder: EventForwarderCell) -> Self {
        Self {
            orchestrator,
            event_forwarder,
            thread_alias: None,
        }
    }

    pub fn arc(
        orchestrator: Arc<OrchestratorState>,
        event_forwarder: EventForwarderCell,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(orchestrator, event_forwarder))
    }

    pub fn arc_for_thread(
        orchestrator: Arc<OrchestratorState>,
        event_forwarder: EventForwarderCell,
        alias: String,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self {
            orchestrator,
            event_forwarder,
            thread_alias: Some(alias),
        })
    }
}

impl AgentTool for DocumentTool {
    fn name(&self) -> &str {
        "document"
    }

    fn label(&self) -> &str {
        "Document"
    }

    fn description(&self) -> &str {
        "Read, write, or append to shared virtual documents for inter-thread data sharing. \
         Documents persist across thread calls within the same session."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["read", "write", "append", "list"],
                        "description": "read: return document contents. write: create or overwrite a document. append: add content to the end of a document. list: show all document names."
                    },
                    "name": {
                        "type": "string",
                        "description": "Document name (required for read/write/append, ignored for list)."
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write or append (required for write/append)."
                    }
                },
                "required": ["operation"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let orchestrator = self.orchestrator.clone();
        let event_forwarder = self.event_forwarder.clone();
        let thread_alias = self.thread_alias.clone();

        Box::pin(async move {
            let operation = params
                .get("operation")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'operation' parameter"))?;
            let name = params.get("name").and_then(|v| v.as_str());
            let content = params.get("content").and_then(|v| v.as_str());

            // Helper to emit from inside the async block
            let emit = |op: &str, name: &str, content: &str| {
                if let Some(fwd) = event_forwarder.lock().ok().and_then(|g| g.clone()) {
                    fwd(AgentEvent::DocumentOp {
                        thread_alias: thread_alias.clone(),
                        op: op.to_string(),
                        name: name.to_string(),
                        content: content.to_string(),
                    });
                }
            };

            match operation {
                "list" => {
                    let names = orchestrator.list_documents();
                    let text = if names.is_empty() {
                        "(no documents)".to_string()
                    } else {
                        names.join("\n")
                    };
                    emit("list", "", &text);
                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text { text }],
                        details: Some(json!({"operation": "list", "count": names.len()})),
                    })
                }

                "read" => {
                    let name = name
                        .ok_or_else(|| anyhow::anyhow!("'name' is required for read operation"))?;
                    match orchestrator.read_document(name) {
                        Some(text) => {
                            let bytes = text.len();
                            emit("read", name, &text);
                            Ok(AgentToolResult {
                                content: vec![UserBlock::Text { text }],
                                details: Some(
                                    json!({"operation": "read", "name": name, "bytes": bytes}),
                                ),
                            })
                        }
                        None => {
                            emit("read", name, "");
                            Ok(AgentToolResult {
                                content: vec![UserBlock::Text {
                                    text: format!("Document '{}' not found.", name),
                                }],
                                details: Some(
                                    json!({"operation": "read", "name": name, "error": true}),
                                ),
                            })
                        }
                    }
                }

                "write" => {
                    let name = name
                        .ok_or_else(|| anyhow::anyhow!("'name' is required for write operation"))?;
                    let content = content.ok_or_else(|| {
                        anyhow::anyhow!("'content' is required for write operation")
                    })?;
                    let bytes = content.len();
                    orchestrator.write_document(name, content.to_string());
                    emit("write", name, content);
                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: format!("Wrote {} bytes to '{}'.", bytes, name),
                        }],
                        details: Some(json!({"operation": "write", "name": name, "bytes": bytes})),
                    })
                }

                "append" => {
                    let name = name.ok_or_else(|| {
                        anyhow::anyhow!("'name' is required for append operation")
                    })?;
                    let content = content.ok_or_else(|| {
                        anyhow::anyhow!("'content' is required for append operation")
                    })?;
                    let bytes = content.len();
                    orchestrator.append_document(name, content);
                    emit("append", name, content);
                    Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: format!("Appended {} bytes to '{}'.", bytes, name),
                        }],
                        details: Some(json!({"operation": "append", "name": name, "bytes": bytes})),
                    })
                }

                _ => Err(anyhow::anyhow!(
                    "Unknown operation '{}'. Use: read, write, append, list.",
                    operation
                )),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::thread::event_forwarder_cell;
    use agent::orchestrator::OrchestratorState;

    fn make_tool() -> DocumentTool {
        DocumentTool::new(OrchestratorState::new(), event_forwarder_cell())
    }

    async fn exec(tool: &DocumentTool, params: Value) -> AgentToolResult {
        tool.execute("test".to_string(), params, None)
            .await
            .unwrap()
    }

    fn text_of(result: &AgentToolResult) -> &str {
        match &result.content[0] {
            UserBlock::Text { text } => text,
            _ => panic!("expected text"),
        }
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let tool = make_tool();

        let result = exec(
            &tool,
            json!({"operation": "write", "name": "spec", "content": "hello world"}),
        )
        .await;
        assert!(text_of(&result).contains("Wrote 11 bytes"));

        let result = exec(&tool, json!({"operation": "read", "name": "spec"})).await;
        assert_eq!(text_of(&result), "hello world");
    }

    #[tokio::test]
    async fn test_append() {
        let tool = make_tool();

        exec(
            &tool,
            json!({"operation": "append", "name": "log", "content": "line 1\n"}),
        )
        .await;
        exec(
            &tool,
            json!({"operation": "append", "name": "log", "content": "line 2\n"}),
        )
        .await;

        let result = exec(&tool, json!({"operation": "read", "name": "log"})).await;
        assert_eq!(text_of(&result), "line 1\nline 2\n");
    }

    #[tokio::test]
    async fn test_read_nonexistent() {
        let tool = make_tool();
        let result = exec(&tool, json!({"operation": "read", "name": "nope"})).await;
        assert!(text_of(&result).contains("not found"));
    }

    #[tokio::test]
    async fn test_list_empty() {
        let tool = make_tool();
        let result = exec(&tool, json!({"operation": "list"})).await;
        assert_eq!(text_of(&result), "(no documents)");
    }

    #[tokio::test]
    async fn test_list_populated() {
        let tool = make_tool();
        exec(
            &tool,
            json!({"operation": "write", "name": "beta", "content": "b"}),
        )
        .await;
        exec(
            &tool,
            json!({"operation": "write", "name": "alpha", "content": "a"}),
        )
        .await;

        let result = exec(&tool, json!({"operation": "list"})).await;
        assert_eq!(text_of(&result), "alpha\nbeta");
    }

    #[tokio::test]
    async fn test_missing_name_on_read() {
        let tool = make_tool();
        let err = tool
            .execute("test".to_string(), json!({"operation": "read"}), None)
            .await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("'name' is required"));
    }

    #[tokio::test]
    async fn test_missing_content_on_write() {
        let tool = make_tool();
        let err = tool
            .execute(
                "test".to_string(),
                json!({"operation": "write", "name": "x"}),
                None,
            )
            .await;
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("'content' is required"));
    }
}
