//! Mirrors: packages/ai/test/interleaved-thinking.test.ts
//!
//! In tau, the core invariant we can enforce today is that a second request can
//! carry forward both prior thinking blocks and tool-result history without
//! losing them at the provider boundary.

mod common;
use common::{create_usage, mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete_simple, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{
    AssistantMessage, ContentBlock, Context, Message, SimpleStreamOptions, StopReason,
    ThinkingLevel, Tool, ToolResultMessage, UserBlock, UserContent, UserMessage,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn calculator_tool() -> Tool {
    Tool {
        name: "calculator".into(),
        description: "Perform basic arithmetic operations".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "a": { "type": "number" },
                "b": { "type": "number" },
                "operation": { "type": "string" }
            }
        }),
    }
}

fn assistant_with_thinking_and_tool_call() -> AssistantMessage {
    AssistantMessage {
        role: "assistant".into(),
        content: vec![
            ContentBlock::Thinking {
                thinking: "I should calculate first.".into(),
                thinking_signature: Some("sig-calc".into()),
                redacted: None,
            },
            ContentBlock::ToolCall {
                id: "calc-1".into(),
                name: "calculator".into(),
                arguments: HashMap::from([
                    ("a".into(), serde_json::json!(328)),
                    ("b".into(), serde_json::json!(29)),
                    ("operation".into(), serde_json::json!("multiply")),
                ]),
                thought_signature: Some("sig-calc".into()),
            },
        ],
        api: "test-interleaved-thinking".into(),
        provider: "test".into(),
        model: "mock".into(),
        usage: create_usage(),
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp: 0,
    }
}

struct InterleavedThinkingProvider {
    seen_contexts: Mutex<Vec<Context>>,
}

impl InterleavedThinkingProvider {
    fn new() -> Self {
        Self {
            seen_contexts: Mutex::new(vec![]),
        }
    }

    fn first_response(&self) -> AssistantMessageEventStream {
        let message = assistant_with_thinking_and_tool_call();
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(ai::types::AssistantMessageEvent::Start {
                partial: message.clone(),
            });
            tx.push(ai::types::AssistantMessageEvent::Done {
                reason: message.stop_reason.clone(),
                message,
            });
        });
        stream
    }

    fn second_response(&self) -> AssistantMessageEventStream {
        let message = AssistantMessage {
            role: "assistant".into(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "I should answer based on the tool result.".into(),
                    thinking_signature: Some("sig-answer".into()),
                    redacted: None,
                },
                ContentBlock::Text {
                    text: "The answer is 9512.".into(),
                    text_signature: None,
                },
            ],
            api: "test-interleaved-thinking".into(),
            provider: "test".into(),
            model: "mock".into(),
            usage: create_usage(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        };
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(ai::types::AssistantMessageEvent::Start {
                partial: message.clone(),
            });
            tx.push(ai::types::AssistantMessageEvent::Done {
                reason: message.stop_reason.clone(),
                message,
            });
        });
        stream
    }
}

impl ApiProvider for InterleavedThinkingProvider {
    fn api(&self) -> &str {
        "test-interleaved-thinking"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.seen_contexts.lock().unwrap().push(context.clone());
        if context.messages.len() == 1 {
            self.first_response()
        } else {
            self.second_response()
        }
    }

    fn stream_simple(
        &self,
        model: &ai::types::Model,
        context: &Context,
        _options: Option<&SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        self.stream(model, context, None)
    }
}

#[tokio::test]
async fn second_request_preserves_prior_thinking_and_tool_result_history() {
    let _guard = registry_lock();
    clear_api_providers();
    let provider = Arc::new(InterleavedThinkingProvider::new());
    register_api_provider(provider.clone());

    let model = mock_model("test-interleaved-thinking", "test");
    let mut opts = SimpleStreamOptions::default();
    opts.reasoning = Some(ThinkingLevel::High);

    let mut context = Context {
        system_prompt: Some(
            "You are a helpful assistant that must use tools for arithmetic.".into(),
        ),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Use calculator to calculate 328 * 29.".into()),
            timestamp: 0,
        })],
        tools: Some(vec![calculator_tool()]),
    };

    let first_response = complete_simple(&model, &context, Some(&opts))
        .await
        .unwrap();
    assert_eq!(first_response.stop_reason, StopReason::ToolUse);
    assert!(first_response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Thinking { .. })));
    assert!(first_response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolCall { .. })));

    context
        .messages
        .push(Message::Assistant(first_response.clone()));
    context
        .messages
        .push(Message::ToolResult(ToolResultMessage {
            role: "toolResult".into(),
            tool_call_id: "calc-1".into(),
            tool_name: "calculator".into(),
            content: vec![UserBlock::Text {
                text: "The answer is 9512 or 19024.".into(),
            }],
            details: None,
            is_error: false,
            timestamp: 0,
        }));

    let second_response = complete_simple(&model, &context, Some(&opts))
        .await
        .unwrap();
    assert_eq!(second_response.stop_reason, StopReason::Stop);
    assert!(second_response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Thinking { .. })));
    assert!(second_response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { text, .. } if text.contains("9512"))));

    let seen = provider.seen_contexts.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].messages.len(), 3);
    match &seen[1].messages[1] {
        Message::Assistant(message) => assert!(message.content.iter().any(|block| matches!(
            block,
            ContentBlock::Thinking { thinking_signature: Some(sig), .. } if sig == "sig-calc"
        ))),
        _ => panic!("expected assistant message in second request"),
    }
    match &seen[1].messages[2] {
        Message::ToolResult(result) => assert_eq!(result.tool_call_id, "calc-1"),
        _ => panic!("expected tool result in second request"),
    }

    clear_api_providers();
}
