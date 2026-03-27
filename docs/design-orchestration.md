# Orchestration & Code Execution Design

**Status:** brainstorm / rough sketch
**Date:** 2025-03-27

This document captures everything from the initial design exploration around
adding Slate-style thread orchestration and Codex-style REPL execution to tau.
Not everything here is immediately actionable — some of it is just interesting
threads worth preserving.

---

## 1. Source Material

### Slate's Thread Weaving (from binary decompilation)

Slate's most novel feature: the LLM generates JavaScript that is compiled via
`Function()` and executed with a `system` object providing orchestration
primitives. The LLM becomes a _programmer of other LLMs_.

**The `system` object:**

| Primitive  | Signature                             | What it does                                                                |
| ---------- | ------------------------------------- | --------------------------------------------------------------------------- |
| `thread`   | `(id, task, capabilities?, options?)` | Spawn a full agent session with tools. Named `id` enables reuse.            |
| `query`    | `(prompt, options?)`                  | Single-shot LLM call, no tools. Optional JSON schema for structured output. |
| `allocate` | `(name)`                              | Create a named virtual document for inter-thread data sharing.              |
| `log`      | `(message, data?)`                    | Record a progress/decision note in the orchestration transcript.            |
| `fromId`   | `(alias)`                             | Retrieve a prior thread/query result by name.                               |

**Key design properties:**

- **Full async JS control flow.** `Promise.all()` for parallelism, try/catch
  for error handling, loops, conditionals, string interpolation of prior results
  into new prompts. Arbitrary code, not a fixed DAG.
- **Thread reuse.** Calling `system.thread("my-agent", newTask)` with an
  existing alias appends to that thread's conversation history. The thread
  re-runs with full prior context + the new message. Lightweight agent memory.
- **Episodic context routing.** Each thread/query produces a trace (transcript).
  Traces can be passed to other threads via `options.traces`, injected as
  `# Prior episodes` in the system message. Two compression levels: full trace
  for the orchestration record, compact trace for downstream context.
- **Document store.** `system.allocate("findings")` creates a virtual document.
  Threads read/write these for shared mutable state without touching the real
  filesystem.
- **Evidence provenance.** Threads mark specific tool call IDs as "evidence"
  for their conclusions, annotated in traces for downstream consumers.
- **Completion signaling.** Threads terminate by calling `complete(result)`,
  `abort(reason)`, or `escalate(problem)`. Each carries optional evidence.
- **Rolling compression.** Long conversations get periodically compressed via a
  dedicated compression model (`behaviorMode: "compression"`).
- **Three model slots.** `main` (orchestrator), `search` (fast/cheap), `execute`
  (sub-agent). Different models for different roles.

**The JS advantage:** Top-level `await` is trivial. Slate wraps code in an async
IIFE:

```javascript
const fn = Function("system", "return (async () => { " + code + " })()");
```

No AST transforms, no compile flags. Every cell is naturally async.

### Codex's `js_repl` (from spec)

A persistent Node-backed kernel where the LLM writes JavaScript with top-level
`await`. Key properties:

- **Persistent state.** Top-level bindings survive across cells. Build up
  context incrementally across multiple REPL calls.
- **Tool callbacks.** `codex.tool(name, args)` calls back into the harness's
  normal tool system from inside the REPL. The kernel is a _superset_ of
  normal tool use.
- **Feature gate.** `js_repl = true` in config. `js_repl_tools_only = true`
  forces all tool use through the REPL — the LLM must write code to do anything.
- **JSON-line transport.** Bidirectional over stdin/stdout. Kernel sends reverse
  RPC requests for `codex.tool()`, host responds with results.
- **Reset.** `js_repl_reset` clears kernel state.
- **Helper globals.** `codex.cwd`, `codex.homeDir`, `codex.tmpDir`,
  `codex.emitImage()`.
- **No external deps.** Kernel ships embedded. `import()` resolves from
  configurable module paths.
- **Careful binding semantics.** Extensive spec around what persists after a
  cell throws. Hoisted `var`/`function` vs lexical `let`/`const`, etc.

### tau's Current Architecture

Three-crate Rust workspace:

```
ai/          — LLM streaming, provider registry (Anthropic, OpenAI, etc.)
agent/       — Generic agent loop, AgentTool trait, event system, context compaction
coding-agent/ — Specific harness: 14 tools, CLI/REPL, JSON-RPC serve mode, sessions
```

**What exists that's relevant:**

- Agent loop is already generic and async (`tokio`). Different harnesses can be
  built on the `agent` crate.
