# Codebase Overview

tau is a Rust agent harness inspired by pi-mono's architecture. It ports the foundational layers (`packages/ai` and `packages/agent`) and adds a minimal `coding-agent` harness on top.

The design is intentionally layered so that `agent` stays generic — different harnesses can be built on top for different agent types (coding, data, research, etc.).

```
ai             LLM streaming primitives (providers, models, event streams)
agent          generic agent loop (tools, steering, follow-ups, events)
coding-agent   built-in tools (bash, file read/write) + REPL/CLI
```

## Current state

- **ai**: OpenAI Responses provider implemented. Anthropic and Kimi are TODO stubs. Model catalog covers ~60 models across three providers.
- **agent**: Feature-complete port of pi-mono's agent loop. `stream_fn` injection for testing, tool wiring to LLM context, full event system.
- **coding-agent**: Three tools (BashTool, FileReadTool, FileWriteTool), interactive REPL, headless `--prompt` mode (in progress).

Tests: ~120 passing, 8 ignored (live API smoke tests gated by `OPENAI_API_KEY` + `RUN_LIVE_PROVIDER_TESTS=1`).

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

**TODO stubs:** Anthropic Messages, Kimi.

### Model registry + catalog (`models.rs`, `catalog.rs`)

Two-level map: `provider → model_id → Arc<Model>`. Auto-populates from catalog on first access. Scoped to anthropic, openai, kimi-coding (~60 models).

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

## `coding-agent` — minimal coding harness

### Tools

- **BashTool** — runs shell commands via `sh -c`. Timeout support, cancellation, output truncation (2000 lines / 30KB), exit code reporting.
- **FileReadTool** — reads text files with offset/limit. Line numbering, binary detection, truncation with continuation hints.
- **FileWriteTool** — writes files, creates parent dirs.

All implement `AgentTool` and are collected via `coding_agent::tools::all_tools()`.

### CLI modes

- **REPL** (default): interactive `> ` prompt loop. Streams text deltas to stdout, tool events to stderr.
- **Headless** (`--prompt "..."`, in progress): non-interactive mode for benchmarks and scripting. Agent loops autonomously until done, then exits.

### Usage

```
OPENAI_API_KEY=sk-... cargo run -p coding-agent
OPENAI_API_KEY=sk-... cargo run -p coding-agent -- --prompt "List all Rust files"
```

---

## Tests

- **Offline unit tests** (~120): type system, stream mechanics, agent loop, tool execution, SSE parsing, message conversion. All run without credentials.
- **Live smoke tests** (8 `#[ignore]`): require both API key AND `RUN_LIVE_PROVIDER_TESTS=1`. Double opt-in gate.
- **Fixture-based contract tests**: SSE response fixtures in `ai/tests/fixtures/` validate provider behavior without network calls.
- **Mock stream injection**: `stream_fn` on `AgentLoopConfig` allows full agent loop testing without any provider.

---

## Gaps and next steps

See `docs/roadmap.md` for the full roadmap.

Key items:
1. **Anthropic/Kimi providers** — TODO stubs exist, implementation deferred
2. **`--prompt` headless mode** — in progress, enables benchmark integration
3. **Session persistence** — JSONL storage for cross-restart continuity
4. **More tools** — grep, find, edit (diff-based) for richer coding agent
5. **Benchmark integration** — terminal-bench adapter to measure harness quality
