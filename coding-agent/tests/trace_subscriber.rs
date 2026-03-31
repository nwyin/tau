//! Tests for TraceSubscriber (INV-1 through INV-5).

use agent::types::{AgentEvent, AgentMessage, AgentToolResult};
use ai::types::{AssistantMessage, ContentBlock, Message, StopReason, Usage, UserBlock};
use coding_agent::trace::{sha256_prefix, TraceConfig, TraceSubscriber};
use serde_json::Value;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_config(task_id: Option<&str>) -> TraceConfig {
    TraceConfig {
        run_id: "test-run-123".to_string(),
        task_id: task_id.map(str::to_string),
        model_id: "gpt-4o".to_string(),
        provider: "openai".to_string(),
        tool_names: vec!["file_read".to_string(), "bash".to_string()],
        edit_mode: "replace".to_string(),
        system_prompt_hash: sha256_prefix("You are a helpful assistant."),
        max_turns: None,
    }
}

fn mock_assistant_message(stop_reason: StopReason) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".to_string(),
        content: vec![ContentBlock::Text {
            text: "Hello".to_string(),
            text_signature: None,
        }],
        api: "openai-responses".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        usage: Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            cost: ai::types::Cost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                total: 0.001,
            },
            total_tokens: 150,
        },
        stop_reason,
        error_message: None,
        timestamp: 0,
    }
}

fn mock_tool_result(_tool_call_id: &str, _tool_name: &str, text: &str) -> AgentToolResult {
    AgentToolResult {
        content: vec![UserBlock::Text {
            text: text.to_string(),
        }],
        details: None,
    }
}

fn turn_end_event(stop_reason: StopReason) -> AgentEvent {
    let am = mock_assistant_message(stop_reason);
    AgentEvent::TurnEnd {
        message: AgentMessage::Llm(Message::Assistant(am)),
        tool_results: vec![],
    }
}

fn read_run_json(dir: &TempDir) -> Value {
    let path = dir.path().join("run.json");
    let text = std::fs::read_to_string(&path).expect("run.json missing");
    serde_json::from_str(&text).expect("run.json is not valid JSON")
}

fn read_trace_lines(dir: &TempDir) -> Vec<Value> {
    let path = dir.path().join("trace.jsonl");
    let text = std::fs::read_to_string(&path).expect("trace.jsonl missing");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("trace line is not valid JSON"))
        .collect()
}

// ---------------------------------------------------------------------------
// INV-1: run.json contains all documented fields; task_id is null when absent
// ---------------------------------------------------------------------------

#[test]
fn test_run_json_fields_complete_no_task_id() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);
    h(&turn_end_event(StopReason::Stop));
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let run = read_run_json(&dir);

    // Required fields present
    assert!(run["run_id"].is_string(), "missing run_id");
    assert!(run["task_id"].is_null(), "task_id should be null");
    assert!(run["model"].is_string(), "missing model");
    assert!(run["provider"].is_string(), "missing provider");
    assert!(run["tools"].is_array(), "missing tools");
    assert!(run["edit_mode"].is_string(), "missing edit_mode");
    assert!(run["start_time"].is_string(), "missing start_time");
    assert!(run["end_time"].is_string(), "missing end_time");
    assert!(!run["wall_clock_ms"].is_null(), "missing wall_clock_ms");
    assert!(run["final_status"].is_string(), "missing final_status");
    assert!(!run["turns"].is_null(), "missing turns");
    assert!(
        !run["total_input_tokens"].is_null(),
        "missing total_input_tokens"
    );
    assert!(
        !run["total_output_tokens"].is_null(),
        "missing total_output_tokens"
    );
    assert!(!run["total_cost"].is_null(), "missing total_cost");
    assert!(!run["tool_calls"].is_null(), "missing tool_calls");
    assert!(
        run["system_prompt_hash"].is_string(),
        "missing system_prompt_hash"
    );
    assert!(run["tau_version"].is_string(), "missing tau_version");

    // Spot-check values
    assert_eq!(run["run_id"], "test-run-123");
    assert_eq!(run["model"], "gpt-4o");
    assert_eq!(run["provider"], "openai");
    assert_eq!(run["edit_mode"], "replace");
    assert_eq!(run["turns"], 1);
    assert_eq!(run["final_status"], "completed");
}

#[test]
fn test_run_json_task_id_present() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(Some("swe-bench-42")));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let run = read_run_json(&dir);
    assert_eq!(run["task_id"], "swe-bench-42");
}

// ---------------------------------------------------------------------------
// INV-2: trace.jsonl events ordered by timestamp
// ---------------------------------------------------------------------------

