//! Tests for AgentStats — performance metrics via event subscription.
//!
//! INV-1: Token totals equal sum of per-turn usage values.
//! INV-2: Turn durations are non-negative; individual turns sum ≤ total duration.
//! INV-3: Tool execution times are non-negative.
//! INV-4: JSON output is valid and contains expected top-level keys.

mod common;
use common::*;

use agent::stats::AgentStats;
use agent::types::{AgentEvent, AgentMessage, AgentToolResult};
use ai::types::{
    AssistantMessage, AssistantMessageEvent, ContentBlock, Cost, Message, StopReason,
    ToolResultMessage, Usage, UserBlock,
};
use serde_json::json;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assistant_message_with_usage(input: u64, output: u64, cost_total: f64) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::Text {
            text: "ok".into(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "mock".into(),
        usage: Usage {
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            total_tokens: input + output,
            cost: Cost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                total: cost_total,
            },
        },
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

fn agent_message(am: AssistantMessage) -> AgentMessage {
    AgentMessage::Llm(Message::Assistant(am))
}

fn empty_tool_result(tool_call_id: &str, tool_name: &str) -> ToolResultMessage {
    ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: tool_call_id.into(),
        tool_name: tool_name.into(),
        content: vec![UserBlock::Text {
            text: "done".into(),
        }],
        details: None,
        is_error: false,
        timestamp: 0,
    }
}

fn text_delta_event(delta: &str) -> Box<AssistantMessageEvent> {
    Box::new(AssistantMessageEvent::TextDelta {
        content_index: 0,
        delta: delta.into(),
        partial: assistant_message_with_usage(0, 0, 0.0),
    })
}

fn tool_result(_tool_name: &str) -> AgentToolResult {
    AgentToolResult {
        content: vec![UserBlock::Text {
            text: "result".into(),
        }],
        details: None,
    }
}

// ---------------------------------------------------------------------------
// INV-1: Token totals equal sum of per-turn usage
// ---------------------------------------------------------------------------

#[test]
fn inv1_token_totals_equal_sum_of_turns() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);

    // Turn 1: 100 in, 50 out, $0.001
    h(&AgentEvent::TurnStart);
    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(100, 50, 0.001)),
        tool_results: vec![],
    });

    // Turn 2: 200 in, 80 out, $0.002
    h(&AgentEvent::TurnStart);
    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(200, 80, 0.002)),
        tool_results: vec![],
    });

    h(&AgentEvent::AgentEnd { messages: vec![] });

    let json = stats.json();
    let totals = &json["totals"];
    assert_eq!(totals["input_tokens"], 300, "INV-1: input sum");
    assert_eq!(totals["output_tokens"], 130, "INV-1: output sum");
    assert!(
        (totals["total_cost"].as_f64().unwrap() - 0.003).abs() < 1e-9,
        "INV-1: cost sum"
    );

    // Also verify turn-level data
    let turns = json["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["input_tokens"], 100);
    assert_eq!(turns[1]["input_tokens"], 200);
}

// ---------------------------------------------------------------------------
// INV-2: Turn durations non-negative, total duration ≥ 0
// ---------------------------------------------------------------------------

#[test]
fn inv2_turn_durations_non_negative() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);
    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(10, 5, 0.0)),
        tool_results: vec![],
    });
    h(&AgentEvent::AgentEnd { messages: vec![] });

    let json = stats.json();
    let total = json["total_duration"].as_f64().unwrap();
    assert!(total >= 0.0, "INV-2: total duration non-negative");

    let turn_dur = json["turns"][0]["duration_secs"].as_f64().unwrap();
    assert!(turn_dur >= 0.0, "INV-2: turn duration non-negative");
}

// ---------------------------------------------------------------------------
// INV-3: Tool execution times non-negative
// ---------------------------------------------------------------------------

#[test]
fn inv3_tool_times_non_negative() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);
    h(&AgentEvent::ToolExecutionStart {
        tool_call_id: "call-1".into(),
        tool_name: "bash".into(),
        args: json!({}),
        thread_id: None,
        thread_alias: None,
    });
    h(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "call-1".into(),
        tool_name: "bash".into(),
        result: tool_result("bash"),
        is_error: false,
        thread_id: None,
        thread_alias: None,
    });
    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(50, 20, 0.0)),
        tool_results: vec![empty_tool_result("call-1", "bash")],
    });
    h(&AgentEvent::AgentEnd { messages: vec![] });

    let json = stats.json();
    let tool_dur = json["turns"][0]["tools"][0]["duration_secs"]
        .as_f64()
        .unwrap();
    assert!(tool_dur >= 0.0, "INV-3: tool duration non-negative");
}

// ---------------------------------------------------------------------------
// INV-4: JSON has expected top-level keys
// ---------------------------------------------------------------------------

#[test]
fn inv4_json_has_expected_keys() {
    let stats = AgentStats::new();
    // Even with no events, json() must produce valid output with expected keys
    let json = stats.json();
    assert!(
        json.get("total_duration").is_some(),
        "INV-4: total_duration"
    );
    assert!(json.get("turns").is_some(), "INV-4: turns");
    assert!(json.get("totals").is_some(), "INV-4: totals");
    // ttft_secs is present (may be null)
    assert!(json.get("ttft_secs").is_some(), "INV-4: ttft_secs");
}

