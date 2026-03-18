//! Mirrors: packages/ai/test/stream.test.ts
//!
//! tau does not yet ship concrete provider implementations, so these tests
//! exercise the stream/complete contract through mock providers registered in
//! the ai registry.

mod common;
use common::{create_usage, mock_model, registry_lock};

use ai::providers::{
    clear_api_providers, complete_simple, register_api_provider, stream, ApiProvider,
};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{
    AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, SimpleStreamOptions,
    StopReason, Tool, UserContent, UserMessage,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
enum MockScenario {
    BasicText,
    ToolCall,
    StreamingText,
    Thinking,
}

struct MockProvider {
    scenario: MockScenario,
    seen_tools: Mutex<Vec<Option<Vec<String>>>>,
}

impl MockProvider {
    fn new(scenario: MockScenario) -> Self {
        Self {
            scenario,
            seen_tools: Mutex::new(vec![]),
        }
    }

    fn make_message(
        &self,
        content: Vec<ContentBlock>,
        stop_reason: StopReason,
        error_message: Option<String>,
    ) -> AssistantMessage {
        AssistantMessage {
            role: "assistant".into(),
            content,
            api: "test-stream".into(),
            provider: "test".into(),
            model: "mock".into(),
            usage: create_usage(),
            stop_reason,
            error_message,
            timestamp: 0,
        }
    }

    fn stream_basic_text(&self) -> AssistantMessageEventStream {
        let message = self.make_message(
            vec![ContentBlock::Text {
                text: "Hello test successful".into(),
                text_signature: None,
            }],
            StopReason::Stop,
            None,
        );
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(AssistantMessageEvent::Start {
                partial: message.clone(),
            });
            tx.push(AssistantMessageEvent::Done {
                reason: message.stop_reason.clone(),
                message,
            });
        });
        stream
    }

    fn stream_tool_call(&self) -> AssistantMessageEventStream {
        let partial = self.make_message(
            vec![ContentBlock::ToolCall {
                id: "tool-1".into(),
                name: "math_operation".into(),
                arguments: HashMap::new(),
                thought_signature: None,
            }],
            StopReason::ToolUse,
            None,
        );
        let final_message = self.make_message(
            vec![ContentBlock::ToolCall {
                id: "tool-1".into(),
                name: "math_operation".into(),
                arguments: HashMap::from([
                    ("a".into(), serde_json::json!(15)),
                    ("b".into(), serde_json::json!(27)),
                    ("operation".into(), serde_json::json!("add")),
                ]),
                thought_signature: None,
            }],
            StopReason::ToolUse,
            None,
        );
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(AssistantMessageEvent::Start {
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::ToolCallStart {
                content_index: 0,
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::ToolCallDelta {
                content_index: 0,
                delta: r#"{"a":15,"b":27,"operation":"add"}"#.into(),
                partial: final_message.clone(),
            });
            tx.push(AssistantMessageEvent::ToolCallEnd {
                content_index: 0,
                tool_call: final_message.content[0].clone(),
                partial: final_message.clone(),
            });
            tx.push(AssistantMessageEvent::Done {
                reason: final_message.stop_reason.clone(),
                message: final_message,
            });
        });
        stream
    }

    fn stream_text_events(&self) -> AssistantMessageEventStream {
        let partial = self.make_message(
            vec![ContentBlock::Text {
                text: String::new(),
                text_signature: None,
            }],
            StopReason::Stop,
            None,
        );
        let final_message = self.make_message(
            vec![ContentBlock::Text {
                text: "1 2 3".into(),
                text_signature: None,
            }],
            StopReason::Stop,
            None,
        );
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(AssistantMessageEvent::Start {
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::TextStart {
                content_index: 0,
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "1 ".into(),
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "2 3".into(),
                partial: final_message.clone(),
            });
            tx.push(AssistantMessageEvent::TextEnd {
                content_index: 0,
                content: "1 2 3".into(),
                partial: final_message.clone(),
            });
            tx.push(AssistantMessageEvent::Done {
                reason: final_message.stop_reason.clone(),
                message: final_message,
            });
        });
        stream
    }

    fn stream_thinking_events(&self) -> AssistantMessageEventStream {
        let partial = self.make_message(
            vec![ContentBlock::Thinking {
                thinking: String::new(),
                thinking_signature: None,
                redacted: None,
            }],
            StopReason::Stop,
            None,
        );
        let final_message = self.make_message(
            vec![
                ContentBlock::Thinking {
                    thinking: "step by step".into(),
                    thinking_signature: None,
                    redacted: None,
                },
                ContentBlock::Text {
                    text: "42".into(),
                    text_signature: None,
                },
            ],
            StopReason::Stop,
            None,
        );
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(AssistantMessageEvent::Start {
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::ThinkingStart {
                content_index: 0,
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "step ".into(),
                partial: partial.clone(),
            });
            tx.push(AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "by step".into(),
                partial: final_message.clone(),
            });
            tx.push(AssistantMessageEvent::ThinkingEnd {
                content_index: 0,
                content: "step by step".into(),
                partial: final_message.clone(),
            });
            tx.push(AssistantMessageEvent::Done {
                reason: final_message.stop_reason.clone(),
                message: final_message,
            });
        });
        stream
    }

    fn run_stream(&self, context: &Context) -> AssistantMessageEventStream {
        self.seen_tools.lock().unwrap().push(
            context
                .tools
                .as_ref()
                .map(|tools| tools.iter().map(|tool| tool.name.clone()).collect()),
        );
        match self.scenario {
            MockScenario::BasicText => self.stream_basic_text(),
            MockScenario::ToolCall => self.stream_tool_call(),
            MockScenario::StreamingText => self.stream_text_events(),
            MockScenario::Thinking => self.stream_thinking_events(),
        }
    }
}

