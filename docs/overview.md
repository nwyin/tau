# Codebase Overview

This is a Rust port of the foundational layers of pi-mono — specifically `packages/ai` and `packages/agent`. It exists as a harness to explore what a Rust implementation of the agent stack feels like, what the pain points are, and whether the architecture translates cleanly.

The two crates mirror the two lowest layers of pi-mono's dependency graph:

```
ai     ←  packages/ai       (LLM streaming primitives)
agent  ←  packages/agent    (agent loop abstraction)
```

The third layer, `packages/coding-agent` (CLI, tools, sessions), is not yet ported.

---

## `ai` — LLM streaming primitives

### Type system (`types.rs`)

The core types map closely to pi-mono's. A few translation notes:

- `Model<TApi>` is a generic in TS (the `api` field narrows the type). In Rust, `Model` is a plain struct with `api: String`, and `known_api` provides string constants (`ANTHROPIC_MESSAGES`, `OPENAI_RESPONSES`, etc.) for use at call sites.
- `ContentBlock` and `Message` use `#[serde(tag = "type")]` and `#[serde(tag = "role")]` — the same tagged-union shape as the TS JSON representation.
- `UserContent` is either plain text or a `Vec<UserBlock>` (text or image), matching pi-mono's `UserContent = string | UserBlock[]`.
- `AssistantMessageEvent` covers the full lifecycle of a streaming response: `Start → TextDelta* → Done/Error`. The `is_terminal()` method identifies when the stream is finished.
- `SimpleStreamOptions` wraps the reasoning/thinking level, API key, session ID, transport, etc. — all the knobs that sit above the raw wire options.

### EventStream (`stream.rs`)

pi-mono's `EventStream` is a push-based async iterable backed by an internal queue and a waiting-consumer array. The Rust port maps this to:

- `mpsc::unbounded_channel` — the queue; `push()` on the sender maps to calling the TS `push()` method
- `oneshot::channel` — the "result" resolution; in TS this is a `Promise` exposed via `stream.result()`
- `EventStreamSender<T>` holds an `is_complete: fn(&T) -> bool` predicate; when a terminal event is pushed, it automatically resolves the oneshot before sending through the channel

`AssistantMessageEventStream` is a thin specialisation where `is_complete` is `AssistantMessageEvent::is_terminal()`. Consumers get both a `Stream` interface (for event-by-event processing) and an `async fn result()` (to just await the final message).

### Provider registry (`providers.rs`)

pi-mono registers API implementations (e.g. `AnthropicMessagesProvider`, `OpenAIResponsesProvider`) against an API string key. The Rust port is structurally identical: a global `OnceLock<RwLock<Registry>>` keyed on `api: &str`, with `register_api_provider` / `get_api_provider`.

The `ApiProvider` trait exposes two methods:
- `stream()` — raw, provider-specific options
- `stream_simple()` — normalized options (handles the reasoning/thinking abstraction)

Top-level `stream()`, `stream_simple()`, `complete()`, `complete_simple()` functions mirror pi-mono's `stream.ts` exports: they look up the provider by `model.api` and delegate.

**No provider implementations exist yet.** The registry is wired but empty. Any call through it will return a `No API provider registered` error. This is the biggest gap between the skeleton and something runnable.

### Model registry + catalog (`models.rs`, `catalog.rs`)

The model registry is a two-level map: `provider → model_id → Arc<Model>`. It auto-populates from `catalog.rs` on first access (via `ModelRegistry::default()`).

The catalog is scoped to three providers — **anthropic**, **openai**, **kimi-coding** — rather than the full pi-mono set (~13 providers, ~800+ models). This is intentional: the full `models.generated.ts` is 13K lines generated from an external source; maintaining a Rust equivalent at full scale isn't the point. The catalog exists to make pure unit tests pass (e.g. `supports_xhigh`) without live API calls.

`supports_xhigh()` mirrors pi-mono's logic: true for `gpt-5.2`/`gpt-5.3` model IDs, or for `anthropic-messages` API with an `opus-4-6`/`opus-4.6` model ID. OpenRouter's opus 4.6 (which uses `openai-completions`) correctly returns false.

---

