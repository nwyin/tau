//! Todo tool: structured task tracking for multi-step work.
//!
//! The model sends the full todo list on every call. This keeps the protocol
//! trivially simple — no incremental ops, no merge conflicts, just "here's
//! the current state." Completed items can be kept for visual confirmation
//! or dropped on the next update.

use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String, // "pending" | "in_progress" | "completed"
}

pub struct TodoTool;

impl TodoTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

/// Render a todo list for display in the terminal.
pub fn render_todos(todos: &[TodoItem]) -> String {
    if todos.is_empty() {
        return "(empty)".to_string();
    }

    let mut lines = Vec::new();
    for item in todos {
        let icon = match item.status.as_str() {
            "completed" => "✓",
            "in_progress" => "→",
            _ => "○",
        };
        lines.push(format!("  {} {}", icon, item.content));
    }

    let total = todos.len();
    let done = todos.iter().filter(|t| t.status == "completed").count();
    let in_progress = todos.iter().filter(|t| t.status == "in_progress").count();

    let mut header = format!("[{}/{}]", done, total);
    if in_progress > 0 {
        if let Some(active) = todos.iter().find(|t| t.status == "in_progress") {
            header.push_str(&format!(" {}", active.content));
        }
    }

    format!("{}\n{}", header, lines.join("\n"))
}

impl AgentTool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn label(&self) -> &str {
        "Todo"
    }

    fn description(&self) -> &str {
        "Track progress on multi-step tasks. Send the complete todo list each time — \
         the previous list is replaced entirely. Use for tasks requiring 3+ steps. \
         Mark exactly one task \"in_progress\" at a time. Mark tasks \"completed\" \
         immediately after finishing, then drop them on the next update. \
         Do not print todos in your response text — the user sees them in the tool output."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "description": "The complete todo list (replaces any previous list).",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {
                                    "type": "string",
                                    "description": "Short task description in imperative form (e.g. \"Run tests\")"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"],
                                    "description": "pending = not started, in_progress = working now, completed = done"
                                }
                            },
                            "required": ["content", "status"]
                        }
                    }
                },
                "required": ["todos"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let todos: Vec<TodoItem> = params
                .get("todos")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let display = render_todos(&todos);

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: display }],
                details: Some(json!({ "todos": todos })),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_empty() {
        assert_eq!(render_todos(&[]), "(empty)");
    }

    #[test]
    fn test_render_mixed_statuses() {
        let todos = vec![
            TodoItem {
                content: "Set up project".to_string(),
                status: "completed".to_string(),
            },
            TodoItem {
                content: "Write tests".to_string(),
                status: "in_progress".to_string(),
            },
            TodoItem {
                content: "Deploy".to_string(),
                status: "pending".to_string(),
            },
        ];
        let output = render_todos(&todos);
        assert!(output.contains("[1/3]"));
        assert!(output.contains("Write tests")); // header shows active
        assert!(output.contains("✓ Set up project"));
        assert!(output.contains("→ Write tests"));
        assert!(output.contains("○ Deploy"));
    }

    #[test]
    fn test_render_all_completed() {
        let todos = vec![
            TodoItem {
                content: "A".to_string(),
                status: "completed".to_string(),
            },
            TodoItem {
                content: "B".to_string(),
                status: "completed".to_string(),
            },
        ];
        let output = render_todos(&todos);
        assert!(output.contains("[2/2]"));
    }

    #[tokio::test]
    async fn test_execute_returns_details() {
        let tool = TodoTool;
        let params = json!({
            "todos": [
                {"content": "Read file", "status": "in_progress"},
                {"content": "Edit file", "status": "pending"}
            ]
        });
        let result = tool.execute("call-1".into(), params, None).await.unwrap();

        // Check details contain the todos
        let details = result.details.unwrap();
        let todos = details["todos"].as_array().unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0]["status"], "in_progress");
    }

    #[tokio::test]
    async fn test_execute_empty_todos() {
        let tool = TodoTool;
        let params = json!({ "todos": [] });
        let result = tool.execute("call-1".into(), params, None).await.unwrap();

        let text = match &result.content[0] {
            UserBlock::Text { text } => text.as_str(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("(empty)"));
    }
}
