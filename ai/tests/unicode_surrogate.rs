//! Mirrors: packages/ai/test/unicode-surrogate.test.ts
//!
//! The original JS regression was about invalid surrogate handling during JSON
//! serialization. Rust `String` values are valid UTF-8 by construction, so the
//! meaningful regression coverage here is:
//! 1. Unicode-rich tool results serialize without error.
//! 2. The serialized form round-trips intact.
//! 3. Provider calls receive the exact Unicode content in context.

mod common;
use common::{create_assistant_message, mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{Context, Message, ToolResultMessage, UserBlock, UserContent, UserMessage};
use std::sync::{Arc, Mutex};

fn emoji_tool_result_text() -> String {
    [
        "Test with emoji 🙈 and other characters:",
        "- Monkey emoji: 🙈",
        "- Thumbs up: 👍",
        "- Heart: ❤️",
        "- Thinking face: 🤔",
        "- Rocket: 🚀",
        "- Mixed text: Mario Zechner wann? Wo? Bin grad äußersr eventuninformiert 🙈",
        "- Japanese: こんにちは",
        "- Chinese: 你好",
        "- Mathematical symbols: ∑∫∂√",
        "- Special quotes: “curly” ‘quotes’",
    ]
    .join("\n")
}

fn build_context_with_unicode_tool_result() -> Context {
    let tool_result = ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: "tool_1".into(),
        tool_name: "test_tool".into(),
        content: vec![UserBlock::Text {
            text: emoji_tool_result_text(),
        }],
        details: None,
        is_error: false,
        timestamp: 0,
    };

    Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![
            Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Use the test tool".into()),
                timestamp: 0,
            }),
            Message::ToolResult(tool_result),
            Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Summarize the tool result briefly.".into()),
                timestamp: 0,
            }),
        ],
        tools: None,
    }
}

struct CapturingProvider {
    seen_contexts: Mutex<Vec<Context>>,
}

impl CapturingProvider {
    fn new() -> Self {
        Self {
            seen_contexts: Mutex::new(vec![]),
        }
    }

    fn stream_with_text(&self, text: &str) -> AssistantMessageEventStream {
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

impl ApiProvider for CapturingProvider {
    fn api(&self) -> &str {
        "test-unicode"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.seen_contexts.lock().unwrap().push(context.clone());
        self.stream_with_text("ok")
    }

    fn stream_simple(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        self.seen_contexts.lock().unwrap().push(context.clone());
        self.stream_with_text("ok")
    }
}

#[test]
fn serializes_unicode_tool_result_without_error() {
    let context = build_context_with_unicode_tool_result();
    let json = serde_json::to_string(&context.messages).expect("unicode tool result should serialize");

    assert!(json.contains("🙈"));
    assert!(json.contains("こんにちは"));
    assert!(json.contains("你好"));
}

#[test]
fn round_trips_unicode_tool_result_text() {
    let context = build_context_with_unicode_tool_result();
    let tool_result = match &context.messages[1] {
        Message::ToolResult(result) => result,
        _ => panic!("expected tool result message"),
    };

    let json = serde_json::to_string(tool_result).unwrap();
    let round_tripped: ToolResultMessage = serde_json::from_str(&json).unwrap();

    let text = match &round_tripped.content[0] {
        UserBlock::Text { text } => text.clone(),
        _ => panic!("expected text tool result"),
    };

    assert_eq!(text, emoji_tool_result_text());
}

#[tokio::test]
async fn passes_unicode_tool_result_to_provider_unchanged() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(CapturingProvider::new());
    register_api_provider(provider.clone());

    let model = mock_model("test-unicode", "test");
    let context = build_context_with_unicode_tool_result();
    let response = complete(&model, &context, Some(&ai::types::StreamOptions::default()))
        .await
        .unwrap();

    assert_eq!(response.role, "assistant");

    let seen = provider.seen_contexts.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let captured_text = match &seen[0].messages[1] {
        Message::ToolResult(result) => match &result.content[0] {
            UserBlock::Text { text } => text.clone(),
            _ => panic!("expected text tool result"),
        },
        _ => panic!("expected tool result message"),
    };
    assert_eq!(captured_text, emoji_tool_result_text());

    clear_api_providers();
}
