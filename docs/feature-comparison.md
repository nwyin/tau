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

### Tool name mapping

Actual tool names used by each harness (matters for training data transfer):

| Tool | tau | kimi-cli | pi-mono | oh-my-pi | crush | codex | opencode |
|------|:---:|:--------:|:-------:|:--------:|:-----:|:-----:|:-------:|
| bash/shell | bash | Shell | bash | bash | bash | shell | bash |
| file read | file_read | ReadFile | read | read | view | read_file | read |
| file edit | file_edit | StrReplaceFile | edit | edit | edit | apply_patch | edit *or* apply_patch |
| file write | file_write | WriteFile | write | write | write | (via apply_patch) | write |
| grep | grep | Grep | grep | grep | grep | grep_files | grep |
| glob/find | glob | Glob | find | find | glob | list_dir | glob |

Every harness converges on the same six core tools: shell execution, file read, file edit, file write, content search, and file search. The divergence is in what else gets added on top, and in tool naming — which affects whether models can transfer training from one harness to another.

---

## Edit Strategy

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Exact string replace | yes | yes | yes | yes | yes | -- | yes | yes |
| Fuzzy match fallback | yes (trimmed-cascade) | -- | yes | -- | -- | yes (context matching) | -- | yes (9 strategies) |
| Hashline (hash-anchored) | yes | -- | -- | yes (default) | yes (default) | -- | -- | -- |
| Unified diff / patch | -- | -- | -- | yes (patch mode) | -- | yes (primary) | -- | yes (GPT models) |
| Multi-edit (batch) | -- | yes | -- | -- | -- | -- | yes | yes |
| Switchable edit mode | yes | -- | -- | yes | -- | -- | -- | -- |
| LSP format-on-write | -- | -- | -- | yes | -- | -- | -- | -- |
| LSP diagnostics-on-edit | -- | -- | -- | yes | -- | -- | yes | yes |
| Ghost snapshot (per-turn git commit) | -- | -- | -- | -- | -- | yes | -- | -- |

### Edit strategy analysis

The most interesting divergence across harnesses. Same model, different edit format, wildly different scores.

**Exact string match** (tau, kimi-cli, pi-mono, crush, opencode for Claude): `{old_string, new_string}`. Simple for the model to understand. Fails when `old_string` appears multiple times or when the model hallucinates whitespace. pi-mono mitigates with fuzzy matching. kimi-cli adds batched replacement (list of exact replacements in one call). Low token cost per edit, high failure rate on large or repetitive files.

**Unified diff / patch** (codex, opencode for GPT): More expressive — multi-hunk edits in one call. Models frequently produce malformed patches (wrong line numbers, missing context). opencode uses a custom patch DSL with function signatures as context anchors instead of line numbers, plus multi-file operations (add, delete, move) in one call. Higher expressiveness, higher fragility.

**Hash-anchored lines** (oh-my-pi, tau): Every line tagged with a short content hash anchor. The model references lines by position+hash, which the tool validates. Eliminates ambiguity (no string matching), but requires re-read after every edit (hashes change). +8% avg across 16 models, 10x improvement for weak models.

**AST-aware edit** (oh-my-pi only): `ast_grep` and `ast_edit` tools operate on syntax tree patterns. Structural matching eliminates whitespace sensitivity entirely. Most precise mechanism, but only works for languages with ast-grep support.

**Model-aware switching** (opencode): Dynamically swaps tools based on which model is running. GPT gets `apply_patch`; Claude gets `edit` + `write`. The most pragmatic acknowledgment that different models have different tool-use strengths. Makes cross-model benchmarking harder since the tool surface isn't constant.

---

## Context Management

| Feature | tau | kimi-cli | pi-mono | oh-my-pi | pi_agent_rust | codex | crush | opencode |
|---------|-----|----------|---------|----------|---------------|-------|-------|----------|
| Auto-compaction | yes (mechanical) | yes | yes | yes | yes | yes | yes | yes |
| Manual /compact | -- | yes | yes | yes | yes | yes | yes | yes |
| LLM-based summarization | -- | yes | yes | yes | yes | yes | yes | yes |
| Background compaction | -- | -- | -- | -- | yes | -- | -- | -- |
| Tool output pruning | yes (truncate + mask) | -- | -- | -- | -- | -- | -- | yes |
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

**tau's position: harness ≠ orchestrator.**

The trend is to embed multi-agent orchestration into the harness itself
(Claude Code agent teams, Codex spawn/wait/send, opencode batch). tau
takes a deliberately different approach: the harness is a single-agent
primitive, and multi-agent orchestration lives in a separate layer
(Hive).

Rationale:

- **The orchestration problem isn't solved.** Every harness experiments
  differently (subagent tools, YAML pipelines, batch parallel). Embedding
  an unsettled abstraction into the foundation cements a bet. Keeping it
  separate lets us swap orchestration strategies without touching the
  agent loop.
- **Clean separation pays off in practice.** The tau↔Hive integration is
  ~1K LOC total (~800 in tau's serve mode, ~400 in Hive's backend
  adapter). Orchestration concerns (process pools, message routing,
  partial failure handling, lease management) stay in Hive. Agent
  concerns (tools, context, prompts, streaming) stay in tau.