// ---------------------------------------------------------------------------
// Critical path: 2-turn run with 1 tool call
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_turn_run_with_tool_call() {
    use agent::agent::{Agent, AgentOptions};

    let stats = AgentStats::new();
    let handler = stats.handler();

    // Turn 1: tool call
    let turn1_msg = {
        let mut map = HashMap::new();
        map.insert("cmd".to_string(), json!("echo hi"));
        AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolCall {
                id: "call-1".into(),
                name: "bash".into(),
                arguments: map,
                thought_signature: None,
            }],
            api: "openai-responses".into(),
            provider: "openai".into(),
            model: "mock".into(),
            usage: Usage {
                input: 100,
                output: 30,
                cache_read: 0,
                cache_write: 0,
                total_tokens: 130,
                cost: Cost {
                    input: 0.0,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                    total: 0.002,
                },
            },
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 0,
        }
    };

    // Turn 2: final response
    let turn2_msg = assistant_message_with_usage(200, 60, 0.004);

    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn_from_messages(vec![
            turn1_msg.clone(),
            turn2_msg.clone(),
        ])),
        ..default_agent_opts()
    });

    // Subscribe stats before prompting
    let _unsub = agent.subscribe(handler);

    agent.prompt("run something").await.unwrap();

    let json = stats.json();
    let turns = json["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2, "2 turns expected");

    let totals = &json["totals"];
    assert_eq!(totals["input_tokens"], 300u64, "INV-1: total input");
    assert_eq!(totals["output_tokens"], 90u64, "INV-1: total output");
    assert_eq!(totals["tool_calls"], 1u64, "1 tool call");
}

// ---------------------------------------------------------------------------
// Critical path: run with no tool calls — message metrics populated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_with_no_tool_calls() {
    use agent::agent::{Agent, AgentOptions};

    let stats = AgentStats::new();
    let handler = stats.handler();

    let msg = assistant_message_with_usage(50, 25, 0.001);
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn_from_messages(vec![msg])),
        ..default_agent_opts()
    });
    let _unsub = agent.subscribe(handler);
    agent.prompt("hello").await.unwrap();

    let json = stats.json();
    let turns = json["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0]["tools"].as_array().unwrap().len(), 0, "no tools");
    let totals = &json["totals"];
    assert_eq!(totals["input_tokens"], 50u64);
    assert_eq!(totals["output_tokens"], 25u64);
    assert_eq!(totals["tool_calls"], 0u64);
}

// ---------------------------------------------------------------------------
// Critical path: TTFT captured on first text delta
// ---------------------------------------------------------------------------

#[test]
fn ttft_captured_on_first_text_delta() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);

    // First text delta → should capture TTFT
    let msg = agent_message(assistant_message_with_usage(0, 0, 0.0));
    h(&AgentEvent::MessageUpdate {
        message: msg.clone(),
        assistant_event: text_delta_event("Hello"),
    });
    // Second delta should NOT overwrite TTFT
    h(&AgentEvent::MessageUpdate {
        message: msg,
        assistant_event: text_delta_event(" world"),
    });

    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(10, 5, 0.0)),
        tool_results: vec![],
    });
    h(&AgentEvent::AgentEnd { messages: vec![] });

    let json = stats.json();
    let ttft = json["ttft_secs"].as_f64();
    assert!(ttft.is_some(), "TTFT should be captured");
    assert!(ttft.unwrap() >= 0.0, "TTFT non-negative");

    let summary = stats.summary();
    assert!(summary.contains("TTFT"), "summary shows TTFT");
}

// ---------------------------------------------------------------------------
// Failure mode: TurnEnd without TurnStart → no panic, graceful degradation
// ---------------------------------------------------------------------------

#[test]
fn turn_end_without_turn_start_no_panic() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);
    // No TurnStart
    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(10, 5, 0.0)),
        tool_results: vec![],
    });
    h(&AgentEvent::AgentEnd { messages: vec![] });

    // Should have recorded 1 turn with zero duration (graceful degradation)
    let json = stats.json();
    let turns = json["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1);
    let turn_dur = turns[0]["duration_secs"].as_f64().unwrap();
    assert!(turn_dur >= 0.0, "duration must be non-negative");
}

// ---------------------------------------------------------------------------
// Failure mode: zero-turn agent run → valid stats with zero values
// ---------------------------------------------------------------------------

#[test]
fn zero_turn_run_valid_stats() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::AgentEnd { messages: vec![] });

    let json = stats.json();
    assert_eq!(json["turns"].as_array().unwrap().len(), 0);
    assert_eq!(json["totals"]["input_tokens"], 0u64);
    assert_eq!(json["totals"]["tool_calls"], 0u64);
    assert!(json["total_duration"].as_f64().unwrap() >= 0.0);

    let summary = stats.summary();
    assert!(summary.contains("Turns: 0"));
}

// ---------------------------------------------------------------------------
// Failure mode: multiple calls to summary()/json() → consistent, no mutation
// ---------------------------------------------------------------------------

#[test]
fn repeated_summary_and_json_consistent() {
    let stats = AgentStats::new();
    let h = stats.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);
    h(&AgentEvent::TurnEnd {
        message: agent_message(assistant_message_with_usage(100, 50, 0.005)),
        tool_results: vec![],
    });
    h(&AgentEvent::AgentEnd { messages: vec![] });

    let s1 = stats.summary();
    let s2 = stats.summary();
    assert_eq!(s1, s2, "summary must be idempotent");

    let j1 = stats.json();
    let j2 = stats.json();
    assert_eq!(j1, j2, "json must be idempotent");

    // No new turns added by repeated calls
    let turns = j1["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1, "no turns added by repeated calls");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_agent_opts() -> agent::agent::AgentOptions {
    agent::agent::AgentOptions {
        initial_state: Some(agent::agent::AgentStateInit {
            model: Some(mock_model()),
            ..Default::default()
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
        max_turns: None,
    }
}
