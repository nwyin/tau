# Coding Agent Harness Feature Comparison

Feature-level comparison across 8 harnesses: **tau**, **kimi-cli**, **pi-mono**, **oh-my-pi**, **pi_agent_rust**, **codex**, **crush**, and **opencode**.

Data collected 2026-03-19 by reading each harness's source code.

---

## Tools

| Tool | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|------|-----|----------|---------|----------|---------------|-------|-------|----------|
| bash/shell | yes | yes | yes | yes | yes | yes | yes | yes |
| file read | yes | yes | yes | yes | yes | yes | yes (view) | yes |
| file write | yes | yes | yes | yes | yes | via shell | yes | yes |
| file edit (exact match) | yes | yes | yes | yes | yes | -- | yes | yes |
| multi-edit | -- | yes | -- | -- | -- | -- | yes | yes |
| hashline edit | yes | -- | -- | yes (invented it) | yes (default) | -- | -- | -- |
| apply_patch (unified diff) | -- | -- | -- | yes (patch mode) | -- | yes (primary) | -- | yes (GPT models) |
| grep/rg | yes | yes | yes | yes | yes | yes | yes | yes |
| glob/find | yes | yes | yes | yes | yes | -- | yes | yes |
| ls | -- | -- | yes | -- | yes | yes | yes | -- |
| web fetch | -- | yes | -- | yes | -- | -- | yes | yes |
| web search | -- | yes | -- | yes (10 providers) | -- | yes (Responses API) | yes | yes (Exa) |
| browser automation | -- | -- | -- | yes (Puppeteer) | -- | -- | -- | -- |
| LSP tool | -- | -- | -- | yes (11 ops) | -- | -- | yes (references) | yes (experimental) |
| notebook edit | -- | -- | -- | yes | -- | -- | -- | -- |
| python/IPython | -- | -- | -- | yes | -- | -- | -- | -- |
| JS REPL | -- | -- | -- | -- | -- | yes (persistent) | -- | -- |
| ast-grep search/edit | -- | -- | -- | yes | -- | -- | -- | -- |
| image generation | -- | -- | -- | yes (Gemini etc.) | -- | -- | -- | -- |
| view image | -- | yes | -- | -- | -- | yes | -- | -- |
| ssh | -- | -- | -- | yes | -- | -- | -- | -- |
| calculator | -- | -- | -- | yes | -- | -- | -- | -- |
| todo/plan tracking | -- | yes | -- | yes | -- | yes (update_plan) | yes | yes |
| sub-agent spawn | -- | yes | ext example | yes (8 types) | -- | yes (spawn/wait/send) | -- | yes |
| batch parallel tools | -- | -- | -- | -- | -- | -- | -- | yes (25 concurrent) |
| download | -- | -- | -- | -- | -- | -- | yes | -- |
| sourcegraph | -- | -- | -- | -- | -- | -- | yes | -- |
| code search | -- | -- | -- | -- | -- | -- | -- | yes (Exa) |
| checkpoint/rewind | -- | -- | -- | yes | -- | -- | -- | -- |
| cancel background job | -- | yes | -- | yes | -- | -- | yes | -- |
| artifacts | -- | -- | -- | yes | -- | yes | -- | -- |
| request user input | -- | yes | -- | yes (ask) | -- | yes | -- | -- |
| MCP tools (dynamic) | -- | yes | -- | yes | stub | yes | yes | yes |
| custom tools (extensions) | -- | yes | yes | yes | yes | via MCP/plugins | -- | yes |

**Tool count (built-in)**: tau 6 | kimi-cli 17 (default agent) | pi-mono 7 | oh-my-pi ~25 | pi_agent_rust 8 | codex ~16 | crush ~16 | opencode ~15

---

## Edit Strategy

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Exact string replace | yes | yes | yes | yes | yes | -- | yes | yes |
| Fuzzy match fallback | -- | -- | yes | -- | -- | yes (context matching) | -- | yes (9 strategies) |
| Hashline (hash-anchored) | yes | -- | -- | yes (default) | yes (default) | -- | -- | -- |
| Unified diff / patch | -- | -- | -- | yes (patch mode) | -- | yes (primary) | -- | yes (GPT models) |
| Multi-edit (batch) | -- | yes | -- | -- | -- | -- | yes | yes |
| Switchable edit mode | yes | -- | -- | yes | -- | -- | -- | -- |
| LSP format-on-write | -- | -- | -- | yes | -- | -- | -- | -- |
| LSP diagnostics-on-edit | -- | -- | -- | yes | -- | -- | yes | yes |
| Ghost snapshot (per-turn git commit) | -- | -- | -- | -- | -- | yes | -- | -- |

---

