//! Mirrors: packages/ai/test/anthropic-tool-name-normalization.test.ts
//!
//! tau does not implement Claude Code OAuth-specific tool-name canonicalization.
//! The minimal invariant we want is simpler: tool names should pass through the
//! ai registry boundary unchanged, and tau should not silently remap names such
//! as `find -> Glob`.

mod common;
use common::{create_usage, mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{ContentBlock, Context, Message, Tool, UserContent, UserMessage};
use std::sync::{Arc, Mutex};

struct EchoToolNameProvider {
    seen_tool_names: Mutex<Vec<Vec<String>>>,
}

impl EchoToolNameProvider {
    fn new() -> Self {
        Self {
            seen_tool_names: Mutex::new(vec![]),
        }
    }

    fn stream_with_tool_name(&self, tool_name: String) -> AssistantMessageEventStream {
        let message = ai::types::AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolCall {
                id: "tool-1".into(),
                name: tool_name,
                arguments: std::collections::HashMap::new(),
                thought_signature: None,
            }],
            api: "test-anthropic-tools".into(),
            provider: "anthropic".into(),
            model: "mock".into(),
            usage: create_usage(),
            stop_reason: ai::types::StopReason::ToolUse,
            error_message: None,
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

impl ApiProvider for EchoToolNameProvider {
    fn api(&self) -> &str {
        "test-anthropic-tools"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        context: &Context,
        _options: Option<&ai::types::StreamOptions>,
    ) -> AssistantMessageEventStream {
        let tool_names: Vec<String> = context
            .tools
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        self.seen_tool_names.lock().unwrap().push(tool_names.clone());
        self.stream_with_tool_name(tool_names.first().cloned().unwrap_or_else(|| "none".into()))
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

fn context_with_tool(name: &str) -> Context {
    Context {
        system_prompt: Some(format!("Use the {name} tool when asked.")),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text(format!("Use the {name} tool.")),
            timestamp: 0,
        })],
        tools: Some(vec![Tool {
            name: name.into(),
            description: "Test tool".into(),
            parameters: serde_json::json!({"type":"object"}),
        }]),
    }
}

async fn assert_tool_name_round_trips(name: &str) {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(EchoToolNameProvider::new());
    register_api_provider(provider.clone());

    let model = mock_model("test-anthropic-tools", "anthropic");
    let response = complete(&model, &context_with_tool(name), Some(&ai::types::StreamOptions::default()))
        .await
        .unwrap();

    let seen = provider.seen_tool_names.lock().unwrap();
    assert_eq!(seen.as_slice(), &[vec![name.to_string()]]);

    let returned_name = match &response.content[0] {
        ContentBlock::ToolCall { name, .. } => name.clone(),
        other => panic!("expected tool call, got {other:?}"),
    };
    assert_eq!(returned_name, name);

    clear_api_providers();
}

#[tokio::test]
async fn lowercase_cc_style_tool_name_passes_through_unchanged() {
    assert_tool_name_round_trips("todowrite").await;
}

#[tokio::test]
async fn builtin_style_tool_name_passes_through_unchanged() {
    assert_tool_name_round_trips("read").await;
}

#[tokio::test]
async fn find_is_not_remapped_to_glob() {
    assert_tool_name_round_trips("find").await;
}

#[tokio::test]
async fn custom_tool_name_passes_through_unchanged() {
    assert_tool_name_round_trips("my_custom_tool").await;
}
