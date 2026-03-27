//! Episode generation — trace formatting from thread message history.

use ai::types::{ContentBlock, Message};

use crate::thread::{Episode, ThreadId, ThreadOutcome};
use crate::types::AgentMessage;

/// Generate an Episode from a completed thread's message history.
pub fn generate_episode(
    thread_id: ThreadId,
    alias: &str,
    task: &str,
    messages: &[AgentMessage],
    outcome: &ThreadOutcome,
    duration_ms: u64,
) -> Episode {
    let turn_count = count_turns(messages);
    let full_trace = format_full_trace(alias, task, messages, outcome, duration_ms, turn_count);
    let compact_trace = format_compact_trace(alias, task, messages, outcome);
    Episode {
        thread_id,
        alias: alias.to_string(),
        task: task.to_string(),
        outcome: outcome.clone(),
        full_trace,
        compact_trace,
        duration_ms,
        turn_count,
    }
}

/// Count assistant turns in the message history.
fn count_turns(messages: &[AgentMessage]) -> u32 {
    messages.iter().filter(|m| m.role() == "assistant").count() as u32
}

/// Full trace: complete transcript with tool args/results, timing.
/// Used as the tool result returned to the orchestrator.
fn format_full_trace(
    alias: &str,
    task: &str,
    messages: &[AgentMessage],
    outcome: &ThreadOutcome,
    duration_ms: u64,
    turn_count: u32,
) -> String {
    let mut out = String::new();
    let secs = duration_ms as f64 / 1000.0;
    out.push_str(&format!(
        "--- Thread: {} [{}] ---\n",
        alias,
        outcome.status_str()
    ));
    out.push_str(&format!("TASK: {}\n", task));
    out.push_str(&format!(
        "DURATION: {:.1}s | {} turns\n\n",
        secs, turn_count
    ));

    let mut current_turn = 0u32;
    for msg in messages {
        match msg {
            AgentMessage::Llm(Message::Assistant(asst)) => {
                current_turn += 1;
                out.push_str(&format!("[Turn {}]\n", current_turn));
                for block in &asst.content {
                    match block {
                        ContentBlock::Text { text, .. } => {
                            out.push_str(&format!("ASSISTANT: {}\n", text));
                        }
                        ContentBlock::ToolCall {
                            id,
                            name,
                            arguments,
                            ..
                        } => {
                            let args_str = format_args_brief(arguments);
                            out.push_str(&format!("TOOL_CALL [{}] {} ({})\n", id, name, args_str));
                        }
                        ContentBlock::Thinking { .. } => {
                            // Omit thinking blocks
                        }
                        ContentBlock::Image { .. } => {
                            out.push_str("ASSISTANT: [image]\n");
                        }
                    }
                }
            }
            AgentMessage::Llm(Message::ToolResult(tr)) => {
                let content_text = tool_result_text(&tr.content);
                let truncated = truncate_lines(&content_text, 20);
                if tr.is_error {
                    out.push_str(&format!(
                        "TOOL_RESULT [{}] {} ERROR:\n{}\n",
                        tr.tool_call_id, tr.tool_name, truncated
                    ));
                } else {
                    out.push_str(&format!(
                        "TOOL_RESULT [{}] {} =>\n{}\n",
                        tr.tool_call_id, tr.tool_name, truncated
                    ));
                }
            }
            AgentMessage::Llm(Message::User(_)) => {
                // Skip user messages (the task prompt) in trace
            }
            AgentMessage::Custom { .. } => {}
        }
        out.push('\n');
    }

    out.push_str(&format!("RESULT: {}\n", outcome.result_text()));
    if let ThreadOutcome::Completed { evidence, .. } = outcome {
        if !evidence.is_empty() {
            out.push_str(&format!("EVIDENCE: [{}]\n", evidence.join(", ")));
        }
    }
    out
}

/// Compact trace: one-liner per tool call, task + result.
/// Used for injection into downstream thread system prompts.
fn format_compact_trace(
    alias: &str,
    task: &str,
    messages: &[AgentMessage],
    outcome: &ThreadOutcome,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "--- Thread: {} [{}] ---\n",
        alias,
        outcome.status_str()
    ));
    out.push_str(&format!("TASK: {}\n", task));

    // Pair tool calls with their results for one-liners
    let mut pending_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args_brief)

    for msg in messages {
        match msg {
            AgentMessage::Llm(Message::Assistant(asst)) => {
                for block in &asst.content {
                    if let ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                        ..
                    } = block
                    {
                        let args_str = format_args_brief(arguments);
                        pending_calls.push((id.clone(), name.clone(), args_str));
                    }
                }
            }
            AgentMessage::Llm(Message::ToolResult(tr)) => {
                // Find matching pending call
                if let Some(pos) = pending_calls
                    .iter()
                    .position(|(id, _, _)| *id == tr.tool_call_id)
                {
                    let (_id, name, args_str) = pending_calls.remove(pos);
                    let summary = tool_result_summary(&tr.content, tr.is_error);
                    out.push_str(&format!("{}({}) => {}\n", name, args_str, summary));
                }
            }
            _ => {}
        }
    }

    out.push_str(&format!("RESULT: {}\n", outcome.result_text()));
    if let ThreadOutcome::Completed { evidence, .. } = outcome {
        if !evidence.is_empty() {
            out.push_str(&format!("EVIDENCE: [{}]\n", evidence.join(", ")));
        }
    }
    out
}

