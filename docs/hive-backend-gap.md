# Gap Analysis: tau as a Hive Backend

Current state and remaining gaps for tau as a Hive backend.

## Context

Hive's `HiveBackend` needs two broad capabilities:

1. **Session management** — create, message, abort, delete, query status, and inspect history.
2. **Event streaming** — emit enough status information for the orchestrator to avoid blind polling.

This document used to assume tau had no backend surface at all. That is no longer true: `tau serve` now exists as a stdio JSON-RPC backend for a single session/process.

---

## 1. Server / RPC layer (landed for the single-session case)

**What Hive needs:** A way to create sessions, send messages, query status, and
receive events — either over WebSocket (like ClaudeWSBackend), stdio JSON-RPC
(like CodexAppServerBackend), or HTTP.

**What tau has:** `tau serve` in [coding-agent/src/serve.rs](../coding-agent/src/serve.rs), plus JSON-RPC transport and handlers under [coding-agent/src/rpc](../coding-agent/src/rpc). One process hosts one session and speaks stdio JSON-RPC.

**Work required:**

**Remaining work:** Validate that the current request/notification surface matches what Hive wants, document the protocol, and decide whether one-process-per-session is sufficient long-term.

---

## 2. Multi-session support (still not started, but maybe unnecessary)

**What Hive needs:** Run N concurrent agent sessions in one process (or spawn N
processes). Each session has its own model, system prompt, tools, working
directory, and message history. Sessions are identified by ID and independently
controllable.

**What tau has:** one session per `tau serve` process. This is enough if Hive is happy to spawn one process per worktree/session.

**Work required:**

- **Session registry**: A `HashMap<SessionId, Agent>` that tracks active
  sessions. Each session owns an `Agent` instance with its own state, model,
  tools, and system prompt.
- **Per-session working directory**: Tools (BashTool, FileReadTool, etc.)
  currently operate on the process's cwd. They need to be scoped to a
  per-session directory (the git worktree Hive creates). This means either:
  - Passing a `cwd` through tool execution context, or
  - Spawning each session as a separate OS process (simpler but heavier)
- **Concurrent execution**: Multiple `Agent::prompt()` calls must run
  concurrently via tokio tasks.

Recommendation remains the same: keep one process per session unless process overhead becomes a demonstrated problem.

---

## 3. Session status detection (partially landed)

**What Hive needs:** Know whether a session is `idle` (waiting for input) or
`busy` (processing). The backend must emit `session.status` events when
transitions happen, and support `get_session_status()` polling as a fallback.

Status values: `idle`, `busy`, `error`, `not_found`.

**What tau has:** the serve path already tracks session status and wires agent lifecycle into the RPC layer. This area should be treated as integration validation, not greenfield work.

**Work required:**

- Map `AgentStart` → busy, `AgentEnd` → idle.
- Expose `get_status() -> SessionStatus` on whatever RPC/protocol layer is
  built.
- Emit status change notifications proactively (not just on poll).

**Remaining work:** verify Hive sees enough detail for busy/idle/error transitions without polling hacks.

---

## 4. External message injection (partially landed)

**What Hive needs:** `send_message_async(session_id, parts, model, system,
directory)` — send a user message to an existing session. Fire-and-forget (the
session processes it asynchronously). The first message also carries the system
prompt.

**What tau has:** the serve path wraps agent execution behind JSON-RPC handlers, so external callers can inject messages without manually embedding tau as a library.

**Work required:**

- Wrap `Agent::prompt()` in a tokio task so it runs in the background.
- Expose a "send message" RPC endpoint that pushes a message to the agent and
  returns immediately.
- Handle the system prompt: on first message, set `agent.set_system_prompt()`
  before calling `prompt()`.

**Remaining work:** confirm that the exact send/abort/status semantics line up with Hive's expectations.

---

## 5. Token usage reporting (partially supported)

**What Hive needs:** Token counts per message (input_tokens, output_tokens) for
cost tracking and per-issue/per-run budgets. The backend provides this via
message metadata or dedicated events.

**What tau has:** `AgentStats` collects input/output/cache tokens and cost per
turn via event subscription. The raw data is there, but it's only available as
a summary at the end of a run (via `--stats` / `--stats-json`). There's no
per-message token reporting during execution.

**Work required:**

- Emit token usage data after each LLM response (per-turn, not just aggregate).
- Include token counts in the `session.status` idle event or as a separate
  `token_usage` notification.
- Expose cumulative usage via `get_messages()` metadata.

**Estimated scope:** ~200 lines. The data is already collected in `AgentStats`;
this is about surfacing it through the protocol.

---

## 6. Permission / tool approval system (not started)

