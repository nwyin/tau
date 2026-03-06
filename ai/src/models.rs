use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::types::{Model, Usage};

// ---------------------------------------------------------------------------
// Model registry
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ModelRegistry {
    // provider -> model_id -> Model
    models: HashMap<String, HashMap<String, Arc<Model>>>,
}

static REGISTRY: std::sync::OnceLock<RwLock<ModelRegistry>> = std::sync::OnceLock::new();

fn registry() -> &'static RwLock<ModelRegistry> {
    REGISTRY.get_or_init(|| RwLock::new(ModelRegistry::default()))
}

pub fn register_model(model: Model) {
    let mut reg = registry().write().unwrap();
    reg.models
        .entry(model.provider.clone())
        .or_default()
        .insert(model.id.clone(), Arc::new(model));
}

pub fn get_model(provider: &str, model_id: &str) -> Option<Arc<Model>> {
    let reg = registry().read().unwrap();
    reg.models.get(provider)?.get(model_id).cloned()
}

pub fn get_providers() -> Vec<String> {
    let reg = registry().read().unwrap();
    reg.models.keys().cloned().collect()
}

pub fn get_models(provider: &str) -> Vec<Arc<Model>> {
    let reg = registry().read().unwrap();
    reg.models
        .get(provider)
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Cost calculation
// ---------------------------------------------------------------------------

pub fn calculate_cost(model: &Model, usage: &mut Usage) {
    usage.cost.input = (model.cost.input / 1_000_000.0) * usage.input as f64;
    usage.cost.output = (model.cost.output / 1_000_000.0) * usage.output as f64;
    usage.cost.cache_read = (model.cost.cache_read / 1_000_000.0) * usage.cache_read as f64;
    usage.cost.cache_write = (model.cost.cache_write / 1_000_000.0) * usage.cache_write as f64;
    usage.cost.total =
        usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

// ---------------------------------------------------------------------------
// Feature checks
// ---------------------------------------------------------------------------

/// Check if a model supports xhigh thinking level.
pub fn supports_xhigh(model: &Model) -> bool {
    if model.id.contains("gpt-5.2") || model.id.contains("gpt-5.3") {
        return true;
    }
    if model.api == crate::types::known_api::ANTHROPIC_MESSAGES {
        return model.id.contains("opus-4-6") || model.id.contains("opus-4.6");
    }
    false
}

pub fn models_are_equal(a: Option<&Model>, b: Option<&Model>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.id == b.id && a.provider == b.provider,
        _ => false,
    }
}