#[test]
fn test_trace_events_ordered_by_timestamp() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);
    h(&turn_end_event(StopReason::Stop));
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let lines = read_trace_lines(&dir);
    assert!(!lines.is_empty(), "trace.jsonl should not be empty");

    let timestamps: Vec<&str> = lines
        .iter()
        .map(|v| v["ts"].as_str().expect("each event must have a ts field"))
        .collect();

    // Verify all timestamps are present and in non-decreasing order.
    for pair in timestamps.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "timestamps out of order: {} > {}",
            pair[0],
            pair[1]
        );
    }
}

// ---------------------------------------------------------------------------
// INV-3: every tool_start has a matching tool_end with same tool_call_id
// ---------------------------------------------------------------------------

#[test]
fn test_tool_start_end_pairing() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);

    let tool_id = "call_abc123";
    let tool_name = "file_read";

    h(&AgentEvent::ToolExecutionStart {
        tool_call_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        args: serde_json::json!({"path": "/tmp/foo.rs"}),
    });

    h(&AgentEvent::ToolExecutionEnd {
        tool_call_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        result: mock_tool_result(tool_id, tool_name, "fn main() {}"),
        is_error: false,
    });

    h(&turn_end_event(StopReason::Stop));
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let lines = read_trace_lines(&dir);

    let starts: Vec<&Value> = lines
        .iter()
        .filter(|v| v["event"] == "tool_start")
        .collect();
    let ends: Vec<&Value> = lines.iter().filter(|v| v["event"] == "tool_end").collect();

    assert_eq!(starts.len(), 1, "expected 1 tool_start");
    assert_eq!(ends.len(), 1, "expected 1 tool_end");

    let start_id = starts[0]["tool_call_id"].as_str().unwrap();
    let end_id = ends[0]["tool_call_id"].as_str().unwrap();
    assert_eq!(
        start_id, end_id,
        "tool_call_id must match between start and end"
    );
    assert_eq!(starts[0]["tool_name"], tool_name);
    assert_eq!(ends[0]["tool_name"], tool_name);
    assert_eq!(ends[0]["is_error"], false);
    assert!(
        ends[0]["duration_ms"].is_number(),
        "duration_ms must be numeric"
    );
}

// ---------------------------------------------------------------------------
// INV-4: wall_clock_ms is non-negative and approximately matches actual duration
// ---------------------------------------------------------------------------

#[test]
fn test_wall_clock_ms_nonnegative() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    // Small sleep to ensure wall_clock_ms > 0
    std::thread::sleep(std::time::Duration::from_millis(5));
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let run = read_run_json(&dir);
    let ms = run["wall_clock_ms"]
        .as_u64()
        .expect("wall_clock_ms must be u64");
    // Should be at least 1ms given we slept 5ms
    assert!(
        ms >= 1,
        "wall_clock_ms should be positive after a small sleep"
    );
    // Should not be enormous (sanity: under 60 seconds)
    assert!(ms < 60_000, "wall_clock_ms unreasonably large: {}", ms);
}

// ---------------------------------------------------------------------------
// INV-5: tool_names in run.json matches the tools actually configured
// ---------------------------------------------------------------------------

#[test]
fn test_tool_names_match_config() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let run = read_run_json(&dir);
    let tools: Vec<String> = run["tools"]
        .as_array()
        .expect("tools must be array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(
        tools.contains(&"file_read".to_string()),
        "missing file_read"
    );
    assert!(tools.contains(&"bash".to_string()), "missing bash");
    assert_eq!(tools.len(), 2, "expected exactly 2 tools");
}

// ---------------------------------------------------------------------------
// Critical path: multiple tool calls produce correct trace lines in order
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_tool_calls_trace_order() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);

    // Tool 1
    h(&AgentEvent::ToolExecutionStart {
        tool_call_id: "id-1".to_string(),
        tool_name: "file_read".to_string(),
        args: serde_json::json!({}),
    });
    h(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "id-1".to_string(),
        tool_name: "file_read".to_string(),
        result: mock_tool_result("id-1", "file_read", "content"),
        is_error: false,
    });

    // Tool 2
    h(&AgentEvent::ToolExecutionStart {
        tool_call_id: "id-2".to_string(),
        tool_name: "bash".to_string(),
        args: serde_json::json!({}),
    });
    h(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "id-2".to_string(),
        tool_name: "bash".to_string(),
        result: mock_tool_result("id-2", "bash", "output"),
        is_error: false,
    });

    h(&turn_end_event(StopReason::Stop));
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let lines = read_trace_lines(&dir);

    // Extract tool events in order
    let tool_events: Vec<&Value> = lines
        .iter()
        .filter(|v| matches!(v["event"].as_str(), Some("tool_start") | Some("tool_end")))
        .collect();

    assert_eq!(
        tool_events.len(),
        4,
        "expected 4 tool events (2 start + 2 end)"
    );
    assert_eq!(tool_events[0]["event"], "tool_start");
    assert_eq!(tool_events[0]["tool_call_id"], "id-1");
    assert_eq!(tool_events[1]["event"], "tool_end");
    assert_eq!(tool_events[1]["tool_call_id"], "id-1");
    assert_eq!(tool_events[2]["event"], "tool_start");
    assert_eq!(tool_events[2]["tool_call_id"], "id-2");
    assert_eq!(tool_events[3]["event"], "tool_end");
    assert_eq!(tool_events[3]["tool_call_id"], "id-2");

    // run.json should show 2 tool_calls
    let run = read_run_json(&dir);
    assert_eq!(run["tool_calls"], 2);
}

