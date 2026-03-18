//! Mirrors: packages/agent/test/agent-loop.test.ts
//! Unit tests for agentLoop and agentLoopContinue using mock streams.

mod common;
use common::*;

use agent::loop_::{agent_loop, agent_loop_continue};
use agent::types::{AgentContext, AgentEvent, AgentLoopConfig, AgentMessage};
use ai::types::{ContentBlock, Message, StopReason, UserBlock};
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn identity_convert(messages: Vec<AgentMessage>) -> agent::types::BoxFuture<anyhow::Result<Vec<Message>>> {
    Box::pin(async move {
        Ok(messages.into_iter().filter_map(|m| m.as_message().cloned()).collect())
    })
}

fn base_config() -> AgentLoopConfig {
    AgentLoopConfig {
        model: mock_model(),
        simple_options: ai::types::SimpleStreamOptions::default(),
        convert_to_llm: Arc::new(|msgs| identity_convert(msgs)),
        transform_context: None,
        stream_fn: None,
        get_api_key: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
    }
}

// ---------------------------------------------------------------------------
// agentLoop
// ---------------------------------------------------------------------------

fn final_messages(events: &[AgentEvent]) -> Vec<AgentMessage> {
    events
        .iter()
        .find_map(|event| match event {
            AgentEvent::AgentEnd { messages } => Some(messages.clone()),
            _ => None,
        })
        .expect("AgentEnd event")
}

#[tokio::test]
async fn emits_events_with_agent_message_types() {
    let context = empty_context();
    let prompt = user_message("Hello");

    let mut config = base_config();
    config.stream_fn = Some(stream_fn_from_messages(vec![mock_assistant_message("Hi there!")]));

    let mut stream = agent_loop(vec![prompt], context, Arc::new(config), None);
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let messages = final_messages(&events);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), "user");
    assert_eq!(messages[1].role(), "assistant");

    let types: Vec<_> = events.iter().map(event_type).collect();
    assert!(types.contains(&"agent_start"));
    assert!(types.contains(&"turn_start"));
    assert!(types.contains(&"message_start"));
    assert!(types.contains(&"message_end"));
    assert!(types.contains(&"turn_end"));
    assert!(types.contains(&"agent_end"));
}

#[tokio::test]
async fn custom_message_types_filtered_by_convert_to_llm() {
    let mut context = empty_context();
    context.messages.push(AgentMessage::Custom {
        role: "notification".into(),
        data: json!({ "text": "ignore me" }),
    });

    let seen_roles = Arc::new(std::sync::Mutex::new(vec![]));
    let roles_ref = Arc::clone(&seen_roles);
    let mut config = base_config();
    config.convert_to_llm = Arc::new(move |messages| {
        let roles_ref = Arc::clone(&roles_ref);
        Box::pin(async move {
            *roles_ref.lock().unwrap() = messages.iter().map(|m| m.role().to_string()).collect();
            Ok(messages.into_iter().filter_map(|m| m.as_message().cloned()).collect())
        })
    });
    config.stream_fn = Some(stream_fn_from_messages(vec![mock_assistant_message("Response")]));

    let mut stream = agent_loop(vec![user_message("Hello")], context, Arc::new(config), None);
    while stream.next().await.is_some() {}

    assert_eq!(*seen_roles.lock().unwrap(), vec!["notification".to_string(), "user".to_string()]);
}

