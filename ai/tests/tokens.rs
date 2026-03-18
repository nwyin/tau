//! Mirrors: packages/ai/test/tokens.test.ts
//! Token statistics when stream is aborted mid-response.

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::stream;
use ai::types::{Context, Message, StopReason, StreamOptions, UserContent, UserMessage};
use futures::StreamExt;

async fn test_tokens_on_abort(model: &ai::types::Model, api_key: Option<String>) {
    let mut opts = StreamOptions::default();
    opts.api_key = api_key;

    let context = Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text(
                "Write a long poem with 20 stanzas about the beauty of nature.".into(),
            ),
            timestamp: 0,
        })],
        tools: None,
    };

    let mut response = stream(model, &context, Some(&opts)).unwrap();
    let mut text = String::new();
    let mut aborted = false;

    while let Some(event) = response.next().await {
        if aborted {
            break;
        }
        if let ai::types::AssistantMessageEvent::TextDelta { delta, .. }
        | ai::types::AssistantMessageEvent::ThinkingDelta { delta: delta, .. } = &event
        {
            text.push_str(delta);
            if text.len() >= 1000 {
                aborted = true;
                // TODO: actual abort signal threading
            }
        }
    }

    let msg = response.result().await;
    assert_eq!(msg.stop_reason, StopReason::Aborted);

    // Providers that send usage early (Anthropic, Google non-CLI) should have non-zero counts.
    // OpenAI-family and Gemini CLI only emit usage in final chunk so they'll have zeros on abort.
    // We just assert the invariant: totalTokens == input + output + cacheRead + cacheWrite.
    let u = &msg.usage;
    assert_eq!(
        u.total_tokens,
        u.input + u.output + u.cache_read + u.cache_write
    );
}

// ---------------------------------------------------------------------------
// Per-provider
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_completions_tokens_on_abort() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    test_tokens_on_abort(&model, env_key("OPENAI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_tokens_on_abort() {
    let model = get_model("openai", "gpt-5-mini").unwrap();
    test_tokens_on_abort(&model, env_key("OPENAI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_tokens_on_abort() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_tokens_on_abort(&model, env_key("ANTHROPIC_API_KEY")).await;
}