## `agent` — agent loop abstraction

### Type system (`types.rs`)

The key divergence from pi-mono here is `AgentMessage`:

```rust
pub enum AgentMessage {
    Llm(Message),
    Custom { role: String, data: Value },
}
```

In pi-mono, `AgentMessage` is defined via TypeScript declaration merging — applications augment the `CustomAgentMessages` interface to add their own message types. Rust has no equivalent, so `AgentMessage::Custom` is an open-ended escape hatch carrying a role string and a raw JSON value.

`AgentTool` is a trait with an async `execute()`:

```rust
fn execute(&self, id: String, params: Value, signal: Option<CancellationToken>, on_update: Option<ToolUpdateFn>)
    -> BoxFuture<Result<AgentToolResult>>;
```

This matches pi-mono's `AgentTool.execute()` signature. Tools are held as `Vec<Arc<dyn AgentTool>>`.

`AgentLoopConfig` holds the four function-pointer hooks that pi-mono passes into the loop:
- `convert_to_llm` — filter/transform `AgentMessage` → `Message` before sending to the LLM
- `transform_context` — prune/reorder the full message history
- `get_steering_messages` — inject user-queued messages mid-loop
- `get_follow_up_messages` — re-enter the loop after it would otherwise finish

### Agent loop (`loop_.rs`)

`agent_loop` and `agent_loop_continue` mirror pi-mono's exports exactly. Both spawn a Tokio task and return an `AgentEventStream` (an `EventStream<AgentEvent>` that terminates on `AgentEvent::AgentEnd`).

The core `run_loop` implements the two-level loop from pi-mono:
- **Outer loop**: checks for follow-up messages; re-enters if any are queued
- **Inner loop**: streams one assistant response, executes tool calls, checks for steering messages after each tool, repeats if there are pending messages

Steering mid-tool-execution skips remaining tools with `is_error: true` and `"Skipped due to queued user message."` — matching the TS behavior that allows user interruptions to preempt in-flight tool sequences.

`stream_assistant_response` applies `transform_context` → `convert_to_llm` → `ai::stream_simple`, then drives the event stream, translating `AssistantMessageEvent`s into `AgentEvent`s.

### Agent struct (`agent.rs`)

`Agent` wraps the loop with:
- `Arc<Mutex<AgentState>>` — model, tools, messages, streaming flag
- Two `VecDeque` queues (steering, follow-up) with `QueueMode::OneAtATime | All`
- A listener list for `AgentEvent` subscriptions (mirrors pi-mono's `subscribe()`)
- `CancellationToken` for `abort()`
- `build_config()` closes over the queues to produce `AgentLoopConfig` callbacks

`prompt()` → `run_loop()` → `agent_loop()` → `drain_stream()`. Each event updates state (appending messages, tracking pending tool calls, clearing `is_streaming` on `AgentEnd`) then fires all listeners.

---

## Tests

Tests are ported from pi-mono's test suites with two conventions:

- **Live API tests** are `#[ignore = "requires ANTHROPIC_API_KEY"]` etc. They exist as full implementations ready to run when credentials are present.
- **Pure unit tests** run without any credentials: `supports_xhigh`, `tool_call_id_normalization` (3 tests for ID normalization logic), agent state/queue management (12 tests), and `agent_loop_continue` panic behavior.

Tests requiring mock stream injection (`stream_fn` on `AgentLoopConfig`) are `#[ignore = "needs stream_fn injection"]`. This is the second biggest gap — without it, the agent loop tests that verify event sequences, tool execution, and steering behavior can't run.

---

## Gaps and natural next steps

In rough priority order:

1. **Provider implementations** — at minimum `anthropic-messages` and `openai-responses`. This is what makes the `#[ignore]` live tests runnable and validates that the streaming pipeline actually works end-to-end.

2. **`stream_fn` injection on `AgentLoopConfig`** — adds a `stream_fn: Option<StreamFn>` field that `stream_assistant_response` uses in place of `ai::stream_simple`. Unlocks all the mock-based agent loop tests without live API calls.

3. **`coding-agent` tier** — tools (bash, file read/write, search), session management, compaction. This is where the actual agent behavior lives in pi-mono.
