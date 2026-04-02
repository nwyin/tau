use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Model slot configuration for role-based model routing.
///
/// Slots allow different models for different roles (e.g., cheap model for
/// search, powerful model for reasoning). All slots default to the main model.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct ModelSlots {
    pub main: Option<String>,
    pub search: Option<String>,
    pub subagent: Option<String>,
    pub reasoning: Option<String>,
}

impl ModelSlots {
    /// Resolve a slot name to a model ID, falling back to the main model.
    pub fn resolve(&self, slot: &str, main_model: &str) -> String {
        let slot_value = match slot {
            "main" => self.main.as_deref(),
            "search" => self.search.as_deref(),
            "subagent" => self.subagent.as_deref(),
            "reasoning" => self.reasoning.as_deref(),
            _ => None,
        };
        slot_value.unwrap_or(main_model).to_string()
    }

    /// Check if a string is a known slot name.
    pub fn is_slot(name: &str) -> bool {
        matches!(name, "main" | "search" | "subagent" | "reasoning")
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TauConfig {
    pub model: String,
    pub max_turns: Option<u32>,
    pub max_api_requests: Option<u32>,
    pub tools: Option<Vec<String>>,
    pub skills: Option<bool>,     // default: true
    pub thinking: Option<String>, // "off"|"minimal"|"low"|"medium"|"high"|"xhigh"
    pub permissions: Option<HashMap<String, String>>,
    pub models: ModelSlots,
}

impl Default for TauConfig {
    fn default() -> Self {
        Self {
            model: "gpt-5.4".to_string(),
            max_turns: None,
            max_api_requests: None,
            tools: None,
            skills: None,
            thinking: Some("high".to_string()),
            permissions: None,
            models: ModelSlots::default(),
        }
    }
}

/// Load config from `~/.tau/config.toml`, falling back to defaults.
pub fn load_config() -> TauConfig {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = std::path::PathBuf::from(home)
        .join(".tau")
        .join("config.toml");
    load_config_from(&path)
}

/// Load config from a specific path, falling back to defaults on any error.
pub fn load_config_from(path: &Path) -> TauConfig {
    match std::fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => TauConfig::default(),
    }
}
