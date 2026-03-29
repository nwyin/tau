//! Tests for agent::context — mechanical context compaction.
//!
//! Invariants verified:
//!   INV-1  Under-budget input → returned unchanged (identity)
//!   INV-2  First user message never removed or masked (original task preserved)
//!   INV-3  Tool results not orphaned from their tool calls (turn integrity)
//!   INV-4  After compaction, estimated tokens ≤ budget
//!   INV-5  Masked tool results contain the tool name in the placeholder text

mod common;

use agent::context::{compact_messages, compute_budget, estimate_tokens};
use agent::types::AgentMessage;
use ai::types::{Message, ToolResultMessage, UserBlock, UserContent};

use common::{
    mock_assistant_message, mock_assistant_message_with_tool_call, mock_model, user_message,
};

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

fn tool_result_msg(tool_call_id: &str, tool_name: &str, content: &str) -> AgentMessage {
    AgentMessage::Llm(Message::ToolResult(ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: tool_call_id.into(),
        tool_name: tool_name.into(),
        content: vec![UserBlock::Text {
            text: content.into(),
        }],
        details: None,
        is_error: false,
        timestamp: 0,
    }))
}

fn assistant_msg(text: &str) -> AgentMessage {
    AgentMessage::Llm(Message::Assistant(mock_assistant_message(text)))
}

fn assistant_with_tool(id: &str, name: &str) -> AgentMessage {
    AgentMessage::Llm(Message::Assistant(mock_assistant_message_with_tool_call(
        id,
        name,
        serde_json::json!({"path": "/tmp"}),
    )))
}

/// Model with a very small context window for easy over-budget testing.
/// budget = (1000 * 0.75) as usize - 100 - 2000 → saturating_sub → 0? No:
/// 750 - 100 - 2000 = saturating 0. We need a bigger window.
/// Use context_window=4000, max_tokens=100 → budget = 3000 - 100 - 2000 = 900.
fn tiny_model() -> ai::types::Model {
    ai::types::Model {
        context_window: 4000,
        max_tokens: 100,
        ..mock_model()
    }
}

/// Generate a string of approximately `chars` bytes (ASCII 'x').
fn big_text(chars: usize) -> String {
    "x".repeat(chars)
}

