//! Shared test helpers.
#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};

pub fn env_key(var: &str) -> Option<String> {
    std::env::var(var).ok()
}

/// Build a minimal Model for use in tests that construct one manually.
pub fn mock_model(api: &str, provider: &str) -> ai::types::Model {
    ai::types::Model {
        id: "mock".into(),
        name: "mock".into(),
        api: api.into(),
        provider: provider.into(),
        base_url: "https://example.invalid".into(),
        reasoning: false,
        input: vec!["text".into()],
        cost: ai::types::ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        headers: None,
    }
}

pub fn create_usage() -> ai::types::Usage {
    ai::types::Usage::default()
}

pub fn create_assistant_message(text: &str) -> ai::types::AssistantMessage {
    ai::types::AssistantMessage {
        role: "assistant".into(),
        content: vec![ai::types::ContentBlock::Text {
            text: text.into(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "mock".into(),
        usage: create_usage(),
        stop_reason: ai::types::StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

pub fn registry_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}
