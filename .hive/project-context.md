# Project Context — tau

## Overview
Rust port of the `ai` and `agent` layers from a TypeScript monorepo (pi-mono), providing LLM streaming primitives and an agent loop abstraction — no provider implementations or CLI/tool layer yet.

## Architecture
- **`ai` crate** — LLM streaming primitives: type system for messages/content blocks/tools, generic `EventStream<T>` backed by mpsc+oneshot channels, provider registry (`OnceLock<RwLock<Registry>>`), model registry with built-in catalog, and top-level `stream`/`complete` helpers.
- **`agent` crate** — Agent loop abstraction built on `ai`: two-level loop (inner: stream→tools→steering; outer: follow-up re-entry), `Agent` struct with `Arc<Mutex<AgentState>>`, steering/follow-up queues, event subscriptions, and cancellation via `CancellationToken`.
- **Data flow**: `Agent::prompt()` → `agent_loop()` spawns a Tokio task → `stream_assistant_response()` applies `transform_context` → `convert_to_llm` → `stream_simple()` (or injected `stream_fn`) → drives `AssistantMessageEventStream` → emits `AgentEvent`s → executes tool calls → checks steering → loops.
- **No provider implementations exist** — the `ApiProvider` trait is defined and the registry is wired, but no concrete providers (Anthropic, OpenAI) are implemented. All live API tests are `#[ignore]`.
- **No coding-agent layer** — the third tier (CLI, tools, sessions) from the TS source is not ported.

## Key Files
- `Cargo.toml` — workspace root, defines shared dependencies (tokio, serde, anyhow, thiserror, uuid, chrono, futures)
- `ai/src/types.rs` — core type system: `Model`, `Message`, `ContentBlock`, `AssistantMessageEvent`, `StreamOptions`, `Tool`, `Context`
- `ai/src/stream.rs` — generic `EventStream<T>` (mpsc+oneshot), `AssistantMessageEventStream` specialization
- `ai/src/providers.rs` — `ApiProvider` trait, global provider registry, top-level `stream`/`complete` functions
- `ai/src/models.rs` — model registry (provider→model_id→Model), `supports_xhigh()`, `calculate_cost()`
- `ai/src/catalog.rs` — built-in model catalog for anthropic, openai, kimi-coding (~60 models)
- `agent/src/types.rs` — `AgentMessage`, `AgentTool` trait, `AgentState`, `AgentLoopConfig` (function-pointer hooks), `AgentEvent` enum
- `agent/src/loop_.rs` — `agent_loop`/`agent_loop_continue` entry points, core `run_loop` with steering/follow-up, tool execution with mid-sequence steering interruption
- `agent/src/agent.rs` — `Agent` struct wrapping the loop with state, queues, subscriptions, cancellation
- `agent/tests/common.rs` — test helpers: `mock_model()`, `instant_stream()`, `stream_fn_from_messages()` for injecting mock LLM responses
- `ai/tests/common.rs` — test helpers: `mock_model()`, `create_assistant_message()`, `registry_lock()`
- `docs/overview.md` — detailed architecture documentation covering type mappings, design decisions, and known gaps

## Build & Test
- **Language**: Rust 2021 edition
- **Package manager**: Cargo (workspace with 2 members: `ai`, `agent`)
- **Build**: `cargo build`
- **Test**: `cargo test` (runs ~38 pure unit tests; 19 more are `#[ignore]` requiring API keys or unimplemented providers)
- **Lint**: `cargo clippy` (no custom config)
- **Format**: `cargo fmt`
- **Type check**: compiler (Rust is statically typed)
- **Pre-commit**: N/A
- **Quirks**: Global registries (`OnceLock<RwLock<...>>`) for both providers and models — tests that mutate registries need serialization. `ai/tests/common.rs` provides `registry_lock()` for this. The `agent` module is named `loop_.rs` (trailing underscore) because `loop` is a Rust keyword.

## Conventions
- Types mirror the TypeScript source closely; serde attributes replicate the JSON shape (`#[serde(tag = "type")]`, `#[serde(rename_all = "camelCase")]`, `#[serde(rename = "toolUse")]`)
- Function-pointer hooks use `Arc<dyn Fn(...) -> BoxFuture<...> + Send + Sync>` type aliases defined in `types.rs`
- `known_api` / `known_provider` modules hold string constants for well-known API/provider identifiers
- Test files are per-feature in `tests/` directory (integration test style), not mirrored to source structure
- `#[ignore = "reason"]` on tests that need API keys or unimplemented features, with descriptive reason strings
- `common.rs` in each crate's `tests/` directory provides shared mock factories
- `AgentMessage::Custom` with `role: String, data: Value` is the open-ended escape hatch replacing TS declaration merging
- Error handling: `anyhow::Result` for fallible operations, `thiserror` for typed errors (currently unused), panics for programming errors (`expect()` on required fields)
- Stream termination uses `is_terminal()` predicate pattern — `EventStreamSender` auto-resolves the oneshot on terminal events

## Dependencies & Integration
- **tokio** (full features) — async runtime, mpsc/oneshot channels
- **serde + serde_json** — serialization matching the TS JSON wire format
- **futures** — `Stream` trait, `StreamExt` for driving event streams
- **tokio-util** — `CancellationToken` for abort signaling
- **chrono** — timestamps on messages
- **uuid** — generating unique IDs (v4)
- **anyhow / thiserror** — error handling
- No external API calls yet — provider implementations are the primary gap

## Gotchas
- **No providers**: Any call through `ai::stream()` or `ai::complete()` will fail with "No API provider registered" — must inject `stream_fn` on `AgentLoopConfig` for testing, or implement providers
- **Global mutable registries**: Both `providers::REGISTRY` and `models::REGISTRY` are `OnceLock<RwLock<...>>` globals. Tests mutating them must serialize (use `registry_lock()`) or will race
- **`loop_.rs` naming**: The agent loop module is `loop_` because `loop` is a reserved keyword — imports use `crate::loop_::`
- **`AgentTool` → `ai::Tool` wiring missing**: `stream_assistant_response` passes `tools: None` to the LLM context (marked with `// TODO`), so tool definitions aren't sent to the model even when tools are registered on the agent
- **Steering interrupts tool sequences**: When steering messages arrive mid-tool-execution, remaining tools are skipped with `is_error: true` — this is intentional behavior matching the TS source
- **`agent_loop_continue` panics** (not errors) if context is empty or ends with an assistant message — these are assertion failures, not recoverable errors
