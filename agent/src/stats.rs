//! AgentStats — collects metrics by subscribing to AgentEvent.
//!
//! Zero changes to the core agent loop. All instrumentation is via the
//! existing event subscriber pattern (`agent.subscribe(stats.handler())`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::types::{AgentEvent, AgentMessage};
use ai::types::{AssistantMessageEvent, Message};

// ---------------------------------------------------------------------------
// Internal data structures
// ---------------------------------------------------------------------------

struct ToolStats {
    name: String,
    duration: Duration,
}

struct TurnStats {
    duration: Duration,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
    tools: Vec<ToolStats>,
}

#[derive(Default)]
struct TokenTotals {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    total_cost: f64,
    tool_calls: u64,
}

#[derive(Default)]
struct StatsInner {
    start_time: Option<Instant>,
    total_duration: Option<Duration>,
    turns: Vec<TurnStats>,
    current_turn_start: Option<Instant>,
    current_tool_starts: HashMap<String, Instant>,
    // Tools that completed this turn (keyed by tool_call_id, stored by name + duration)
    current_turn_tools: Vec<ToolStats>,
    first_token_time: Option<Duration>, // relative to start
    first_token_captured: bool,
    totals: TokenTotals,
}

// ---------------------------------------------------------------------------
// AgentStats — public API
// ---------------------------------------------------------------------------

/// Collects performance metrics from AgentEvent subscriptions.
///
/// # Example
/// ```ignore
/// let stats = AgentStats::new();
/// agent.subscribe(stats.handler());
/// agent.prompt("hello").await?;
/// eprintln!("{}", stats.summary());
/// ```
#[derive(Clone)]
pub struct AgentStats {
    inner: Arc<Mutex<StatsInner>>,
}

impl AgentStats {
    pub fn new() -> Self {
        AgentStats {
            inner: Arc::new(Mutex::new(StatsInner::default())),
        }
    }

    /// Returns a closure suitable for `agent.subscribe()`.
    pub fn handler(&self) -> impl Fn(&AgentEvent) + Send + Sync + Clone + 'static {
        let inner = Arc::clone(&self.inner);
        move |event: &AgentEvent| {
            let mut s = inner.lock().unwrap();
            handle_event(&mut s, event);
        }
    }

    /// Human-readable summary for stderr output.
    pub fn summary(&self) -> String {
        let s = self.inner.lock().unwrap();
        build_summary(&s)
    }

    /// Machine-readable JSON output.
    pub fn json(&self) -> Value {
        let s = self.inner.lock().unwrap();
        build_json(&s)
    }
}

impl Default for AgentStats {
    fn default() -> Self {
        AgentStats::new()
    }
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

fn handle_event(s: &mut StatsInner, event: &AgentEvent) {
    match event {
        AgentEvent::AgentStart => {
            s.start_time = Some(Instant::now());
        }

        AgentEvent::AgentEnd { .. } => {
            if let Some(start) = s.start_time {
                s.total_duration = Some(start.elapsed());
            }
        }

        AgentEvent::TurnStart => {
            s.current_turn_start = Some(Instant::now());
            s.current_turn_tools.clear();
        }

        AgentEvent::TurnEnd { message, .. } => {
            let duration = s
                .current_turn_start
                .take()
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);

            let (input_tokens, output_tokens, cost) = extract_usage(message);
            s.totals.input += input_tokens;
            s.totals.output += output_tokens;
            s.totals.total_cost += cost;
            s.totals.tool_calls += s.current_turn_tools.len() as u64;

            let tools = std::mem::take(&mut s.current_turn_tools);
            s.turns.push(TurnStats {
                duration,
                input_tokens,
                output_tokens,
                cost,
                tools,
            });
        }

        AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
            s.current_tool_starts
                .insert(tool_call_id.clone(), Instant::now());
        }

        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            ..
        } => {
            let duration = s
                .current_tool_starts
                .remove(tool_call_id)
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);
            s.current_turn_tools.push(ToolStats {
                name: tool_name.clone(),
                duration,
            });
        }

        AgentEvent::MessageUpdate {
            assistant_event, ..
        } => {
            if !s.first_token_captured {
                if let AssistantMessageEvent::TextDelta { .. } = assistant_event.as_ref() {
                    s.first_token_time = s.start_time.map(|t| t.elapsed()).or(Some(Duration::ZERO));
                    s.first_token_captured = true;
                }
            }
        }

        _ => {}
    }
}

