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

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY"]
async fn google_total_tokens() {
    let model = get_model("google", "gemini-2.5-flash").unwrap();
    test_total_tokens(&model, env_key("GEMINI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires XAI_API_KEY"]
async fn xai_total_tokens() {
    let model = get_model("xai", "grok-3-fast").unwrap();
    test_total_tokens(&model, env_key("XAI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires MISTRAL_API_KEY"]
async fn mistral_total_tokens() {
    let model = get_model("mistral", "devstral-medium-latest").unwrap();
    test_total_tokens(&model, env_key("MISTRAL_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires AWS credentials"]
async fn bedrock_total_tokens() {
    let model = get_model("amazon-bedrock", "global.anthropic.claude-sonnet-4-5-20250929-v1:0").unwrap();
    test_total_tokens(&model, None).await;
}

#[tokio::test]
#[ignore = "requires GitHub Copilot OAuth token"]
async fn github_copilot_total_tokens() {
    let model = get_model("github-copilot", "gpt-4o").unwrap();
    test_total_tokens(&model, None).await;
}

#[tokio::test]
#[ignore = "requires Gemini CLI OAuth token"]
async fn google_gemini_cli_total_tokens() {
    let model = get_model("google-gemini-cli", "gemini-2.5-flash").unwrap();
    test_total_tokens(&model, None).await;
}
