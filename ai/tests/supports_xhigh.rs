//! Mirrors: packages/ai/test/supports-xhigh.test.ts

mod common;

use ai::models::{get_model, supports_xhigh};

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
fn returns_false_for_openrouter_opus_4_6_openai_completions_api() {
    // OpenRouter uses openai-completions API, not anthropic-messages, so xhigh is not supported.
    let model = get_model("openrouter", "anthropic/claude-opus-4.6");
    assert!(model.is_some(), "anthropic/claude-opus-4.6 on openrouter should be registered");
    assert!(!supports_xhigh(model.as_deref().unwrap()));
}
