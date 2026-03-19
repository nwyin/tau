use coding_agent::config::load_config_from;
use tempfile::TempDir;

#[test]
fn config_missing_file_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.toml");
    let config = load_config_from(&path);
    assert_eq!(config.model, "gpt-4o-mini");
    assert_eq!(config.edit_mode, "replace");
}

#[test]
fn config_partial_toml_fills_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"gpt-4o\"\n").unwrap();
    let config = load_config_from(&path);
    assert_eq!(config.model, "gpt-4o");
    assert_eq!(config.edit_mode, "replace"); // default
}

#[test]
fn config_full_toml() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "model = \"claude-sonnet\"\nedit_mode = \"hashline\"\n",
    )
    .unwrap();
    let config = load_config_from(&path);
    assert_eq!(config.model, "claude-sonnet");
    assert_eq!(config.edit_mode, "hashline");
}

#[test]
fn config_invalid_toml_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "this is not valid toml {{{{").unwrap();
    let config = load_config_from(&path);
    assert_eq!(config.model, "gpt-4o-mini");
    assert_eq!(config.edit_mode, "replace");
}

#[test]
fn config_empty_file_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let config = load_config_from(&path);
    assert_eq!(config.model, "gpt-4o-mini");
    assert_eq!(config.edit_mode, "replace");
}