#[tokio::test]
async fn transform_context_applied_before_convert_to_llm() {
    let context = AgentContext {
        system_prompt: "You are helpful.".into(),
        messages: vec![
            user_message("old message 1"),
            AgentMessage::Llm(Message::Assistant(mock_assistant_message("old response 1"))),
            user_message("old message 2"),
            AgentMessage::Llm(Message::Assistant(mock_assistant_message("old response 2"))),
        ],
        tools: vec![],
    };

    let transformed_roles = Arc::new(std::sync::Mutex::new(vec![]));
    let converted_roles = Arc::new(std::sync::Mutex::new(vec![]));
    let transformed_ref = Arc::clone(&transformed_roles);
    let converted_ref = Arc::clone(&converted_roles);

    let mut config = base_config();
    config.transform_context = Some(Arc::new(move |messages, _| {
        let transformed_ref = Arc::clone(&transformed_ref);
        Box::pin(async move {
            let pruned = messages.into_iter().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>();
            *transformed_ref.lock().unwrap() = pruned.iter().map(|m| m.role().to_string()).collect();
            pruned
        })
    }));
    config.convert_to_llm = Arc::new(move |messages| {
        let converted_ref = Arc::clone(&converted_ref);
        Box::pin(async move {
            *converted_ref.lock().unwrap() = messages.iter().map(|m| m.role().to_string()).collect();
            Ok(messages.into_iter().filter_map(|m| m.as_message().cloned()).collect())
        })
    });
    config.stream_fn = Some(stream_fn_from_messages(vec![mock_assistant_message("Response")]));

    let mut stream = agent_loop(vec![user_message("new message")], context, Arc::new(config), None);
    while stream.next().await.is_some() {}

    assert_eq!(*transformed_roles.lock().unwrap(), vec!["assistant".to_string(), "user".to_string()]);
    assert_eq!(*converted_roles.lock().unwrap(), vec!["assistant".to_string(), "user".to_string()]);
}

#[tokio::test]
async fn handles_tool_calls_and_results() {
    let executed = Arc::new(std::sync::Mutex::new(vec![]));
    struct RecordingEchoTool(Arc<std::sync::Mutex<Vec<String>>>);
    impl agent::types::AgentTool for RecordingEchoTool {
        fn name(&self) -> &str { "echo" }
        fn label(&self) -> &str { "Echo" }
        fn description(&self) -> &str { "Echo tool" }
        fn parameters(&self) -> &Value {
            static PARAMS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            PARAMS.get_or_init(|| json!({"type":"object","properties":{"value":{"type":"string"}}}))
        }
        fn execute(
            &self,
            _tool_call_id: String,
            params: Value,
            _signal: Option<CancellationToken>,
            _on_update: Option<agent::types::ToolUpdateFn>,
        ) -> agent::types::BoxFuture<anyhow::Result<agent::types::AgentToolResult>> {
            let executed = Arc::clone(&self.0);
            Box::pin(async move {
                let value = params.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
                executed.lock().unwrap().push(value.clone());
                Ok(agent::types::AgentToolResult {
                    content: vec![UserBlock::Text { text: format!("echoed: {value}") }],
                    details: Some(json!({ "value": value })),
                })
            })
        }
    }

    let context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: vec![Arc::new(RecordingEchoTool(Arc::clone(&executed)))],
    };

    let mut config = base_config();
    config.stream_fn = Some(stream_fn_from_messages(vec![
        mock_assistant_message_with_tool_call("tool-1", "echo", json!({ "value": "hello" })),
        mock_assistant_message("done"),
    ]));

    let mut stream = agent_loop(vec![user_message("echo something")], context, Arc::new(config), None);
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(*executed.lock().unwrap(), vec!["hello".to_string()]);
    assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolExecutionStart { .. })));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolExecutionEnd { is_error: false, .. })));

    let messages = final_messages(&events);
    assert!(matches!(messages.last(), Some(AgentMessage::Llm(Message::Assistant(msg))) if msg.content.iter().any(|b| matches!(b, ContentBlock::Text { text, .. } if text == "done"))));
}

