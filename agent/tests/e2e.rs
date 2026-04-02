//! Mirrors: packages/agent/test/e2e.test.ts
//! End-to-end smoke tests against live LLM providers.
//! Requires OPENAI_API_KEY and RUN_LIVE_PROVIDER_TESTS=1.

mod common;

use std::sync::Arc;

use agent::agent::{Agent, AgentOptions, AgentStateInit};
use agent::types::{AgentTool, AgentToolResult, ThinkingLevel};
use ai::models::get_model;
use ai::types::{Message, UserBlock};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

async fn basic_prompt(model: ai::types::Model) {
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful assistant. Keep your responses concise.".into()),
            model: Some(model),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(vec![]),
        }),
        ..default_agent_opts_no_model()
    });

    agent
        .prompt("What is 2+2? Answer with just the number.")
        .await
        .unwrap();

    agent.with_state(|s| {
        assert!(!s.is_streaming);
        assert_eq!(s.messages.len(), 2);
        assert_eq!(s.messages[0].role(), "user");
        assert_eq!(s.messages[1].role(), "assistant");

        let assistant = match &s.messages[1] {
            agent::types::AgentMessage::Llm(Message::Assistant(message)) => message,
            other => panic!("expected assistant message, got {}", other.role()),
        };
        assert!(assistant
            .content
            .iter()
            .any(|block| matches!(block, ai::types::ContentBlock::Text { .. })));
    });
}

struct CalculateTool;

impl AgentTool for CalculateTool {
    fn name(&self) -> &str {
        "calculate"
    }

    fn label(&self) -> &str {
        "Calculate"
    }

    fn description(&self) -> &str {
        "Perform arithmetic calculations"
    }

    fn parameters(&self) -> &Value {
        static PARAMS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        PARAMS.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" },
                    "operation": {
                        "type": "string",
                        "enum": ["add", "subtract", "multiply", "divide"]
                    }
                },
                "required": ["a", "b", "operation"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
    ) -> agent::types::BoxFuture<anyhow::Result<AgentToolResult>> {
        Box::pin(async move {
            let a = params.get("a").and_then(Value::as_f64).unwrap_or_default();
            let b = params.get("b").and_then(Value::as_f64).unwrap_or_default();
            let operation = params
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or("add");

            let result = match operation {
                "add" => a + b,
                "subtract" => a - b,
                "multiply" => a * b,
                "divide" => a / b,
                other => anyhow::bail!("unsupported operation: {other}"),
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: format!("{result}"),
                }],
                details: Some(json!({ "result": result })),
            })
        })
    }
}

async fn tool_execution(model: ai::types::Model) {
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some(
                "You are a helpful assistant. Always use the calculate tool for math.".into(),
            ),
            model: Some(model),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(vec![Arc::new(CalculateTool)]),
        }),
        ..default_agent_opts_no_model()
    });

    agent
        .prompt("Calculate 123 * 456 using the calculate tool.")
        .await
        .unwrap();

    agent.with_state(|s| {
        assert!(!s.is_streaming);
        assert!(s.messages.iter().any(|m| m.role() == "toolResult"));

        let tool_result_text = s
            .messages
            .iter()
            .find_map(|message| match message {
                agent::types::AgentMessage::Llm(Message::ToolResult(result)) => Some(
                    result
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            UserBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            })
            .unwrap_or_default();

        assert!(tool_result_text.contains("56088"));

        let final_assistant = s.messages.last().expect("final assistant message");
        let final_text = match final_assistant {
            agent::types::AgentMessage::Llm(Message::Assistant(message)) => message
                .content
                .iter()
                .find_map(|block| match block {
                    ai::types::ContentBlock::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
            other => panic!("expected assistant message, got {}", other.role()),
        };

        assert!(
            final_text.contains("56088") || final_text.contains("56,088"),
            "final assistant text did not contain computed result: {final_text}"
        );
    });
}

// ---------------------------------------------------------------------------
// Smoke tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live provider test: requires OPENAI_API_KEY and RUN_LIVE_PROVIDER_TESTS=1"]
async fn openai_basic_prompt() {
    if std::env::var("RUN_LIVE_PROVIDER_TESTS").is_err() {
        eprintln!("Skipping: set RUN_LIVE_PROVIDER_TESTS=1 to run live provider tests");
        return;
    }
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "live provider test: requires OPENAI_API_KEY and RUN_LIVE_PROVIDER_TESTS=1"]
async fn openai_tool_execution() {
    if std::env::var("RUN_LIVE_PROVIDER_TESTS").is_err() {
        eprintln!("Skipping: set RUN_LIVE_PROVIDER_TESTS=1 to run live provider tests");
        return;
    }
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    tool_execution((*model).clone()).await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_agent_opts_no_model() -> AgentOptions {
    AgentOptions {
        initial_state: None,
        convert_to_llm: None,
        transform_context: None,
        stream_fn: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        max_turns: None,
    }
}
