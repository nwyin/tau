# Roadmap

Current as of 2026-03-18.

## Architecture

tau is three crates, intentionally layered for reuse:

```
ai              LLM streaming (providers, models, event streams)
  │
agent           generic agent loop (tools, steering, events)
  │
coding-agent    coding tools + CLI  ← you are here
```

The `agent` crate is generic — it has no opinion about what tools exist or what domain the agent operates in. Different harnesses (coding, data, research) can be built on top by providing different tool sets and system prompts.

pi-mono's architecture is the reference:
- `ai` ← `packages/ai` (done)
- `agent` ← `packages/agent` (done)
- `coding-agent` ← minimal subset of `packages/coding-agent` (in progress)

We intentionally skip pi-mono's web UI, proxy support, Slack bot, TUI, session branching, extensions, skills, and package system. tau is a local-only harness.

## Completed

- [x] `ai` crate: type system, EventStream, provider registry, model catalog
- [x] OpenAI Responses provider: full SSE parsing, tool call ID normalization, reasoning, cost calculation
- [x] `agent` crate: agent loop, Agent struct, steering/follow-up queues, event system, tool wiring
- [x] `stream_fn` injection for mock-based testing
- [x] `coding-agent` crate: BashTool, FileReadTool, FileWriteTool, interactive REPL
- [x] CI: GitHub Actions (test, clippy, fmt), pre-commit hook
- [x] Test policy: offline-first, fixture-based contracts, live tests double opt-in gated

## In progress

- [ ] `--prompt` headless mode for coding-agent (non-interactive, for benchmarks/scripting)

## Next priorities

### More tools
- GrepTool — search file contents by regex
- FindTool — find files by glob pattern
- FileEditTool — diff-based editing (old_string → new_string), avoids rewriting entire files

These match the standard coding agent toolkit from pi-mono's `packages/coding-agent/src/core/tools/`.

### Benchmark integration
- terminal-bench adapter: Python shim that installs tau's binary in a Docker container and drives it via `--prompt`
- Gives a concrete quality number for the minimal harness against 240+ real tasks
- Requires `--prompt` mode to be complete first

### Session persistence
- JSONL message log written to disk during agent execution
- Reload on next invocation via `--session <id>` flag
- Enables resuming work across process restarts
- No branching, no compaction — just linear replay for now

### Anthropic provider
- Implement `AnthropicMessagesProvider` following the same pattern as OpenAI
- Unblocks Anthropic models (Claude) and cross-provider handoff tests
- Kimi provider is lower priority

## Design decisions

### Why not port all of pi-mono's coding-agent?

pi-mono's `packages/coding-agent` is ~120 source files — a full product with TUI, session branching, compaction, extensions, skills, themes, RPC, OAuth, and package management. tau doesn't need any of that. It needs:
- Tools that let an LLM interact with the filesystem and shell
- A way to run it (REPL + headless)
- Good enough to benchmark

### Why keep `agent` generic?

The user plans to build different agent harnesses in the future — not just coding agents. Keeping tool implementations, CLI, and domain-specific logic in separate crates means `agent` stays reusable. A new harness is ~80 lines of glue: pick tools, pick a system prompt, wire up events.

### Live API test policy

Live provider tests require double opt-in: `OPENAI_API_KEY` + `RUN_LIVE_PROVIDER_TESTS=1`. Unit tests are fully offline and deterministic. Fixture-based contract tests validate provider wire formats without network calls. See `docs/test-migration-todo.md` for details.

### Why only OpenAI for now?

tau targets OpenAI, Anthropic, and Kimi. OpenAI was implemented first because it has the broadest model selection in the catalog and the Responses API is well-documented. Anthropic is next. Kimi is low priority unless a concrete use case emerges.