- SubagentTool exists but spawns a subprocess (`tau -p "task" --yolo`). No
  shared state, no trace access, ~100ms+ startup overhead.
- JSON-RPC serve mode exists (for Hive orchestrator). Stateful, stdio-based.
- Event system with full lifecycle hooks (turn start/end, tool execution, etc.).
- Session persistence via JSONL.
- Context compaction: 3-tier (truncate large outputs, mask old turns, aggressive
  head+tail).
- Provider abstraction with ~65 models across Anthropic, OpenAI, OpenRouter, etc.

---

## 2. The Synthesis: `py_repl` + Orchestration

The core idea: a Python REPL tool that unifies Codex-style computation with
Slate-style orchestration. The LLM writes Python that can both _compute_ (data
processing, parsing, filtering) and _orchestrate_ (spawn threads, run queries,
share documents).

### The `tau` object

```python
# === Computation (Codex-style) ===
tau.cwd                                          # working directory
tau.home_dir                                     # home directory
tau.tmp_dir                                      # per-session scratch dir
await tau.tool("grep", {"pattern": "TODO"})      # call any tau tool
await tau.tool("bash", {"command": "cargo test"})

# === Orchestration (Slate-style) ===
await tau.thread("scanner", "Find auth endpoints", ["read", "grep"])
await tau.query("What framework is this?")
tau.allocate("shared-findings")
tau.log("Starting phase 2")
tau.from_id("scanner")
```

### Why this is interesting

1. **No LLM round-trips for glue logic.** Filtering results, formatting prompts,
   conditional branching — all happen in Python, instantly. Multi-turn tool
   calling wastes an LLM call every time the agent needs to think between steps.

2. **Persistent kernel = incremental build-up.** The LLM doesn't generate one
   monolithic orchestration script. It can explore in cell 1, orchestrate in
   cell 2, process results in cell 3. State accumulates naturally in Python
   variables.

3. **`tau.tool()` = superset of normal tools.** Every tau tool is accessible
   from inside the REPL. `py_repl_tools_only` mode forces everything through
   code, which is a fundamentally different interaction model.

4. **Python's stdlib is free.** `json`, `re`, `statistics`, `pathlib`,
   `collections` — all available for data processing between orchestration
   steps. No need for the LLM to reinvent string parsing.

5. **Natural for ML/research workflows.** tau's thesis is co-training. An
   orchestration script that can `import json`, parse experiment results,
   compute metrics, then spawn follow-up threads based on the numbers — that's
   Python's sweet spot.

---

## 3. The Top-Level Await Problem

This is the one real knot.

**JavaScript:** Top-level `await` is a first-class language feature. Wrapping
in an async IIFE is trivial and correct. Codex and Slate both use this.

**Python:** Top-level `await` is not valid in `exec()`. Workarounds:

| Approach                                   | Complexity        | Issues                                                                                      |
| ------------------------------------------ | ----------------- | ------------------------------------------------------------------------------------------- |
| `ast.PyCF_ALLOW_TOP_LEVEL_AWAIT` (3.10+)   | Low               | Documented as "for REPLs", returns coroutine you manually run, weird namespace interactions |
| AST transform (IPython-style)              | High (~500 lines) | Handles yield, return, class defs, decorators... fragile                                    |
| Require `async def main(tau): ...`         | Trivial           | Verbose, LLM will forget, ugly                                                              |
| Make `tau.*` calls blocking (hide asyncio) | Medium            | Simpler kernel but loses `asyncio.gather()` for parallelism                                 |

### Possible resolution: hybrid

The kernel runs an asyncio event loop. `tau.tool()` and `tau.thread()` are
blocking from the LLM's perspective (they use `asyncio.run_coroutine_threadsafe`
or similar internally), but `tau.parallel()` provides an explicit fan-out
primitive:

```python
# These are blocking — simple, no await needed
result = tau.tool("grep", {"pattern": "TODO"})
analysis = tau.query("What framework is this?")

# Explicit parallel primitive — the one place concurrency surfaces
scanner, impl = tau.parallel(
    tau.Thread("scanner", "Find auth endpoints", ["read", "grep"]),
    tau.Thread("impl", f"Implement login using {analysis.output}", ["read", "write"]),
)

# Back to simple blocking calls
if "flask" in analysis.output.lower():
    tau.thread("impl", "Add CSRF protection", ["read", "write"], traces=[scanner])
```

