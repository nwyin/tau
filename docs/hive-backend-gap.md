# Gap Analysis: tau as a Hive Backend

What tau needs to implement the `HiveBackend` interface and serve as an
orchestrator-driven coding agent.

## Context

Hive's `HiveBackend` (defined in `hive/src/hive/backends/base.py`) requires two
capabilities:

1. **Session management** — create, message, abort, delete, query status, get
   messages, handle permissions.
2. **Event streaming** — emit `session.status` events so the orchestrator can
   detect idle/busy transitions without polling.

The existing backends (ClaudeWSBackend, CodexAppServerBackend) both spawn CLI
processes and bridge their protocols into this interface. A tau backend would
replace the CLI process with tau's own agent loop, running in-process or as a
subprocess that Hive controls.

---

## 1. Server / RPC layer (not started)

**What Hive needs:** A way to create sessions, send messages, query status, and
receive events — either over WebSocket (like ClaudeWSBackend), stdio JSON-RPC
(like CodexAppServerBackend), or HTTP.

**What tau has:** Nothing. tau is a library + CLI binary. There is no server,
no RPC protocol, no WebSocket endpoint, no stdio JSON-RPC handler.

**Work required:**

Build a server mode for tau. Two viable approaches:

- **WebSocket server** (like ClaudeWSBackend): tau listens on a port, Hive
  connects and speaks a message protocol. Hive would spawn `tau --serve
  ws://127.0.0.1:<port>/<session_id>` or tau could host its own multi-session
  WS server.
- **Stdio JSON-RPC** (like CodexAppServerBackend): tau reads JSON lines on
  stdin, writes responses/notifications on stdout. Hive spawns `tau app-server
  --listen stdio://` and drives it. This is simpler to implement and test.

Recommendation: start with stdio JSON-RPC. It avoids port management, works
naturally with process lifecycle, and matches the pattern Hive already uses for
Codex. The protocol needs:

- Requests: `session/create`, `session/send`, `session/status`,
  `session/abort`, `session/delete`, `session/list`, `session/messages`
- Notifications (tau → Hive): `session.status` (idle/busy), `token_usage`

**Estimated scope:** New `tau-server` crate or `--serve` mode in coding-agent.
~800-1200 lines for the transport + protocol layer.

---

## 2. Multi-session support (not started)

**What Hive needs:** Run N concurrent agent sessions in one process (or spawn N
processes). Each session has its own model, system prompt, tools, working
directory, and message history. Sessions are identified by ID and independently
controllable.

**What tau has:** The `Agent` struct supports exactly one session at a time.
`coding-agent/src/main.rs` creates a single `Agent`, runs it, and exits. There
is no session registry, no multi-session management, no way to create a second
agent in the same process.

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

Recommendation: for the first cut, use one process per session (Hive spawns
`tau --serve-session <id> --cwd <worktree>`). This sidesteps per-tool cwd
scoping entirely. Multi-session-per-process can come later.

---

## 3. Session status detection (not started)

**What Hive needs:** Know whether a session is `idle` (waiting for input) or
`busy` (processing). The backend must emit `session.status` events when
transitions happen, and support `get_session_status()` polling as a fallback.

Status values: `idle`, `busy`, `error`, `not_found`.

**What tau has:** The `Agent` struct has `is_streaming: bool` in its state, and
emits `AgentStart`/`AgentEnd` events. But there is no externally-queryable
status API, no status event emission in a format Hive can consume, and no
mapping from agent lifecycle to idle/busy.

**Work required:**

- Map `AgentStart` → busy, `AgentEnd` → idle.
- Expose `get_status() -> SessionStatus` on whatever RPC/protocol layer is
  built.
- Emit status change notifications proactively (not just on poll).

**Estimated scope:** Small — ~100 lines, mostly wiring existing events to the
protocol layer.

---

## 4. External message injection (partially supported)

**What Hive needs:** `send_message_async(session_id, parts, model, system,
directory)` — send a user message to an existing session. Fire-and-forget (the
session processes it asynchronously). The first message also carries the system
prompt.

**What tau has:** `Agent::prompt(input)` and `Agent::follow_up(msg)`. These
exist but are synchronous from the caller's perspective (`prompt()` blocks
until the agent loop completes). There's no async "fire and forget" message
injection from an external caller.