## Context Management

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Auto-compaction | -- | yes | yes | yes | yes | yes | yes | yes |
| Manual /compact | -- | yes | yes | yes | yes | yes | yes | yes |
| LLM-based summarization | -- | yes | yes | yes | yes | yes | yes | yes |
| Background compaction | -- | -- | -- | -- | yes | -- | -- | -- |
| Tool output pruning | -- | -- | -- | -- | -- | -- | -- | yes |
| Context overflow recovery | -- | -- | yes | yes | -- | -- | -- | yes |
| Context promotion (model upgrade) | -- | -- | -- | yes | -- | -- | -- | -- |
| TTSR (pattern-triggered rules) | -- | -- | -- | yes | -- | -- | -- | -- |
| Autonomous memory (cross-session) | -- | -- | -- | yes | -- | yes | -- | -- |
| Branch summarization | -- | -- | yes | -- | yes | -- | -- | -- |
| Thinking level control | -- | yes | yes | yes | yes | yes (reasoning effort) | yes | -- |

---

## Sub-agents / Parallel Execution

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Sub-agent spawning | -- | yes | ext example | yes (8 types) | -- | yes (spawn/wait/send/resume/close) | -- | yes |
| Max concurrent sub-agents | -- | -- | -- | 32 | -- | -- | -- | -- |
| Background async jobs | -- | yes (shell tasks, 4 default) | -- | yes (100 max) | -- | -- | -- | -- |
| Isolation (worktree) | -- | -- | -- | yes | -- | -- | -- | yes |
| Isolation (fuse overlay) | -- | -- | -- | yes | -- | -- | -- | -- |
| Swarm orchestration | -- | -- | -- | yes (YAML pipelines) | -- | -- | -- | -- |
| Parallel tool calls | -- | -- | -- | -- | yes (8 concurrent) | yes (read/write lock) | yes | yes (batch, 25) |
| Plan→build agent switch | -- | yes | -- | -- | -- | yes (/plan) | -- | yes |
| Inter-agent messaging | -- | -- | -- | -- | -- | yes (send_input) | -- | -- |

---

## Permission Model

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Per-tool permissions | -- | yes | ext example | yes | yes (capability policy) | yes | yes | yes |
| Allow once / always | -- | yes (approve for session) | -- | -- | yes (with expiry) | -- | -- | yes |
| Approval modes (suggest/auto/full) | -- | -- | -- | -- | -- | yes (4 levels) | -- | -- |
| Guardian auto-reviewer | -- | -- | -- | -- | -- | yes (GPT-5.4 risk scoring) | -- | -- |
| Bash command AST parsing | -- | -- | -- | -- | yes (ast-grep) | -- | -- | yes (tree-sitter) |
| Secret redaction | -- | -- | -- | yes | yes | -- | -- | -- |
| Sandbox (OS-level) | -- | -- | ext example | -- | -- | yes (seatbelt/landlock/restricted token) | -- | -- |
| Network proxy (domain filtering) | -- | -- | -- | -- | yes | yes (HTTP+SOCKS5, MITM) | -- | -- |
| Plan mode (read-only) | -- | yes | ext example | yes | -- | yes | -- | yes |
| Exec policy rules engine | -- | -- | -- | -- | -- | yes (TOML allowlist/denylist) | -- | -- |
| Extension capability policy | -- | -- | -- | -- | yes (safe/balanced/permissive) | -- | -- | -- |
| Risk controller (anomaly detection) | -- | -- | -- | -- | yes | -- | -- | -- |

---

## Session Management

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Persistence format | JSONL | JSONL + `state.json` + `wire.jsonl` | JSONL (v3) | JSONL | JSONL (v3) + segment store + SQLite | SQLite | SQLite | SQLite |
| Session resume | yes | yes | yes | yes | yes | yes | yes | yes |
| Session picker (fuzzy) | -- | yes (browser/search) | yes | -- | yes | -- | yes | yes |
| Branch/fork tree | -- | yes | yes | yes | yes | yes (fork) | -- | -- |
| Session naming | -- | -- | yes | yes | yes | yes (rename) | -- | -- |
| Session sharing | -- | yes (ZIP/Markdown export) | yes (gist) | -- | yes (gist) | -- | yes | yes |
| HTML export | -- | -- | yes | -- | yes | -- | -- | -- |
| Headless/print mode | yes | yes | yes | -- | yes | yes (exec) | -- | yes |
| RPC mode | -- | yes (ACP + Wire) | yes | -- | yes | -- | -- | -- |
| App server (HTTP) | -- | yes (web + vis) | -- | -- | -- | yes | -- | yes |
| Stats (--stats) | yes | -- | yes | -- | yes | yes | yes | yes |
| Session undo/revert | -- | -- | -- | yes (checkpoint) | -- | yes (ghost snapshot) | -- | yes (git snapshot) |

