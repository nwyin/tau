use coding_agent::config::load_config_from;
use tempfile::TempDir;

#[test]
fn config_invalid_toml_returns_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "this is not valid toml {{{{").unwrap();
    let config = load_config_from(&path);
    // Just verify it doesn't panic and returns a valid config
    assert!(!config.model.is_empty());
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