- **Embedding leaks complexity.** A harness with native swarm support
  must manage process pools, shared filesystem state, inter-agent
  message routing, and partial failure recovery — all inside what should
  be a single-agent loop.

The one thing worth adding harness-native: **parallel tool execution**
(run N tool calls concurrently within a single turn). This covers the
80% case (3 greps in parallel, 5 file reads at once) without any
orchestration complexity. The 20% case (multiple agents on different
subtasks, communicating results) belongs in the orchestrator.

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

## Underexplored dimensions

The tables above capture discrete features, but several design surfaces
cut across all of them. These are harder to compare in a matrix but
matter enormously for daily-driver quality.

### System prompt engineering

The system prompt is arguably the biggest design surface — it IS the
product. Considerations:

- **Construction**: static template vs dynamic (adapts to loaded tools,
  cwd, project type, git state). kimi-cli injects tool-specific
  guidelines per tool. Claude Code's system prompt is ~8K tokens of
  carefully tuned instructions.
- **Project-level rules**: CLAUDE.md, `.cursorrules`, `.tau.md` — how
  persistent per-project instructions are loaded, where they're injected,
  whether they survive compaction.
- **Per-model adaptation**: different models need different guidance.
  Anthropic models benefit from XML tags, OpenAI from structured JSON
  examples. Some harnesses swap prompt sections per provider.
- **Guideline injection**: "read before edit", "don't over-engineer",
  "use existing patterns" — these behavioral guidelines are what make an
  agent feel helpful vs annoying.

### Tool result formatting

How tool output is presented back to the model is a huge lever. Gets no
attention in feature comparisons but determines whether the model
recovers or spirals.

- **Line numbers**: file_read with `cat -n` style vs raw content. Lets
  the model reference specific lines for edits.
- **Truncation markers**: `[truncated N lines]`, `Total output: N lines`
  so the model knows it's seeing a subset and can request more.
