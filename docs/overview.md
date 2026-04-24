# Codebase Overview

tau is a Rust agent harness inspired by pi-mono's architecture. It keeps LLM provider code, the generic agent runtime, and the coding harness in separate crates.

The design is intentionally layered so that `agent` stays generic — different harnesses can be built on top for different agent types (coding, data, research, etc.).

```
ai             LLM streaming primitives: providers, model catalog, auth, streams
agent          generic agent loop: tools, events, compaction, stats, orchestration state
coding-agent   tau binary: TUI, CLI, serve mode, tools, sessions, traces
```

## Current state

- **ai**: OpenAI Responses, OpenAI-compatible Chat Completions, and Anthropic Messages providers implemented. Model catalog covers direct OpenAI/Anthropic plus OpenRouter families. Property-based tests (proptest) for SSE parser and type serde.
- **agent**: Feature-complete port of pi-mono's agent loop. `stream_fn` injection for testing, tool wiring to LLM context, full event system. Performance instrumentation via `AgentStats` subscriber.
- **coding-agent**: Full-screen TUI, headless `--prompt` mode, JSON-RPC `serve` mode, sessions, traces, permissions, skills, filesystem/search/web tools, and orchestration tools (`thread`, `query`, `document`, `log`, `from_id`, `py_repl`).

For current implementation status and recently fixed runtime correctness issues, see [`docs/implementation.md`](implementation.md).

---

## `ai` — LLM streaming primitives

### Type system (`types.rs`)

Core types map closely to pi-mono's:

- `Model` — plain struct with `api: String` (vs TS generic `Model<TApi>`). `known_api` module provides string constants (`OPENAI_RESPONSES`, `ANTHROPIC_MESSAGES`, etc.).
- `ContentBlock` and `Message` — `#[serde(tag = "type")]` / `#[serde(tag = "role")]` tagged unions matching the JSON wire format.
- `UserContent` — either plain text or `Vec<UserBlock>` (text/image).
- `AssistantMessageEvent` — streaming lifecycle: `Start → TextDelta* → Done/Error`. `is_terminal()` detects stream end.
- `SimpleStreamOptions` — reasoning level, API key, session ID, transport, thinking budgets.

### EventStream (`stream.rs`)

- `mpsc::unbounded_channel` — the event queue
- `oneshot::channel` — result resolution (TS `stream.result()`)
- `EventStreamSender<T>` — auto-resolves oneshot when terminal event is pushed
- `AssistantMessageEventStream` — specialization for assistant message events

### Provider registry (`providers.rs`)

Global `OnceLock<RwLock<Registry>>` keyed on API string. `register_api_provider()` / `get_api_provider()`.

`ApiProvider` trait:
- `stream()` — raw options
- `stream_simple()` — normalized options with reasoning abstraction

Top-level `stream()`, `stream_simple()`, `complete()`, `complete_simple()` look up provider by `model.api` and delegate.

**Implemented:** OpenAI Responses (`ai/src/providers/openai_responses.rs`). Full SSE parsing, tool call ID normalization, reasoning effort clamping, cost calculation, service tier multipliers.

**Implemented:** Anthropic Messages (`ai/src/providers/anthropic.rs`). Native JSON streaming, thinking support (budget-based and adaptive), tool argument accumulation, stop reason mapping.

OpenRouter-backed model families such as Gemini, Qwen, Grok, DeepSeek, and Kimi all route through the same `openai-chat` backend rather than bespoke providers.

### Model registry + catalog (`models.rs`, `catalog.rs`)

Two-level map: `provider → model_id → Arc<Model>`. Auto-populates from catalog on first access. Scoped to anthropic, openai, and openrouter-backed model families.

---

## `agent` — agent loop abstraction

### Type system (`types.rs`)

- `AgentMessage` — `Llm(Message)` or `Custom { role, data }` (open-ended escape hatch replacing TS declaration merging)
- `AgentTool` trait — `name()`, `label()`, `description()`, `parameters()` (JSON Schema), `execute()` (async). Held as `Vec<Arc<dyn AgentTool>>`.
- `AgentLoopConfig` — function-pointer hooks: `convert_to_llm`, `transform_context`, `get_steering_messages`, `get_follow_up_messages`, `stream_fn`
- `AgentEvent` — full lifecycle: `AgentStart/End`, `TurnStart/End`, `MessageStart/Update/End`, `ToolExecutionStart/Update/End`

### Agent loop (`loop_.rs`)

Two-level loop:
- **Outer**: checks for follow-up messages, re-enters if queued
- **Inner**: streams assistant response → executes tool calls → checks steering → repeats

`stream_assistant_response` applies `transform_context` → `convert_to_llm` → provider stream (or injected `stream_fn`). Tool definitions are converted from `AgentTool` to `ai::Tool` and sent to the LLM.

Steering mid-tool-execution skips remaining tools with `is_error: true`.

### Agent struct (`agent.rs`)