#[tokio::test]
async fn injects_steering_messages_and_skips_remaining_tool_calls() {
    let executed = Arc::new(std::sync::Mutex::new(vec![]));
    struct RecordingEchoTool(Arc<std::sync::Mutex<Vec<String>>>);
    impl agent::types::AgentTool for RecordingEchoTool {
        fn name(&self) -> &str { "echo" }
        fn label(&self) -> &str { "Echo" }
        fn description(&self) -> &str { "Echo tool" }
        fn parameters(&self) -> &Value {
            static PARAMS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            PARAMS.get_or_init(|| json!({"type":"object","properties":{"value":{"type":"string"}}}))
        }
        fn execute(
            &self,
            _tool_call_id: String,
            params: Value,
            _signal: Option<CancellationToken>,
            _on_update: Option<agent::types::ToolUpdateFn>,
        ) -> agent::types::BoxFuture<anyhow::Result<agent::types::AgentToolResult>> {
            let executed = Arc::clone(&self.0);
            Box::pin(async move {
                let value = params.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
                executed.lock().unwrap().push(value.clone());
                Ok(agent::types::AgentToolResult {
                    content: vec![UserBlock::Text { text: format!("ok:{value}") }],
                    details: Some(json!({ "value": value })),
                })
            })
        }
    }

    let queued_delivered = Arc::new(AtomicBool::new(false));
    let queued_ref = Arc::clone(&queued_delivered);
    let saw_interrupt = Arc::new(AtomicBool::new(false));
    let interrupt_ref = Arc::clone(&saw_interrupt);
    let call_index = Arc::new(AtomicUsize::new(0));
    let call_index_ref = Arc::clone(&call_index);
    let executed_for_queue = Arc::clone(&executed);

    let mut config = base_config();
    config.get_steering_messages = Some(Arc::new(move || {
        let queued_ref = Arc::clone(&queued_ref);
        let executed = Arc::clone(&executed_for_queue);
        Box::pin(async move {
            if executed.lock().unwrap().len() == 1 && !queued_ref.swap(true, Ordering::SeqCst) {
                vec![user_message("interrupt")]
            } else {
                vec![]
            }
        })
    }));
    config.stream_fn = Some(stream_fn_once(move |_model, context, _options| {
        let index = call_index_ref.fetch_add(1, Ordering::SeqCst);
        if index == 1 {
            let has_interrupt = context.messages.iter().any(|m| match m {
                Message::User(msg) => matches!(&msg.content, ai::types::UserContent::Text(text) if text == "interrupt"),
                _ => false,
            });
            interrupt_ref.store(has_interrupt, Ordering::SeqCst);
        }

        if index == 0 {
            instant_stream(ai::types::AssistantMessage {
                content: vec![
                    ContentBlock::ToolCall {
                        id: "tool-1".into(),
                        name: "echo".into(),
                        arguments: [("value".into(), json!("first"))].into_iter().collect(),
                        thought_signature: None,
                    },
                    ContentBlock::ToolCall {
                        id: "tool-2".into(),
                        name: "echo".into(),
                        arguments: [("value".into(), json!("second"))].into_iter().collect(),
                        thought_signature: None,
                    },
                ],
                stop_reason: StopReason::ToolUse,
                ..mock_assistant_message("")
            })
        } else {
            instant_stream(mock_assistant_message("done"))
        }
    }));

    let context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: vec![Arc::new(RecordingEchoTool(Arc::clone(&executed)))],
    };

    let mut stream = agent_loop(vec![user_message("start")], context, Arc::new(config), None);
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(*executed.lock().unwrap(), vec!["first".to_string()]);

    let tool_ends: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolExecutionEnd { is_error, result, .. } => Some((*is_error, result.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(tool_ends.len(), 2);
    assert!(!tool_ends[0].0);
    assert!(tool_ends[1].0);
    assert!(matches!(&tool_ends[1].1.content[0], UserBlock::Text { text } if text.contains("Skipped due to queued user message")));

    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::MessageStart { message: AgentMessage::Llm(Message::User(msg)) }
            if matches!(&msg.content, ai::types::UserContent::Text(text) if text == "interrupt")
    )));
    assert!(saw_interrupt.load(Ordering::SeqCst));
}

// ---------------------------------------------------------------------------
// agentLoopContinue
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Cannot continue: no messages in context")]
fn throws_when_context_has_no_messages() {
    let context = empty_context(); // no messages
    let config = Arc::new(base_config());
    agent::loop_::agent_loop_continue(context, config, None);
}

