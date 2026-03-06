//! Mirrors: packages/ai/test/stream.test.ts
//! All tests are #[ignore] — require live API credentials.

mod common;
use common::env_key;

use ai::providers::complete_simple;
use ai::types::{Context, SimpleStreamOptions, StreamOptions, UserMessage, UserContent};

async fn basic_text_generation(model: &ai::types::Model, api_key: Option<String>) {
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = api_key;

    let context = Context {
        system_prompt: Some("You are a helpful assistant. Be concise.".into()),
        messages: vec![ai::types::Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Reply with exactly: 'Hello test successful'".into()),
            timestamp: 0,
        })],
        tools: None,
    };

    let response = complete_simple(model, &context, Some(&opts)).await.unwrap();
    assert_eq!(response.role, "assistant");
    assert!(!response.content.is_empty());
    assert!(matches!(response.stop_reason, ai::types::StopReason::Stop));
}

async fn handle_tool_call(model: &ai::types::Model, api_key: Option<String>) {
    todo!("tool call test — needs tool registration infrastructure")
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_claude_3_5_haiku_basic_text_generation() {
    let model = ai::models::get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    let key = env_key("ANTHROPIC_API_KEY");
    basic_text_generation(&model, key).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_claude_3_5_haiku_tool_call() {
    let model = ai::models::get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    handle_tool_call(&model, env_key("ANTHROPIC_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// OpenAI (completions)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_completions_gpt_4o_mini_basic_text_generation() {
    let model = ai::models::get_model("openai", "gpt-4o-mini").unwrap();
    basic_text_generation(&model, env_key("OPENAI_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// OpenAI (responses)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_gpt_5_mini_basic_text_generation() {
    let model = ai::models::get_model("openai", "gpt-5-mini").unwrap();
    basic_text_generation(&model, env_key("OPENAI_API_KEY")).await;
}

