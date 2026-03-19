use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TauConfig {
    pub model: String,
    pub edit_mode: String, // "replace" | "hashline"
    pub max_turns: Option<u32>,
}

impl Default for TauConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            edit_mode: "replace".to_string(),
            max_turns: None,
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
