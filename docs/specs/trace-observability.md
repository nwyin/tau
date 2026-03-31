# Trace Observability: Full-Content JSONL Event Stream

## Context

A `TraceSubscriber` already exists (`coding-agent/src/trace.rs`) that writes
`trace.jsonl` + `run.json` for benchmarking. It handles agent lifecycle,
turn, tool, and thread events. But it's opt-in (benchmarking only), truncates
content to 100 chars, and is missing orchestration-level events (document ops,
episode injection, evidence citations, query lifecycle, thinking content,
context compaction).

This spec extends the trace system into an always-on, full-content
observability layer for studying routing behavior.

## Requirements

### Phase 1: Extend AgentEvent with Missing Variants

Add new variants to `AgentEvent` in `agent/src/types.rs`:

1. `DocumentOp { thread_alias: Option<String>, op: String, name: String, content: String }` — emitted by DocumentTool on every read/write/append/list operation. `content` is the full document content (for write/append) or the read result.

2. `EpisodeInject { source_aliases: Vec<String>, target_alias: String, target_thread_id: String }` — emitted by ThreadTool when prior episodes are injected into a new thread's system prompt.

3. `EvidenceCite { thread_alias: String, thread_id: String, tool_call_ids: Vec<String> }` — emitted when a thread calls `complete` with evidence. Sourced from the completion tool's outcome signal.

4. `QueryStart { query_id: String, prompt: String, model: String }` — emitted by QueryTool before the LLM call.

5. `QueryEnd { query_id: String, output: String, duration_ms: u64 }` — emitted by QueryTool after the LLM call.

6. `ContextCompact { thread_alias: Option<String>, before_tokens: u64, after_tokens: u64, strategy: String }` — emitted when context compaction runs.

Each tool that emits these events needs access to the agent's event emitter. Use the existing `agent.subscribe()` pattern — the tools emit events by calling a shared emitter function (same pattern as `EventForwarderCell` in ThreadTool).

### Phase 2: TraceSubscriber Changes

Modify `coding-agent/src/trace.rs`:

1. **Full content mode**: Remove the 100-char truncation in `extract_result_summary`. Write full tool args and full result content. The `tool_start` event already includes full `args`; `tool_end` should include full result text (not just first 100 chars).

2. **Handle new event variants**: Add match arms for all 6 new events. Each writes a JSONL line with:
   - `ts`: RFC3339 timestamp
   - `event`: event type name (e.g., `"document_op"`, `"episode_inject"`)
   - All fields from the event variant
   - `thread_alias` and `thread_id` where applicable

3. **Thread context on tool events**: Add `thread_alias: Option<String>` and `thread_id: Option<String>` to `tool_start` and `tool_end` trace events. The TraceSubscriber should track active threads (from `ThreadStart`/`ThreadEnd`) and tag tool events with the current thread context.

4. **Thinking content**: Extract thinking/reasoning content from `MessageUpdate` events when present. Write as `{"event": "thinking", "content": "...", "thread_alias": ...}`.

### Phase 3: Always-On Activation

1. **Trace directory**: `.tau/traces/<session_id>/` (separate from sessions). Contains `trace.jsonl` and `run.json`.

2. **Always-on**: Every session (interactive REPL, `--prompt`, serve mode) creates a TraceSubscriber automatically. No flag needed.

3. **Wire up in agent_builder.rs**: After agent creation, create TraceSubscriber and subscribe it. The trace_dir should be derived from the session ID.

### Phase 4: Orchestration Summary

Add an `OrchestrationSummary` generator that can be called after orchestration sequences:

1. **Data structure** (in `agent/src/orchestrator.rs`):
   ```rust
   pub struct OrchestrationSummary {
       pub status: String,           // "completed" | "partial" | "aborted"
       pub accomplishments: Vec<String>,
       pub blockers: Vec<String>,
       pub decisions: Vec<QueryDecision>,
       pub results: Vec<ThreadResult>,
   }
   ```

2. **Generation**: `OrchestratorState::summarize() -> OrchestrationSummary` — iterates the episode log, classifies by outcome, extracts accomplishments (completed threads' results, truncated to 200 chars) and blockers (aborted/escalated threads).

3. **Trace event**: Emit a `summary` event to the trace when summary is generated.

4. **Accessible to tools**: The summary should be retrievable by the py_repl via `tau.summary()` or by the main agent via a tool, so the LLM can use it for routing decisions.

## Verification

- `cargo build --workspace` compiles cleanly
- `cargo test --workspace` — all existing tests pass
- `cargo test --test trace_subscriber` — existing trace tests still pass
- New tests:
  - `test_document_op_events` — DocumentTool emits document_op events that appear in trace.jsonl
  - `test_episode_inject_events` — ThreadTool with episodes param emits episode_inject event
  - `test_evidence_cite_events` — complete with evidence emits evidence_cite event
  - `test_full_content_not_truncated` — tool results are not truncated in trace output
  - `test_thread_context_on_tool_events` — tool events within threads have thread_alias/thread_id
  - `test_orchestration_summary` — summary generation from episode log

## Success Criteria

- All verification commands pass
- Running any tau session produces a `.tau/traces/<session_id>/trace.jsonl`
- `jq '.event' trace.jsonl | sort | uniq -c` shows all event types
- `jq 'select(.event == "document_op")' trace.jsonl` returns document operations with full content
- `jq 'select(.thread_alias != null)' trace.jsonl` shows all thread-scoped events
- `rg "episode_inject" trace.jsonl` finds episode routing events
- `jq 'select(.event == "evidence_cite")' trace.jsonl` shows evidence citations when threads use them
- Orchestration summary is generatable from the episode log
