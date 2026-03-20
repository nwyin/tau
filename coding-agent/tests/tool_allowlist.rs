use coding_agent::tools::{tools_for_edit_mode, tools_from_allowlist};

// INV-1: tools_from_allowlist with valid names returns exactly those tools, in order
#[test]
fn test_allowlist_valid_names_returns_in_order() {
    let names: Vec<String> = vec![
        "bash".to_string(),
        "file_read".to_string(),
        "glob".to_string(),
    ];
    let tools = tools_from_allowlist(&names, "replace");
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
    let tools = tools_from_allowlist(&names, "replace");
    // Only bash and glob should be returned; nonexistent_tool is silently dropped
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name(), "bash");
    assert_eq!(tools[1].name(), "glob");
}

// INV-3: Edit mode substitution — "file_read" in hashline mode resolves to HashFileReadTool
#[test]
fn test_allowlist_hashline_edit_mode_substitution() {
    let names: Vec<String> = vec!["file_read".to_string(), "file_edit".to_string()];

    let replace_tools = tools_from_allowlist(&names, "replace");
    assert_eq!(replace_tools.len(), 2);
    assert_eq!(replace_tools[0].name(), "file_read");
    assert_eq!(replace_tools[1].name(), "file_edit");

    let hashline_tools = tools_from_allowlist(&names, "hashline");
    assert_eq!(hashline_tools.len(), 2);
    // Canonical name is always "file_read"/"file_edit" from the registry key perspective,
    // but the underlying impl differs — hash tools report different names
    assert_eq!(hashline_tools[0].name(), "hash_file_read");
    assert_eq!(hashline_tools[1].name(), "hash_file_edit");
}

// INV-4: Empty allowlist returns empty vec
#[test]
fn test_allowlist_empty_returns_empty() {
    let tools = tools_from_allowlist(&[], "replace");
    assert!(tools.is_empty());
}

// INV-5: Default path (no allowlist) returns same tools as tools_for_edit_mode
#[test]
fn test_default_path_matches_tools_for_edit_mode() {
    for mode in &["replace", "hashline"] {
        let default = tools_for_edit_mode(mode);
        // Collect the full allowlist from tools_for_edit_mode names
        let names: Vec<String> = default
            .iter()
            .map(|t| {
                // Map hash tool names back to canonical names for registry lookup
                match t.name() {
                    "hash_file_read" => "file_read".to_string(),
                    "hash_file_edit" => "file_edit".to_string(),
                    other => other.to_string(),
                }
            })
            .collect();
        let from_allowlist = tools_from_allowlist(&names, mode);
        assert_eq!(
            default.len(),
            from_allowlist.len(),
            "tool count mismatch for mode '{}'",
            mode
        );
        for (d, a) in default.iter().zip(from_allowlist.iter()) {
            assert_eq!(d.name(), a.name(), "tool name mismatch for mode '{}'", mode);
        }
    }
}

// Critical path: allowlist with all valid names returns correct tool count and names
#[test]
fn test_allowlist_all_valid_names_replace_mode() {
    let names: Vec<String> = vec![
        "bash".to_string(),
        "file_read".to_string(),
        "file_edit".to_string(),
        "file_write".to_string(),
        "glob".to_string(),
        "grep".to_string(),
    ];
    let tools = tools_from_allowlist(&names, "replace");
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
    let tools = tools_from_allowlist(&names, "replace");
    assert_eq!(tools.len(), 3);
    assert_eq!(tools[0].name(), "bash");
    assert_eq!(tools[1].name(), "grep");
    assert_eq!(tools[2].name(), "file_write");
}