/// Extract (input_tokens, output_tokens, total_cost) from an AgentMessage.
fn extract_usage(msg: &AgentMessage) -> (u64, u64, f64) {
    if let AgentMessage::Llm(Message::Assistant(am)) = msg {
        let usage = &am.usage;
        let cost = usage.cost.total;
        (usage.input, usage.output, cost)
    } else {
        (0, 0, 0.0)
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 10.0 {
        format!("{:.1}s", secs)
    } else {
        format!("{:.2}s", secs)
    }
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000 {
        format!("{},{:03}", n / 1_000, n % 1_000)
    } else {
        n.to_string()
    }
}

fn build_summary(s: &StatsInner) -> String {
    let total = s
        .total_duration
        .unwrap_or(s.start_time.map(|t| t.elapsed()).unwrap_or(Duration::ZERO));

    let ttft = s
        .first_token_time
        .map(|d| format!(" (TTFT: {})", fmt_duration(d)))
        .unwrap_or_default();

    let mut out = format!(
        "=== Agent Statistics ===\nTotal time: {}{}\nTurns: {}\n",
        fmt_duration(total),
        ttft,
        s.turns.len()
    );

    for (i, turn) in s.turns.iter().enumerate() {
        let tools_str = if turn.tools.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = turn
                .tools
                .iter()
                .map(|t| format!("{}({})", t.name, fmt_duration(t.duration)))
                .collect();
            format!(" | tools: {}", parts.join(", "))
        };
        out.push_str(&format!(
            "  Turn {}: {} | {} in, {} out | ${:.3}{}\n",
            i + 1,
            fmt_duration(turn.duration),
            fmt_tokens(turn.input_tokens),
            fmt_tokens(turn.output_tokens),
            turn.cost,
            tools_str,
        ));
    }

    let cache_str = if s.totals.cache_read > 0 {
        format!(" ({} cached)", fmt_tokens(s.totals.cache_read))
    } else {
        String::new()
    };

    out.push_str(&format!(
        "Totals: {} in, {} out{} | ${:.3}\nTool calls: {}",
        fmt_tokens(s.totals.input),
        fmt_tokens(s.totals.output),
        cache_str,
        s.totals.total_cost,
        s.totals.tool_calls,
    ));

    out
}

fn build_json(s: &StatsInner) -> Value {
    let total_secs = s
        .total_duration
        .unwrap_or(s.start_time.map(|t| t.elapsed()).unwrap_or(Duration::ZERO))
        .as_secs_f64();

    let turns: Vec<Value> = s
        .turns
        .iter()
        .map(|t| {
            let tools: Vec<Value> = t
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "duration_secs": tool.duration.as_secs_f64(),
                    })
                })
                .collect();
            json!({
                "duration_secs": t.duration.as_secs_f64(),
                "input_tokens": t.input_tokens,
                "output_tokens": t.output_tokens,
                "cost": t.cost,
                "tools": tools,
            })
        })
        .collect();

    json!({
        "total_duration": total_secs,
        "ttft_secs": s.first_token_time.map(|d| d.as_secs_f64()),
        "turns": turns,
        "totals": {
            "input_tokens": s.totals.input,
            "output_tokens": s.totals.output,
            "cache_read_tokens": s.totals.cache_read,
            "cache_write_tokens": s.totals.cache_write,
            "total_cost": s.totals.total_cost,
            "tool_calls": s.totals.tool_calls,
        },
    })
}
