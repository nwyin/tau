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

// ---------------------------------------------------------------------------
// Google
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY"]
async fn google_gemini_2_5_flash_basic_text_generation() {
    let model = ai::models::get_model("google", "gemini-2.5-flash").unwrap();
    basic_text_generation(&model, env_key("GEMINI_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// xAI
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires XAI_API_KEY"]
async fn xai_grok_3_fast_basic_text_generation() {
    let model = ai::models::get_model("xai", "grok-3-fast").unwrap();
    basic_text_generation(&model, env_key("XAI_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// Mistral
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires MISTRAL_API_KEY"]
async fn mistral_devstral_basic_text_generation() {
    let model = ai::models::get_model("mistral", "devstral-medium-latest").unwrap();
    basic_text_generation(&model, env_key("MISTRAL_API_KEY")).await;
}

// ---------------------------------------------------------------------------
// Amazon Bedrock
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires AWS credentials"]
async fn bedrock_claude_sonnet_4_5_basic_text_generation() {
    let model = ai::models::get_model("amazon-bedrock", "global.anthropic.claude-sonnet-4-5-20250929-v1:0").unwrap();
    basic_text_generation(&model, None).await;
}

// ---------------------------------------------------------------------------
// GitHub Copilot (OAuth)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires GitHub Copilot OAuth token"]
async fn github_copilot_gpt_4o_basic_text_generation() {
    let model = ai::models::get_model("github-copilot", "gpt-4o").unwrap();
    basic_text_generation(&model, None /* OAuth token from ~/.pi/agent/oauth.json */).await;
}

// ---------------------------------------------------------------------------
// Google Gemini CLI (OAuth)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires Gemini CLI OAuth token"]
async fn google_gemini_cli_basic_text_generation() {
    let model = ai::models::get_model("google-gemini-cli", "gemini-2.5-flash").unwrap();
    basic_text_generation(&model, None).await;
}