Wraps the loop with state management:
- `Arc<Mutex<AgentState>>` — model, tools, messages, streaming flag
- Steering/follow-up `VecDeque` queues with `QueueMode::OneAtATime | All`
- Event subscriptions via `subscribe()`
- `CancellationToken` for `abort()`
- `prompt()` / `continue_()` entry points

---

## `coding-agent` — coding harness

### Tools

- **BashTool** — runs shell commands via `sh -c`. Timeout support, cancellation, output truncation (2000 lines / 30KB), exit code reporting.
- **FileReadTool** — reads text files with numbered lines plus offset/limit support.
- **FileWriteTool** — writes files, creates parent dirs.
- **FileEditTool** — exact-match string replacement (`old_string` → `new_string`) with a trimmed/unicode fuzzy fallback when exact matching fails.
- **GrepTool** — searches file contents by regex pattern.
- **GlobTool** — searches for files by name pattern.
- **WebFetchTool / WebSearchTool** — fetch pages and run Exa-backed web search.
- **SubagentTool / TodoTool** — subprocess delegation and full-replace progress tracking.
- **ThreadTool / QueryTool / DocumentTool / LogTool / FromIdTool / PyReplTool** — in-process orchestration, shared virtual documents, reusable worker threads, single-shot queries, and persistent Python orchestration.

All implement `AgentTool`. The default direct tool set is defined in `coding_agent::tools::default_tools()`, and agent construction adds the orchestration tools backed by shared `OrchestratorState`.

### CLI modes

- **REPL** (default): interactive `> ` prompt loop. Streams text deltas to stdout, tool events to stderr.
- **Headless** (`--prompt "..."`): non-interactive mode for benchmarks and scripting. Agent loops autonomously until done, then exits.
- **Session persistence**: `--session <id>` resumes a session, `--resume` picks up the most recent. Sessions stored as JSONL in `~/.tau/sessions/`.
- **Stats**: `--stats` prints token/cost/latency summary to stderr. `--stats-json <path>` writes machine-readable JSON.

### Usage

```
OPENAI_API_KEY=sk-... cargo run -p coding-agent -- --prompt "List all Rust files"
ANTHROPIC_API_KEY=sk-... cargo run -p coding-agent -- --model claude-sonnet-4-6 --prompt "Explain this repo"
OPENROUTER_API_KEY=sk-... cargo run -p coding-agent -- --model moonshotai/kimi-k2.5 --prompt "Explain this repo"
```

---

## Tests

- **Offline unit tests** (~120): type system, stream mechanics, agent loop, tool execution, SSE parsing, message conversion. All run without credentials.
- **Live smoke tests** (8 `#[ignore]`): require both API key AND `RUN_LIVE_PROVIDER_TESTS=1`. Double opt-in gate.
- **Fixture-based contract tests**: SSE response fixtures in `ai/tests/fixtures/` validate provider behavior without network calls.
- **Mock stream injection**: `stream_fn` on `AgentLoopConfig` allows full agent loop testing without any provider.

---

## Design decisions

**Why three crates?** The `agent` crate is generic — it has no opinion about what tools exist or what domain the agent operates in. A new harness is ~80 lines of glue: pick tools, pick a system prompt, wire up events. `coding-agent` is one such harness; others (data, research, etc.) can be built on the same foundation without importing coding-specific dependencies.

**Why not port all of pi-mono's coding-agent?** pi-mono's `packages/coding-agent` is ~120 source files — TUI, session branching, compaction, extensions, skills, themes, RPC, OAuth, and package management. tau needs tools that let an LLM interact with the filesystem and shell, a way to run it, and good enough quality to benchmark. Everything else is optional.

**Why OpenAI and Anthropic first?** These two cover the models that matter most for benchmarking. OpenAI was implemented first (well-documented Responses API), Anthropic second (validates the provider abstraction with a meaningfully different wire format). Everything else rides the OpenAI-compatible chat backend where practical, including Kimi via OpenRouter.

**Live API test policy.** Live provider tests require double opt-in: `OPENAI_API_KEY` + `RUN_LIVE_PROVIDER_TESTS=1`. Unit tests are fully offline and deterministic. Fixture-based contract tests validate provider wire formats without network calls. See `docs/test-migration-todo.md`.

**No proxy, no web UI.** pi-mono supports browser-based agents via a CORS proxy (`streamProxy`) and a Lit web component library. tau is local-only — agents run on the machine, call providers directly. This is a deliberate scope cut.

**Benchmarking as a first-class concern.** tau is designed to be benchmarked — `--prompt` mode, `--stats` instrumentation, and the `benchmarks/` directory exist from early on. The goal is not just to build a harness but to measure how harness design affects model performance. See `docs/benchmarking.md`.

**Edit format discipline.** tau currently uses exact string replacement with a limited fuzzy fallback. Hash-anchored line editing was investigated but not adopted because local benchmark results did not generalize beyond the JavaScript/TypeScript-style cases where the technique was originally reported to perform well.