**What Hive needs:** Optional. ClaudeWSBackend runs with `bypassPermissions`
and auto-allows all tool calls. CodexAppServerBackend auto-accepts approval
requests. Hive's `base.py` has `get_pending_permissions()` and
`reply_permission()` but they default to no-ops.

**What tau has:** No permission system at all. Tools execute unconditionally.

**Work required for MVP:** Nothing — if tau runs with full permissions (like
ClaudeWSBackend does), the permission methods can be no-ops. This is acceptable
for a first integration.

**Work required for production:** A tool execution hook that can intercept,
queue, and gate tool calls pending external approval. This is a significant
feature (~500+ lines) but not needed for initial integration.

---

## 7. Per-session working directory scoping (handled by the current one-process model)

**What Hive needs:** Each worker session operates in its own git worktree.
Tools must read/write/execute relative to that directory, not the daemon's cwd.

**What tau has:** `tau serve --cwd <worktree>` changes the process cwd up front, which is adequate for one process per session.

**Work required:**

If tau ever grows multi-session-per-process, tool cwd scoping becomes real work:
- BashTool: pass `cwd` to `Command::new().current_dir(session_cwd)`
- FileReadTool/FileWriteTool/FileEditTool: resolve paths relative to session
  cwd, prevent path traversal outside the worktree
- GlobTool/GrepTool: scope searches to session cwd

---

## 8. Model routing per session (supported)

**What Hive needs:** Different sessions can use different models. The
orchestrator passes `model` when sending messages.

**What tau has:** `Agent::set_model()` exists. The model can be changed at any time. The model catalog covers direct Anthropic/OpenAI plus OpenRouter-backed families.

**Work required:** Wire the `model` parameter from the RPC `send_message` call
to `agent.set_model()`. Minimal.

---

## 9. Session lifecycle management (partially supported)

**What Hive needs:**
- `create_session(directory, title, permissions)` → `{id: ...}`
- `abort_session(session_id)` → interrupt a running session
- `delete_session(session_id)` → kill and clean up
- `cleanup_session(session_id)` → abort + delete (best-effort)
- `list_sessions()` → enumerate active sessions

**What tau has:**
- `Agent::new()` creates an agent (no ID assignment, no registry)
- `Agent::abort()` cancels via `CancellationToken` (works)
- No delete/cleanup/list functionality
- Session IDs exist in `SessionManager` but are for JSONL persistence, not
  runtime lifecycle

**Work required:**

- Session ID generation and registry
- `list_sessions()` — iterate the registry
- `delete_session()` — abort + remove from registry + clean up resources
- `cleanup_session()` — abort + delete with error suppression

**Estimated scope:** ~200 lines for the registry + lifecycle methods.

---

## 10. Process lifecycle integration (partially landed)

**What Hive needs:** The backend manages the tau process. Hive spawns it,
monitors it, and kills it on cleanup. ClaudeWSBackend uses process groups
(SIGTERM → SIGKILL). CodexAppServerBackend uses stdin/stdout lifecycle.

**What tau has:** `tau serve` handles Ctrl-C/shutdown, drains the active agent task, and exits explicitly once stdin closes.

**Work required:**

- Graceful shutdown: handle SIGTERM, drain active sessions, exit cleanly.
- Startup handshake: signal readiness to the parent process (e.g., print a
  ready line on stdout, or respond to an `initialize` RPC).
- Health: respond to health check queries (or just: if the process is alive and
  responding to RPC, it's healthy).

**Remaining work:** standardize a readiness/initialize handshake if Hive needs one, and document process lifecycle assumptions.

---

## Summary: priority order

| # | Feature | Effort | Needed for MVP |
|---|---------|--------|----------------|
| 1 | Validate and document current RPC layer | Medium | Yes |
| 2 | Keep one-process-per-session unless proven insufficient | Small | Yes |
| 3 | Confirm status/event semantics with Hive | Small | Yes |
| 4 | Confirm message injection semantics with Hive | Small | Yes |
| 5 | Token usage reporting | Small | Yes |
| 6 | Permission system | None (MVP) | No |
| 7 | Per-session cwd scoping beyond the current process model | Medium | No |
| 8 | Model routing per session | Trivial | Yes |
| 9 | Session lifecycle management | Medium | Yes |
| 10 | Process lifecycle/handshake polish | Small | Yes |

### Recommended approach

**Phase 1 — Treat current `tau serve` as the integration target:**

Hive spawns `tau serve --cwd <worktree>` as a subprocess and drives JSON-RPC over stdio.

**Phase 2 — Close the remaining gaps:**

Status semantics, token usage notifications, lifecycle cleanup, and protocol docs.

**Phase 3 — Production hardening:**

Permission system, sandboxing, health monitoring, and only then multi-session-per-process if it materially helps.
