# Project Context — tau

## Overview
A minimal Rust coding agent harness: three-crate workspace providing LLM streaming primitives (`ai`), a generic agent loop (`agent`), and a coding-specific CLI/server (`coding-agent`) that ships as the `tau` binary.

## Architecture
- **`ai`** — LLM provider abstraction: streaming SSE parsing, model registry (~65 models across OpenAI Responses, Anthropic Messages, and OpenAI-compatible Chat Completions/OpenRouter), cost tracking, content block types. No coding-specific logic.
- **`agent`** — Generic agent loop: two-level loop (outer: follow-ups, inner: stream → tool execution → steering), event bus with subscriber pattern, cancellation via `CancellationToken`, stats collection. Defines the `AgentTool` trait.
- **`coding-agent`** — Concrete harness: CLI (clap), TOML config, JSONL session persistence, system prompt builder, tool implementations (bash, file_read/write/edit, grep, glob, hashline variants, pycg/pycfg structural analysis), JSON-RPC serve mode for orchestrator integration (Hive), tracing.
- **Data flow**: User input → `Agent::prompt()` → `agent_loop()` spawns a tokio task → streams LLM response via provider → executes tool calls sequentially → emits `AgentEvent`s to subscribers → loops until no more tool calls or follow-ups.
- **Provider registry**: Global `OnceLock<RwLock<Registry>>` mapping API surface strings (`"openai-responses"`, `"anthropic-messages"`, `"openai-chat"`) to `ApiProvider` trait objects. `register_builtin_providers()` called at startup.

## Key Files
- `ai/src/types.rs` — Core types: `ContentBlock`, `Message`, `AssistantMessage`, `Model`, `StreamOptions`, `AssistantMessageEvent`
- `ai/src/providers/mod.rs` — `ApiProvider` trait, provider registry, top-level `stream()`/`complete()` helpers
- `ai/src/providers/anthropic.rs` — Anthropic Messages API provider
- `ai/src/providers/openai_responses.rs` — OpenAI Responses API provider
- `ai/src/providers/openai_chat.rs` — OpenAI-compatible Chat Completions provider (OpenRouter, Groq, etc.)
- `ai/src/models.rs` + `ai/src/catalog.rs` — Model registry with per-model cost/context/capability metadata
- `agent/src/loop_.rs` — Core agent loop: streaming, tool dispatch, steering, follow-ups
- `agent/src/types.rs` — `AgentTool` trait, `AgentEvent` enum, `AgentLoopConfig`, `AgentMessage`
- `agent/src/agent.rs` — `Agent` struct: state management, event bus, `prompt()`/`abort()` API
- `coding-agent/src/main.rs` — Binary entry point: REPL + headless modes, session management, Ctrl-C handling
- `coding-agent/src/tools/mod.rs` — Tool registry: `all_tools()`, `tools_for_edit_mode()`, `tools_from_allowlist()`
- `coding-agent/src/agent_builder.rs` — Shared agent construction (model/key/tool resolution) for CLI and serve mode
- `coding-agent/src/serve.rs` — JSON-RPC stdio server for orchestrator integration
- `coding-agent/src/cli.rs` — Clap CLI definition (`tau`, `tau serve`, `tau models`)
- `coding-agent/src/config.rs` — TOML config loading (`~/.tau/config.toml`)

## Build & Test
- **Language**: Rust 2021 edition, requires 1.75+
- **Package manager**: Cargo (workspace with 3 members: `ai`, `agent`, `coding-agent`)
- **Build**: `cargo build` (debug), `cargo build --release -p coding-agent` (release binary)
- **Install**: `cargo install --path coding-agent` (puts `tau` on PATH)
- **Test**: `cargo test` — ~300 offline tests, all pass, no API keys needed. Completes in ~1.5s.
- **Lint**: `cargo clippy -- -D warnings`
- **Format**: `cargo fmt` (check: `cargo fmt --check`)
- **Type check**: Compiler (no separate type checker needed for Rust)
- **Benchmarks**: `cargo bench` — criterion benchmarks for SSE parsing, message serde, agent construction
- **CI**: GitHub Actions — fmt, clippy, test, coverage (cargo-llvm-cov → Codecov), bench (on main), musl static binary build
- **Pre-commit**: N/A (CI enforces fmt + clippy)
- **Quirks**: CI installs `ripgrep` via apt (`sudo apt-get install -y ripgrep`) because grep tool tests shell out to `rg`. Live provider tests gated behind `OPENAI_API_KEY` + `RUN_LIVE_PROVIDER_TESTS=1`.

## Conventions
- Snake_case for all Rust identifiers; module files named `loop_.rs` (trailing underscore to avoid keyword collision)
- Tests live in `crate/tests/` as integration tests (separate files per concern, e.g., `tools_test.rs`, `agent_loop.rs`, `proptest_serde.rs`)
- Common test helpers in `tests/common.rs` (shared `text_content()` extractor, mock setup)
- Each tool struct implements `AgentTool` trait with `name()`, `description()`, `parameters()` (JSON Schema), `execute()` returning `BoxFuture<Result<AgentToolResult>>`
- Tools provide `::arc()` constructor returning `Arc<dyn AgentTool>`
- Error handling: `anyhow::Result` for fallible operations, `thiserror` for typed errors in `ai` crate
- Serde uses camelCase for JSON fields (`#[serde(rename_all = "camelCase")]`) and tagged enums (`#[serde(tag = "type")]` or `#[serde(tag = "role")]`)
- Async runtime: tokio with `features = ["full"]`; agent loop spawned as `tokio::spawn` tasks
- Event bus: subscriber callbacks (`Fn(AgentEvent) + Send + Sync`) registered via `agent.subscribe()`, return unsubscribe handle
- Config resolution order: CLI flag > env var (`TAU_MODEL`, `TAU_MAX_TURNS`) > config file (`~/.tau/config.toml`) > default

## Dependencies & Integration
- **reqwest** (rustls-tls) for HTTP to LLM providers
- **clap** (derive) for CLI parsing
- **tokio** (full) async runtime
- **serde/serde_json** for all serialization
- **globset/ignore** for gitignore-aware file discovery
- **sha2** for content hashing (hashline mode, trace fingerprints)
- **criterion** for benchmarks, **proptest** for property-based serde/SSE tests
- **External binaries**: `rg` (ripgrep) required at runtime for grep tool; optional `pycg`/`pycfg` binaries for structural analysis tools
- **Codex OAuth**: Falls back to `~/.codex/auth.json` for OpenAI auth when no `OPENAI_API_KEY` set
- **JSON-RPC serve mode**: `tau serve --cwd <path>` for orchestrator (Hive) integration over stdio

## Gotchas
- The grep tool shells out to `rg` — it must be on PATH or grep tests/usage will fail
- Provider registry is a global singleton (`OnceLock<RwLock>`); tests that register/clear providers can interfere if run in parallel within the same process (integration tests in separate binaries avoid this)
- `loop_.rs` is named with trailing underscore because `loop` is a Rust keyword
- Tool calls are executed sequentially (not parallel) — steering messages can interrupt a batch, skipping remaining calls
- The `agent` crate's `ThinkingLevel` is a superset of `ai`'s (adds `Off` variant); conversion via `.to_ai()`
- Session files are JSONL in `~/.tau/sessions/`; no auto-cleanup
- Default model is `gpt-4o-mini` (hardcoded in `config.rs` default)
- Static musl binary built in CI for Linux x86_64; no macOS/ARM release artifacts
- `hashline` edit mode uses content-hash tags as line anchors — edits fail on stale hashes, requiring re-read after edit