/// Generate a multi-line string with `n` lines, each ~10 chars.
fn many_lines(n: usize) -> String {
    (0..n)
        .map(|i| format!("line {:06}", i))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// INV-1: Under-budget input → returned unchanged
// ---------------------------------------------------------------------------

#[test]
fn under_budget_returns_unchanged() {
    let model = mock_model(); // budget ≈ 2096 tokens
    let messages = vec![user_message("Hello"), assistant_msg("World")];
    let result = compact_messages(messages.clone(), &model);
    assert_eq!(result.len(), messages.len());
    for (a, b) in result.iter().zip(messages.iter()) {
        assert_eq!(a.role(), b.role());
    }
}

// ---------------------------------------------------------------------------
// INV-2: First user message preserved
// ---------------------------------------------------------------------------

#[test]
fn first_user_message_never_removed() {
    let model = tiny_model();
    let budget = compute_budget(&model);

    // Build enough turns to exceed budget.
    let mut messages = vec![user_message("Original task — preserve this exactly")];
    for i in 0..15 {
        let id = format!("c{i}");
        messages.push(assistant_with_tool(&id, "bash"));
        messages.push(tool_result_msg(&id, "bash", &big_text(300)));
        messages.push(user_message("follow up"));
    }

    let before = estimate_tokens(&messages);
    assert!(
        before > budget,
        "test setup: messages must start over budget"
    );

    let result = compact_messages(messages, &model);

    // First element must be a User message with the original text.
    match &result[0] {
        AgentMessage::Llm(Message::User(um)) => {
            if let UserContent::Text(text) = &um.content {
                assert_eq!(text, "Original task — preserve this exactly");
            } else {
                panic!("Expected text user content");
            }
        }
        other => panic!("Expected User message, got {:?}", other.role()),
    }
}

// ---------------------------------------------------------------------------
// INV-4: After compaction, estimated tokens ≤ budget
// ---------------------------------------------------------------------------

#[test]
fn compaction_brings_under_budget() {
    let model = tiny_model();
    let budget = compute_budget(&model);

    let mut messages = vec![user_message("Do the thing")];
    for i in 0..20 {
        let id = format!("c{i}");
        messages.push(assistant_with_tool(&id, "bash"));
        messages.push(tool_result_msg(&id, "bash", &big_text(300)));
        messages.push(user_message("next step"));
    }

    let before = estimate_tokens(&messages);
    assert!(before > budget, "test must start over budget");

    let result = compact_messages(messages, &model);
    let after = estimate_tokens(&result);
    assert!(
        after <= budget,
        "after compaction {after} should be ≤ budget {budget}"
    );
}

// ---------------------------------------------------------------------------
// INV-5: Masked tool results contain the tool name
// ---------------------------------------------------------------------------

#[test]
fn masked_tool_results_contain_tool_name() {
    let model = tiny_model();
    let budget = compute_budget(&model);

    // Two turns: the old turn uses "special_tool_xyz".
    // Each tool result is 2000 chars (500 tokens) so together they exceed the
    // ~900-token budget, forcing tier 2 to mask the older turn.
    let messages = vec![
        user_message("first task"),
        assistant_with_tool("c1", "special_tool_xyz"),
        tool_result_msg("c1", "special_tool_xyz", &big_text(2000)),
        user_message("second task"),
        assistant_with_tool("c2", "bash"),
        tool_result_msg("c2", "bash", &big_text(2000)),
    ];

    let before = estimate_tokens(&messages);
    assert!(
        before > budget,
        "test setup: messages must exceed budget ({before} > {budget})"
    );

    let result = compact_messages(messages, &model);

    // At least one tool result must mention "special_tool_xyz" in its content.
    let found = result.iter().any(|msg| {
        if let AgentMessage::Llm(Message::ToolResult(tr)) = msg {
            tr.content.iter().any(|b| {
                if let UserBlock::Text { text } = b {
                    text.contains("special_tool_xyz")
                } else {
                    false
                }
            })
        } else {
            false
        }
    });

    assert!(
        found,
        "a masked tool result should mention 'special_tool_xyz'"
    );
}

// ---------------------------------------------------------------------------
// INV-3: Tool results not orphaned from their tool calls
// ---------------------------------------------------------------------------

#[test]
fn tool_results_have_matching_assistant_tool_call() {
    let model = tiny_model();

    let messages = vec![
        user_message("task 1"),
        assistant_with_tool("c1", "bash"),
        tool_result_msg("c1", "bash", &big_text(400)),
        user_message("task 2"),
        assistant_with_tool("c2", "bash"),
        tool_result_msg("c2", "bash", &big_text(400)),
        user_message("task 3"),
        assistant_with_tool("c3", "bash"),
        tool_result_msg("c3", "bash", &big_text(400)),
    ];

    let result = compact_messages(messages, &model);

    // Collect all tool_call_ids from AssistantMessages.
    let mut assistant_ids: Vec<String> = vec![];
    let mut tool_result_ids: Vec<String> = vec![];

    for msg in &result {
        match msg {
            AgentMessage::Llm(Message::Assistant(am)) => {
                for (id, _, _) in am.tool_calls() {
                    assistant_ids.push(id.to_string());
                }
            }
            AgentMessage::Llm(Message::ToolResult(tr)) => {
                tool_result_ids.push(tr.tool_call_id.clone());
            }
            _ => {}
        }
    }

    // Every ToolResult's call id must appear in some AssistantMessage's tool calls.
    for result_id in &tool_result_ids {
        assert!(
            assistant_ids.contains(result_id),
            "ToolResult with id {result_id} has no corresponding AssistantMessage tool call"
        );
    }
}

// ---------------------------------------------------------------------------
// Tier 1: large tool output triggers truncation (stays ≤ budget)
// ---------------------------------------------------------------------------

#[test]
fn tier1_large_tool_output_truncated_and_under_budget() {
    let model = mock_model(); // big budget; but a 60KB output still exceeds it
    let budget = compute_budget(&model);

    // 60 KB output: 15_000 tokens >> budget 2096.
    let huge = big_text(60_000);
    let messages = vec![
        user_message("do stuff"),
        assistant_with_tool("c1", "bash"),
        tool_result_msg("c1", "bash", &huge),
    ];

    let before = estimate_tokens(&messages);
    assert!(before > budget, "test must start over budget");

    let result = compact_messages(messages, &model);

    // The tool result must carry a truncation marker.
    let has_marker = result.iter().any(|msg| {
        if let AgentMessage::Llm(Message::ToolResult(tr)) = msg {
            tr.content
                .iter()
                .any(|b| matches!(b, UserBlock::Text { text } if text.contains("[... truncated")))
        } else {
            false
        }
    });
    assert!(has_marker, "large tool output should be truncated");

    let after = estimate_tokens(&result);
    assert!(
        after <= budget,
        "after truncation {after} should be ≤ budget {budget}"
    );
}

// ---------------------------------------------------------------------------
// Tier 1: tool output at exactly the line limit is NOT truncated
// ---------------------------------------------------------------------------

#[test]
fn tier1_at_line_limit_not_truncated() {
    // 2000 lines × 10 chars = 20_000 chars.  Use a large-context model so
    // this fits in budget — verifying tier 1 does NOT fire at exactly the limit.
    let output = many_lines(2000);
    assert_eq!(output.lines().count(), 2000);

    let mut model = mock_model();
    model.context_window = 100_000; // large enough: budget ≈ 73_000 tokens
    model.max_tokens = 4096;

    let messages = vec![
        user_message("x"),
        assistant_with_tool("c1", "bash"),
        tool_result_msg("c1", "bash", &output),
    ];

    // Under budget → returned unchanged, no truncation marker.
    let budget = compute_budget(&model);
    let before = estimate_tokens(&messages);
    assert!(
        before <= budget,
        "test setup: output must fit in budget ({before} ≤ {budget})"
    );

    let result = compact_messages(messages, &model);

    let has_marker = result.iter().any(|msg| {
        if let AgentMessage::Llm(Message::ToolResult(tr)) = msg {
            tr.content
                .iter()
                .any(|b| matches!(b, UserBlock::Text { text } if text.contains("[... truncated")))
        } else {
            false
        }
    });
    assert!(
        !has_marker,
        "exactly-at-limit output should NOT be truncated"
    );
}

// ---------------------------------------------------------------------------
// Tier 2: many turns exceed budget → old turns masked, recent kept
// ---------------------------------------------------------------------------

#[test]
fn tier2_old_turns_masked_recent_kept() {
    let model = tiny_model();
    let budget = compute_budget(&model);

    // 8 turns of moderate size.
    let mut messages = vec![];
    for i in 0..8 {
        let id = format!("c{i}");
        messages.push(user_message(&format!("task {i}")));
        messages.push(assistant_with_tool(&id, "bash"));
        messages.push(tool_result_msg(&id, "bash", &big_text(200)));
    }

    let before = estimate_tokens(&messages);
    if before <= budget {
        // Ensure we're over budget by padding.
        for i in 8..20 {
            let id = format!("c{i}");
            messages.push(user_message(&format!("task {i}")));
            messages.push(assistant_with_tool(&id, "bash"));
            messages.push(tool_result_msg(&id, "bash", &big_text(200)));
        }
    }
    let before = estimate_tokens(&messages);
    assert!(before > budget, "test must start over budget");

    let result = compact_messages(messages, &model);

    // At least one tool result should be an omission placeholder.
    let masked_count = result
        .iter()
        .filter(|msg| {
            if let AgentMessage::Llm(Message::ToolResult(tr)) = msg {
                tr.content
                    .iter()
                    .any(|b| matches!(b, UserBlock::Text { text } if text.contains("omitted")))
            } else {
                false
            }
        })
        .count();

    assert!(
        masked_count > 0,
        "old turns should have omission placeholders"
    );
}

// ---------------------------------------------------------------------------
// Edge: all messages are user messages — keeps first + recent, drops middle
// ---------------------------------------------------------------------------

#[test]
fn all_user_messages_keeps_first_and_recent() {
    let model = tiny_model();
    let budget = compute_budget(&model);

    // 40 user messages each ~200 chars (50 tokens).
    let messages: Vec<AgentMessage> = (0..40)
        .map(|i| user_message(&format!("{:03}: {}", i, "z".repeat(200))))
        .collect();

    let before = estimate_tokens(&messages);
    if before <= budget {
        panic!("test setup: 40×200-char messages must exceed budget {budget}");
    }

    let result = compact_messages(messages, &model);

    // First message preserved verbatim.
    match &result[0] {
        AgentMessage::Llm(Message::User(um)) => {
            if let UserContent::Text(t) = &um.content {
                assert!(t.starts_with("000: "), "first message must be preserved");
            }
        }
        _ => panic!("first result must be a user message"),
    }

    // Not all 40 messages kept (middle ones dropped).
    assert!(result.len() < 40, "middle user messages should be dropped");
}

// ---------------------------------------------------------------------------
// Edge: tiny budget — no panic, returns something
// ---------------------------------------------------------------------------

#[test]
fn tiny_budget_no_panic() {
    // budget = (200 * 0.75) as usize - 50 - 2000 → saturating → 0
    let mut model = mock_model();
    model.context_window = 200;
    model.max_tokens = 50;

    let messages = vec![
        user_message("do this"),
        assistant_with_tool("c1", "bash"),
        tool_result_msg("c1", "bash", "output"),
    ];

    // Must not panic.
    let _result = compact_messages(messages, &model);
}

// ---------------------------------------------------------------------------
// Single massive tool result in recent turn → overflow fallback
// ---------------------------------------------------------------------------

#[test]
fn overflow_fallback_truncates_recent_large_result() {
    let model = tiny_model();
    let budget = compute_budget(&model);

    // Single turn whose tool result vastly exceeds the budget on its own.
    // budget ≈ 900 tokens = 3600 chars; use 20 KB.
    let messages = vec![
        user_message("start"),
        assistant_with_tool("c1", "bash"),
        tool_result_msg("c1", "bash", &big_text(20_000)),
    ];

    let before = estimate_tokens(&messages);
    assert!(before > budget, "must start over budget");

    let result = compact_messages(messages, &model);

    // Result should still have the tool result (just truncated), and be ≤ budget.
    let has_tool_result = result
        .iter()
        .any(|msg| matches!(msg, AgentMessage::Llm(Message::ToolResult(_))));
    assert!(
        has_tool_result,
        "tool result should still be present after overflow fallback"
    );

    let after = estimate_tokens(&result);
    assert!(
        after <= budget,
        "overflow fallback should bring {after} ≤ budget {budget}"
    );
}
