//! Integration tests: agent + tools round-trip via mock stream.

use std::sync::{Arc, Mutex};

use agent::types::{AgentEvent, StreamAssistantFn};
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{AssistantMessage, ContentBlock, StopReason, Usage};
use coding_agent::tools::all_tools;
use serde_json::json;

fn mock_model() -> ai::types::Model {
    ai::types::Model {
        id: "mock".into(),
        name: "mock".into(),
        api: "openai-responses".into(),
        provider: "openai".into(),
        base_url: "https://example.invalid".into(),
        reasoning: false,
        input: vec!["text".into()],
        cost: ai::types::ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        headers: None,
        compat: None,
    }
}

fn instant_stream(msg: AssistantMessage) -> AssistantMessageEventStream {
    let (mut tx, stream) = assistant_message_event_stream();
    tokio::spawn(async move {
        tx.push(ai::types::AssistantMessageEvent::Start {
            partial: msg.clone(),
        });
        let reason = msg.stop_reason.clone();
        tx.push(ai::types::AssistantMessageEvent::Done {
            reason,
            message: msg,
        });
    });
    stream
}

fn stream_fn_from_messages(messages: Vec<AssistantMessage>) -> StreamAssistantFn {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let messages = Arc::new(messages);
    let index = Arc::new(AtomicUsize::new(0));
    Arc::new(move |_model, _ctx, _opts| {
        let i = index.fetch_add(1, Ordering::SeqCst);
        let msg = messages
            .get(i)
            .cloned()
            .unwrap_or_else(|| messages.last().cloned().expect("at least one mock message"));
        Ok(instant_stream(msg))
    })
}

fn tool_call_message(id: &str, tool_name: &str, args: serde_json::Value) -> AssistantMessage {
    let mut map = std::collections::HashMap::new();
    if let serde_json::Value::Object(obj) = args {
        for (k, v) in obj {
            map.insert(k, v);
        }
    }
    AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::ToolCall {
            id: id.into(),
            name: tool_name.into(),
            arguments: map,
            thought_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "mock".into(),
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp: 0,
    }
}

fn text_message(text: &str) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::Text {
            text: text.into(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "mock".into(),
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

/// Verify that a bash tool call is executed and its result is collected.
#[tokio::test]
async fn test_bash_tool_round_trip() {
    let tools = all_tools();

    // First call: assistant requests bash "echo test"
    // Second call: assistant gives final answer after seeing tool result
    let stream_fn = stream_fn_from_messages(vec![
        tool_call_message("call_1", "bash", json!({"command": "echo test"})),
        text_message("Done."),
    ]);

    let collected_events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
    let events_clone = Arc::clone(&collected_events);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(mock_model()),
            system_prompt: Some("You are helpful.".into()),
            tools: Some(tools),
            thinking_level: None,
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: Some(stream_fn),
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    });

    let _unsub = agent.subscribe(move |event| {
        events_clone.lock().unwrap().push(event.clone());
    });

    agent.prompt("run echo test").await.unwrap();

    let events = collected_events.lock().unwrap();

    // Should have tool execution events
    let tool_start = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::ToolExecutionStart { tool_name, .. } if tool_name == "bash"
        )
    });
    assert!(tool_start, "expected ToolExecutionStart for bash");

    let tool_end = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::ToolExecutionEnd { tool_name, is_error: false, .. } if tool_name == "bash"
        )
    });
    assert!(tool_end, "expected successful ToolExecutionEnd for bash");

    // Should have AgentEnd
    let agent_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::AgentEnd { .. }));
    assert!(agent_end, "expected AgentEnd event");
}

/// Verify that tool definitions are sent to LLM context (tools field is Some with 3 entries).
#[tokio::test]
async fn test_tools_sent_to_llm_context() {
    let tools = all_tools();

    let captured_context: Arc<Mutex<Option<ai::types::Context>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured_context);

    let stream_fn: StreamAssistantFn = Arc::new(move |_model, ctx, _opts| {
        *captured_clone.lock().unwrap() = Some(ctx.clone());
        Ok(instant_stream(text_message("ok")))
    });

    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(mock_model()),
            system_prompt: Some("You are helpful.".into()),
            tools: Some(tools),
            thinking_level: None,
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: Some(stream_fn),
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    });

    agent.prompt("hello").await.unwrap();

    let ctx = captured_context.lock().unwrap();
    let ctx = ctx.as_ref().expect("context should have been captured");
    let tool_defs = ctx.tools.as_ref().expect("tools should be Some");
    assert_eq!(
        tool_defs.len(),
        5,
        "expected 5 tool definitions, got {}",
        tool_defs.len()
    );

    let names: Vec<&str> = tool_defs.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"bash"), "missing bash tool");
    assert!(names.contains(&"file_edit"), "missing file_edit tool");
    assert!(names.contains(&"file_read"), "missing file_read tool");
    assert!(names.contains(&"file_write"), "missing file_write tool");
    assert!(names.contains(&"grep"), "missing grep tool");
}