// ---------------------------------------------------------------------------
// Failure mode: output directory doesn't exist → subscriber creates it
// ---------------------------------------------------------------------------

#[test]
fn test_creates_output_directory() {
    let base_dir = TempDir::new().unwrap();
    let trace_dir = base_dir.path().join("deep").join("nested").join("dir");
    assert!(!trace_dir.exists(), "dir should not exist yet");

    let t = TraceSubscriber::new(&trace_dir, make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    assert!(
        trace_dir.exists(),
        "subscriber should have created the directory"
    );
    assert!(
        trace_dir.join("trace.jsonl").exists(),
        "trace.jsonl should exist"
    );
    assert!(trace_dir.join("run.json").exists(), "run.json should exist");
}

// ---------------------------------------------------------------------------
// Failure mode: empty run → run.json valid with 0 turns and 0 tool_calls
// ---------------------------------------------------------------------------

#[test]
fn test_empty_run_produces_valid_run_json() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    // Bare minimum: AgentStart + AgentEnd, no turns, no tools
    h(&AgentEvent::AgentStart);
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let run = read_run_json(&dir);
    assert_eq!(run["turns"], 0, "turns should be 0 for empty run");
    assert_eq!(run["tool_calls"], 0, "tool_calls should be 0 for empty run");
    assert_eq!(run["total_input_tokens"], 0);
    assert_eq!(run["total_output_tokens"], 0);
    assert_eq!(run["final_status"], "completed");
}

// ---------------------------------------------------------------------------
// Token and cost accumulation across turns
// ---------------------------------------------------------------------------

#[test]
fn test_token_accumulation() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);

    // Turn 1: 100 in, 50 out
    h(&AgentEvent::TurnStart);
    h(&turn_end_event(StopReason::Stop));

    // Turn 2: simulate another turn with usage
    h(&AgentEvent::TurnStart);
    h(&turn_end_event(StopReason::Stop));

    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let run = read_run_json(&dir);
    assert_eq!(run["turns"], 2, "expected 2 turns");
    // Each mock turn has 100 input + 50 output from mock_assistant_message
    assert_eq!(run["total_input_tokens"], 200);
    assert_eq!(run["total_output_tokens"], 100);
}

// ---------------------------------------------------------------------------
// result_summary truncation
// ---------------------------------------------------------------------------

#[test]
fn test_result_content_full_no_truncation() {
    let dir = TempDir::new().unwrap();
    let t = TraceSubscriber::new(dir.path(), make_config(None));
    let h = t.handler();

    h(&AgentEvent::AgentStart);
    h(&AgentEvent::TurnStart);

    let long_text = "x".repeat(200);
    h(&AgentEvent::ToolExecutionStart {
        tool_call_id: "id-long".to_string(),
        tool_name: "file_read".to_string(),
        args: serde_json::json!({}),
    });
    h(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "id-long".to_string(),
        tool_name: "file_read".to_string(),
        result: mock_tool_result("id-long", "file_read", &long_text),
        is_error: false,
    });

    h(&turn_end_event(StopReason::Stop));
    h(&AgentEvent::AgentEnd { messages: vec![] });
    t.finalize();

    let lines = read_trace_lines(&dir);
    let tool_end = lines
        .iter()
        .find(|v| v["event"] == "tool_end")
        .expect("tool_end event missing");
    let content = tool_end["result_content"].as_str().unwrap();
    assert_eq!(
        content.len(),
        200,
        "result_content should contain full content without truncation"
    );
}

// ---------------------------------------------------------------------------
// sha256_prefix helper
// ---------------------------------------------------------------------------

#[test]
fn test_sha256_prefix_length_and_determinism() {
    let hash = sha256_prefix("hello world");
    assert_eq!(hash.len(), 16, "sha256_prefix should return 16 hex chars");
    // Deterministic
    assert_eq!(hash, sha256_prefix("hello world"));
    // Different inputs produce different hashes
    assert_ne!(hash, sha256_prefix("different input"));
    // All hex chars
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "must be hex");
}
