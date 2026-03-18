//! Mirrors: packages/ai/test/total-tokens.test.ts
//! Verifies that totalTokens = input + output + cacheRead + cacheWrite.

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::complete_simple;
use ai::types::{Context, Message, SimpleStreamOptions, UserContent, UserMessage};

async fn test_total_tokens(model: &ai::types::Model, api_key: Option<String>) {
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = api_key;

    let context = Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("What is 2+2?".into()),
            timestamp: 0,
        })],
        tools: None,
    };

    let response = complete_simple(model, &context, Some(&opts)).await.unwrap();
    let u = &response.usage;
    assert_eq!(
        u.total_tokens,
        u.input + u.output + u.cache_read + u.cache_write,
        "totalTokens must equal input + output + cacheRead + cacheWrite"
    );
}

// ---------------------------------------------------------------------------
// Per-provider
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_total_tokens() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_total_tokens(&model, env_key("ANTHROPIC_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_completions_total_tokens() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    test_total_tokens(&model, env_key("OPENAI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_total_tokens() {
    let model = get_model("openai", "gpt-5-mini").unwrap();
    test_total_tokens(&model, env_key("OPENAI_API_KEY")).await;
}
