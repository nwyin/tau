use coding_agent::tools::{default_tools, tools_from_allowlist};

// INV-1: tools_from_allowlist with valid names returns exactly those tools, in order
#[test]
fn test_allowlist_valid_names_returns_in_order() {
    let names: Vec<String> = vec![
        "bash".to_string(),
        "file_read".to_string(),
        "glob".to_string(),
    ];
    let tools = tools_from_allowlist(&names);
    assert_eq!(tools.len(), 3);
    assert_eq!(tools[0].name(), "bash");
    assert_eq!(tools[1].name(), "file_read");
    assert_eq!(tools[2].name(), "glob");
}

// INV-2: Unknown tool names produce a warning and are omitted (not an error)
#[test]
fn test_allowlist_unknown_names_omitted() {
    let names: Vec<String> = vec![
        "bash".to_string(),
        "nonexistent_tool".to_string(),
        "glob".to_string(),
    ];
    let tools = tools_from_allowlist(&names);
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name(), "bash");
    assert_eq!(tools[1].name(), "glob");
}

// INV-3: Default path returns same tools as allowlist with all names
#[test]
fn test_default_path_matches_allowlist() {
    let default = default_tools();
    let names: Vec<String> = default.iter().map(|t| t.name().to_string()).collect();
    let from_allowlist = tools_from_allowlist(&names);
    assert_eq!(default.len(), from_allowlist.len());
    for (d, a) in default.iter().zip(from_allowlist.iter()) {
        assert_eq!(d.name(), a.name());
    }
}

// Critical path: allowlist with all valid names returns correct tool count and names
#[test]
fn test_allowlist_all_valid_names() {
    let names: Vec<String> = vec![
        "bash".to_string(),
        "file_read".to_string(),
        "file_edit".to_string(),
        "file_write".to_string(),
        "glob".to_string(),
        "grep".to_string(),
    ];
    let tools = tools_from_allowlist(&names);
    assert_eq!(tools.len(), 6);
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(tool_names.contains(&"bash"));
    assert!(tool_names.contains(&"file_read"));
    assert!(tool_names.contains(&"file_edit"));
    assert!(tool_names.contains(&"file_write"));
    assert!(tool_names.contains(&"glob"));
    assert!(tool_names.contains(&"grep"));
}

// Critical path: allowlist with mix of valid and invalid names returns only valid ones
#[test]
fn test_allowlist_mixed_valid_invalid() {
    let names: Vec<String> = vec![
        "bash".to_string(),
        "unknown_tool_x".to_string(),
        "grep".to_string(),
        "also_unknown".to_string(),
        "file_write".to_string(),
    ];
    let tools = tools_from_allowlist(&names);
    assert_eq!(tools.len(), 3);
    assert_eq!(tools[0].name(), "bash");
    assert_eq!(tools[1].name(), "grep");
    assert_eq!(tools[2].name(), "file_write");
}
