/// Extract a human-readable detail string from tool arguments.
pub fn extract_tool_detail(tool_name: &str, args: &serde_json::Value) -> String {
    crate::tools::summarize_tool_call(tool_name, args)
}

/// Extract an expanded body for tools that benefit from showing full content.
/// Returns None for most tools (body is populated later from the result).
pub fn extract_tool_body(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "py_repl" => args
            .get("code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}
