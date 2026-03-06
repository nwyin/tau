//! Mirrors: packages/ai/test/supports-xhigh.test.ts

mod common;

use ai::models::{get_model, supports_xhigh};
#[allow(unused_imports)]
use ai::types;

#[test]
fn returns_true_for_anthropic_opus_4_6() {
    let model = get_model("anthropic", "claude-opus-4-6");
    assert!(model.is_some(), "claude-opus-4-6 should be registered");
    assert!(supports_xhigh(model.as_deref().unwrap()));
}

#[test]
fn returns_false_for_non_opus_anthropic_models() {
    let model = get_model("anthropic", "claude-sonnet-4-5");
    assert!(model.is_some(), "claude-sonnet-4-5 should be registered");
    assert!(!supports_xhigh(model.as_deref().unwrap()));
}

#[test]
fn returns_false_for_opus_4_6_on_non_anthropic_api() {
    // Opus 4.6 on a non-anthropic-messages API should not support xhigh.
    let model = ai::types::Model {
        id: "claude-opus-4-6".into(),
        name: "Claude Opus 4.6".into(),
        api: ai::types::known_api::OPENAI_RESPONSES.into(),
        provider: "openai".into(),
        base_url: "https://api.openai.com/v1".into(),
        reasoning: true,
        input: vec!["text".into(), "image".into()],
        cost: ai::types::ModelCost { input: 5.0, output: 25.0, cache_read: 0.5, cache_write: 6.25 },
        context_window: 200_000,
        max_tokens: 128_000,
        headers: None,
        compat: None,
    };
    assert!(!supports_xhigh(&model));
}
