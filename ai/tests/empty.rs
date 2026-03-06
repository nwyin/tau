//! Mirrors: packages/ai/test/empty.test.ts

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::complete;
use ai::types::{Context, Message, StopReason, StreamOptions, UserContent, UserMessage};

async fn test_empty_message(model: &ai::types::Model, api_key: Option<String>) {
    let mut opts = StreamOptions::default();
    opts.api_key = api_key;

    let context = Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Blocks(vec![]), // completely empty content array
            timestamp: 0,
        })],
        tools: None,
    };

    let response = complete(model, &context, Some(&opts)).await.unwrap();
    assert_eq!(response.role, "assistant");
    if response.stop_reason == StopReason::Error {
        assert!(response.error_message.is_some());
    } else {
        assert!(!response.content.is_empty());
    }
}

async fn test_empty_string_message(model: &ai::types::Model, api_key: Option<String>) {
    let mut opts = StreamOptions::default();
    opts.api_key = api_key;

    let context = Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text(String::new()),
            timestamp: 0,
        })],
        tools: None,
    };

    let response = complete(model, &context, Some(&opts)).await.unwrap();
    assert_eq!(response.role, "assistant");
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_empty_content_array() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_empty_message(&model, env_key("ANTHROPIC_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_empty_string_content() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_empty_string_message(&model, env_key("ANTHROPIC_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// OpenAI
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_completions_empty_content_array() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    test_empty_message(&model, env_key("OPENAI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_empty_content_array() {
    let model = get_model("openai", "gpt-5-mini").unwrap();
    test_empty_message(&model, env_key("OPENAI_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// Google
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY"]
async fn google_gemini_2_5_flash_empty_content_array() {
    let model = get_model("google", "gemini-2.5-flash").unwrap();
    test_empty_message(&model, env_key("GEMINI_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// Bedrock
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires AWS credentials"]
async fn bedrock_empty_content_array() {
    let model = get_model("amazon-bedrock", "global.anthropic.claude-sonnet-4-5-20250929-v1:0").unwrap();
    test_empty_message(&model, None).await;
}
