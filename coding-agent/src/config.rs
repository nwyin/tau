use serde::Deserialize;
use std::path::Path;

/// Edit mode controls the behavior of `file_read` and `file_edit` tools.
///
/// Both modes present the same tool names to the model (`file_read`, `file_edit`).
/// The mode determines the parameter schema, description, and execution behavior.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EditMode {
    /// Search-and-replace: `{path, old_string, new_string}`
    #[default]
    Replace,
    /// Hash-anchored edits: `{path, edits: [{op, pos, end, lines}]}`
    Hashline,
}

impl EditMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "hashline" => EditMode::Hashline,
            _ => EditMode::Replace,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TauConfig {
    pub model: String,
    pub edit_mode: String, // "replace" | "hashline"
    pub max_turns: Option<u32>,
    pub tools: Option<Vec<String>>,
    pub skills: Option<bool>, // default: true
}

impl Default for TauConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            edit_mode: "replace".to_string(),
            max_turns: None,
            tools: None,
            skills: None,
        }
    }
}

impl TauConfig {
    pub fn edit_mode_enum(&self) -> EditMode {
        EditMode::parse(&self.edit_mode)
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
