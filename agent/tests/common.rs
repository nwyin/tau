//! Shared test helpers for agent tests.

use agent::types::{AgentContext, AgentMessage, StreamAssistantFn};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{AssistantMessage, ContentBlock, StopReason, Usage};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub fn mock_model() -> ai::types::Model {
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

pub fn mock_assistant_message(text: &str) -> AssistantMessage {
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

pub fn mock_assistant_message_with_tool_call(
    id: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> AssistantMessage {
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

pub fn user_message(text: &str) -> AgentMessage {
    AgentMessage::user(text)
}

pub fn empty_context() -> AgentContext {
    AgentContext {
        system_prompt: "You are helpful.".into(),
        messages: vec![],
        tools: vec![],
    }
}

/// Create a mock stream that immediately resolves with the given AssistantMessage.
pub fn instant_stream(msg: AssistantMessage) -> AssistantMessageEventStream {
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

pub fn pending_stream() -> AssistantMessageEventStream {
    let (_tx, stream) = assistant_message_event_stream();
    stream
}

pub fn stream_fn_once(
    f: impl Fn(
            ai::types::Model,
            ai::types::Context,
            Option<ai::types::SimpleStreamOptions>,
        ) -> AssistantMessageEventStream
        + Send
        + Sync
        + 'static,
) -> StreamAssistantFn {
    Arc::new(move |model, context, options| Ok(f(model, context, options)))
}

pub fn stream_fn_from_messages(messages: Vec<AssistantMessage>) -> StreamAssistantFn {
    let messages = Arc::new(messages);
    let index = Arc::new(AtomicUsize::new(0));

    stream_fn_once(move |_model, _context, _options| {
        let i = index.fetch_add(1, Ordering::SeqCst);
        let msg = messages
            .get(i)
            .cloned()
            .unwrap_or_else(|| messages.last().cloned().expect("at least one mock message"));
        instant_stream(msg)
    })
}
