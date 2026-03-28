//! TraceSubscriber — writes run.json and trace.jsonl for benchmark analysis.
//!
//! Follows the same pattern as AgentStats: hook into `agent.subscribe()` via
//! a handler closure, collect events into internal state, write output on end.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use agent::types::{AgentEvent, AgentMessage, AgentToolResult};
use ai::types::{Message, StopReason, UserBlock};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Public configuration
// ---------------------------------------------------------------------------

pub struct TraceConfig {
    pub run_id: String,
    pub task_id: Option<String>,
    pub model_id: String,
    pub provider: String,
    pub tool_names: Vec<String>,
    pub edit_mode: String,
    /// SHA-256 of the system prompt, first 16 hex chars.
    /// Compute with `trace::sha256_prefix(system_prompt)` before calling new().
    pub system_prompt_hash: String,
    /// Used to distinguish "max_turns_reached" from "completed".
    pub max_turns: Option<u32>,
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct TraceInner {
    start_time: Option<DateTime<Utc>>,
    start_instant: Option<Instant>,
    end_time: Option<DateTime<Utc>>,
    wall_clock_ms: u64,
    turns: u32,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cost: f64,
    tool_calls: u32,
    final_status: String,
    error_message: Option<String>,
    last_stop_reason: Option<StopReason>,
    max_turns: Option<u32>,
    /// active: tool_call_id -> (timestamp, tool_name, instant)
    tool_starts: HashMap<String, (DateTime<Utc>, String, Instant)>,
    trace_writer: Option<File>,
}

impl Default for TraceInner {
    fn default() -> Self {
        TraceInner {
            start_time: None,
            start_instant: None,
            end_time: None,
            wall_clock_ms: 0,
            turns: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost: 0.0,
            tool_calls: 0,
            final_status: "completed".to_string(),
            error_message: None,
            last_stop_reason: None,
            max_turns: None,
            tool_starts: HashMap::new(),
            trace_writer: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Subscribes to AgentEvents and writes structured trace files.
///
/// # Example
/// ```ignore
/// let t = TraceSubscriber::new("/tmp/traces", TraceConfig { ... });
/// let _unsub = agent.subscribe(t.handler());
/// agent.prompt("hello").await?;
/// t.finalize(); // writes run.json
/// ```
#[derive(Clone)]
pub struct TraceSubscriber {
    inner: Arc<Mutex<TraceInner>>,
    trace_dir: PathBuf,
    config: Arc<TraceConfig>,
}

impl TraceSubscriber {
    pub fn new(trace_dir: impl AsRef<Path>, config: TraceConfig) -> Self {
        let max_turns = config.max_turns;
        let inner = TraceInner {
            max_turns,
            ..TraceInner::default()
        };
        TraceSubscriber {
            inner: Arc::new(Mutex::new(inner)),
            trace_dir: trace_dir.as_ref().to_path_buf(),
            config: Arc::new(config),
        }
    }

    /// Returns a closure suitable for `agent.subscribe()`.
    pub fn handler(&self) -> impl Fn(&AgentEvent) + Send + Sync + Clone + 'static {
        let inner = Arc::clone(&self.inner);
        let trace_dir = self.trace_dir.clone();
        move |event: &AgentEvent| {
            let mut s = inner.lock().unwrap();
            handle_event(&mut s, event, &trace_dir);
        }
    }

    /// Write run.json to the trace directory. Call after the agent run completes.
    pub fn finalize(&self) {
        let s = self.inner.lock().unwrap();
        write_run_json(&s, &self.trace_dir, &self.config);
    }
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Compute SHA-256 of a string and return the first 16 hex chars.
pub fn sha256_prefix(s: &str) -> String {
    let hash = Sha256::digest(s.as_bytes());
    hash.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

/// tau version string: "0.1.0 (abc1234)" or just "0.1.0".
pub fn tau_version() -> String {
    let ver = env!("CARGO_PKG_VERSION");
    match option_env!("TAU_GIT_SHA") {
        Some(sha) => format!("{} ({})", ver, sha),
        None => ver.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

fn handle_event(s: &mut TraceInner, event: &AgentEvent, trace_dir: &Path) {
    let now = Utc::now();

    match event {
        AgentEvent::AgentStart => {
            s.start_time = Some(now);
            s.start_instant = Some(Instant::now());

            // Ensure output directory exists and open trace.jsonl for appending.
            let _ = fs::create_dir_all(trace_dir);
            let trace_path = trace_dir.join("trace.jsonl");
            match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&trace_path)
            {
                Ok(f) => s.trace_writer = Some(f),
                Err(e) => eprintln!("Warning: failed to open trace.jsonl: {}", e),
            }

            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "agent_start",
                }),
            );
        }

        AgentEvent::AgentEnd { .. } => {
            s.end_time = Some(now);
            if let Some(inst) = s.start_instant {
                s.wall_clock_ms = inst.elapsed().as_millis() as u64;
            }

            s.final_status = determine_status(&s.last_stop_reason, s.turns, s.max_turns);

            let mut agent_end = json!({
                "ts": now.to_rfc3339(),
                "event": "agent_end",
                "status": s.final_status,
            });
            if let Some(ref err) = s.error_message {
                agent_end["error_message"] = json!(err);
            }
            write_trace_event(s, &agent_end);
        }

        AgentEvent::TurnStart => {
            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "turn_start",
                }),
            );
        }

