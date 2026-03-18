pub mod anthropic;
mod kimi;
pub mod openai_responses;
pub mod openai_responses_shared;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::stream::AssistantMessageEventStream;
use crate::types::{Context, Model, SimpleStreamOptions, StreamOptions};

// ---------------------------------------------------------------------------
// ApiProvider trait
// ---------------------------------------------------------------------------

pub trait ApiProvider: Send + Sync {
    fn api(&self) -> &str;

    /// Raw stream — provider-specific options passed through.
    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: Option<&StreamOptions>,
    ) -> AssistantMessageEventStream;

    /// Simple stream — handles reasoning/thinking abstraction.
    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<&SimpleStreamOptions>,
    ) -> AssistantMessageEventStream;
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Registry {
    providers: HashMap<String, Arc<dyn ApiProvider>>,
}

static REGISTRY: std::sync::OnceLock<RwLock<Registry>> = std::sync::OnceLock::new();

fn registry() -> &'static RwLock<Registry> {
    REGISTRY.get_or_init(|| RwLock::new(Registry::default()))
}

pub fn register_api_provider(provider: Arc<dyn ApiProvider>) {
    let mut reg = registry().write().unwrap();
    reg.providers.insert(provider.api().to_string(), provider);
}

pub fn get_api_provider(api: &str) -> Option<Arc<dyn ApiProvider>> {
    let reg = registry().read().unwrap();
    reg.providers.get(api).cloned()
}

pub fn unregister_api_provider(api: &str) {
    let mut reg = registry().write().unwrap();
    reg.providers.remove(api);
}

pub fn clear_api_providers() {
    let mut reg = registry().write().unwrap();
    reg.providers.clear();
}

// ---------------------------------------------------------------------------
// Built-in provider registration
// ---------------------------------------------------------------------------

/// Register all built-in providers. Call once at startup.
pub fn register_builtin_providers() {
    register_api_provider(Arc::new(openai_responses::OpenAIResponsesProvider::new()));
    register_api_provider(Arc::new(anthropic::AnthropicProvider::new()));
}

// ---------------------------------------------------------------------------
// Top-level stream / complete helpers (mirrors pi-ai's stream.ts)
// ---------------------------------------------------------------------------

use anyhow::{anyhow, Result};

pub fn stream(
    model: &Model,
    context: &Context,
    options: Option<&StreamOptions>,
) -> Result<AssistantMessageEventStream> {
    let provider = get_api_provider(&model.api)
        .ok_or_else(|| anyhow!("No API provider registered for api: {}", model.api))?;
    Ok(provider.stream(model, context, options))
}

pub fn stream_simple(
    model: &Model,
    context: &Context,
    options: Option<&SimpleStreamOptions>,
) -> Result<AssistantMessageEventStream> {
    let provider = get_api_provider(&model.api)
        .ok_or_else(|| anyhow!("No API provider registered for api: {}", model.api))?;
    Ok(provider.stream_simple(model, context, options))
}

pub async fn complete(
    model: &Model,
    context: &Context,
    options: Option<&StreamOptions>,
) -> Result<crate::types::AssistantMessage> {
    Ok(stream(model, context, options)?.result().await)
}

pub async fn complete_simple(
    model: &Model,
    context: &Context,
    options: Option<&SimpleStreamOptions>,
) -> Result<crate::types::AssistantMessage> {
    Ok(stream_simple(model, context, options)?.result().await)
}
