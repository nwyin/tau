//! Mirrors: packages/ai/test/tool-call-without-result.test.ts
//! Sends an assistant message with a tool call but no subsequent tool result,
//! then a new user message — verifies the provider handles it gracefully.

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::complete_simple;
use ai::types::{
    ContentBlock, Context, Message, SimpleStreamOptions, StopReason, ToolResultMessage,
    UserContent, UserMessage,
};
use std::collections::HashMap;

fn build_context_with_orphan_tool_call(api: &str, provider: &str) -> Context {
    // Assistant message with a tool call but no tool result follows
    let assistant_msg = ai::types::AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::ToolCall {
            id: "orphan-call-1".into(),
            name: "some_tool".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("arg".into(), serde_json::Value::String("value".into()));
                m
            },
            thought_signature: None,
        }],
        api: api.into(),
        provider: provider.into(),
        model: "test".into(),
        usage: ai::types::Usage::default(),
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp: 0,
    };

    Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![
            Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Use some_tool".into()),
                timestamp: 0,
            }),
            Message::Assistant(assistant_msg),
            // Deliberately no ToolResult here
            Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Never mind, just say hi.".into()),
                timestamp: 0,
            }),
        ],
        tools: None,
    }
}

async fn test_tool_call_without_result(model: &ai::types::Model, api_key: Option<String>) {
    let context = build_context_with_orphan_tool_call(&model.api, &model.provider);
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = api_key;
    let response = complete_simple(model, &context, Some(&opts)).await.unwrap();
    // Provider should handle gracefully — either produce a response or a clean error.
    assert_eq!(response.role, "assistant");
}

// ---------------------------------------------------------------------------
// Per-provider
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_tool_call_without_result() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_tool_call_without_result(&model, env_key("ANTHROPIC_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_tool_call_without_result() {
    let model = get_model("openai", "gpt-5-mini").unwrap();
    test_tool_call_without_result(&model, env_key("OPENAI_API_KEY")).await;
}