---

## Provider / Model Support

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Anthropic | yes | yes | yes | yes | yes | -- | yes | yes |
| OpenAI | yes | yes | yes | yes | yes | yes (Responses API) | yes | yes |
| Google/Gemini | -- | yes | yes | yes | yes | -- | -- | yes |
| AWS Bedrock | -- | -- | -- | -- | yes | -- | -- | yes |
| Azure OpenAI | -- | -- | -- | -- | yes | -- | -- | yes |
| OpenRouter | -- | yes (OpenAI-compat) | yes | -- | yes | -- | yes | yes |
| GitHub Copilot | -- | -- | yes | yes | yes | -- | yes | yes |
| Ollama / local models | -- | -- | -- | -- | -- | yes | -- | -- |
| 50+ OpenAI-compat presets | -- | -- | -- | -- | yes | -- | -- | -- |
| Custom model config | -- | yes | yes | yes | yes | yes | yes | yes |
| Model cycling (Ctrl+P) | -- | yes | yes | yes | yes | -- | yes | -- |
| OAuth flows | -- | yes | yes | yes | yes | yes | yes | yes |
| Vercel AI SDK abstraction | -- | -- | -- | -- | -- | -- | -- | yes |

---

## Extension / Plugin System

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Extension API | -- | yes (plugins + YAML agents) | yes (TS) | yes (TS) | yes (JS/QuickJS + Rust + WASM) | yes (plugins) | -- | yes (TS plugin) |
| Skills (markdown) | -- | yes | yes | yes | yes | yes | yes | yes |
| Custom tool registration | -- | yes | yes | yes | yes | via MCP | -- | yes |
| Package manager (install/remove) | -- | -- | yes | yes | yes | -- | -- | -- |
| Hook system | -- | -- | yes (30+ events) | yes (20+ events) | -- | yes (5 lifecycle hooks) | -- | yes (plugin hooks) |
| Custom themes | -- | -- | yes | yes | yes | yes (.tmTheme) | -- | yes |
| Prompt templates | -- | yes | yes | yes | yes | -- | -- | -- |
| Custom agents (markdown) | -- | yes (YAML) | -- | yes | -- | -- | -- | yes |
| MCP client | -- | yes | stub | yes | stub | yes (stdio + HTTP) | yes | yes |
| MCP server mode | -- | -- | -- | -- | -- | yes | -- | -- |
| Apps/connectors marketplace | -- | -- | -- | -- | -- | yes | -- | -- |
| Extension index/registry | -- | -- | -- | -- | yes (NPM/GitHub) | -- | -- | -- |

---

## UI/UX

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| TUI framework | basic REPL | prompt-toolkit shell | custom (diff renderer) | pi-tui | charmed (bubbletea port) | ratatui (Rust) | bubbletea | SolidJS (@opentui) |
| Themes | -- | -- | yes (JSON) | yes (65+) | yes (3 built-in) | yes (.tmTheme) | -- | yes (33) |
| Markdown rendering | -- | -- | yes | yes | yes | yes | yes | -- |
| Syntax highlighting | -- | -- | yes (cli-highlight) | yes (syntect/Rust) | yes (glamour) | yes (pulldown-cmark) | yes | -- |
| Terminal image display | -- | -- | yes (Kitty/iTerm2) | yes (Kitty/iTerm2) | yes (Kitty/iTerm2) | -- | yes | -- |
| Diff view | -- | yes (approval diff preview) | unified | unified | unified | yes (syntax-highlighted) | unified + split | -- |
| Clipboard paste (text+image) | -- | yes | yes | yes | yes | yes (/copy) | -- | -- |
| External editor | -- | yes | yes | -- | yes | yes ($VISUAL) | -- | -- |
| Autocomplete | -- | yes | yes | yes | yes | yes (nucleo fuzzy) | yes | -- |
| Configurable keybindings | -- | -- | yes | yes | yes | -- | -- | yes |
| Speech-to-text / voice | -- | -- | -- | yes (Whisper) | -- | yes (realtime, gpt-4o-mini-transcribe) | -- | -- |
| Desktop notifications | -- | -- | -- | -- | -- | yes (session hook) | yes | -- |
| Web UI | -- | yes | yes (Lit) | -- | -- | yes (app-server + Electron) | -- | -- |
| IDE integration | -- | yes (ACP + VS Code) | yes (RPC) | -- | yes (RPC) | yes (app-server) | -- | yes (ACP for Zed) |

---

