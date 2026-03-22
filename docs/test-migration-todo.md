# Test Migration TODO

Scope: `tau` intentionally targets three protocol surfaces:
- direct OpenAI Responses
- direct Anthropic Messages
- OpenAI-compatible Chat Completions endpoints such as OpenRouter

There is no separate Kimi provider. Kimi-family models are expected to run through the OpenRouter/OpenAI-chat path.

## 1. Agent mock-stream coverage

- [x] Add `stream_fn` injection to `AgentLoopConfig`.
- [x] Thread `stream_fn` through `AgentOptions` and `Agent`.
- [x] Port `tau/agent/tests/agent_loop.rs` mock-stream cases.
- [x] Port `tau/agent/tests/agent_test.rs` streaming and continue cases.
- [x] Verify with `cargo test -p agent --tests`.

## 2. Agent e2e parity for supported providers

- [x] Port tool execution coverage into `tau/agent/tests/e2e.rs`.
- [x] Port abort coverage into `tau/agent/tests/e2e.rs`.
- [x] Port state update / event coverage into `tau/agent/tests/e2e.rs`.
- [x] Port multi-turn context retention coverage into `tau/agent/tests/e2e.rs`.
- [x] Keep provider matrix limited to direct OpenAI, direct Anthropic, and OpenAI-compatible chat surfaces as `tau` support evolves.
- [x] Agent tool definitions wired through to LLM context in `stream_assistant_response()` (commit `48ba375`).
- [x] OpenAI Responses provider implemented and registered (commit `cac395b`).
- [x] Anthropic provider implemented (commit 776127d).
- [x] Drop the separate Kimi-provider plan; Kimi rides the OpenRouter/OpenAI-chat backend.

## 3. High-value AI regression ports

- [x] Port `tau/ai/tests/cache_retention.rs`.
- [x] Port `tau/ai/tests/unicode_surrogate.rs`.
- [x] Port `tau/ai/tests/openai_responses_reasoning_replay_e2e.rs`.
- [x] Port `tau/ai/tests/anthropic_tool_name_normalization.rs`.
- [x] Port `tau/ai/tests/xhigh.rs`.
- [x] Port `tau/ai/tests/context_overflow.rs`.

## 4. AI provider-specific follow-ups

- [x] Expand `tau/ai/tests/stream_test.rs` tool-call coverage.
- [x] Port `tau/ai/tests/image_tool_result.rs`.
- [x] Port `tau/ai/tests/interleaved_thinking.rs`.
- [x] Decide whether `openai-completions` compatibility tests belong in `tau`'s minimal scope.
Decision: keep generic stream-contract coverage, but do not chase broad `openai-completions` compatibility parity unless `tau` grows a concrete completions implementation.

## 5. Live API test policy

Established 2026-03-18. Live provider tests require double opt-in: both the API key (e.g. `OPENAI_API_KEY`) AND `RUN_LIVE_PROVIDER_TESTS=1`.

Pared from 33 `#[ignore]` tests to 8 (commit `22b00fe`):
- 2 in `agent/tests/e2e.rs` — `openai_basic_prompt`, `openai_tool_execution`
- 2 in `ai/tests/cross_provider_handoff.rs` — cross-provider history forwarding
- 1 in `ai/tests/tool_call_id_normalization.rs` — live OpenAI ID format validation
- 3 in `ai/tests/openai_responses_provider.rs` — provider integration smoke tests

Deleted test files (all were live-only, no offline tests): `abort.rs`, `tokens.rs`, `total_tokens.rs`, `empty.rs`, `tool_call_without_result.rs`. Deleted 13 provider-specific agent e2e stubs that were outside the maintained direct-provider surface.

Fixture-based contract tests in `ai/tests/openai_responses_provider.rs` provide offline coverage for SSE parsing, message conversion, tool call ID normalization, reasoning effort, and cost calculation.

## 6. Deferred by design

- [ ] Leave non-target-provider ports out unless `tau` expands beyond direct OpenAI / Anthropic plus the generic OpenAI-compatible chat backend.
- [ ] Add more integration coverage for the `openai-chat` backend as additional OpenRouter model families become important.
