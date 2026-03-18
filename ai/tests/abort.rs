//! Mirrors: packages/ai/test/abort.test.ts
//! All tests are #[ignore] — require live API credentials.

mod common;
use common::env_key;

use ai::models::get_model;
use ai::providers::{complete, stream};
use ai::types::{
    Context, Message, SimpleStreamOptions, StopReason, StreamOptions, UserContent, UserMessage,
};
use futures::StreamExt;

async fn test_abort_signal(model: &ai::types::Model, api_key: Option<String>) {
    let context = Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text(
                "What is 15 + 27? Think step by step. Then list 50 first names.".into(),
            ),
            timestamp: 0,
        })],
        tools: None,
    };

    let mut opts = StreamOptions::default();
    opts.api_key = api_key.clone();

    let cancel = tokio_util::sync::CancellationToken::new();
    // TODO: thread cancel token into stream() call
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
        }
        if text.len() >= 50 {
            cancel.cancel();
            aborted = true;
        }
    }

    let msg = response.result().await;
    assert_eq!(msg.stop_reason, StopReason::Aborted);
    assert!(!msg.content.is_empty());

    // Follow-up after abort
    let mut follow_up_ctx = context;
    follow_up_ctx.messages.push(Message::Assistant(msg));
    follow_up_ctx.messages.push(Message::User(UserMessage {
        role: "user".into(),
        content: UserContent::Text("Please continue, but only generate 5 names.".into()),
        timestamp: 0,
    }));

    let mut opts2 = StreamOptions::default();
    opts2.api_key = api_key;
    let follow_up = complete(model, &follow_up_ctx, Some(&opts2)).await.unwrap();
    assert_eq!(follow_up.stop_reason, StopReason::Stop);
    assert!(!follow_up.content.is_empty());
}

// ---------------------------------------------------------------------------
// Per-provider tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_abort_signal() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    test_abort_signal(&model, env_key("ANTHROPIC_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_completions_abort_signal() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    test_abort_signal(&model, env_key("OPENAI_API_KEY")).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_abort_signal() {
    let model = get_model("openai", "gpt-5-mini").unwrap();
    test_abort_signal(&model, env_key("OPENAI_API_KEY")).await;
}
