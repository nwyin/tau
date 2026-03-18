//! Mirrors: packages/ai/test/xhigh.test.ts
//!
//! tau's current surface can exercise xhigh reasoning at the provider registry
//! boundary. These tests validate that:
//! 1. `xhigh` reaches providers through `SimpleStreamOptions`.
//! 2. providers can gate behavior using `supports_xhigh(model)`.

mod common;
use common::{create_usage, mock_model, registry_lock};

use ai::models::{get_model, supports_xhigh};
use ai::providers::{clear_api_providers, complete_simple, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{
    ContentBlock, Context, Message, SimpleStreamOptions, StopReason, ThinkingLevel, UserContent,
    UserMessage,
};
use std::sync::{Arc, Mutex};

struct XHighProvider {
    seen_reasoning: Mutex<Vec<Option<ThinkingLevel>>>,
}

impl XHighProvider {
    fn new() -> Self {
        Self {
            seen_reasoning: Mutex::new(vec![]),
        }
    }

    fn stream_for_model(
        &self,
        model: &ai::types::Model,
        reasoning: Option<ThinkingLevel>,
    ) -> AssistantMessageEventStream {
        let message = if reasoning == Some(ThinkingLevel::XHigh) && !supports_xhigh(model) {
            ai::types::AssistantMessage {
                role: "assistant".into(),
                content: vec![ContentBlock::Text {
                    text: String::new(),
                    text_signature: None,
                }],
                api: model.api.clone(),
                provider: model.provider.clone(),
                model: model.id.clone(),
                usage: create_usage(),
                stop_reason: StopReason::Error,
                error_message: Some("xhigh reasoning is not supported for this model".into()),
                timestamp: 0,
            }
        } else {
            ai::types::AssistantMessage {
                role: "assistant".into(),
                content: vec![
                    ContentBlock::Thinking {
                        thinking: "deliberating".into(),
                        thinking_signature: None,
                        redacted: None,
                    },
                    ContentBlock::Text {
                        text: "42".into(),
                        text_signature: None,
                    },
                ],
                api: model.api.clone(),
                provider: model.provider.clone(),
                model: model.id.clone(),
                usage: create_usage(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            }
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

impl ApiProvider for XHighProvider {
    fn api(&self) -> &str {
        "test-xhigh"
    }

    fn stream(
        &self,
        model: &ai::types::Model,
        _context: &Context,
        _options: Option<&ai::types::StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.stream_for_model(model, None)
    }

    fn stream_simple(
        &self,
        model: &ai::types::Model,
        _context: &Context,
        options: Option<&SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let reasoning = options.and_then(|opts| opts.reasoning.clone());
        self.seen_reasoning.lock().unwrap().push(reasoning.clone());
        self.stream_for_model(model, reasoning)
    }
}

fn make_context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("What is 20 + 22? Think step by step.".into()),
            timestamp: 0,
        })],
        tools: None,
    }
}

#[tokio::test]
async fn supported_xhigh_model_accepts_xhigh_reasoning() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(XHighProvider::new());
    register_api_provider(provider.clone());

    let mut model = get_model("openai", "gpt-5.2-codex")
        .unwrap()
        .as_ref()
        .clone();
    model.api = "test-xhigh".into();

    let mut opts = SimpleStreamOptions::default();
    opts.reasoning = Some(ThinkingLevel::XHigh);

    let response = complete_simple(&model, &make_context(), Some(&opts))
        .await
        .unwrap();
    assert_eq!(
        provider.seen_reasoning.lock().unwrap().as_slice(),
        &[Some(ThinkingLevel::XHigh)]
    );
    assert_eq!(response.stop_reason, StopReason::Stop);
    assert!(response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Thinking { .. })));
    assert!(response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { text, .. } if text == "42")));

    clear_api_providers();
}

#[tokio::test]
async fn unsupported_xhigh_model_returns_error_for_xhigh_reasoning() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(XHighProvider::new());
    register_api_provider(provider.clone());

    let mut model = get_model("openai", "gpt-5-mini").unwrap().as_ref().clone();
    model.api = "test-xhigh".into();

    let mut opts = SimpleStreamOptions::default();
    opts.reasoning = Some(ThinkingLevel::XHigh);

    let response = complete_simple(&model, &make_context(), Some(&opts))
        .await
        .unwrap();
    assert_eq!(
        provider.seen_reasoning.lock().unwrap().as_slice(),
        &[Some(ThinkingLevel::XHigh)]
    );
    assert_eq!(response.stop_reason, StopReason::Error);
    assert!(response.error_message.unwrap_or_default().contains("xhigh"));

    clear_api_providers();
}

#[test]
fn non_anthropic_non_gpt52_models_do_not_support_xhigh() {
    let model = mock_model("openai-responses", "openai");
    assert!(!supports_xhigh(&model));
}