This sidesteps the top-level await problem entirely. The Python code looks
synchronous. Parallelism is explicit via `tau.parallel()` which takes
unevaluated thread/query specs and runs them concurrently under the hood.

**Tradeoff:** Less expressive than raw `asyncio.gather()`. You can't do
arbitrary async patterns. But 90% of orchestration is "fan out, collect, decide,
fan out again" — and `tau.parallel()` handles that cleanly.

### Alternative: JS for orchestration, Python for computation

Two tools instead of one:

- `py_repl` — persistent Python kernel, `tau.tool()` for callbacks, no async.
  Used for data processing, analysis, computation.
- `orchestrate` — one-shot JS DSL (like Slate), `system.thread()` etc. Used
  for multi-agent coordination.

The LLM picks the right tool for the job. Clean separation, each half is
simple. But the split is ugly — you lose the ability to mix computation and
orchestration in one script.

### Alternative: just use JS

Skip Python. Embed a lightweight JS runtime (QuickJS via rquickjs, or Boa).
Async IIFE works natively. Orchestration + computation in one tool. The
tradeoff is the LLM writes JS instead of Python, and you lose `import numpy`.

For orchestration scripts (which are mostly glue logic), JS is fine. The LLM
is good at both. And for tau's mini-harness identity, zero external runtime
deps is attractive.

But we're experimenting, not optimizing for purity.

---

## 4. Architecture Options

### Option A: Python subprocess + JSON-RPC (recommended starting point)

```
┌────────────────────────────────────────────────┐
│  tau (Rust)                                    │
│                                                │
│  ┌─────────────┐     ┌──────────────────────┐ │
│  │ py_repl tool │◄───►│ Python kernel        │ │
│  │              │ JSON│ (subprocess)          │ │
│  │  reverse RPC │ line│                       │ │
│  │  dispatcher  │     │ tau.tool() → RPC      │ │
│  └──────┬──────┘     │ tau.thread() → RPC    │ │
│         │            │ tau.query() → RPC     │ │
│         ▼            │ tau.parallel() → RPC  │ │
│  ┌──────────────┐    └──────────────────────┘ │
│  │ In-process   │                              │
│  │ agent loops  │  (tokio tasks for threads)   │
│  └──────────────┘                              │
└────────────────────────────────────────────────┘
```

- Zero new Rust dependencies
- Python kernel is ~200-300 lines, stdlib-only, embedded via `include_str!`
- Bidirectional JSON-line protocol (same pattern as Codex)
- `tau.thread()` reverse RPC → Rust spawns in-process agent loop → returns
- Blocking API from Python's perspective (kernel manages async internally)

### Option B: Embedded JS (QuickJS/Boa)

- Add `rquickjs` or `boa_engine` crate dependency
- Async IIFE for top-level await, clean and correct
- `system` object exposed as native JS bindings
- One-shot execution (like Slate) or persistent REPL (like Codex)
- No external runtime dependency, but adds ~2-5 MB to binary

### Option C: Hybrid (JS orchestration + Python computation)

- `orchestrate` tool: embedded JS, one-shot, Slate-style DSL
- `py_repl` tool: Python subprocess, persistent kernel, sync API
- LLM picks the right tool. Two simpler implementations.

### Option D: Node/Bun subprocess (full Codex-style)

- Spawn Node/Bun as the kernel process
- Closest to Codex's js_repl — most proven architecture
- Requires Node/Bun installed (or bundled)
- Top-level await works natively

---

## 5. Pieces to Build (regardless of which option)

### 5.1 In-Process Thread Spawning

The current SubagentTool spawns a subprocess. All orchestration paths need
in-process thread spawning via tokio tasks.

**Required changes:**

- `agent/src/loop_.rs`: Extract `run_to_completion()` — an agent loop variant
  that runs autonomously until a completion tool is called. The current loop
  is oriented around interactive turns; this variant runs headless with a
  termination condition.
- Completion tools: `complete(result, evidence?)`, `abort(reason, evidence?)`,
  `escalate(problem, evidence?)`. These signal thread termination.
- Tool filtering: threads get a restricted tool set based on `capabilities`
  (read, write, grep, terminal, websearch). No recursive orchestration (or
  maybe allow it with a depth limit?).

### 5.2 Orchestrator State

Shared mutable state for an orchestration run:

```rust
pub struct OrchestratorState {
    /// Completed threads/queries indexed by alias
    results: DashMap<String, SequenceEntry>,

    /// Virtual documents for inter-thread data sharing
    documents: DashMap<String, String>,

    /// Ordered log of everything that happened
    sequence: Mutex<Vec<SequenceEntry>>,

    /// For generating unique IDs
    counter: AtomicU64,
}

pub enum SequenceEntry {
    Thread { id: String, task: String, status: Status, trace: String,
             compact_trace: String, output: Option<String>, duration_ms: u64 },
    Query  { id: String, prompt: String, output: String, duration_ms: u64 },
    Log    { message: String, data: Option<Value> },
}

pub enum Status { Completed, Aborted, Escalated }
```

### 5.3 Trace Formatting

Two levels, following Slate's pattern:

- **Full trace:** Complete transcript — thinking blocks, text, tool calls with
  args and results, file diffs. Used in the orchestration summary returned to
  the main agent.
- **Compact trace:** Compressed. Tool calls as one-liners:
  `TOOL [id] >>> grep({"pattern":"TODO"}) => 14 matches`. Used when injecting
  prior episodes as context for downstream threads.

**Prior episode injection format:**

```
# Prior episodes

--- Query: analyze-framework ---
PROMPT: What framework does this project use?
OUTPUT: Flask 2.3 with SQLAlchemy

--- Thread: scanner [completed] ---
TASK: Find all auth endpoints
STATUS: completed
OUTPUT: Found 3 endpoints: /login, /logout, /oauth/callback
  [compact trace indented here]
```

### 5.4 The REPL Kernel (if going Python route)

Embedded Python script, ~200-300 lines:

- JSON-line protocol over stdin/stdout
- Persistent namespace across cells
- `tau` object with `tool()`, `thread()`, `query()`, `parallel()`,
  `allocate()`, `log()`, `from_id()`
- Blocking API (kernel manages its own event loop for concurrent threads)
- Stdout/stderr capture per cell
- Timeout support (per-cell, configurable)
- Reset command

### 5.5 The JS DSL (if going JS route)

Either embedded (QuickJS/Boa) or subprocess (Node/Bun):

- Async IIFE wrapper
- `system` object with `thread()`, `query()`, `allocate()`, `log()`, `fromId()`
- One-shot execution per `orchestrate` tool call
- Return value becomes orchestration output
- Exception propagation

### 5.6 System Prompt Additions

Threads need a modified system prompt:

- Identity: "You are a sub-agent working on a specific task."
- Prior episodes section (if traces provided)
- Document references (if docs provided)
- Restricted tool list (based on capabilities)
- Completion instructions: "Call `complete` with your result when done,
  `abort` if you can't proceed, `escalate` if you need human input."

The main agent needs:

- `py_repl` / `orchestrate` tool description with usage examples
- Guidance on when to use orchestration vs direct tool calls
- Examples of common patterns (fan-out/fan-in, iterative refinement, etc.)

### 5.7 Feature Gates

```toml
# .tau/config.toml

[features]
# Enable the REPL tool
py_repl = true        # or js_repl = true

# Force all tool use through the REPL
repl_tools_only = true

# Enable orchestration primitives (thread/query/parallel)
orchestration = true
```

Orchestration could be gated separately from the REPL — you might want
`tau.thread()` available in the REPL without forcing all tool use through it.

---

## 6. Interesting Threads (Not Yet Resolved)

### 6.1 Thread Reuse as Agent Memory

Slate's thread reuse (calling the same alias appends to existing conversation)
is a simple but powerful form of intra-orchestration memory. A thread can be
"consulted" multiple times, building up expertise:

```python
# First call: scanner learns the codebase
tau.thread("scanner", "Map all database models", ["read", "grep"])

# Second call: scanner already knows the models, now finds issues
tau.thread("scanner", "Which of these models have N+1 query risks?",
           ["read", "grep"])
```

This is worth preserving regardless of which language/architecture we pick.

### 6.2 Evidence System

Slate's evidence tracking (threads mark tool call IDs as supporting evidence
for conclusions) is interesting for:

- **Provenance.** The orchestrator can trace _why_ a thread concluded something.
- **Debugging.** When a thread's output is wrong, you can inspect the evidence.
- **Trust calibration.** Threads that cite more evidence might be more reliable.

Not clear yet how to surface this in tau's UI or whether it's worth the
complexity for a first pass.

### 6.3 Rolling Compression

Slate compresses long conversations via a dedicated model/behavior mode. tau
already has 3-tier context compaction, but it's mechanical (truncate, mask,
head+tail). A model-driven compression step could preserve semantic content
better.

Could be a separate feature from orchestration — useful for any long session.

### 6.4 Model Slots for Roles

