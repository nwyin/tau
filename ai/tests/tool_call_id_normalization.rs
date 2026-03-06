//! Mirrors: packages/ai/test/tool-call-id-normalization.test.ts
//! Regression test for issue #1022 — long pipe-separated tool call IDs from
//! OpenAI Responses API must be normalized before being sent to other providers.

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::complete_simple;
use ai::types::{
    ContentBlock, Context, Message, SimpleStreamOptions, StopReason, ToolResultMessage,
    UserBlock, UserContent, UserMessage,
};
use std::collections::HashMap;

/// Exact tool call ID from issue #1022 that caused "call_id too long" errors.
const FAILING_TOOL_CALL_ID: &str = "call_pAYbIr76hXIjncD9UE4eGfnS|\
    t5nnb2qYMFWGSsr13fhCd1CaCu3t3qONEPuOudu4HSVEtA8YJSL6FAZUxvoOoD792VIJWl91g87\
    EdqsCWp9krVsdBysQoDaf9lMCLb8BS4EYi4gQd5kBQBYLlgD71PYwvf+TbMD9J9/5OMD42oxSR\
    j8H+vRf78/l2Xla33LWz4nOgsddBlbvabICRs8GHt5C9PK5keFtzyi3lsyVKNlfduK3iphsZqs\
    4MLv4zyGJnvZo/+QzShyk5xnMSQX/f98+aEoNflEApCdEOXipipgeiNWnpFSHbcwmMkZoJhURN\
    u+JEz3xCh1mrXeYoN5o+trLL3IXJacSsLYXDrYTipZZbJFRPAucgbnjYBC+/ZzJOfkwCs+Gkw7\
    EoZR7ZQgJ8ma+9586n4tT4cI8DEhBSZsWMjrCt8dxKg==";

fn build_prefilled_messages() -> Vec<Message> {
    let user_msg = Message::User(UserMessage {
        role: "user".into(),
        content: UserContent::Text("Use the echo tool to echo 'hello'".into()),
        timestamp: 0,
    });

    let assistant_msg = Message::Assistant(ai::types::AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::ToolCall {
            id: FAILING_TOOL_CALL_ID.into(),
            name: "echo".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("message".into(), serde_json::Value::String("hello".into()));
                m
            },
            thought_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "github-copilot".into(),
        model: "gpt-5.2-codex".into(),
        usage: ai::types::Usage {
            input: 100, output: 50, cache_read: 0, cache_write: 0, total_tokens: 150,
            cost: ai::types::Cost::default(),
        },
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp: 0,
    });

    let tool_result = Message::ToolResult(ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: FAILING_TOOL_CALL_ID.into(),
        tool_name: "echo".into(),
        content: vec![UserBlock::Text { text: "hello".into() }],
        details: None,
        is_error: false,
        timestamp: 0,
    });

    let follow_up = Message::User(UserMessage {
        role: "user".into(),
        content: UserContent::Text("Say hi".into()),
        timestamp: 0,
    });

    vec![user_msg, assistant_msg, tool_result, follow_up]
}

// ---------------------------------------------------------------------------
// Unit-testable: prefilled context (no live LLM needed for structure checks)
// ---------------------------------------------------------------------------

#[test]
fn failing_tool_call_id_contains_pipe() {
    assert!(FAILING_TOOL_CALL_ID.contains('|'), "Test ID must be pipe-separated");
}

#[test]
fn failing_tool_call_id_is_long() {
    assert!(FAILING_TOOL_CALL_ID.len() > 100, "Test ID must be long enough to trigger the bug");
}

#[test]
fn prefilled_messages_build_correctly() {
    let msgs = build_prefilled_messages();
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0].role(), "user");
    assert_eq!(msgs[1].role(), "assistant");
    assert_eq!(msgs[2].role(), "toolResult");
    assert_eq!(msgs[3].role(), "user");
}

// ---------------------------------------------------------------------------
// Live tests — require credentials
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires OpenRouter API key"]
async fn openrouter_handles_prefilled_long_pipe_id() {
    let model = get_model("openrouter", "openai/gpt-5.2-codex").unwrap();
    let messages = build_prefilled_messages();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("OPENROUTER_API_KEY");

    let response = complete_simple(
        &model,
        &Context { system_prompt: Some("You are a helpful assistant.".into()), messages, tools: None },
        Some(&opts),
    ).await.unwrap();

    assert_ne!(response.stop_reason, StopReason::Error, "Should not fail with call_id too long");
    assert!(response.error_message.is_none());
}

#[tokio::test]
#[ignore = "requires OpenAI Codex OAuth token"]
async fn openai_codex_handles_prefilled_long_pipe_id() {
    let model = get_model("openai-codex", "gpt-5.2-codex").unwrap();
    let messages = build_prefilled_messages();
    let mut opts = SimpleStreamOptions::default();
    // API key resolved from OAuth storage

    let response = complete_simple(
        &model,
        &Context { system_prompt: Some("You are a helpful assistant.".into()), messages, tools: None },
        Some(&opts),
    ).await.unwrap();

    assert_ne!(response.stop_reason, StopReason::Error);
}

#[tokio::test]
#[ignore = "requires GitHub Copilot + OpenRouter OAuth tokens"]
async fn github_copilot_to_openrouter_normalizes_pipe_id() {
    // Step 1: generate tool call with github-copilot
    // Step 2: complete with openrouter — should not fail
    todo!("cross-provider live handoff test")
}
