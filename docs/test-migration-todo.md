# Test Migration TODO

Scope: `tau` intentionally targets only OpenAI, Anthropic, and Kimi. Missing tests for Google, Bedrock, GitHub Copilot, and other non-target providers are intentionally deferred.

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
- [x] Keep provider matrix limited to OpenAI / Anthropic / Kimi as `tau` support evolves.
- [ ] Provider implementations still need to be registered in `tau/ai` before these live e2e tests can execute.
- [ ] Agent tool definitions still need conversion into `ai::Tool` in `stream_assistant_response()` before live tool-execution e2e can pass.

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

## 5. Deferred by design

- [ ] Leave non-target-provider ports out unless `tau` expands beyond OpenAI / Anthropic / Kimi.
