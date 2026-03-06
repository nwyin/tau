//! Mirrors: packages/ai/test/openai-responses-reasoning-replay-e2e.test.ts
//!
//! tau does not yet build OpenAI Responses payloads, so the regression we can
//! cover today is narrower: reasoning-bearing assistant history and tool-call
//! history must survive the provider boundary without causing local failures or
//! being silently discarded.

mod common;
use common::{create_assistant_message, mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{
    AssistantMessage, ContentBlock, Context, Message, StopReason, StreamOptions, Tool,
    ToolResultMessage, UserBlock, UserContent, UserMessage,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn test_tool() -> Tool {
    Tool {
        name: "double_number".into(),
        description: "Doubles a number and returns the result".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "value": { "type": "number" } },
            "required": ["value"]
        }),
    }
}

fn user_message(text: &str) -> Message {
    Message::User(UserMessage {
        role: "user".into(),
        content: UserContent::Text(text.into()),
        timestamp: 0,
    })
}

fn assistant_with_reasoning_and_tool_call(
    model: &ai::types::Model,
    tool_call_id: &str,
    stop_reason: StopReason,
) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".into(),
        content: vec![
            ContentBlock::Thinking {
                thinking: "I should use the tool.".into(),
                thinking_signature: Some("sig-1".into()),
                redacted: None,
            },
            ContentBlock::ToolCall {
                id: tool_call_id.into(),
                name: "double_number".into(),
                arguments: HashMap::from([("value".into(), serde_json::json!(21))]),
                thought_signature: Some("sig-1".into()),
            },
        ],
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: ai::types::Usage::default(),
        stop_reason,
        error_message: None,
        timestamp: 0,
    }
}

struct CapturingReplayProvider {
    seen_contexts: Mutex<Vec<Context>>,
}

impl CapturingReplayProvider {
    fn new() -> Self {
        Self {
            seen_contexts: Mutex::new(vec![]),
        }
    }

    fn finish_with_text(&self, text: &str) -> AssistantMessageEventStream {
        let message = create_assistant_message(text);
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(ai::types::AssistantMessageEvent::Start { partial: message.clone() });
            tx.push(ai::types::AssistantMessageEvent::Done {
                reason: message.stop_reason.clone(),
                message,
            });
        });
        stream
    }
}

impl ApiProvider for CapturingReplayProvider {
    fn api(&self) -> &str {
        "test-openai-replay"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.seen_contexts.lock().unwrap().push(context.clone());
        self.finish_with_text("42")
    }

    fn stream_simple(
        &self,
        model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        self.stream(model, context, None)
    }
}

#[tokio::test]
async fn reasoning_only_aborted_history_is_forwarded_without_local_error() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(CapturingReplayProvider::new());
    register_api_provider(provider.clone());

    let model = mock_model("test-openai-replay", "openai");
    let assistant = AssistantMessage {
        content: vec![ContentBlock::Thinking {
            thinking: "interrupted reasoning".into(),
            thinking_signature: Some("sig-aborted".into()),
            redacted: None,
        }],
        stop_reason: StopReason::Aborted,
        ..create_assistant_message("")
    };
    let context = Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![
            user_message("Use the double_number tool to double 21."),
            Message::Assistant(assistant),
            user_message("Say hello to confirm you can continue."),
        ],
        tools: Some(vec![test_tool()]),
    };

    let response = complete(&model, &context, Some(&StreamOptions::default())).await.unwrap();
    assert_eq!(response.stop_reason, StopReason::Stop);

    let seen = provider.seen_contexts.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].messages.len(), 3);
    match &seen[0].messages[1] {
        Message::Assistant(message) => {
            assert_eq!(message.stop_reason, StopReason::Aborted);
            assert!(matches!(
                &message.content[0],
                ContentBlock::Thinking { thinking_signature: Some(sig), .. } if sig == "sig-aborted"
            ));
        }
        _ => panic!("expected assistant message"),
    }

    clear_api_providers();
}

#[tokio::test]
async fn same_provider_different_model_history_keeps_reasoning_and_tool_call_ids() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(CapturingReplayProvider::new());
    register_api_provider(provider.clone());

    let mut model_b = mock_model("test-openai-replay", "openai");
    model_b.id = "gpt-5.2-codex".into();

    let model_a = ai::types::Model {
        id: "gpt-5-mini".into(),
        ..model_b.clone()
    };

    let assistant = assistant_with_reasoning_and_tool_call(&model_a, "fc_123", StopReason::ToolUse);
    let tool_result = Message::ToolResult(ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: "fc_123".into(),
        tool_name: "double_number".into(),
        content: vec![UserBlock::Text { text: "42".into() }],
        details: None,
        is_error: false,
        timestamp: 0,
    });

    let context = Context {
        system_prompt: Some("You are a helpful assistant. Answer concisely.".into()),
        messages: vec![
            user_message("Use the double_number tool to double 21."),
            Message::Assistant(assistant),
            tool_result,
            user_message("What was the result? Answer with just the number."),
        ],
        tools: Some(vec![test_tool()]),
    };

    let response = complete(&model_b, &context, Some(&StreamOptions::default())).await.unwrap();
    assert_eq!(response.stop_reason, StopReason::Stop);

    let seen = provider.seen_contexts.lock().unwrap();
    let captured = &seen[0];
    match &captured.messages[1] {
        Message::Assistant(message) => {
            assert!(message.content.iter().any(|block| matches!(
                block,
                ContentBlock::Thinking { thinking_signature: Some(sig), .. } if sig == "sig-1"
            )));
            assert!(message.content.iter().any(|block| matches!(
                block,
                ContentBlock::ToolCall { id, .. } if id == "fc_123"
            )));
        }
        _ => panic!("expected assistant message"),
    }
    match &captured.messages[2] {
        Message::ToolResult(result) => assert_eq!(result.tool_call_id, "fc_123"),
        _ => panic!("expected tool result"),
    }

    clear_api_providers();
}

#[tokio::test]
async fn cross_provider_history_keeps_foreign_tool_call_ids_intact() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(CapturingReplayProvider::new());
    register_api_provider(provider.clone());

    let mut openai_model = mock_model("test-openai-replay", "openai");
    openai_model.id = "gpt-5.2-codex".into();

    let anthropic_model = ai::types::Model {
        id: "claude-sonnet-4-5".into(),
        provider: "anthropic".into(),
        ..openai_model.clone()
    };

    let assistant = assistant_with_reasoning_and_tool_call(&anthropic_model, "toolu_123", StopReason::ToolUse);
    let tool_result = Message::ToolResult(ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: "toolu_123".into(),
        tool_name: "double_number".into(),
        content: vec![UserBlock::Text { text: "42".into() }],
        details: None,
        is_error: false,
        timestamp: 0,
    });

    let context = Context {
        system_prompt: Some("You are a helpful assistant. Answer concisely.".into()),
        messages: vec![
            user_message("Use the double_number tool to double 21."),
            Message::Assistant(assistant),
            tool_result,
            user_message("What was the result? Answer with just the number."),
        ],
        tools: Some(vec![test_tool()]),
    };

    let response = complete(&openai_model, &context, Some(&StreamOptions::default())).await.unwrap();
    assert_eq!(response.stop_reason, StopReason::Stop);

    let seen = provider.seen_contexts.lock().unwrap();
    match &seen[0].messages[1] {
        Message::Assistant(message) => assert!(message.content.iter().any(|block| matches!(
            block,
            ContentBlock::ToolCall { id, .. } if id == "toolu_123"
        ))),
        _ => panic!("expected assistant message"),
    }

    clear_api_providers();
}