        AgentEvent::TurnEnd { message, .. } => {
            s.turns += 1;

            let (input_tokens, output_tokens, cost) = extract_usage(message);
            s.total_input_tokens += input_tokens;
            s.total_output_tokens += output_tokens;
            s.total_cost += cost;

            // Track last stop reason and error for final_status determination.
            if let AgentMessage::Llm(Message::Assistant(am)) = message {
                s.last_stop_reason = Some(am.stop_reason.clone());
                if am.error_message.is_some() {
                    s.error_message = am.error_message.clone();
                }
            }

            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "turn_end",
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                }),
            );
        }

        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            s.tool_starts.insert(
                tool_call_id.clone(),
                (now, tool_name.clone(), Instant::now()),
            );

            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "tool_start",
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "args": args,
                }),
            );
        }

        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => {
            let duration_ms = s
                .tool_starts
                .remove(tool_call_id)
                .map(|(_, _, inst)| inst.elapsed().as_millis() as u64)
                .unwrap_or(0);
            s.tool_calls += 1;

            let result_summary = extract_result_summary(result);

            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "tool_end",
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "duration_ms": duration_ms,
                    "is_error": is_error,
                    "result_summary": result_summary,
                }),
            );
        }

        AgentEvent::ThreadStart {
            thread_id,
            alias,
            task,
            model,
        } => {
            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "thread_start",
                    "thread_id": thread_id,
                    "alias": alias,
                    "task": task,
                    "model": model,
                }),
            );
        }

        AgentEvent::ThreadEnd {
            thread_id,
            alias,
            outcome,
            duration_ms,
        } => {
            write_trace_event(
                s,
                &json!({
                    "ts": now.to_rfc3339(),
                    "event": "thread_end",
                    "thread_id": thread_id,
                    "alias": alias,
                    "outcome": outcome.status_str(),
                    "duration_ms": duration_ms,
                }),
            );
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// run.json writer
// ---------------------------------------------------------------------------

fn write_run_json(s: &TraceInner, trace_dir: &Path, config: &TraceConfig) {
    let _ = fs::create_dir_all(trace_dir);
    let run_json_path = trace_dir.join("run.json");

    let obj = json!({
        "run_id": config.run_id,
        "task_id": config.task_id,
        "model": config.model_id,
        "provider": config.provider,
        "tools": config.tool_names,
        "edit_mode": config.edit_mode,
        "start_time": s.start_time.map(|t| t.to_rfc3339()),
        "end_time": s.end_time.map(|t| t.to_rfc3339()),
        "wall_clock_ms": s.wall_clock_ms,
        "final_status": s.final_status,
        "error_message": s.error_message,
        "turns": s.turns,
        "total_input_tokens": s.total_input_tokens,
        "total_output_tokens": s.total_output_tokens,
        "total_cost": s.total_cost,
        "tool_calls": s.tool_calls,
        "system_prompt_hash": config.system_prompt_hash,
        "tau_version": tau_version(),
    });

    let json_str = serde_json::to_string_pretty(&obj).unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = fs::write(&run_json_path, json_str) {
        eprintln!("Warning: failed to write run.json: {}", e);
    }
}

// ---------------------------------------------------------------------------
// trace.jsonl writer
// ---------------------------------------------------------------------------

fn write_trace_event(s: &mut TraceInner, event: &Value) {
    if let Some(ref mut writer) = s.trace_writer {
        let line = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
        let _ = writeln!(writer, "{}", line);
        let _ = writer.flush();
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn determine_status(
    last_stop_reason: &Option<StopReason>,
    turns: u32,
    max_turns: Option<u32>,
) -> String {
    match last_stop_reason {
        Some(StopReason::Error) => "error".to_string(),
        Some(StopReason::Aborted) => "aborted".to_string(),
        Some(StopReason::ToolUse) => {
            // Still in tool-call mode when the loop ended → max_turns_reached.
            if max_turns.is_some_and(|max| turns >= max) {
                "max_turns_reached".to_string()
            } else {
                "completed".to_string()
            }
        }
        _ => "completed".to_string(),
    }
}

fn extract_usage(msg: &AgentMessage) -> (u64, u64, f64) {
    if let AgentMessage::Llm(Message::Assistant(am)) = msg {
        let usage = &am.usage;
        (usage.input, usage.output, usage.cost.total)
    } else {
        (0, 0, 0.0)
    }
}

/// Extract a short summary (first 100 chars) from the tool result's text content.
fn extract_result_summary(result: &AgentToolResult) -> String {
    for block in &result.content {
        if let UserBlock::Text { text } = block {
            let trimmed = text.trim();
            if trimmed.len() <= 100 {
                return trimmed.to_string();
            } else {
                return format!("{}...", &trimmed[..100]);
            }
        }
    }
    String::new()
}
