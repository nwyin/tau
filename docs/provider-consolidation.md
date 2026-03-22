# Provider Consolidation: OpenAI Chat Completions + OpenRouter

## Problem

tau currently has two provider backends — `openai-responses` and `anthropic-messages` —
each ~1,000 lines of SSE parsing, message conversion, and tool format handling. To add
a new model family (Mistral, Llama, Gemini, Kimi, DeepSeek, Grok, etc.), we'd have to
write another ~1,000-line backend from scratch.

Meanwhile, **OpenRouter** offers a unified API gateway that presents 200+ models
(including Anthropic, Google, Meta, Mistral) behind the standard OpenAI Chat Completions
interface. One backend covers them all.

## Goal

Add an `openai-chat` provider that speaks the standard `/v1/chat/completions` SSE
protocol. This one backend covers:
- OpenRouter (all models: Anthropic, Google, Meta, Mistral, DeepSeek, etc.)
- Direct OpenAI Chat Completions (legacy/compat path)
- Any OpenAI-compatible endpoint (Groq, Together, Ollama, LiteLLM, vLLM, etc.)

After this, tau has three backends total:
1. `openai-responses` — direct OpenAI Responses API (GPT-5+, o-series, Codex features)
2. `anthropic-messages` — direct Anthropic Messages API (prompt caching, thinking signatures)
3. `openai-chat` — everything else via OpenAI Chat Completions protocol

## Current Architecture

```
ai/src/providers/
├── mod.rs                     # ApiProvider trait + registry (122 lines)
├── anthropic.rs               # Anthropic Messages API (945 lines)
├── openai_responses.rs        # OpenAI Responses API (517 lines)
├── openai_responses_shared.rs # Shared OpenAI logic (873 lines)
└── openai_chat_shared.rs      # Shared Chat Completions helpers
```

The `ApiProvider` trait is clean:

```rust
pub trait ApiProvider: Send + Sync {
    fn api(&self) -> &str;
    fn stream(&self, model: &Model, context: &Context, options: Option<&StreamOptions>)
        -> AssistantMessageEventStream;
    fn stream_simple(&self, model: &Model, context: &Context, options: Option<&SimpleStreamOptions>)
        -> AssistantMessageEventStream;
}
```

Models declare `api: String` (e.g. `"openai-responses"`, `"anthropic-messages"`), which
routes them to the right provider at runtime. Adding a third `api: "openai-chat"` is
purely additive — no existing code changes.

## OpenAI Chat Completions vs Responses API

| Aspect | Chat Completions (`/v1/chat/completions`) | Responses API (`/v1/responses`) |
|--------|------------------------------------------|--------------------------------|
| Message format | `messages: [{role, content}]` | `input: [{type, ...}]` items |
| System prompt | `{role: "system", content}` message | Top-level `instructions` field |
| Tool calls (request) | `tools: [{type: "function", function: {name, description, parameters}}]` | `tools: [{type: "function", name, description, parameters}]` |
| Tool calls (response) | `delta.tool_calls: [{index, id, function: {name, arguments}}]` | `response.function_call_arguments.delta` events |
| Tool results | `{role: "tool", tool_call_id, content}` | `{type: "function_call_output", call_id, output}` |
| SSE terminator | `data: [DONE]` | `response.completed` event |
| SSE delta path | `choices[0].delta.content` | `response.output_text.delta` |
| Reasoning/thinking | `delta.reasoning_content` (varies by provider) | `response.reasoning_summary_text.delta` |
| Usage | `usage` in final chunk (with `stream_options.include_usage`) | `response.completed` event |
| Tool call IDs | Simple string (`call_abc123`) | Pipe-separated (`call_id\|item_id`) |
| Stop reasons | `finish_reason`: `stop`, `length`, `tool_calls` | `response.completed.status` |
| Prompt caching | Not built-in (OpenRouter: `cache_control` on content parts) | `prompt_cache_key` + `prompt_cache_retention` |
| Service tier | Not applicable | `flex`/`priority` multipliers |

## Implementation Plan

### Step 1: `openai_chat.rs` — Provider Implementation (~150 lines)