- **Error formatting**: actionable hints ("file not found — did you mean
  X?") vs raw stderr. Some harnesses parse common errors and suggest
  fixes.
- **Structured metadata**: tool results can carry `details` (JSON
  metadata) separate from `content` (what the model sees). tau has this
  via `AgentToolResult.details` but it's underused.

### Error recovery and self-correction

What happens when a tool call fails? Most harnesses just pass the error
back and hope the model adapts. But there's design space here:

- **Retry policies**: auto-retry transient failures (network, rate
  limits) with backoff. Distinct from "model tries again."
- **Context overflow recovery**: detect overflow error, auto-compact,
  retry the same request. Codex and opencode do this.
- **Edit conflict resolution**: when file_edit fails (match not found),
  some harnesses (opencode) try fuzzy matching or re-read the file
  before failing.
- **Cascading fallbacks**: model downgrades on rate limit, provider
  fallback chains.
- **Self-correction loops**: some harnesses detect "the model is stuck"
  (same tool call repeated 3x) and inject a nudge or abort.

### Conversation steering and dynamic injection

Keeping the agent on track over long sessions without burning context.

- **System reminders**: kimi-cli injects `<system-reminder>` tags before
  each step with task context, plan state, recent diagnostics.
- **Git status injection**: some harnesses inject current branch/status
  before each turn so the model stays aware.
- **Diagnostic injection**: LSP errors injected after edits (oh-my-pi,
  crush, opencode) so the model self-corrects immediately.
- **Task context refresh**: re-injecting the original task description
  periodically so the model doesn't drift.
- **TTSR (pattern-triggered rules)**: oh-my-pi injects specific rules
  when it detects certain patterns (e.g., "you're editing a test file,
  remember to run tests after").

### Project detection and onboarding

How a harness learns about a new project. Determines first-impression
quality.

- **Language/framework detection**: package.json, Cargo.toml, pyproject.toml
  → adapt tools, system prompt, suggestions.
- **Git state**: branch, dirty files, recent commits — injected into
  context for awareness.
- **Config files**: CLAUDE.md, .cursorrules, project-level settings
  that persist across sessions.
- **Repo map / codebase summary**: aider's approach — build a structural
  map of the codebase and inject a compact summary. Expensive but
  effective for "understand the whole project" tasks.

### Cost control and model routing

Running an agent is expensive. Controlling costs matters for daily use.

- **Cost caps**: per-session or per-task token/dollar limits. pi_agent_rust
  has this.
- **Model routing**: use a cheap model for simple tasks (grep, file read)
  and expensive model for reasoning/edits. No harness does this well yet.
- **Prompt caching**: Anthropic's cache_control, OpenAI's response cache.
  Some harnesses (crush, pi_agent_rust) explicitly manage cache prefixes.
- **Token budgets for tools**: limit how much context a single tool result
  can consume (prevent one huge file read from dominating the window).

### Git safety

Preventing the agent from doing irreversible damage to the repo.

- **Ghost snapshots**: codex creates a git commit after each turn, enabling
  rollback to any point.
- **Auto-stash**: stash dirty state before risky operations.
- **Dangerous command detection**: blocking force push, `rm -rf`,
  `git reset --hard` unless explicitly allowed.
- **Worktree isolation**: run the agent in a git worktree so mistakes
  don't affect the main working tree.

### Pre-flight validation

Validating tool args before execution to prevent wasted turns.

- **File existence checks**: does the file exist before trying to edit?
- **Regex validation**: is the pattern valid before running grep?
- **Path containment**: is the path inside the project root? Prevents
  accidental reads/writes outside the workspace.
- **Argument type checking**: tool parameters declared as JSON Schema but
  models sometimes send wrong types.

### Streaming and incremental UX

How partial responses render. Pure UX but determines whether the tool
feels responsive or sluggish.

- **Text streaming**: show tokens as they arrive, not after completion.
- **Tool call preview**: show tool name and args before execution starts.
- **Progress indicators**: spinner or elapsed time for long tool calls
  (bash commands, large file reads).
- **Incremental diff**: show edits as they're proposed, not after applied.

### Observability and debugging

How you understand what the agent is doing and why.

- **Verbose mode**: show the full prompt sent to the API, raw
  request/response, token counts per message.
- **Trace files**: structured logs of every event (tau has this via
  trace.jsonl).
- **Token breakdown**: per-message token counts so you can see what's
  consuming context.
- **Tool call audit**: full args and results for every tool call, not
  just success/failure.
- **Cost tracking**: running total of dollars spent in the session.

### Caching and performance

Beyond prompt caching — harness-level performance.

- **File content caching**: avoid re-reading files that haven't changed.
  Use file mtime or git status to invalidate.
- **Tool result deduplication**: if the model calls the same grep twice,
  return cached result.
- **Warm starts**: pre-loading project context (repo map, recent git
  history) before the first user message.
- **Connection pooling**: reuse HTTP connections to API providers across
  turns.

### Multi-modal and rich content

Beyond text — handling images, PDFs, notebooks.

- **Image input**: screenshot analysis, diagram understanding. oh-my-pi,
  codex, pi_agent_rust support this.
- **Image generation**: oh-my-pi can generate images via Gemini.
- **PDF reading**: some harnesses can ingest PDFs directly.
- **Notebook support**: oh-my-pi has native Jupyter notebook editing.
- **Terminal screenshots**: capturing terminal output as images for
  visual debugging.

### Testing and quality assurance

How harnesses ensure their own quality.

- **Property-based testing**: tau and pi_agent_rust use proptest.
- **Concurrency testing**: pi_agent_rust uses loom for concurrency
  verification.
- **Benchmark suites**: standardized evals (SWE-bench, HumanEval,
  custom benchmarks).
- **Mutation testing**: some harnesses use mutmut or similar.
- **Trace replay**: replaying recorded traces to verify deterministic
  behavior.

---

## Summary: What tau needs for daily-driver status

Based on the table above, here are the features that appear across 4+ harnesses (table stakes for a daily driver), grouped by priority:

### Must-have (present in 5+ harnesses)

1. **~~Auto-compaction~~** — ✅ Implemented (mechanical: chars/4 estimation, tool output truncation, observation masking, turn-boundary eviction). LLM-based summarization and `/compact` command are future work.
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
11. **~~Fuzzy edit fallback~~** — ✅ Implemented (trimmed-cascade: trailing whitespace, both-side trim, unicode normalization). Benchmarks show 84% recovery on synthetic near-misses, 0% false positives, ~40μs. In practice never fires with current models (real failures are hallucinated content).

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

### tau's design philosophy

1. **Minimize the model's decision surface.** Fewer tools = fewer wrong choices = more predictable behavior. The model should spend tokens on the *task*, not on deciding *which tool to use*.
2. **Bash is the escape hatch.** Anything not worth a dedicated tool goes through bash. The threshold for adding a tool: it must be measurably better than the bash equivalent across benchmarks.
3. **Delegation lives outside the agent.** Sub-agents, planning, and coordination are handled by the hive orchestrator, not by giving the model tools to manage its own complexity.
4. **Edit strategy as a variable, not a constant.** Both exact-match and hashline editing as switchable modes. The bet is that having both in one harness enables direct A/B comparison — and that better edit reliability matters more than more tool variety.
5. **Benchmarking decides.** The toolset should grow based on measured impact, not feature parity with other harnesses.

### What to consider adding next

Based on convergence across harnesses and likely benchmark impact:

- **Implicit LSP diagnostics** — opencode's approach of wiring LSP feedback into the edit tool result (not as a separate tool). The model gets type error feedback for free, without tool-choice overhead.
- **Multiedit / batched replace** — kimi-cli, crush, and opencode all reduce round trips. Worth testing whether a batched exact-replace tool helps before jumping to patch mode.
- **Model-aware tool filtering** — opencode's dynamic tool composition. If tau supports multiple models with different edit strengths, conditional tool selection could help. Adds complexity.
- **Plan mode** — kimi-cli's version is stronger than a todo tool: read-only exploration plus explicit approval before writing. Worth studying for a safe "research first" mode.