## Sandbox / Security

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| macOS seatbelt | -- | -- | ext example | -- | -- | yes | -- | -- |
| Linux bubblewrap + landlock | -- | -- | ext example | -- | -- | yes | -- | -- |
| Windows restricted token | -- | -- | -- | -- | -- | yes | -- | -- |
| Network namespace isolation | -- | -- | -- | -- | -- | yes | -- | -- |
| MITM proxy with domain filtering | -- | -- | -- | -- | yes | yes | -- | -- |
| Process hardening (no ptrace, no coredump) | -- | -- | -- | -- | -- | yes | -- | -- |
| OTEL audit telemetry | -- | -- | -- | -- | -- | yes | -- | -- |

---

## Other Notable

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Language | Rust | Python | TypeScript | TypeScript | Rust | Rust + TypeScript | Go | TypeScript |
| Native addon | -- | -- | -- | yes (N-API Rust) | -- | -- (pure Rust core) | -- | -- |
| SDK/embedding API | -- | yes (Python SDK + ACP) | yes | -- | yes | yes (TS SDK) | -- | -- |
| Feature flags system | -- | -- | -- | -- | -- | yes (50+ flags, lifecycle stages) | -- | yes (env var flags) |
| Doctor/health check | -- | -- | -- | -- | yes | -- | -- | -- |
| Trace JIT (hostcall optimization) | -- | -- | -- | -- | yes | -- | -- | -- |
| Property-based testing | yes | -- | -- | -- | yes | -- | -- | -- |
| Loom concurrency tests | -- | -- | -- | -- | yes | -- | -- | -- |
| jemalloc | -- | -- | -- | -- | yes | -- | -- | -- |

---

## Summary: What tau needs for daily-driver status

Based on the table above, here are the features that appear across 4+ harnesses (table stakes for a daily driver), grouped by priority:

### Must-have (present in 5+ harnesses)

1. **Auto-compaction** — Every harness except tau has this. Without it, long sessions hit context limits and die. This is the single biggest gap.
2. **Permission model** — At minimum, per-tool allow/deny. Every harness except tau has some form of this.
3. **Sub-agent spawning** — kimi-cli, oh-my-pi, codex, and opencode have it natively; pi-mono has an extension example. Parallelism is the difference between "wait 5 minutes" and "wait 1 minute."
4. **MCP support** — kimi-cli, oh-my-pi, codex, crush, and opencode all expose this. Unlocks external tool servers without writing code.
5. **Skills (markdown)** — All harnesses except tau. Reusable prompt snippets loaded as slash commands.

### High-value (present in 3-4 harnesses, high daily-driver impact)

6. **Web fetch/search** — kimi-cli, oh-my-pi, codex, crush, and opencode. Needed for looking up docs, APIs, error messages.
7. **Todo/plan tracking** — kimi-cli, oh-my-pi, codex, crush, and opencode. Keeps the agent organized on multi-step tasks.
8. **LSP diagnostics on edit** — oh-my-pi, crush, opencode. Immediate feedback on syntax/type errors after edits.
9. **Session picker / resume UX** — 6 harnesses now have a real picker, search flow, or browser session manager. tau has `--resume` but no browser.
10. **Session undo/revert** — codex (ghost snapshot), oh-my-pi (checkpoint), opencode (git snapshot). Safety net for when the agent breaks things.
11. **Fuzzy edit fallback** — pi-mono, codex, opencode. Models frequently produce slightly-wrong whitespace; fuzzy matching saves retries.

### Nice-to-have (quality-of-life)

12. **Themes** — pi-mono, oh-my-pi, pi_agent_rust, codex, opencode.
13. **Terminal image display** — pi-mono, oh-my-pi, pi_agent_rust, crush.
14. **Markdown rendering** — pi-mono, oh-my-pi, pi_agent_rust, codex, crush.
15. **Configurable keybindings** — pi-mono, oh-my-pi, pi_agent_rust, opencode.
16. **Multi-edit (batch)** — kimi-cli, crush, and opencode.
17. **Session branching/fork** — kimi-cli, pi-mono, oh-my-pi, pi_agent_rust, and codex.
18. **Sandbox (OS-level)** — codex has the gold standard here; pi-mono has an extension example.
19. **Voice / speech-to-text** — oh-my-pi (Whisper), codex (realtime).

### tau's unique advantages to preserve

- **Hashline edit** — Only oh-my-pi and pi_agent_rust share this. Switchable edit mode for A/B comparison is unique to tau.
- **Three-crate layered architecture** — Clean separation of LLM primitives, agent loop, and coding harness. Most harnesses are monolithic.
- **Property-based testing** — Only tau and pi_agent_rust have proptest coverage.
- **Minimal footprint** — Easier to fork, hack, and understand than any other harness.