impl ApiProvider for MockProvider {
    fn api(&self) -> &str {
        "test-stream"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.run_stream(context)
    }

    fn stream_simple(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        self.run_stream(context)
    }
}

fn base_context(prompt: &str) -> Context {
    Context {
        system_prompt: Some("You are a helpful assistant. Be concise.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text(prompt.into()),
            timestamp: 0,
        })],
        tools: None,
    }
}

#[tokio::test]
async fn basic_text_generation_completes_successfully() {
    let _guard = registry_lock();
    clear_api_providers();
    register_api_provider(Arc::new(MockProvider::new(MockScenario::BasicText)));

    let model = mock_model("test-stream", "test");
    let response = complete_simple(
        &model,
        &base_context("Reply with exactly: 'Hello test successful'"),
        Some(&SimpleStreamOptions::default()),
    )
    .await
    .unwrap();

    assert_eq!(response.role, "assistant");
    assert!(matches!(response.stop_reason, StopReason::Stop));
    assert!(response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { text, .. } if text.contains("Hello test successful"))));

    clear_api_providers();
}

#[tokio::test]
async fn handle_tool_call_emits_complete_tool_call_payload() {
    let _guard = registry_lock();
    clear_api_providers();
    let provider = Arc::new(MockProvider::new(MockScenario::ToolCall));
    register_api_provider(provider.clone());

    let mut context = base_context("Calculate 15 + 27 using the math_operation tool.");
    context.tools = Some(vec![Tool {
        name: "math_operation".into(),
        description: "Perform basic arithmetic operations".into(),
        parameters: serde_json::json!({"type":"object"}),
    }]);

    let model = mock_model("test-stream", "test");
    let mut s = stream(&model, &context, Some(&ai::types::StreamOptions::default())).unwrap();
    let mut has_tool_start = false;
    let mut has_tool_delta = false;
    let mut has_tool_end = false;
    let mut accumulated_tool_args = String::new();

    while let Some(event) = s.next().await {
        match event {
            AssistantMessageEvent::ToolCallStart {
                content_index,
                partial,
            } => {
                has_tool_start = true;
                assert_eq!(content_index, 0);
                assert!(
                    matches!(&partial.content[0], ContentBlock::ToolCall { name, .. } if name == "math_operation")
                );
            }
            AssistantMessageEvent::ToolCallDelta { delta, .. } => {
                has_tool_delta = true;
                accumulated_tool_args.push_str(&delta);
            }
            AssistantMessageEvent::ToolCallEnd {
                content_index,
                tool_call,
                ..
            } => {
                has_tool_end = true;
                assert_eq!(content_index, 0);
                assert!(
                    matches!(tool_call, ContentBlock::ToolCall { name, .. } if name == "math_operation")
                );
            }
            _ => {}
        }
    }

    let response = s.result().await;
    assert!(has_tool_start);
    assert!(has_tool_delta);
    assert!(has_tool_end);
    let parsed: serde_json::Value = serde_json::from_str(&accumulated_tool_args).unwrap();
    assert_eq!(parsed["a"], 15);
    assert_eq!(parsed["b"], 27);
    assert_eq!(parsed["operation"], "add");
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert_eq!(
        provider.seen_tools.lock().unwrap().as_slice(),
        &[Some(vec!["math_operation".to_string()])]
    );

    clear_api_providers();
}

#[tokio::test]
async fn handle_streaming_text_events() {
    let _guard = registry_lock();
    clear_api_providers();
    register_api_provider(Arc::new(MockProvider::new(MockScenario::StreamingText)));

    let model = mock_model("test-stream", "test");
    let mut s = stream(
        &model,
        &base_context("Count from 1 to 3"),
        Some(&ai::types::StreamOptions::default()),
    )
    .unwrap();
    let mut text_started = false;
    let mut text_chunks = String::new();
    let mut text_completed = false;

    while let Some(event) = s.next().await {
        match event {
            AssistantMessageEvent::TextStart { .. } => text_started = true,
            AssistantMessageEvent::TextDelta { delta, .. } => text_chunks.push_str(&delta),
            AssistantMessageEvent::TextEnd { .. } => text_completed = true,
            _ => {}
        }
    }

    let response = s.result().await;
    assert!(text_started);
    assert_eq!(text_chunks, "1 2 3");
    assert!(text_completed);
    assert!(response
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { text, .. } if text == "1 2 3")));

    clear_api_providers();
}

#[tokio::test]
async fn handle_thinking_events() {
    let _guard = registry_lock();
    clear_api_providers();
    register_api_provider(Arc::new(MockProvider::new(MockScenario::Thinking)));

    let model = mock_model("test-stream", "test");
    let mut opts = SimpleStreamOptions::default();
    opts.reasoning = Some(ai::types::ThinkingLevel::High);

    let mut s =
        ai::providers::stream_simple(&model, &base_context("Think step by step"), Some(&opts))
            .unwrap();
    let mut thinking_started = false;
    let mut thinking_chunks = String::new();
    let mut thinking_completed = false;

    while let Some(event) = s.next().await {
        match event {
            AssistantMessageEvent::ThinkingStart { .. } => thinking_started = true,
            AssistantMessageEvent::ThinkingDelta { delta, .. } => thinking_chunks.push_str(&delta),
            AssistantMessageEvent::ThinkingEnd { .. } => thinking_completed = true,
            _ => {}
        }
    }

    let response = s.result().await;
    assert!(thinking_started);
    assert_eq!(thinking_chunks, "step by step");
    assert!(thinking_completed);
    assert_eq!(response.stop_reason, StopReason::Stop);
    assert!(response.content.iter().any(|block| matches!(block, ContentBlock::Thinking { thinking, .. } if thinking == "step by step")));

    clear_api_providers();
}
