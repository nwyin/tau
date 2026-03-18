//! Mirrors: packages/ai/test/cross-provider-handoff.test.ts
//! Verifies conversation history can be handed off between providers.

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::complete_simple;
use ai::types::{Context, Message, SimpleStreamOptions, StopReason, UserContent, UserMessage};

async fn test_cross_provider_handoff(
    model_a: &ai::types::Model,
    model_b: &ai::types::Model,
    key_a: Option<String>,
    key_b: Option<String>,
) {
    // Turn 1 with model A
    let mut opts_a = SimpleStreamOptions::default();
    opts_a.base.api_key = key_a;

    let ctx_a = Context {
        system_prompt: Some("You are a helpful assistant. Be concise.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Say exactly: 'First provider response'".into()),
            timestamp: 0,
        })],
        tools: None,
    };

    let response_a = complete_simple(model_a, &ctx_a, Some(&opts_a))
        .await
        .unwrap();
    assert_ne!(response_a.stop_reason, StopReason::Error);

    // Hand off to model B, including model A's response in history
    let mut opts_b = SimpleStreamOptions::default();
    opts_b.base.api_key = key_b;

    let ctx_b = Context {
        system_prompt: ctx_a.system_prompt.clone(),
        messages: vec![
            ctx_a.messages[0].clone(),
            Message::Assistant(response_a),
            Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Now say: 'Second provider response'".into()),
                timestamp: 0,
            }),
        ],
        tools: None,
    };

    let response_b = complete_simple(model_b, &ctx_b, Some(&opts_b))
        .await
        .unwrap();
    assert_ne!(response_b.stop_reason, StopReason::Error);
}

// ---------------------------------------------------------------------------
// Per-provider pair tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live provider test: requires ANTHROPIC_API_KEY, OPENAI_API_KEY, and RUN_LIVE_PROVIDER_TESTS=1"]
async fn anthropic_to_openai_handoff() {
    if std::env::var("RUN_LIVE_PROVIDER_TESTS").is_err() {
        eprintln!("Skipping: set RUN_LIVE_PROVIDER_TESTS=1 to run live provider tests");
        return;
    }
    let a = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    let b = get_model("openai", "gpt-4o-mini").unwrap();
    test_cross_provider_handoff(
        &a,
        &b,
        env_key("ANTHROPIC_API_KEY"),
        env_key("OPENAI_API_KEY"),
    )
    .await;
}

#[tokio::test]
#[ignore = "live provider test: requires ANTHROPIC_API_KEY, OPENAI_API_KEY, and RUN_LIVE_PROVIDER_TESTS=1"]
async fn openai_to_anthropic_handoff() {
    if std::env::var("RUN_LIVE_PROVIDER_TESTS").is_err() {
        eprintln!("Skipping: set RUN_LIVE_PROVIDER_TESTS=1 to run live provider tests");
        return;
    }
    let a = get_model("openai", "gpt-4o-mini").unwrap();
    let b = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_cross_provider_handoff(
        &a,
        &b,
        env_key("OPENAI_API_KEY"),
        env_key("ANTHROPIC_API_KEY"),
    )
    .await;
}