New file: `ai/src/providers/openai_chat.rs`

```rust
pub struct OpenAIChatProvider { client: reqwest::Client }

impl ApiProvider for OpenAIChatProvider {
    fn api(&self) -> &str { "openai-chat" }
    fn stream(...) { /* resolve key, build body, POST, parse SSE */ }
    fn stream_simple(...) { /* convert ThinkingLevel → reasoning_effort */ }
}
```

API key resolution order:
1. `options.api_key` (explicit)
2. Model's provider → env var mapping:
   - `"openrouter"` → `OPENROUTER_API_KEY`
   - `"openai"` → `OPENAI_API_KEY`
   - `"groq"` → `GROQ_API_KEY`
   - `"together"` → `TOGETHER_API_KEY`
   - fallback → `OPENAI_API_KEY`

Request headers:
- `Authorization: Bearer {api_key}`
- OpenRouter: add `HTTP-Referer` and `X-Title` headers
- Custom headers from `model.headers`

### Step 2: `openai_chat_shared.rs` — Message & SSE Logic (~500 lines)

New file: `ai/src/providers/openai_chat_shared.rs`

#### Request body builder

```rust
pub fn build_chat_request_body(model: &Model, context: &Context, opts: &ChatRequestOptions) -> Value {
    json!({
        "model": model.id,
        "messages": convert_chat_messages(model, context),
        "tools": convert_chat_tools(context.tools),
        "stream": true,
        "stream_options": { "include_usage": true },
        "temperature": opts.temperature,
        "max_tokens": opts.max_tokens,
        // optional: "reasoning_effort", "tool_choice"
    })
}
```

#### Message conversion

```
System prompt → {role: "system", content: text}

UserMessage:
  Text → {role: "user", content: "text"}
  Blocks → {role: "user", content: [{type: "text", text}, {type: "image_url", image_url: {url: "data:..."}}]}

AssistantMessage:
  Text only → {role: "assistant", content: "text"}
  With tool calls → {role: "assistant", content: text_or_null, tool_calls: [{id, type: "function", function: {name, arguments: json_string}}]}
  Thinking → skip (or include as content prefix if provider supports reasoning_content)

ToolResultMessage → {role: "tool", tool_call_id: id, content: text}
```

