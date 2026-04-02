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
            let episodes = args
                .get("episodes")
                .and_then(|v| v.as_array())
                .filter(|a| !a.is_empty())
                .map(|a| {
                    let names: Vec<&str> = a.iter().filter_map(|v| v.as_str()).collect();
                    format!(" [episodes: {}]", names.join(", "))
                })
                .unwrap_or_default();
            format!("{}: {}{}", alias, task, episodes)
        }
        "query" => {
            let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            let alias = args
                .get("alias")
                .and_then(|v| v.as_str())
                .map(|a| format!("[{}] ", a))
                .unwrap_or_default();
            let display = if prompt.len() > 70 {
                format!("{}...", &prompt[..67])
            } else {
                prompt.to_string()
            };
            format!("{}{}", alias, display)
        }
        "document" => {
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let size = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| format!(" ({} chars)", c.len()))
                .unwrap_or_default();
            format!("{} {}{}", op, name, size)
        }
        "log" => args
            .get("message")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() > 80 {
                    format!("{}...", &s[..77])
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        "from_id" => args
            .get("alias")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "todo" => {
            let todos = args.get("todos").and_then(|v| v.as_array());
            match todos {
                Some(arr) => {
                    let done = arr
                        .iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed"))
                        .count();
                    format!("[{}/{}]", done, arr.len())
                }
                None => String::new(),
            }
        }
        "complete" => args
            .get("result")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() > 80 {
                    format!("{}...", &s[..77])
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        "abort" => args
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() > 80 {
                    format!("{}...", &s[..77])
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        "escalate" => args
            .get("problem")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() > 80 {
                    format!("{}...", &s[..77])
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        "py_repl" => {
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let line_count = code.lines().count();
            format!("{} lines", line_count)
        }
        _ => String::new(),
    }
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
