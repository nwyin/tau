use coding_agent::config::load_config_from;
use tempfile::TempDir;

#[test]
fn config_missing_file_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.toml");
    let config = load_config_from(&path);
    assert_eq!(config.model, "gpt-5.4");
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
    assert_eq!(config.model, "gpt-5.4");
    assert_eq!(config.edit_mode, "replace");
}

#[test]
fn config_empty_file_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let config = load_config_from(&path);
    assert_eq!(config.model, "gpt-5.4");
    assert_eq!(config.edit_mode, "replace");
}

#[test]
fn config_max_turns_from_toml() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "max_turns = 50\n").unwrap();
    let config = load_config_from(&path);
    assert_eq!(config.max_turns, Some(50));
}

#[test]
fn config_max_turns_defaults_to_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.toml");
    let config = load_config_from(&path);
    assert!(config.max_turns.is_none());
}

#[test]
fn config_permissions_from_toml() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[permissions]
bash = "allow"
file_read = "deny"
file_edit = "ask"
"#,
    )
    .unwrap();
    let config = load_config_from(&path);
    let perms = config.permissions.unwrap();
    assert_eq!(perms.get("bash").unwrap(), "allow");
    assert_eq!(perms.get("file_read").unwrap(), "deny");
    assert_eq!(perms.get("file_edit").unwrap(), "ask");
}

#[test]
fn config_permissions_defaults_to_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"gpt-4o\"\n").unwrap();
    let config = load_config_from(&path);
    assert!(config.permissions.is_none());
}