Slate uses different models for different roles (main, search, execute). tau's
SubagentTool already supports `model` override. Worth generalizing:

```toml
[models]
main = "claude-sonnet-4-6"
thread = "claude-sonnet-4-6"     # or cheaper: "claude-haiku-4-5"
query = "claude-haiku-4-5"       # fast for single-shot decisions
compression = "claude-haiku-4-5" # for context compression
```

### 6.5 `repl_tools_only` as a Fundamentally Different Mode

Codex's `js_repl_tools_only` is interesting because it changes the entire
interaction model. Instead of the LLM calling tools directly, it writes code
that calls tools. This means:

- The LLM is always "thinking in code"
- Every action is programmatic and reproducible
- Tool calls have surrounding context (variable assignments, comments, control
  flow) that makes intent clearer
- The conversation becomes a growing program, not a series of isolated tool
  calls

This might be worth exploring as a first-class mode for tau, not just a feature
flag. A "code-first" agent that always works through a REPL.

### 6.6 Orchestration Transcript as Artifact

Slate generates a full `# Orchestration Transcript` markdown document
summarizing everything that happened: sequence of threads/queries, their
results, decisions made, timing. This is returned to the main agent as the
`orchestrate` tool result.

This transcript is useful for:

- The main agent to understand what happened and decide next steps
- The user to review what the orchestration did
- Debugging orchestration scripts
- Potentially: feeding into session history for future reference

### 6.7 Recursive Orchestration

Can a thread spawn its own orchestration? Slate doesn't seem to allow this
(subagent permission restrictions). But it's an interesting question — a
thread that encounters a complex sub-problem could decompose it further.

Probably needs a depth limit to prevent runaway recursion. Or just disallow
it for the first pass.

### 6.8 The Serve Mode Connection

tau already has a JSON-RPC serve mode for Hive. The py_repl's reverse RPC
pattern is structurally similar. There might be a unification:

- tau as a "tool server" that can be driven by any client (Hive, py_repl
  kernel, external scripts)
- Standard JSON-RPC interface for `tool.call`, `thread.spawn`, `query`, etc.
- The py_repl kernel is just another client

### 6.9 Interactive vs Autonomous Orchestration

Two modes for how orchestration interacts with the user:

- **Autonomous:** The orchestration runs to completion, user sees the final
  transcript. Fast, no interruption, but opaque.
- **Interactive:** Thread events stream to the TUI. User can see sub-agents
  working, potentially intervene (cancel a thread, redirect, provide input).

tau's event system already supports the interactive path. The question is
whether the py_repl tool should emit events for thread activity or stay opaque
until completion.

---

## 7. Rough Implementation Ordering

If we were to start building:

1. **In-process thread spawning + completion tools** — the foundation.
   Refactor agent loop, add `run_to_completion()`, add complete/abort/escalate
   tools. Test by calling directly from Rust.

2. **Orchestrator state + trace formatting** — the coordination layer.
   `OrchestratorState`, trace generators, prior episode injection. Test with
   hardcoded orchestration scripts.

3. **The kernel** — whichever language/architecture we pick. Start with the
   simplest thing that works (probably a Python subprocess with blocking API
   and `tau.parallel()` for concurrency).

4. **The tool** — `py_repl` or `orchestrate` or both. Wire the kernel to
   the Rust side via JSON-line protocol. Reverse RPC dispatcher for
   `tau.tool()`, `tau.thread()`, etc.

5. **System prompt + examples** — teach the LLM how to use the new tools.
   This is where the magic happens or doesn't.

6. **Iterate** — the first version will be wrong. The interesting part is
   finding out _how_ it's wrong.

---

## 8. Open Questions

- **Language choice for the DSL.** Python (familiar, stdlib, top-level await
  problem) vs JS (clean async, less familiar for ML workflows, lighter embed)?
  Or both?
- **Persistent REPL vs one-shot scripts?** REPL is more powerful (incremental
  state) but more complex (binding semantics, kernel lifecycle). One-shot is
  simpler but each orchestration is self-contained.
- **Blocking vs async Python API?** Blocking is simpler for the LLM to write.
  Async is more expressive. The `tau.parallel()` hybrid is a middle ground.
- **Where does this run in the Hive picture?** If tau is a backend for Hive,
  does the orchestration happen at the Hive level or the tau level? Both?
- **How do we evaluate this?** What does "good orchestration" look like? Need
  concrete tasks that benefit from multi-agent coordination to test against.

  ~/.claude/plans/optimized-yawning-acorn.md
