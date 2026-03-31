/// Extract a human-readable detail string from tool arguments.
pub fn extract_tool_detail(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| {
                let first_line = s.lines().next().unwrap_or(s);
                if first_line.len() > 80 {
                    format!("{}...", &first_line[..77])
                } else {
                    first_line.to_string()
                }
            })
            .unwrap_or_default(),
        "file_read" | "file_write" | "file_edit" => args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "glob" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "grep" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "web_fetch" => args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "web_search" => args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "thread" => {
            let alias = args
                .get("alias")
                .and_then(|v| v.as_str())
                .unwrap_or("thread");
            let task = args
                .get("task")
                .and_then(|v| v.as_str())
                .map(|s| {
                    let first = s.lines().next().unwrap_or(s);
                    if first.len() > 60 {
                        format!("{}...", &first[..57])
                    } else {
                        first.to_string()
                    }
                })
                .unwrap_or_default();
            format!("{}: {}", alias, task)
        }
        _ => String::new(),
    }
}
