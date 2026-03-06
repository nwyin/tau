//! Mirrors: packages/agent/test/e2e.test.ts
//! End-to-end integration tests against live LLM providers.

mod common;

use std::sync::Arc;

use agent::agent::{Agent, AgentOptions, AgentStateInit};
use agent::types::{AgentEvent, AgentTool, AgentToolResult, ThinkingLevel, ToolUpdateFn};
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

    agent.prompt("What is 2+2? Answer with just the number.").await.unwrap();

    agent.with_state(|s| {
        assert!(!s.is_streaming);
        assert_eq!(s.messages.len(), 2);
        assert_eq!(s.messages[0].role(), "user");
        assert_eq!(s.messages[1].role(), "assistant");

        let assistant = match &s.messages[1] {
            agent::types::AgentMessage::Llm(Message::Assistant(message)) => message,
            other => panic!("expected assistant message, got {}", other.role()),
        };
        assert!(assistant.content.iter().any(|block| matches!(block, ai::types::ContentBlock::Text { .. })));
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
        _on_update: Option<ToolUpdateFn>,
    ) -> agent::types::BoxFuture<anyhow::Result<AgentToolResult>> {
        Box::pin(async move {
            let a = params.get("a").and_then(Value::as_f64).unwrap_or_default();
            let b = params.get("b").and_then(Value::as_f64).unwrap_or_default();
            let operation = params.get("operation").and_then(Value::as_str).unwrap_or("add");

            let result = match operation {
                "add" => a + b,
                "subtract" => a - b,
                "multiply" => a * b,
                "divide" => a / b,
                other => anyhow::bail!("unsupported operation: {other}"),
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: format!("{result}") }],
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

async fn abort_execution(model: ai::types::Model) {
    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(vec![]),
        }),
        ..default_agent_opts_no_model()
    }));

    let worker = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move {
            agent
                .prompt("Write a long answer counting from 1 to 5000 with commentary between numbers.")
                .await
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    agent.abort();
    let _ = worker.await.unwrap();

    agent.with_state(|s| {
        assert!(!s.is_streaming);
        assert!(s.messages.len() >= 2);

        let last = s.messages.last().expect("last message");
        if let agent::types::AgentMessage::Llm(Message::Assistant(message)) = last {
            assert!(
                matches!(
                    message.stop_reason,
                    ai::types::StopReason::Aborted | ai::types::StopReason::Error
                ),
                "expected aborted/error stop reason, got {:?}",
                message.stop_reason
            );
        }
    });
}

async fn state_updates(model: ai::types::Model) {
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(vec![]),
        }),
        ..default_agent_opts_no_model()
    });

    let events = Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));
    let events_ref = Arc::clone(&events);
    let _unsubscribe = agent.subscribe(move |event| {
        let name = match event {
            AgentEvent::AgentStart => "agent_start",
            AgentEvent::AgentEnd { .. } => "agent_end",
            AgentEvent::TurnStart => "turn_start",
            AgentEvent::TurnEnd { .. } => "turn_end",
            AgentEvent::MessageStart { .. } => "message_start",
            AgentEvent::MessageUpdate { .. } => "message_update",
            AgentEvent::MessageEnd { .. } => "message_end",
            AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
            AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
            AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
        };
        events_ref.lock().unwrap().push(name);
    });

    agent.prompt("Count from 1 to 5.").await.unwrap();

    let captured = events.lock().unwrap().clone();
    assert!(captured.contains(&"agent_start"));
    assert!(captured.contains(&"agent_end"));
    assert!(captured.contains(&"message_start"));
    assert!(captured.contains(&"message_end"));
    assert!(captured.contains(&"turn_start"));
    assert!(captured.contains(&"turn_end"));
}

async fn multi_turn_conversation(model: ai::types::Model) {
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(vec![]),
        }),
        ..default_agent_opts_no_model()
    });

    agent.prompt("My name is Alice.").await.unwrap();
    agent.with_state(|s| assert_eq!(s.messages.len(), 2));

    agent.prompt("What is my name?").await.unwrap();
    agent.with_state(|s| {
        assert_eq!(s.messages.len(), 4);

        let last = s.messages.last().expect("assistant reply");
        let last_text = match last {
            agent::types::AgentMessage::Llm(Message::Assistant(message)) => message
                .content
                .iter()
                .find_map(|block| match block {
                    ai::types::ContentBlock::Text { text, .. } => Some(text.to_lowercase()),
                    _ => None,
                })
                .unwrap_or_default(),
            other => panic!("expected assistant message, got {}", other.role()),
        };

        assert!(last_text.contains("alice"), "expected Alice in final response: {last_text}");
    });
}

// ---------------------------------------------------------------------------
// Per-provider e2e tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and registered anthropic provider"]
async fn anthropic_basic_prompt() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY, registered anthropic provider, and AgentTool -> ai::Tool wiring"]
async fn anthropic_tool_execution() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    tool_execution((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and registered anthropic provider"]
async fn anthropic_abort_execution() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    abort_execution((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and registered anthropic provider"]
async fn anthropic_state_updates() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    state_updates((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and registered anthropic provider"]
async fn anthropic_multi_turn_conversation() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    multi_turn_conversation((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and registered openai provider"]
async fn openai_basic_prompt() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY, registered openai provider, and AgentTool -> ai::Tool wiring"]
async fn openai_tool_execution() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    tool_execution((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and registered openai provider"]
async fn openai_abort_execution() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    abort_execution((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and registered openai provider"]
async fn openai_state_updates() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    state_updates((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and registered openai provider"]
async fn openai_multi_turn_conversation() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    multi_turn_conversation((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires KIMI_API_KEY and registered kimi provider"]
async fn kimi_basic_prompt() {
    let model = get_model("kimi-coding", "kimi-k2-thinking").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires KIMI_API_KEY, registered kimi provider, and AgentTool -> ai::Tool wiring"]
async fn kimi_tool_execution() {
    let model = get_model("kimi-coding", "kimi-k2-thinking").unwrap();
    tool_execution((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires KIMI_API_KEY and registered kimi provider"]
async fn kimi_abort_execution() {
    let model = get_model("kimi-coding", "kimi-k2-thinking").unwrap();
    abort_execution((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires KIMI_API_KEY and registered kimi provider"]
async fn kimi_state_updates() {
    let model = get_model("kimi-coding", "kimi-k2-thinking").unwrap();
    state_updates((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires KIMI_API_KEY and registered kimi provider"]
async fn kimi_multi_turn_conversation() {
    let model = get_model("kimi-coding", "kimi-k2-thinking").unwrap();
    multi_turn_conversation((*model).clone()).await;
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
        transport: None,
        max_retry_delay_ms: None,
    }
}