**Work required:**

- Wrap `Agent::prompt()` in a tokio task so it runs in the background.
- Expose a "send message" RPC endpoint that pushes a message to the agent and
  returns immediately.
- Handle the system prompt: on first message, set `agent.set_system_prompt()`
  before calling `prompt()`.

**Estimated scope:** ~150 lines.

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

## 7. Per-session working directory scoping (not started)

**What Hive needs:** Each worker session operates in its own git worktree.
Tools must read/write/execute relative to that directory, not the daemon's cwd.

**What tau has:** All tools operate on the process's cwd. BashTool runs `sh -c`
in the current directory. FileReadTool/FileWriteTool use absolute paths but
don't enforce a root. There is no concept of a per-session working directory.

**Work required:**

If using one-process-per-session: just `cd` to the worktree before starting.
Trivial.

If using multi-session-per-process:
- BashTool: pass `cwd` to `Command::new().current_dir(session_cwd)`
- FileReadTool/FileWriteTool/FileEditTool: resolve paths relative to session
  cwd, prevent path traversal outside the worktree
- GlobTool/GrepTool: scope searches to session cwd

**Estimated scope:** ~200 lines for the multi-session approach. Free for
one-process-per-session.

---

## 8. Model routing per session (supported)

**What Hive needs:** Different sessions can use different models. The
orchestrator passes `model` when sending messages.

**What tau has:** `Agent::set_model()` exists. The model can be changed at any
time. The model catalog covers Anthropic, OpenAI, and Kimi models.

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

## 10. Process lifecycle integration (not started)

**What Hive needs:** The backend manages the tau process. Hive spawns it,
monitors it, and kills it on cleanup. ClaudeWSBackend uses process groups
(SIGTERM → SIGKILL). CodexAppServerBackend uses stdin/stdout lifecycle.

**What tau has:** tau is a normal CLI binary. No daemon mode, no graceful
shutdown protocol, no health check endpoint.

**Work required:**

- Graceful shutdown: handle SIGTERM, drain active sessions, exit cleanly.
- Startup handshake: signal readiness to the parent process (e.g., print a
  ready line on stdout, or respond to an `initialize` RPC).
- Health: respond to health check queries (or just: if the process is alive and
  responding to RPC, it's healthy).

**Estimated scope:** ~150 lines.

---

## Summary: priority order

| # | Feature | Effort | Needed for MVP |
|---|---------|--------|----------------|
| 1 | Server/RPC layer (stdio JSON-RPC) | Large | Yes |
| 2 | Multi-session or one-process-per-session | Medium | Yes (one-per-process is simpler) |
| 3 | Session status detection | Small | Yes |
| 4 | External message injection | Small | Yes |
| 5 | Token usage reporting | Small | Yes |
| 6 | Permission system | None (MVP) | No |
| 7 | Per-session cwd scoping | None (if 1-per-process) | Depends on approach |
| 8 | Model routing per session | Trivial | Yes |
| 9 | Session lifecycle management | Medium | Yes |
| 10 | Process lifecycle integration | Small | Yes |

### Recommended approach

**Phase 1 — One process per session (simplest viable backend):**

Hive spawns `tau --headless --cwd <worktree> --model <model>` as a subprocess.
Hive writes a prompt to tau's stdin. tau runs the agent loop and exits. Hive
monitors the process: running = busy, exited = idle/done.

This requires almost no tau changes — just reliable exit codes and maybe a
result file protocol (like `.hive-result.jsonl`). The Hive-side backend adapter
is ~300 lines of Python wrapping subprocess management.

Downside: no mid-session message injection, no status events, no abort (only
SIGTERM). Equivalent to how Hive's Claude backend works at the most basic level.

**Phase 2 — Stdio JSON-RPC server:**

Add `tau serve` that speaks JSON-RPC over stdin/stdout. One long-lived process,
multiple sessions. Full lifecycle control, status events, token reporting. This
is the real integration that unlocks feature parity with ClaudeWSBackend.

**Phase 3 — Production hardening:**

Permission system, sandboxing, connection pooling, health monitoring, graceful
degradation.
