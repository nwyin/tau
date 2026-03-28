//! Thread and episode types for orchestration.

use serde::{Deserialize, Serialize};

/// Unique identifier for a thread instance.
pub type ThreadId = String;

/// How a thread terminated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ThreadOutcome {
    /// Thread called complete(result).
    #[serde(rename = "completed")]
    Completed {
        result: String,
        #[serde(default)]
        evidence: Vec<String>,
    },
    /// Thread called abort(reason).
    #[serde(rename = "aborted")]
    Aborted { reason: String },
    /// Thread called escalate(problem).
    #[serde(rename = "escalated")]
    Escalated { problem: String },
    /// Thread hit max_turns, timeout, or was cancelled externally.
    #[serde(rename = "timed_out")]
    TimedOut,
}

impl ThreadOutcome {
    pub fn status_str(&self) -> &str {
        match self {
            ThreadOutcome::Completed { .. } => "completed",
            ThreadOutcome::Aborted { .. } => "aborted",
            ThreadOutcome::Escalated { .. } => "escalated",
            ThreadOutcome::TimedOut => "timed_out",
        }
    }

    pub fn result_text(&self) -> &str {
        match self {
            ThreadOutcome::Completed { result, .. } => result,
            ThreadOutcome::Aborted { reason } => reason,
            ThreadOutcome::Escalated { problem } => problem,
            ThreadOutcome::TimedOut => "(timed out)",
        }
    }
}

/// Compressed representation of a completed thread action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub thread_id: ThreadId,
    pub alias: String,
    pub task: String,
    pub outcome: ThreadOutcome,
    /// Complete formatted transcript for the orchestrator.
    pub full_trace: String,
    /// Compressed one-liner-per-action transcript for downstream thread injection.
    pub compact_trace: String,
    pub duration_ms: u64,
    pub turn_count: u32,
}
