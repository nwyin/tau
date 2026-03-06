//! Mirrors: packages/ai/test/image-tool-result.test.ts
//!
//! tau currently validates this at the provider boundary: tool results may
//! contain image blocks, and providers receive them unchanged alongside any
//! adjacent text blocks.

mod common;
use common::{create_assistant_message, mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{Context, Message, ToolResultMessage, UserBlock, UserContent, UserMessage};
use std::sync::{Arc, Mutex};

struct CapturingImageProvider {
    seen_contexts: Mutex<Vec<Context>>,
}

impl CapturingImageProvider {
    fn new() -> Self {
        Self {
            seen_contexts: Mutex::new(vec![]),
        }
    }

    fn finish_ok(&self) -> AssistantMessageEventStream {
        let message = create_assistant_message("red circle");
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

impl ApiProvider for CapturingImageProvider {
    fn api(&self) -> &str {
        "test-image-tool-result"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.seen_contexts.lock().unwrap().push(context.clone());
        self.finish_ok()
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

fn base_context_with_tool_result(content: Vec<UserBlock>) -> Context {
    Context {
        system_prompt: Some("You are a helpful assistant that uses tools when asked.".into()),
        messages: vec![
            Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Describe the result from the tool.".into()),
                timestamp: 0,
            }),
            Message::ToolResult(ToolResultMessage {
                role: "toolResult".into(),
                tool_call_id: "tool-1".into(),
                tool_name: "get_circle".into(),
                content,
                details: None,
                is_error: false,
                timestamp: 0,
            }),
        ],
        tools: None,
    }
}

#[tokio::test]
async fn tool_result_with_only_image_reaches_provider_unchanged() {
    let _guard = registry_lock();
    clear_api_providers();
    let provider = Arc::new(CapturingImageProvider::new());
    register_api_provider(provider.clone());

    let model = mock_model("test-image-tool-result", "test");
    let context = base_context_with_tool_result(vec![UserBlock::Image {
        data: "ZmFrZV9pbWFnZQ==".into(),
        mime_type: "image/png".into(),
    }]);

    let response = complete(&model, &context, Some(&ai::types::StreamOptions::default())).await.unwrap();
    assert_eq!(response.role, "assistant");

    let seen = provider.seen_contexts.lock().unwrap();
    let tool_result = match &seen[0].messages[1] {
        Message::ToolResult(result) => result,
        _ => panic!("expected tool result"),
    };
    assert!(matches!(
        &tool_result.content[0],
        UserBlock::Image { mime_type, .. } if mime_type == "image/png"
    ));

    clear_api_providers();
}

#[tokio::test]
async fn tool_result_with_text_and_image_reaches_provider_unchanged() {
    let _guard = registry_lock();
    clear_api_providers();
    let provider = Arc::new(CapturingImageProvider::new());
    register_api_provider(provider.clone());

    let model = mock_model("test-image-tool-result", "test");
    let context = base_context_with_tool_result(vec![
        UserBlock::Text {
            text: "This is a geometric shape with a diameter of 100 pixels.".into(),
        },
        UserBlock::Image {
            data: "ZmFrZV9pbWFnZQ==".into(),
            mime_type: "image/png".into(),
        },
    ]);

    let response = complete(&model, &context, Some(&ai::types::StreamOptions::default())).await.unwrap();
    assert_eq!(response.role, "assistant");

    let seen = provider.seen_contexts.lock().unwrap();
    let tool_result = match &seen[0].messages[1] {
        Message::ToolResult(result) => result,
        _ => panic!("expected tool result"),
    };
    assert_eq!(tool_result.content.len(), 2);
    assert!(matches!(
        &tool_result.content[0],
        UserBlock::Text { text } if text.contains("100 pixels")
    ));
    assert!(matches!(
        &tool_result.content[1],
        UserBlock::Image { mime_type, .. } if mime_type == "image/png"
    ));

    clear_api_providers();
}