Key differences from Responses conversion:
- No pipe-separated tool call IDs (just use the `id` field directly)
- Arguments as JSON string (not parsed HashMap)
- Tool results use `role: "tool"` (not `type: "function_call_output"`)
- No signature round-tripping needed (Chat Completions doesn't have item IDs)
- Consecutive same-role messages: merge user messages (some providers require alternating)

#### SSE event processing

Chat Completions SSE is simpler than Responses — each chunk has:

```json
{"choices": [{"delta": {"content": "hello", "tool_calls": [...]}, "finish_reason": null}], "usage": {...}}
```

State machine:

```rust
enum ChatStreamState {
    Text,           // accumulating content deltas
    ToolCall(usize), // accumulating tool_calls[index] arguments
    Reasoning,       // accumulating reasoning_content (if present)
}
```

Processing loop:
1. Parse `choices[0].delta`
2. If `delta.reasoning_content` (or `reasoning` or `reasoning_text`): emit ThinkingDelta
3. If `delta.content`: emit TextDelta
4. If `delta.tool_calls`: for each tool call delta:
   - New index with `id` + `function.name` → emit ToolCallStart
   - Existing index with `function.arguments` → emit ToolCallDelta, accumulate JSON
5. If `finish_reason` present:
   - Parse accumulated tool call arguments
   - Emit ToolCallEnd for each tool
   - Extract `usage` from final chunk
   - Map finish_reason: `"stop"` → Stop, `"length"` → Length, `"tool_calls"` → ToolUse
   - Emit Done

#### Tool format conversion

```rust
pub fn convert_chat_tools(tools: &[Tool]) -> Vec<Value> {
    tools.iter().map(|t| json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters,
        }
    })).collect()
}
```

Note: Chat Completions wraps in `{type: "function", function: {...}}` while
Responses uses `{type: "function", name, description, parameters}` (flat).

### Step 3: Provider Registration

In `providers/mod.rs`:

```rust
pub mod openai_chat;
pub mod openai_chat_shared;

pub fn register_builtin_providers() {
    register_api_provider(Arc::new(openai_responses::OpenAIResponsesProvider::new()));
    register_api_provider(Arc::new(openai_chat::OpenAIChatProvider::new()));
    register_api_provider(Arc::new(anthropic::AnthropicProvider::new()));
}
```

And in `types.rs`:

```rust
pub mod known_api {
    pub const OPENAI_RESPONSES: &str = "openai-responses";
    pub const OPENAI_CHAT: &str = "openai-chat";
    pub const ANTHROPIC_MESSAGES: &str = "anthropic-messages";
}
```

### Step 4: Catalog — OpenRouter Models

Add OpenRouter models to `catalog.rs`. These all use `api: "openai-chat"` and
`base_url: "https://openrouter.ai/api/v1"`:

```rust
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";

// Provider helper
const OR: Prov = Prov {
    api: "openai-chat",
    provider: "openrouter",
    base_url: OPENROUTER_URL,
};
```

Initial model set (high-value models not already covered by direct providers):

| Model ID | Provider | Notes |
|----------|----------|-------|
| `google/gemini-2.5-pro` | OpenRouter | Best Google model |
| `google/gemini-2.5-flash` | OpenRouter | Fast/cheap Google |
| `meta-llama/llama-4-maverick` | OpenRouter | Latest Meta |
| `meta-llama/llama-4-scout` | OpenRouter | Smaller Meta |
| `mistralai/mistral-large-latest` | OpenRouter | Mistral flagship |
| `mistralai/devstral-small` | OpenRouter | Coding-focused Mistral |
| `deepseek/deepseek-chat-v3` | OpenRouter | Strong coding model |
| `deepseek/deepseek-r1` | OpenRouter | Reasoning model |
| `x-ai/grok-3` | OpenRouter | xAI flagship |
| `qwen/qwen-3-235b` | OpenRouter | Strong open model |

Also add direct OpenAI Chat Completions models for when users prefer `/chat/completions`
over `/responses` (some API wrappers only support chat):

```rust
const OA_CHAT: Prov = Prov {
    api: "openai-chat",
    provider: "openai",
    base_url: "https://api.openai.com/v1",
};
```

### Step 5: Agent Builder — OpenRouter Key Resolution

In `coding-agent/src/agent_builder.rs`, add OpenRouter to the provider/key resolution:

```rust
// Existing:
// "openai" → OPENAI_API_KEY
// "anthropic" → ANTHROPIC_API_KEY
// New:
"openrouter" => std::env::var("OPENROUTER_API_KEY").ok(),
"groq" => std::env::var("GROQ_API_KEY").ok(),
"together" => std::env::var("TOGETHER_API_KEY").ok(),
```

### Step 6: Provider Quirks via `model.compat`

The `Model` struct already has `compat: Option<serde_json::Value>` for provider-specific
overrides. Use this for Chat Completions quirks instead of hardcoding:

```rust
// In catalog.rs, models that need quirks:
m("mistralai/mistral-large", ..., Some(json!({
    "tool_call_id_format": "alphanumeric_9",   // Mistral requires 9-char IDs
    "requires_tool_name_in_result": true,       // Mistral needs name in tool result
})))

m("anthropic/claude-3.5-sonnet", ..., Some(json!({
    "cache_control": true,                      // Add cache_control on content parts
})))
```

The `openai_chat_shared.rs` conversion functions check `model.compat` for these flags.
This keeps quirk handling data-driven rather than scattered across conditionals.

## Provider Quirks Reference

Quirks discovered from oh-my-pi and opencode that `openai-chat` must handle:

| Quirk | Affected Providers | Handling |
|-------|-------------------|----------|
| Tool call ID format | Mistral (9 alphanumeric chars) | Normalize via `compat.tool_call_id_format` |
| `name` in tool result | Mistral, some older models | Add `name` field to `role: "tool"` message |
| Reasoning field name | Varies: `reasoning_content`, `reasoning`, `reasoning_text` | Check all three in delta |
| `max_completion_tokens` vs `max_tokens` | o1/o3 on some endpoints | Use `compat.max_tokens_field` |
| Empty content with tool calls | Some providers reject `null` content | Use `""` instead of `null` |
| `cache_control` on content | Anthropic via OpenRouter | Add `cache_control: {type: "ephemeral"}` on last text part |
| `tool_choice` support | Not all models | Check `compat.supports_tool_choice` |
| Consecutive same-role messages | Anthropic, some others | Merge before sending |

## What Can Be Shared

Some logic is reusable across `openai_responses_shared.rs` and `openai_chat_shared.rs`:

| Logic | Sharable? | Notes |
|-------|-----------|-------|
| `calculate_cost()` | Yes | Already in `models.rs` |
| `supports_xhigh()` | Yes | Already in `models.rs` |
| Tool call ID normalization | Partially | Chat Completions uses simpler IDs (no pipes) |
| Thinking level → reasoning effort string | Yes | Extract to shared helper |
| SSE line parsing (`data: ...` extraction) | Yes | Extract to `providers/sse.rs` |
| API key resolution by provider | Yes | Extract to `providers/auth.rs` |

Create a small `providers/shared.rs` module for truly shared helpers:

```rust
// providers/shared.rs
pub fn parse_sse_line(line: &str) -> Option<&str>;        // Extract after "data: "
pub fn is_sse_done(line: &str) -> bool;                    // Check "data: [DONE]"
pub fn thinking_level_to_effort(level: &ThinkingLevel) -> &str;
pub fn resolve_api_key(provider: &str, explicit: Option<&str>) -> Result<String>;
```

## Migration Path

This is purely additive. No existing behavior changes:

1. **Add `openai_chat.rs` + `openai_chat_shared.rs`** — new provider implementation
2. **Add `shared.rs`** — extract common SSE/auth helpers (optional, can defer)
3. **Register in `mod.rs`** — one line addition
4. **Add catalog entries** — new models with `api: "openai-chat"`
5. **Update agent_builder** — new provider → env var mappings
6. **Delete `kimi.rs` stub** — Kimi models go through OpenRouter/OpenAI-chat, not a bespoke provider

Existing `openai-responses` and `anthropic-messages` models continue to work unchanged.
Users pick their preferred path by model selection:
- `gpt-5.4-mini` (openai-responses) — direct OpenAI with all Responses API features
- `claude-sonnet-4-6` (anthropic-messages) — direct Anthropic with caching + thinking signatures
- `google/gemini-2.5-pro` (openai-chat via OpenRouter) — any model, unified format

## Estimated Scope

| Component | Lines | Effort |
|-----------|-------|--------|
| `openai_chat.rs` | ~150 | Provider struct, key resolution, stream methods |
| `openai_chat_shared.rs` | ~500 | Message conversion, SSE parsing, tool handling |
| `shared.rs` | ~60 | Extracted SSE + auth helpers |
| `catalog.rs` additions | ~100 | 10-15 OpenRouter models |
| `mod.rs` + `types.rs` | ~10 | Registration + constant |
| `agent_builder.rs` | ~10 | New provider key mappings |
| Tests | ~200 | SSE parsing, message conversion, serde round-trips |
| **Total** | **~1,030** | |

For comparison: the existing OpenAI Responses provider is 1,390 lines. Chat Completions
is significantly simpler (no item IDs, no pipe-separated tool call IDs, no reasoning
signature reconstruction, no service tiers, no prompt caching keys). The 650-line
estimate reflects that simplicity.

## Future Considerations

- **Dynamic model discovery**: OpenRouter has a `/models` endpoint. Could fetch at
  startup instead of hardcoding catalog entries. Defer until model list feels stale.
- **OpenRouter provider routing**: `provider: {order: [...], only: [...]}` in request
  body. Add when users need to pin upstream providers.
- **Strict schema mode**: Some providers support `strict: true` on tool parameters
  (forces exact JSON schema compliance). Add when we see argument parsing failures.
- **Kimi routing**: Keep Kimi on the generic OpenRouter/OpenAI-chat path unless there is a strong reason to carry a dedicated direct endpoint.