#[tokio::test]
async fn continue_from_context_without_user_message_events() {
    let context = AgentContext {
        system_prompt: "You are helpful.".into(),
        messages: vec![user_message("Hello")],
        tools: vec![],
    };

    let mut config = base_config();
    config.stream_fn = Some(stream_fn_from_messages(vec![mock_assistant_message("Response")]));

    let mut stream = agent_loop_continue(context, Arc::new(config), None);
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let messages = final_messages(&events);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role(), "assistant");

    let message_end_events: Vec<_> =
        events.iter().filter(|event| matches!(event, AgentEvent::MessageEnd { .. })).collect();
    assert_eq!(message_end_events.len(), 1);
    assert!(matches!(message_end_events[0], AgentEvent::MessageEnd { message } if message.role() == "assistant"));
}

#[tokio::test]
async fn continue_allows_custom_last_message_converted_by_convert_to_llm() {
    let context = AgentContext {
        system_prompt: "You are helpful.".into(),
        messages: vec![AgentMessage::Custom {
            role: "custom".into(),
            data: json!({ "text": "Hook content" }),
        }],
        tools: vec![],
    };

    let mut config = base_config();
    config.convert_to_llm = Arc::new(|messages| {
        Box::pin(async move {
            Ok(messages
                .into_iter()
                .filter_map(|message| match message {
                    AgentMessage::Custom { data, .. } => {
                        data.get("text").and_then(Value::as_str).map(|text| Message::User(ai::types::UserMessage::new(text)))
                    }
                    AgentMessage::Llm(message) => Some(message),
                })
                .collect())
        })
    });
    config.stream_fn = Some(stream_fn_from_messages(vec![mock_assistant_message("Response to custom message")]));

    let mut stream = agent_loop_continue(context, Arc::new(config), None);
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let messages = final_messages(&events);
    assert_eq!(messages.len(), 1);
    assert!(matches!(
        &messages[0],
        AgentMessage::Llm(Message::Assistant(msg))
            if msg.content.iter().any(|block| matches!(block, ContentBlock::Text { text, .. } if text == "Response to custom message"))
    ));
}

// ---------------------------------------------------------------------------
// Tool definition wiring tests
// ---------------------------------------------------------------------------

/// A minimal AgentTool whose parameters are stored directly on the struct.
struct SimpleTool {
    tool_name: &'static str,
    tool_desc: &'static str,
    tool_params: Value,
}

impl agent::types::AgentTool for SimpleTool {
    fn name(&self) -> &str { self.tool_name }
    fn label(&self) -> &str { "unused-label" }
    fn description(&self) -> &str { self.tool_desc }
    fn parameters(&self) -> &Value { &self.tool_params }
    fn execute(
        &self,
        _id: String,
        _params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: Option<agent::types::ToolUpdateFn>,
    ) -> agent::types::BoxFuture<anyhow::Result<agent::types::AgentToolResult>> {
        Box::pin(async {
            Ok(agent::types::AgentToolResult { content: vec![], details: None })
        })
    }
}

#[tokio::test]
async fn tool_definitions_wired_to_llm_context() {
    // INV-1: When agent has tools, LLM context receives matching tool definitions.
    // INV-3: label/execute are not present in the ai::types::Tool (only name/desc/params).
    let captured: Arc<std::sync::Mutex<Option<Vec<ai::types::Tool>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let captured_ref = Arc::clone(&captured);

    let mut config = base_config();
    config.stream_fn = Some(stream_fn_once(move |_model, context, _options| {
        *captured_ref.lock().unwrap() = context.tools.clone();
        instant_stream(mock_assistant_message("done"))
    }));

    let context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: vec![
            Arc::new(SimpleTool {
                tool_name: "tool_a",
                tool_desc: "First tool",
                tool_params: json!({"type":"object","properties":{"x":{"type":"number"}}}),
            }),
            Arc::new(SimpleTool {
                tool_name: "tool_b",
                tool_desc: "Second tool",
                tool_params: json!({"type":"object","properties":{"y":{"type":"string"}}}),
            }),
        ],
    };

    let mut stream = agent_loop(vec![user_message("go")], context, Arc::new(config), None);
    while stream.next().await.is_some() {}

    let guard = captured.lock().unwrap();
    let tools = guard.as_ref().expect("tools should be Some when tools are registered");
    assert_eq!(tools.len(), 2);

    let a = tools.iter().find(|t| t.name == "tool_a").expect("tool_a in context");
    assert_eq!(a.description, "First tool");
    assert_eq!(a.parameters, json!({"type":"object","properties":{"x":{"type":"number"}}}));

    let b = tools.iter().find(|t| t.name == "tool_b").expect("tool_b in context");
    assert_eq!(b.description, "Second tool");
    assert_eq!(b.parameters, json!({"type":"object","properties":{"y":{"type":"string"}}}));
}