/// Format tool call arguments as a brief string.
fn format_args_brief(args: &std::collections::HashMap<String, serde_json::Value>) -> String {
    if args.is_empty() {
        return String::new();
    }
    args.iter()
        .map(|(k, v)| {
            let v_str = match v {
                serde_json::Value::String(s) => {
                    if s.len() > 60 {
                        format!("\"{}...\"", &s[..57])
                    } else {
                        format!("\"{}\"", s)
                    }
                }
                other => {
                    let s = other.to_string();
                    if s.len() > 60 {
                        format!("{}...", &s[..57])
                    } else {
                        s
                    }
                }
            };
            format!("{}={}", k, v_str)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extract text content from tool result blocks.
fn tool_result_text(blocks: &[ai::types::UserBlock]) -> String {
    blocks
        .iter()
        .map(|b| match b {
            ai::types::UserBlock::Text { text } => text.as_str(),
            ai::types::UserBlock::Image { .. } => "[image]",
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One-line summary of a tool result for compact trace.
fn tool_result_summary(blocks: &[ai::types::UserBlock], is_error: bool) -> String {
    let text = tool_result_text(blocks);
    let first_line = text.lines().next().unwrap_or("(empty)");
    let line_count = text.lines().count();

    let summary = if first_line.len() > 80 {
        format!("{}...", &first_line[..77])
    } else if line_count > 1 {
        format!("{} ({} lines)", first_line, line_count)
    } else {
        first_line.to_string()
    };

    if is_error {
        format!("ERROR: {}", summary)
    } else {
        summary
    }
}

/// Truncate text to at most N lines, adding a marker if truncated.
fn truncate_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    let half = max_lines / 2;
    let mut out = String::new();
    for line in &lines[..half] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!(
        "[... {} lines omitted ...]\n",
        lines.len() - max_lines
    ));
    for line in &lines[lines.len() - half..] {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai::types::{
        AssistantMessage, ContentBlock, StopReason, ToolResultMessage, UserBlock, UserMessage,
    };

    fn make_assistant(text: &str, tool_calls: Vec<ContentBlock>) -> AgentMessage {
        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_string(),
                text_signature: None,
            });
        }
        content.extend(tool_calls);
        AgentMessage::Llm(Message::Assistant(AssistantMessage {
            role: "assistant".to_string(),
            content,
            api: "anthropic-messages".to_string(),
            provider: "anthropic".to_string(),
            model: "test".to_string(),
            usage: Default::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 0,
        }))
    }

    fn make_tool_result(call_id: &str, name: &str, text: &str, is_error: bool) -> AgentMessage {
        AgentMessage::Llm(Message::ToolResult(ToolResultMessage {
            role: "toolResult".to_string(),
            tool_call_id: call_id.to_string(),
            tool_name: name.to_string(),
            content: vec![UserBlock::Text {
                text: text.to_string(),
            }],
            details: None,
            is_error,
            timestamp: 0,
        }))
    }

    fn make_user(text: &str) -> AgentMessage {
        AgentMessage::Llm(Message::User(UserMessage::new(text)))
    }

    fn make_tool_call(id: &str, name: &str, args: &[(&str, &str)]) -> ContentBlock {
        ContentBlock::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
                .collect(),
            thought_signature: None,
        }
    }

    #[test]
    fn test_compact_trace_basic() {
        let messages = vec![
            make_user("Find auth endpoints"),
            make_assistant(
                "Searching for auth endpoints.",
                vec![make_tool_call("tc1", "grep", &[("pattern", "auth")])],
            ),
            make_tool_result(
                "tc1",
                "grep",
                "src/auth.py:1: @app.post('/login')\nsrc/auth.py:5: @app.post('/logout')",
                false,
            ),
            make_assistant(
                "Found endpoints.",
                vec![make_tool_call(
                    "tc2",
                    "file_read",
                    &[("path", "src/auth.py")],
                )],
            ),
            make_tool_result("tc2", "file_read", "# auth module\n...", false),
        ];

        let outcome = ThreadOutcome::Completed {
            result: "Found /login and /logout".to_string(),
            evidence: vec![],
        };
        let trace = format_compact_trace("scanner", "Find auth endpoints", &messages, &outcome);

        assert!(trace.contains("--- Thread: scanner [completed] ---"));
        assert!(trace.contains("TASK: Find auth endpoints"));
        assert!(trace.contains("grep(pattern=\"auth\") =>"));
        assert!(trace.contains("file_read(path=\"src/auth.py\") =>"));
        assert!(trace.contains("RESULT: Found /login and /logout"));
    }

    #[test]
    fn test_full_trace_basic() {
        let messages = vec![
            make_user("Find TODOs"),
            make_assistant(
                "I'll search for TODO comments.",
                vec![make_tool_call("tc1", "grep", &[("pattern", "TODO")])],
            ),
            make_tool_result("tc1", "grep", "file.rs:10: // TODO: fix this", false),
        ];

        let outcome = ThreadOutcome::Completed {
            result: "Found 1 TODO".to_string(),
            evidence: vec![],
        };
        let trace = format_full_trace("finder", "Find TODOs", &messages, &outcome, 1500, 1);

        assert!(trace.contains("--- Thread: finder [completed] ---"));
        assert!(trace.contains("DURATION: 1.5s | 1 turns"));
        assert!(trace.contains("[Turn 1]"));
        assert!(trace.contains("ASSISTANT: I'll search for TODO comments."));
        assert!(trace.contains("TOOL_CALL [tc1] grep"));
        assert!(trace.contains("TOOL_RESULT [tc1] grep =>"));
        assert!(trace.contains("RESULT: Found 1 TODO"));
    }

    #[test]
    fn test_episode_generation() {
        let messages = vec![
            make_user("Find TODOs"),
            make_assistant("Searching.", vec![]),
        ];
        let outcome = ThreadOutcome::Aborted {
            reason: "No tools available".to_string(),
        };
        let ep = generate_episode(
            "t-001".to_string(),
            "finder",
            "Find TODOs",
            &messages,
            &outcome,
            500,
        );

        assert_eq!(ep.thread_id, "t-001");
        assert_eq!(ep.alias, "finder");
        assert_eq!(ep.turn_count, 1);
        assert_eq!(ep.duration_ms, 500);
        assert!(ep.full_trace.contains("[aborted]"));
        assert!(ep.compact_trace.contains("RESULT: No tools available"));
    }

    #[test]
    fn test_full_trace_with_evidence() {
        let messages = vec![
            make_user("Find TODOs"),
            make_assistant(
                "Found them.",
                vec![make_tool_call("tc1", "grep", &[("pattern", "TODO")])],
            ),
            make_tool_result("tc1", "grep", "file.rs:10: // TODO: fix", false),
        ];

        let outcome = ThreadOutcome::Completed {
            result: "Found 1 TODO".to_string(),
            evidence: vec!["tc1".to_string()],
        };
        let trace = format_full_trace("finder", "Find TODOs", &messages, &outcome, 1500, 1);
        assert!(trace.contains("RESULT: Found 1 TODO"));
        assert!(trace.contains("EVIDENCE: [tc1]"));
    }

    #[test]
    fn test_compact_trace_with_evidence() {
        let messages = vec![
            make_user("Find endpoints"),
            make_assistant(
                "",
                vec![
                    make_tool_call("tc1", "grep", &[("pattern", "route")]),
                    make_tool_call("tc2", "file_read", &[("path", "src/app.py")]),
                ],
            ),
            make_tool_result("tc1", "grep", "3 matches", false),
            make_tool_result("tc2", "file_read", "app code", false),
        ];

        // With evidence
        let outcome = ThreadOutcome::Completed {
            result: "Found routes".to_string(),
            evidence: vec!["tc1".to_string(), "tc2".to_string()],
        };
        let trace = format_compact_trace("scanner", "Find endpoints", &messages, &outcome);
        assert!(trace.contains("EVIDENCE: [tc1, tc2]"));

        // Without evidence — no EVIDENCE line
        let outcome_no_ev = ThreadOutcome::Completed {
            result: "Found routes".to_string(),
            evidence: vec![],
        };
        let trace2 = format_compact_trace("scanner", "Find endpoints", &messages, &outcome_no_ev);
        assert!(!trace2.contains("EVIDENCE"));
    }

    #[test]
    fn test_truncate_lines() {
        let text = (0..30)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let truncated = truncate_lines(&text, 10);
        assert!(truncated.contains("line 0"));
        assert!(truncated.contains("line 29"));
        assert!(truncated.contains("[... 20 lines omitted ...]"));
    }
}
