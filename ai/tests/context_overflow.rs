//! Mirrors: packages/ai/test/context-overflow.test.ts
//!
//! tau does not yet expose provider-specific overflow helpers, so these tests
//! cover the local contract we can enforce today:
//! 1. overflow-sized user content reaches the provider unchanged,
//! 2. providers can surface a standardized error response,
//! 3. callers can classify that response from stop reason + message text.

mod common;
use common::{mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{AssistantMessage, ContentBlock, Context, Message, StopReason, StreamOptions, Usage, UserContent, UserMessage};
use std::sync::{Arc, Mutex};

const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. ";

fn generate_overflow_content(context_window: u64) -> String {
    let target_tokens = context_window + 10_000;
    let target_chars = target_tokens * 4;
    let repetitions = ((target_chars as f64) / (LOREM_IPSUM.len() as f64)).ceil() as usize;
    LOREM_IPSUM.repeat(repetitions)
}

fn is_context_overflow(response: &AssistantMessage, context_window: u64) -> bool {
    if response.stop_reason != StopReason::Error {
        return false;
    }

    let msg = response
        .error_message
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    msg.contains("context")
        || msg.contains("maximum context length")
        || msg.contains("prompt is too long")
        || response.usage.input > context_window
        || response.usage.cache_read > context_window
}

struct OverflowProvider {
    seen_user_lengths: Mutex<Vec<usize>>,
    expected_context_window: u64,
}

impl OverflowProvider {
    fn new(expected_context_window: u64) -> Self {
        Self {
            seen_user_lengths: Mutex::new(vec![]),
            expected_context_window,
        }
    }

    fn overflow_stream(&self, context_window: u64) -> AssistantMessageEventStream {
        let message = AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: String::new(),
                text_signature: None,
            }],
            api: "test-context-overflow".into(),
            provider: "test".into(),
            model: "overflow".into(),
            usage: Usage {
                input: context_window + 1,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                total_tokens: context_window + 1,
                cost: ai::types::Cost::default(),
            },
            stop_reason: StopReason::Error,
            error_message: Some("prompt is too long: exceeds the context window".into()),
            timestamp: 0,
        };

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

impl ApiProvider for OverflowProvider {
    fn api(&self) -> &str {
        "test-context-overflow"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&StreamOptions>,
    ) -> AssistantMessageEventStream {
        let text_len = context
            .messages
            .iter()
            .find_map(|message| match message {
                Message::User(UserMessage {
                    content: UserContent::Text(text),
                    ..
                }) => Some(text.len()),
                _ => None,
            })
            .unwrap_or_default();

        self.seen_user_lengths.lock().unwrap().push(text_len);
        self.overflow_stream(self.expected_context_window)
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
async fn overflow_content_reaches_provider_and_is_classified_as_context_overflow() {
    let _guard = registry_lock();
    clear_api_providers();

    let context_window = 8_192;
    let provider = Arc::new(OverflowProvider::new(context_window));
    register_api_provider(provider.clone());

    let mut model = mock_model("test-context-overflow", "test");
    model.context_window = context_window;

    let overflow_content = generate_overflow_content(context_window);
    let context = Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text(overflow_content.clone()),
            timestamp: 0,
        })],
        tools: None,
    };

    let response = complete(&model, &context, Some(&StreamOptions::default())).await.unwrap();

    assert_eq!(provider.seen_user_lengths.lock().unwrap().as_slice(), &[overflow_content.len()]);
    assert_eq!(response.stop_reason, StopReason::Error);
    assert!(response.error_message.as_deref().unwrap_or_default().contains("context window"));
    assert!(is_context_overflow(&response, context_window));

    clear_api_providers();
}

#[test]
fn helper_detects_common_overflow_error_patterns() {
    let response = AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::Text {
            text: String::new(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "gpt-4o".into(),
        usage: Usage::default(),
        stop_reason: StopReason::Error,
        error_message: Some("maximum context length exceeded".into()),
        timestamp: 0,
    };

    assert!(is_context_overflow(&response, 128_000));
}

#[test]
fn helper_rejects_non_overflow_errors() {
    let response = AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::Text {
            text: String::new(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "gpt-4o".into(),
        usage: Usage::default(),
        stop_reason: StopReason::Error,
        error_message: Some("rate limit exceeded".into()),
        timestamp: 0,
    };

    assert!(!is_context_overflow(&response, 128_000));
}