#[tokio::test]
async fn no_tools_sends_none_to_llm_context() {
    // INV-2: When agent has no tools, context.tools is None (not Some([])).
    let captured: Arc<std::sync::Mutex<Option<Option<Vec<ai::types::Tool>>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let captured_ref = Arc::clone(&captured);

    let mut config = base_config();
    config.stream_fn = Some(stream_fn_once(move |_model, context, _options| {
        *captured_ref.lock().unwrap() = Some(context.tools.clone());
        instant_stream(mock_assistant_message("done"))
    }));

    let context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: vec![], // no tools
    };

    let mut stream = agent_loop(vec![user_message("go")], context, Arc::new(config), None);
    while stream.next().await.is_some() {}

    let guard = captured.lock().unwrap();
    let tools_opt = guard.as_ref().expect("stream_fn was called");
    assert!(tools_opt.is_none(), "tools must be None when agent has no tools registered");
}

#[tokio::test]
async fn tool_definitions_present_during_round_trip() {
    // Critical path: tool defs sent to LLM, tool call received, tool executed.
    let executed = Arc::new(std::sync::Mutex::new(false));
    let executed_ref = Arc::clone(&executed);
    let captured_tool_names: Arc<std::sync::Mutex<Vec<String>>> =
        Arc::new(std::sync::Mutex::new(vec![]));
    let names_ref = Arc::clone(&captured_tool_names);

    struct RoundTripTool {
        executed: Arc<std::sync::Mutex<bool>>,
    }
    impl agent::types::AgentTool for RoundTripTool {
        fn name(&self) -> &str { "roundtrip" }
        fn label(&self) -> &str { "Round Trip" }
        fn description(&self) -> &str { "A round-trip test tool" }
        fn parameters(&self) -> &Value {
            static P: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            P.get_or_init(|| json!({"type":"object"}))
        }
        fn execute(
            &self,
            _id: String,
            _params: Value,
            _signal: Option<tokio_util::sync::CancellationToken>,
            _on_update: Option<agent::types::ToolUpdateFn>,
        ) -> agent::types::BoxFuture<anyhow::Result<agent::types::AgentToolResult>> {
            let ex = Arc::clone(&self.executed);
            Box::pin(async move {
                *ex.lock().unwrap() = true;
                Ok(agent::types::AgentToolResult {
                    content: vec![UserBlock::Text { text: "ok".into() }],
                    details: None,
                })
            })
        }
    }

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_ref = Arc::clone(&call_count);

    let mut config = base_config();
    config.stream_fn = Some(stream_fn_once(move |_model, context, _options| {
        let idx = count_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if idx == 0 {
            // Capture tool names from the first context call
            if let Some(tools) = &context.tools {
                *names_ref.lock().unwrap() =
                    tools.iter().map(|t| t.name.clone()).collect();
            }
            instant_stream(mock_assistant_message_with_tool_call(
                "rt-1",
                "roundtrip",
                json!({}),
            ))
        } else {
            instant_stream(mock_assistant_message("complete"))
        }
    }));

    let context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: vec![Arc::new(RoundTripTool { executed: executed_ref })],
    };

    let mut stream = agent_loop(vec![user_message("go")], context, Arc::new(config), None);
    while stream.next().await.is_some() {}

    assert!(
        *executed.lock().unwrap(),
        "tool execute() must have been called"
    );
    let names = captured_tool_names.lock().unwrap().clone();
    assert_eq!(names, vec!["roundtrip".to_string()], "tool definition must be in LLM context");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn event_type(e: &AgentEvent) -> &'static str {
    match e {
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
    }
}
