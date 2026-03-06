//! Built-in model catalog — ported from models.generated.ts
//!
//! Only the models needed for unit tests are included here.
//! Full catalog can be added incrementally.

use crate::types::{known_api, Model, ModelCost};

pub fn builtin_models() -> Vec<Model> {
    vec![
        // -----------------------------------------------------------------------
        // anthropic
        // -----------------------------------------------------------------------
        Model {
            id: "claude-opus-4-6".into(),
            name: "Claude Opus 4.6".into(),
            api: known_api::ANTHROPIC_MESSAGES.into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            reasoning: true,
            input: vec!["text".into(), "image".into()],
            cost: ModelCost {
                input: 5.0,
                output: 25.0,
                cache_read: 0.5,
                cache_write: 6.25,
            },
            context_window: 200_000,
            max_tokens: 128_000,
            headers: None,
            compat: None,
        },
        Model {
            id: "claude-sonnet-4-5".into(),
            name: "Claude Sonnet 4.5 (latest)".into(),
            api: known_api::ANTHROPIC_MESSAGES.into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            reasoning: true,
            input: vec!["text".into(), "image".into()],
            cost: ModelCost {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
            context_window: 200_000,
            max_tokens: 64_000,
            headers: None,
            compat: None,
        },
        // -----------------------------------------------------------------------
        // openrouter
        // -----------------------------------------------------------------------
        Model {
            id: "anthropic/claude-opus-4.6".into(),
            name: "Anthropic: Claude Opus 4.6".into(),
            api: known_api::OPENAI_COMPLETIONS.into(),
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            reasoning: true,
            input: vec!["text".into(), "image".into()],
            cost: ModelCost {
                input: 5.0,
                output: 25.0,
                cache_read: 0.5,
                cache_write: 6.25,
            },
            context_window: 1_000_000,
            max_tokens: 128_000,
            headers: None,
            compat: None,
        },
    ]
}
