# claw-code vs tau: Comprehensive Comparison

A detailed inventory of every notable difference between **claw-code** (clean-room Rust port of leaked Claude Code) and **tau** (our independently-designed harness). Both are Rust CLI coding agents targeting the Anthropic API; the architectures diverge significantly in philosophy, scope, and capability.

> **claw-code** repo: `/Users/tau/projects/harnesses/claw-code/rust/`
> **tau** repo: `/Users/tau/projects/harnesses/tau/`

---

## 1. Design Philosophy

| Dimension | claw-code | tau |
|-----------|-----------|-----|
| Origin | Clean-room port of leaked Claude Code TypeScript source | Independent design, research-first harness |
| Goal | Replicate Claude Code's architecture in Rust | Study harness engineering; build a competitive daily-driver |
| API vendor lock | Anthropic-only | Multi-provider (Anthropic, OpenAI, OpenRouter, Groq, Together, DeepSeek) |
| Binary name | `claw` | `tau` |
| Default model | `claude-opus-4-6` | `gpt-5.4` |
| License | MIT | MIT |
| Maturity | Single commit, functional MVP, many stubs | Active development, 270+ tests, 10 benchmark suites |

claw-code faithfully mirrors the TypeScript Claude Code layout (config hierarchy, permission modes, hook schemas, MCP transport types). tau treats the Claude Code architecture as one reference among many (Codex, oh-my-pi, Slate, aider, pi-mono) and cherry-picks patterns that survive empirical evaluation.

---

## 2. Workspace Structure

### claw-code: 6 crates

```
rust/crates/
  api/              Anthropic HTTP client + SSE
  runtime/          Config, conversation loop, session, permissions, hooks, MCP, OAuth
  tools/            All 19 tools in one lib.rs (~3,200 lines)
  commands/         Slash-command registry
  rusty-claude-cli/ Binary entry point, REPL, TUI rendering
  compat-harness/   TS manifest extraction for parity auditing
```

### tau: 3 crates + external TUI

```
ai/                Provider-agnostic LLM streaming primitives
agent/             Generic agent loop, orchestration, episodes, context compaction
coding-agent/      Concrete coding harness: tools, CLI, TUI, prompts, skills, trace, RPC
  (depends on external `ruse` TUI crate at ../../tuis/ruse.sh/crates/ruse)
```

**Key difference:** tau enforces a strict three-layer separation where `ai/` knows nothing about agents and `agent/` knows nothing about coding. This makes the agent loop reusable for non-coding agents (data-agent, research-agent). claw-code's `runtime/` crate mixes agent loop logic with coding-specific concerns (file_ops, bash execution, CLAUDE.md discovery).

### File counts

| Metric | claw-code | tau |
|--------|-----------|-----|
| Rust source files | ~35 | ~126 |
| Rust LOC (approx) | ~22,800 | ~25,000+ |
| Test files | 1 (Python) | 24 (Rust) + 16 benchmark suites |
| Documentation files | 3 (.md) | 12+ (.md) + specs/ |
| Python files | 67 (porting workspace) | 135 (benchmarks) |

---

## 3. API Client & Provider Support

### Provider coverage

| Provider | claw-code | tau |
|----------|-----------|-----|
| Anthropic Messages API | Yes | Yes |
| OpenAI Responses API | No | Yes |
| OpenAI Chat Completions | No | Yes (OpenRouter, Groq, Together, DeepSeek) |
| OpenRouter | No | Yes (via Chat Completions) |

claw-code has a single `AnthropicClient` struct. tau has a trait-based `ApiProvider` registry with three concrete implementations, a global `OnceLock<RwLock<Registry>>`, and runtime provider registration/deregistration.

### API key resolution

| Source | claw-code | tau |
|--------|-----------|-----|
| `ANTHROPIC_API_KEY` | Yes | Yes |
| `OPENAI_API_KEY` | No | Yes |
| `OPENROUTER_API_KEY` | No | Yes |
| `GROQ_API_KEY` | No | Yes |
| `TOGETHER_API_KEY` | No | Yes |
| `DEEPSEEK_API_KEY` | No | Yes |
| Codex OAuth (ChatGPT backend) | No | Yes |

### SSE parsing

| Feature | claw-code | tau |
|---------|-----------|-----|
| Parser type | Stateful `SseParser` struct with `push(chunk)` | Per-provider `collect_sse_events()` async functions |
| Chunked parsing | Yes (explicit chunk boundary handling) | Yes (line-by-line from reqwest response) |
| Multiline JSON | Yes (joins `data:` lines) | Yes |
| Termination | `message_stop` event (Anthropic-specific) | Provider-specific: `message_stop` or `[DONE]` |
| Retry logic | Built-in exponential backoff (200ms base, 2s max, 2 attempts) | No built-in retry (relies on caller) |
| Property tests | No | Yes (proptest for SSE parsing and message serde) |

### Streaming abstraction

claw-code returns `Vec<StreamEvent>` synchronously from the stream call. tau uses a two-channel abstraction (`mpsc` for events + `oneshot` for terminal result) wrapped in `EventStream<T>` / `EventStreamSender<T>`, allowing fine-grained incremental consumption.

### Request building

| Feature | claw-code | tau |
|---------|-----------|-----|
| System prompt format | String | Array with `cache_control: ephemeral` |
| Tool caching | No | Yes (last tool gets `cache_control: ephemeral`) |
| Last-user-message caching | No | Yes (Anthropic ephemeral cache on last user block) |
| Prompt caching (OpenAI) | N/A | Yes (`prompt_cache_key` + `prompt_cache_retention: 24h`) |
| Service tier support | No | Yes (flex 0.5x, priority 2.0x multiplier) |

---

## 4. Model Catalog

| Dimension | claw-code | tau |
|-----------|-----------|-----|
| Total models | 3 aliases (opus, sonnet, haiku) | ~65 models across 3 providers |
| Anthropic models | 3 | 20+ (Claude 3 through 4.6, all variants) |
| OpenAI models | None | 50+ (GPT-4 through 5.4, o-series, Codex) |
| OpenRouter models | None | Gemini, Qwen, Grok, DeepSeek, Kimi, Llama |
| Model struct fields | Implicit (just ID string) | 12 fields: id, name, api, provider, base_url, reasoning, input types, cost, context_window, max_tokens, headers, compat |
| Pricing data | Hardcoded per-family (Haiku/Sonnet/Opus) | Per-model in catalog (input, output, cache_read, cache_write per 1M tokens) |
| Model registration | Static aliases only | Dynamic `register_model()` + global registry |

### Model aliases

claw-code maps `opus` -> `claude-opus-4-6`, `sonnet` -> `claude-sonnet-4-6`, `haiku` -> `claude-haiku-4-5-20251213`.

tau uses the full catalog with provider-qualified lookup: `find_model("claude-sonnet-4-6")` searches across all providers.

---

## 5. Extended Thinking / Reasoning

| Feature | claw-code | tau |
|---------|-----------|-----|
| Thinking support | No (not in API request building) | Yes, full support |
| Thinking levels | None | Minimal, Low, Medium, High, XHigh |
| Anthropic adaptive thinking | No | Yes (Opus 4.6+ gets `type: adaptive`) |
| Anthropic budget thinking | No | Yes (budget_tokens 1024-16000 for non-Opus) |
| OpenAI reasoning effort | No | Yes (effort clamping) |
| Thinking content blocks | No | Yes (`ContentBlock::Thinking` with signature for replay) |
| XHigh detection | No | Yes (GPT-5.2+, Opus 4.6+) |
| Configurable | No | Yes (`--thinking level`, config.toml `[agent.thinking]`) |

---

## 6. Agent Loop / Conversation Runtime

### Architecture

| Aspect | claw-code | tau |
|--------|-----------|-----|
| Core type | `ConversationRuntime<C, T>` generic over ApiClient + ToolExecutor traits | `Agent` wrapping `Arc<Mutex<AgentState>>` with pluggable functions |
| Execution model | Synchronous (tokio `block_on`) | Fully async (tokio tasks) |
| Tool execution | Sequential (one tool at a time in loop) | Parallel (`tokio::spawn` for all tool calls in a turn) |
| Event system | None (direct return of TurnSummary) | Full lifecycle events (AgentStart/End, TurnStart/End, MessageStart/Update/End, ToolExecutionStart/Update/End) |
| Event subscribers | None | Multiple concurrent subscribers via `agent.subscribe()` |
| Cancellation | None | `CancellationToken` propagated through entire loop |
| Steering messages | None | Mid-loop injection via `get_steering_messages` callback |
| Follow-up messages | None | Post-loop continuation via `get_follow_up_messages` callback |
| Pluggable LLM | Via trait object (`ApiClient`) | Via `stream_fn` injection (enables full mock testing) |
| Max iterations | `usize::MAX` | Configurable `max_turns` (config.toml, CLI) |

### Turn lifecycle

**claw-code:**
1. Append user message
2. Stream API response
3. Record usage
4. For each tool call (sequentially): check permission -> run pre-hook -> execute tool -> run post-hook -> add result
5. If no tool calls, break
6. Check auto-compaction threshold

**tau:**
1. Inject pending steering messages
2. Apply `transform_context` (e.g., compaction)
3. Stream API response via `stream_fn` or provider
4. Extract tool calls
5. Execute all tool calls in parallel via `tokio::spawn`
6. Collect results in original order
7. Emit events throughout
8. Check for steering-after-tools
9. Loop or break

The parallel tool execution in tau is a significant performance difference -- a turn with 5 file reads completes in the time of 1 read, not 5.

---

## 7. Tool System

### Tool trait

**claw-code:** No formal tool trait. Tools are a match arm in a giant `execute_tool(name, input)` function in `tools/src/lib.rs`. Tool specs are defined as `ToolDefinition` structs (name, description, input_schema).

**tau:** Formal `AgentTool` trait:
```rust
trait AgentTool: Send + Sync {
    fn name(&self) -> String;
    fn label(&self) -> String;
    fn description(&self) -> String;
    fn parameters(&self) -> Value;  // JSON Schema
    fn execute(&self, tool_call_id, params, signal, on_update) -> BoxFuture<Result<AgentToolResult>>;
}
```
Each tool is its own struct implementing the trait, with its own file.

### Tool inventory

| Tool | claw-code | tau | Notes |
|------|-----------|-----|-------|
| bash / shell | `bash` | `bash` | tau: 120s default, 2000 line / 30KB truncation. claw: sandbox support, background tasks |
| file read | `read_file` | `file_read` | tau: two output modes (line-number vs hashline). claw: single mode |
| file write | `write_file` | `file_write` | Both create parent dirs. claw generates structured patch + git diff |
| file edit | `edit_file` | `file_edit` | tau: two modes (replace + hashline). claw: replace only |
| glob | `glob_search` | `glob` | Both gitignore-aware, mtime-sorted. tau caps at 1000, claw also caps |
| grep | `grep_search` | `grep` | Both shell out to ripgrep. tau: 100 match default. claw: rich options (output_mode, multiline, offset, head_limit) |
| web fetch | `WebFetch` | `web_fetch` | tau: HTML stripping, entity decoding, 50KB cap. claw: reqwest blocking client |
| web search | `WebSearch` | `web_search` | tau: Exa API. claw: stub/placeholder |
| agent/subagent | `Agent` (stub) | `subagent` + `thread` | tau: both subprocess (`subagent`) and in-process (`thread`). claw: stub only |
| todo | `TodoWrite` | `todo` | tau: full state replacement protocol. claw: structured list |
| notebook edit | `NotebookEdit` | -- | claw only |
| config | `Config` | -- | claw only (get/set harness settings) |
| REPL | `REPL` | `py_repl` | tau: persistent Python kernel with reverse RPC (`tau.thread()`, `tau.query()`, etc.) |
| PowerShell | `PowerShell` | -- | claw only (Windows) |
| skill | `Skill` | -- | claw: loads local SKILL.md. tau: skills via system prompt injection |
| tool search | `ToolSearch` | -- | claw: deferred tool discovery |
| sleep | `Sleep` | -- | claw only |
| send message | `SendUserMessage`/`Brief` | -- | claw only |
| structured output | `StructuredOutput` | -- | claw only |
| query | -- | `query` | tau only: single-shot LLM call without tools (cheap routing) |
| document | -- | `document` | tau only: virtual in-memory documents for inter-thread sharing |
| log | -- | `log` | tau only: append to orchestration log |
| from_id | -- | `from_id` | tau only: retrieve completed thread/query episode by alias |

**Tool count:** claw-code has 19 tool specs (many stubs). tau has 16 tools (all implemented).

### Edit strategy

| Feature | claw-code | tau |
|---------|-----------|-----|
| Replace mode | Yes (exact string match) | Yes (exact + fuzzy fallback cascade) |
| Hashline mode | No | Yes (hash-anchored line edits, +8% accuracy avg, 10x for weak models) |
| Fuzzy matching | No | Yes (trim_end -> trim_both -> unicode normalization) |
| Hash algorithm | N/A | xxHash32 with custom 16-char alphabet (ZPMQVRWSNKTXJBYH) |
| Multi-edit per call | `replace_all` flag | Hashline: array of `{op, pos, end, lines}` edits applied bottom-up |
| File context on error | No | Yes (shows surrounding lines when match not found) |
| Structured patch output | Yes (unified diff hunks) | No |
| Git diff output | Yes (optional) | No |

---

## 8. Context Management & Compaction

| Feature | claw-code | tau |
|---------|-----------|-----|
| Token estimation | API response `input_tokens` field | `chars / 4` heuristic (industry standard) |
| Budget factor | None (uses raw token count) | 0.75 (absorbs ~20-30% estimation error) |
| Auto-compaction trigger | 200K input tokens (env-configurable) | `(context_window * 0.75) - max_tokens - 2000` |
| Compaction strategy | Preserve last 4 messages, summarize removed | Three-tier: tool truncation -> turn masking -> aggressive truncation |
| Tool output truncation (Tier 1) | No separate tier | Truncate outputs >= 50KB or >= 2000 lines to 40% head + 40% tail |
| Turn masking (Tier 2) | No | Walk backwards, mask old turns: clear text/thinking, keep tool call names, replace results with "[output from X omitted]" |
| Overflow fallback (Tier 3) | No | Aggressively truncate to 20% head + 20% tail |
| First user message preserved | No special handling | Always kept (original task context) |
| Custom messages preserved | N/A | Yes (invisible to LLM, never masked) |
| Compaction output | XML-wrapped summary message | In-place mutation (no LLM summarization) |
| `/compact` command | Yes | Not yet (planned) |
| Image token estimation | N/A | 1200 tokens per image block |

tau's mechanical-only compaction (no LLM summarization call) is a deliberate design choice based on JetBrains research showing 98% task accuracy with 52% cost reduction vs LLM summarization.

---

## 9. Permission System

| Feature | claw-code | tau |
|---------|-----------|-----|
| Permission modes | ReadOnly, WorkspaceWrite, DangerFullAccess, Prompt, Allow | Allow, Deny, Ask (per-tool policy) |
| Default mode | `DangerFullAccess` | Ask for write/exec tools, Allow for read tools |
| Hierarchy model | Mode-based (tool requires minimum mode) | Policy-based (each tool has independent policy) |
| `--yolo` flag | No (but default is full access) | Yes (bypasses all permission checks) |
| Session upgrade | Escalation prompting from lower modes | AlwaysAllow upgrades Ask -> Allow for session duration |
| Interactive prompting | `CliPermissionPrompter` | Async `PromptFn` (compatible with TUI) |
| Config source | Per-tool `BTreeMap<String, PermissionMode>` | `~/.tau/config.toml` permissions map |

### Tool permission defaults

**claw-code:** bash/REPL/PowerShell/Agent require DangerFullAccess. write/edit/todo/notebook require WorkspaceWrite. read/glob/grep/web require ReadOnly.

**tau:** bash/file_edit/file_write/subagent/thread/py_repl require Ask. file_read/glob/grep/web_fetch/web_search/todo are auto-Allow.

---

## 10. Session Persistence

| Feature | claw-code | tau |
|---------|-----------|-----|
| Format | Single JSON file (full session dump) | JSONL (append-only, one line per message) |
| Location | User-specified path | `~/.tau/sessions/{session_id}.jsonl` |
| Session ID | Filename-based | 8 hex digits, auto-generated |
| Resume support | `--resume` loads from path | `--session ID` or `--resume` (latest for cwd) |
| Schema versioning | `version: u32` field | Header line with version, id, timestamp, cwd |
| Crash recovery | Full file rewrite on save | Append-only (survives mid-write crashes) |
| CWD tracking | No | Yes (sessions scoped to working directory) |
| List sessions | No | `list_for_cwd(cwd)` |

tau's JSONL append-only format is more resilient to crashes and supports incremental writes without reserializing the entire history.

---

## 11. Orchestration / Multi-Agent

This is the largest architectural divergence between the two projects.

| Feature | claw-code | tau |
|---------|-----------|-----|
| Sub-agent support | Stub `Agent` tool | Full implementation: `subagent` (subprocess) + `thread` (in-process) |
| In-process threads | No | Yes (`ThreadTool` spawns tokio tasks sharing `OrchestratorState`) |
| Thread reuse | No | Yes (reuse alias to append to thread's conversation history) |
| Episode system | No | Yes (`Episode` with full_trace + compact_trace for downstream injection) |
| Virtual documents | No | Yes (`DocumentTool` for inter-thread data sharing without filesystem) |
| Query tool | No | Yes (single-shot LLM call for classification/routing, no tool loop) |
| Model slots | No | Yes (main, search, subagent, reasoning -- each slot can use different model) |
| Episode injection | No | Yes (threads can request prior episode context via `episodes` parameter) |
| Orchestration log | No | Yes (`LogTool` appends to `_orchestration_log` virtual document) |
| Python REPL | No | Yes (`PyReplTool` with persistent kernel, reverse RPC: `tau.thread()`, `tau.query()`, `tau.document()`, `tau.execute()`) |
| Event forwarding | No | Yes (inner thread events forwarded to parent via `EventForwarderCell`) |
| Capability aliases | No | Yes ("read" -> [file_read, grep, glob], "write" -> [+file_edit, file_write], "full" -> all) |
| Completion signaling | No | Yes (CompleteTool, AbortTool, EscalateTool injected into threads) |

tau's orchestration system is inspired by Slate's JavaScript DSL (documented in `docs/design-orchestration.md` and `docs/slate-gap-analysis.md`). claw-code has no equivalent -- its `Agent` tool is a stub that would spawn a subprocess.

---

## 12. CLI Interface

### Arguments

| Flag | claw-code | tau |
|------|-----------|-----|
| Prompt | Positional arg to `prompt` subcommand | `-p` / `--prompt` |
| Model | `--model` (default: claude-opus-4-6) | `-m` / `--model` (default: from config) |
| Permission mode | `--permission-mode` (3 modes) | `--yolo` (bypass all) |
| Output format | `--output-format` (text/json/ndjson) | `--stats` / `--stats-json PATH` |
| System prompt | Via config files only | `--system-prompt` (direct override) |
| Session | No flag (manual path) | `--session ID`, `--resume`, `--no-session` |
| Skip permissions | `--dangerously-skip-permissions` | `--yolo` |
| Tools | `--allowedTools` (with aliases: read, write, edit, glob, grep) | `--tools tool1,tool2` (allowlist) |
| Print mode | `--print` (non-interactive output) | N/A (headless via `-p`) |
| Thinking | No | `--thinking level` |
| Skills | No | `--no-skills`, `--skill PATH` (repeatable) |
| Tracing | No | `--trace-output DIR` |
| Task ID | No | `--task-id ID` (for benchmark integration) |
| Config file | `--config PATH` | Automatic (`~/.tau/config.toml`) |

### Subcommands

| Command | claw-code | tau |
|---------|-----------|-----|
| Interactive REPL | Default (no subcommand) | Default (no subcommand) |
| One-shot prompt | `prompt "text"` subcommand | `-p "text"` flag |
| Login/Logout | `login` / `logout` subcommands | No (API key based) |
| Dump manifests | `dump-manifests` subcommand | No |
| Bootstrap plan | `bootstrap-plan` subcommand | No |
| List models | No | `models` subcommand (with `--provider` filter) |
| Server mode | No | `serve` subcommand (JSON-RPC over stdio) |

### Slash commands

| Command | claw-code | tau |
|---------|-----------|-----|
| /help | Yes | Yes |
| /status | Yes | No (metrics in sidebar) |
| /compact | Yes | Yes (shows token/context stats) |
| /clear | Yes (--confirm flag) | Yes (clears output) |
| /cost | Yes | No (cost shown in sidebar) |
| /config | Yes (env/hooks/model subsections) | No |
| /memory | Yes (shows CLAUDE.md sources) | No |
| /version | Yes | No |
| /model | Yes (switch model) | Yes (switch model) |
| /permissions | Yes (switch mode) | No |
| /resume | Yes | Yes (with optional session ID) |
| /export | Yes (to file) | No |
| /diff | Yes (git diff) | No |
| /init | Yes (create CLAUDE.md) | No |
| /bughunter | Yes (scope param) | No |
| /commit | Yes (LLM-generated message) | No |
| /pr | Yes (draft PR via gh CLI) | No |
| /issue | Yes (draft issue via gh CLI) | No |
| /ultraplan | Yes (deep planning) | No |
| /teleport | Yes (jump to symbol) | No |
| /session | Yes (list/switch) | Yes (/sessions lists for cwd) |
| /thinking | No | Yes (cycle off/low/medium/high/xhigh) |
| /skills | No | Yes (list available skills) |
| /yolo | No | Yes (toggle auto-approve) |
| /debug | No | Yes (toggle debug logging) |
| /skill:\<name\> | No | Yes (dynamic per loaded skill) |

claw-code has 21 registered slash commands focused on git workflows (/commit, /pr, /issue) and Claude Code compatibility. tau has 10 core commands plus dynamic skill-based commands, focused on model control and orchestration.

---

## 13. TUI / Terminal Rendering

| Feature | claw-code | tau |
|---------|-----------|-----|
| Framework | Custom `TerminalRenderer` + rustyline | External `ruse` TUI crate (Elm-style architecture) |
| Rendering model | Streaming markdown state machine | Elm-style update/view model (49KB model.rs) |
| Input | Rustyline line editor | Custom editor component |
| Markdown rendering | Basic: bold, italic, code blocks | Full: via pulldown-cmark + syntect |
| Syntax highlighting | Mock/placeholder | Real (syntect-based) |
| Spinner | Custom `Spinner` struct | Animation system (anim.rs) |
| Sidebar | No | Yes (thread/tool display) |
| Status bar | No | Yes (model, tokens, cost) |
| Permission dialog | CLI prompt (y/n) | Dedicated dialog component (dialog/permissions.rs) |
| Theme support | No | Yes (theme.rs) |
| Layout system | No | Yes (layout.rs) |
| Agent event bridge | No | Yes (bridge.rs: TUI <-> agent communication) |

### Key bindings

| Action | claw-code | tau |
|--------|-----------|-----|
| Submit | Enter | Enter |
| Newline | Ctrl+J / Shift+Enter | Shift+Enter |
| Interrupt | Ctrl+C | Ctrl+C (first: abort, second: exit) |
| Exit | Ctrl+D | Ctrl+D |
| Cycle thinking | N/A | Ctrl+T |
| Toggle focus | N/A | Tab (editor <-> chat) |
| Scroll chat | N/A | j/k |
| Jump messages | N/A | J/K |
| Expand/collapse | N/A | Space |
| Permission allow | y (text prompt) | a (dialog widget) |
| Permission deny | n (text prompt) | d (dialog widget) |
| Always allow | N/A | s (session-wide) |

### Rendering details

**claw-code markdown rendering (`render.rs`, 796 lines):**
- Headings: Cyan (H1), White bold (H2), Blue (H3)
- Bold: Yellow. Italic: Magenta. Code: Green background.
- Code blocks: bordered with Unicode box drawing, syntect highlighting (base16-ocean.dark)
- Tables: Unicode box drawing characters
- Spinner: 10-frame braille animation
- Streaming: `MarkdownStreamState` buffers incomplete blocks

**tau TUI (`tui/`, 13 files):**
- Elm-style update/view model (49KB model.rs)
- Collapsible thinking blocks and tool calls with status badges
- Sidebar with thread monitoring, session list, token metrics
- Animated gradient spinner
- Context-aware status bar hints per focus mode
- Permission dialog as dedicated widget (not text prompt)
- Auto-following viewport that scrolls on new messages

tau's TUI is substantially more sophisticated, with a dedicated external crate, Elm-style architecture, sidebar for thread monitoring, and proper layout/theming.

---

## 14. System Prompt Construction

| Feature | claw-code | tau |
|---------|-----------|-----|
| Builder pattern | `SystemPromptBuilder` with fluent API | Function-based assembly from markdown files |
| Static sections | Embedded strings (intro, system, tasks, actions) | Separate .md files (identity, system, doing_tasks, executing_with_care, tone_and_output) |
| Dynamic boundary | `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` marker | No explicit boundary |
| CLAUDE.md discovery | Walk up directory tree, scan for CLAUDE.md/.claude-instructions | No (tau uses its own skill system) |
| Instruction file limits | 4,000 chars per file, 12,000 total | No limits (skills loaded on-demand) |
| Tool listing | Not in system prompt | Dynamic "# Available tools" section with first sentence of each tool description |
| Tool usage guidelines | Not in system prompt | Conditional sections based on enabled tools |
| Orchestration guidance | Not in system prompt | 6.6KB orchestration.md injected when thread tool enabled |
| Skill listing | Not in system prompt | "# Available skills" with progressive disclosure |
| OS/environment context | Yes (OS name, version, date, cwd) | Yes (cwd, date in prompt) |
| Git context | Yes (git status, git diff) | No (agent discovers via tools) |

### Prompt files

**claw-code:** All prompt text is inline Rust strings. No external prompt files.

**tau:** 8 dedicated prompt files in `coding-agent/prompts/`:
- `identity.md` -- agent identity
- `system.md` -- core behavior
- `doing_tasks.md` -- task execution
- `executing_with_care.md` -- safety/caution
- `tone_and_output.md` -- communication style
- `orchestration.md` -- thread/episode guidance (6.6KB)
- `thread_identity.md` -- sub-agent identity
- `py_kernel.py` -- Python REPL kernel (6.8KB)

---

## 15. Configuration System

| Feature | claw-code | tau |
|---------|-----------|-----|
| Format | JSON (`.claude.json`, `settings.json`) | TOML (`~/.tau/config.toml`) |
| User config | `~/.claude.json` or `~/.claude/settings.json` | `~/.tau/config.toml` |
| Project config | `./.claude.json` in repo root | `.tau/config.toml` in project root |
| Local overrides | `./.claude.json` in cwd | No (planned) |
| Discovery | Walk up directory tree | Fixed locations |
| Merge strategy | `BTreeMap` merge with source tracking | TOML parsing with CLI override |
| Feature extraction | `RuntimeFeatureConfig` with typed fields | `TauConfig` struct with serde |
| MCP config | Full (6 transport types: Stdio, SSE, HTTP, WS, SDK, ClaudeAiProxy) | No MCP config |
| Hook config | Yes (`pre_tool_use`, `post_tool_use` command lists) | No hook config |
| OAuth config | Yes (client_id, callback_port, auth_server_metadata_url) | No (API key based) |
| Sandbox config | Yes (filesystem mode, allowed mounts, network isolation) | No |
| Model slots | No | Yes (main, search, subagent, reasoning) |
| Edit mode | No | Yes (`replace` or `hashline`) |
| Thinking config | No | Yes (`off`, `minimal`, `low`, `medium`, `high`, `xhigh`) |
| Max turns | No | Yes |
| Tool allowlist | No | Yes |
| Skills toggle | No | Yes |

### tau config.toml example

```toml
[agent]
model = "claude-sonnet-4-6"
edit_mode = "hashline"
max_turns = 50

[agent.thinking]
level = "medium"

[agent.models]
search = "gpt-5.4-mini"
subagent = "gpt-5.4-mini"
reasoning = "o4-mini"
```

claw-code's config mirrors Claude Code's JSON format exactly. tau uses TOML with agent-specific ergonomics.

---

## 16. Hook System

| Feature | claw-code | tau |
|---------|-----------|-----|
| Hook types | PreToolUse, PostToolUse | None (event subscription instead) |
| Implementation | Parsed from config, HookRunner struct, shell command execution | N/A |
| Execution status | **Config-only** -- parsed but NOT wired into conversation loop | N/A |
| Payload format | JSON to stdin + env vars (HOOK_EVENT, HOOK_TOOL_NAME, etc.) | N/A |
| Exit codes | 0=allow, 2=deny, other=warn | N/A |
| Hook feedback | stdout merged into tool result | N/A |

claw-code has the hook infrastructure fully implemented but not connected to the runtime loop (a known parity gap). tau takes a different approach entirely -- using the event subscription system (`agent.subscribe()`) for observability, and the permission system for access control, rather than shell-based hooks.

---

## 17. Skills System

| Feature | claw-code | tau |
|---------|-----------|-----|
| Discovery | Local SKILL.md files only | `~/.tau/skills/`, project `.tau/skills/`, CLI `--skill PATH` |
| Format | Not well-defined | YAML frontmatter (name, description) + markdown body |
| Loading | `Skill` tool reads file content | System prompt injection (progressive disclosure) |
| Validation | No | Name validation (lowercase alphanumeric + hyphens, 1-64 chars) |
| Bundled skills | None | None (user-provided) |
| Skill registry | None | None (planned) |
| Integration | Tool-based (read on demand) | Prompt-based (names listed in system prompt, content via file_read) |

---

## 18. MCP (Model Context Protocol) Support

| Feature | claw-code | tau |
|---------|-----------|-----|
| Config parsing | Full (6 transport types) | None |
| Transport types | Stdio, SSE, HTTP, WebSocket, SDK, ClaudeAiProxy | None |
| Tool discovery | `McpListToolsResult` | None |
| Resource discovery | `McpListResourcesResult` | None |
| Tool execution | `McpToolCallParams/Result` | None |
| JSON-RPC | Full protocol structs | None |
| Server bootstrap | `McpClientBootstrap` | None |
| Name normalization | `mcp_tool_prefix()`, `mcp_tool_name()` | None |
| Integration status | Partial (config + protocol, not fully wired) | Not implemented |

claw-code has extensive MCP infrastructure (config, protocol types, naming, bootstrap) but it's not fully integrated into the conversation loop. tau has no MCP support.

---

## 19. OAuth & Authentication

| Feature | claw-code | tau |
|---------|-----------|-----|
| OAuth flow | Full PKCE flow with local callback server | Codex OAuth token reading from `~/.codex/auth.json` |
| Token storage | `~/.claude/oauth_token.json` | Reads from `~/.codex/auth.json` |
| Token refresh | Yes (`refresh_oauth_token()`) | Yes (automatic with 5-min buffer) |
| JWT parsing | No | Yes (custom JWT expiry extraction) |
| Login command | `claw login` (starts OAuth) | None (reads existing tokens) |
| Logout command | `claw logout` | None |
| Bearer tokens | Yes (`ANTHROPIC_AUTH_TOKEN`) | Yes (for ChatGPT backend) |
| Dual auth | Yes (`ApiKeyAndBearer`) | No (one auth method per provider) |

---

## 20. Usage Tracking & Cost Estimation

| Feature | claw-code | tau |
|---------|-----------|-----|
| Token tracking struct | `TokenUsage` (input, output, cache_creation, cache_read) | `Usage` (input, output, cache_read, cache_write, total, cost) |
| Cost calculation | `estimate_cost_usd_with_pricing()` per-family pricing | `calculate_cost()` per-model from catalog |
| Pricing source | Hardcoded per family (Haiku/Sonnet/Opus) | Per-model in catalog.rs |
| Cumulative tracking | `UsageTracker` across session | `AgentStats` subscriber + cumulative in serve mode |
| Per-turn tracking | Yes | Yes |
| Cost in output | Via /cost command and /status | Via `--stats`, `--stats-json`, status bar in TUI |
| Service tier pricing | No | Yes (flex 0.5x, priority 2.0x) |

---

## 21. Observability / Tracing

| Feature | claw-code | tau |
|---------|-----------|-----|
| Trace format | None | JSONL event stream (`trace.jsonl`) + run summary (`run.json`) |
| Always-on tracing | No | Yes (every session auto-creates trace) |
| Event types | None | 16 types: agent_start/end, turn, tool, thinking, thread, episode, document, query, context_compact |
| Trace directory | N/A | `~/.tau/traces/{session_id}/` |
| Analysis tooling | None | Comprehensive jq query guide (`docs/trace-analysis.md`) |
| Thread context | N/A | Yes (active thread stack tracked on all events) |
| Orchestration summary | N/A | `OrchestrationSummary` from episode log |
| Run metadata | N/A | run_id, task_id, model, provider, tools, edit_mode, system_prompt_hash |

tau's tracing infrastructure is designed for studying routing behavior and harness engineering -- a core research goal. claw-code has no equivalent.

---

## 22. RPC / Server Mode

| Feature | claw-code | tau |
|---------|-----------|-----|
| Server mode | No | Yes (`tau serve --cwd PATH`) |
| Protocol | N/A | JSON-RPC 2.0 over stdio |
| Methods | N/A | initialize, session/send, session/status, session/messages, session/abort, shutdown |
| Notifications | N/A | Real-time agent events (tool start/end, message start/end, thread start/end) |
| Session status | N/A | Idle / Busy / Error state machine |
| Concurrent requests | N/A | Prevented (one prompt at a time) |
| Usage tracking | N/A | Cumulative across all prompts in session |

tau's serve mode enables integration as a backend for IDEs, web UIs, or other orchestrators. claw-code has no equivalent.

---

## 23. Testing & Quality

| Dimension | claw-code | tau |
|-----------|-----------|-----|
| Test count | 27 Rust + 24 Python (porting workspace) | 270+ Rust (offline) + 8 live smoke tests |
| Test framework | `#[test]` (sync) | `#[tokio::test]` (async) |
| Property tests | No | Yes (proptest for SSE parsing, message serde) |
| Mock LLM | No (tests hit API or skip) | Yes (`stream_fn_from_messages()` injection) |
| Integration tests | 1 Python file | Multi-turn session tests, agent loop tests, serve mode tests |
| Tool tests | In single tools/lib.rs | Per-tool test files (16 test files) |
| CI coverage | No | Yes (cargo-llvm-cov + Codecov) |
| Pre-commit hooks | No | Yes (fmt + clippy + test) |
| Test isolation | Manual | TempDir-based filesystem isolation |
| Failing tests | 1 (`skill_loads_local_skill_prompt`) | 0 (all passing) |

### Test file inventory (tau)

- `tools_test.rs`, `tool_details.rs`, `tool_allowlist.rs`
- `hashline_test.rs`, `hash_tools_test.rs`
- `glob_test.rs`, `grep_test.rs`
- `config_test.rs`, `system_prompt_test.rs`, `session_test.rs`
- `prompt_mode_test.rs`, `serve_test.rs`
- `web_fetch_test.rs`, `web_search_test.rs`
- `integration.rs`, `trace_subscriber.rs`
- `agent_test.rs`, `agent_loop.rs`, `context_test.rs`, `stats_test.rs`, `e2e.rs`

---

## 24. Benchmarking

| Dimension | claw-code | tau |
|-----------|-----------|-----|
| Benchmark suites | 0 | 10 (fuzzy-match, post-edit-diagnostics, compaction-recall, compaction-efficiency, parallel-ops, subagent-decomposition, todo-tracking, flask-books, terminal-bench, harbor) |
| Rust microbenchmarks | 0 | 3 (agent_construction, message_serde, sse_parsing via Criterion) |
| Benchmark runner | None | `scripts/bench.sh` (unified Rust + Python) |
| Terminal-Bench integration | None | Harbor adapter for 89 Docker-based tasks |
| Result storage | None | Local JSON + remote Cloudflare R2 via rclone |
| Result querying | None | DuckDB-compatible JSON format |
| Cost tracking | None | Phased: $0 (offline) through ~$68 (full suite) |
| Shared infrastructure | None | 8 reusable Python modules (config, session, result, reporter, verifier, variants, store, miner) |
| Commit mining | None | Yes (extract fixtures from git history of other repos) |

---

## 25. CI/CD

| Feature | claw-code | tau |
|---------|-----------|-----|
| CI platform | None | GitHub Actions |
| Format check | No | `cargo fmt --check` |
| Lint | No | `cargo clippy -- -D warnings` |
| Test | No | `cargo test` (270+ tests) |
| Coverage | No | cargo-llvm-cov + Codecov |
| Benchmarks in CI | No | Criterion on main branch, alert on >50% regression |
| Release binary | No | Static musl binary on `v*` tags |
| Pre-commit hooks | No | fmt + clippy + test |

---

## 26. Documentation

| Document type | claw-code | tau |
|---------------|-----------|-----|
| README | Yes (project backstory, porting narrative) | Yes (architecture, quickstart, testing) |
| Architecture overview | Via PARITY.md (gap analysis) | `docs/overview.md` (full codebase walkthrough) |
| Benchmark docs | No | `docs/benchmarking.md`, `docs/benchmarks-landscape.md` |
| Context management | No | `docs/context-management.md` (survey of 11 harnesses) |
| Orchestration design | No | `docs/design-orchestration.md` (30.5KB Slate comparison) |
| Gap analysis | PARITY.md (TS vs Rust) | `docs/slate-gap-analysis.md` (22.3KB), `docs/feature-comparison.md` (34.9KB, 9-harness matrix) |
| Literature review | No | `docs/harness-lit-review.md` (15.5KB, papers + builders) |
| Trace analysis | No | `docs/trace-analysis.md` (jq query cookbook) |
| LSP feedback | No | `docs/lsp-feedback-sequence.md` |
| Release process | No | `docs/releases.md` |
| Specs | No | `docs/specs/trace-observability.md` |
| Total doc volume | ~20KB | ~180KB+ |

---

## 27. Git Workflow Integration

| Feature | claw-code | tau |
|---------|-----------|-----|
| /commit | Yes (runs `git add -A`, LLM generates message, creates commit) | No (via bash tool) |
| /pr | Yes (generates title/body, creates via `gh pr create` if available) | No (via bash tool) |
| /issue | Yes (generates title/body, creates via `gh issue create` if available) | No (via bash tool) |
| /diff | Yes (shows `git diff`) | No (via bash tool) |
| Git context in prompt | Yes (git status + git diff injected into system prompt) | No (agent discovers via tools) |

claw-code has dedicated slash commands that run LLM-assisted git workflows -- generating commit messages, drafting PRs, and filing issues. tau delegates all git operations to the agent via the bash tool, which means the agent can do the same things but without dedicated shortcuts.

---

## 28. Sandbox & Security

| Feature | claw-code | tau |
|---------|-----------|-----|
| Sandbox support | Yes (`SandboxConfig` with filesystem mode, allowed mounts, network isolation, namespace restrictions) | No |
| Bash sandboxing | Yes (Linux containers) | No |
| Background tasks | Yes (spawn with null stdio, return task ID) | No |

claw-code inherits Claude Code's sandbox infrastructure for Linux container isolation. tau relies on the permission system and tool-level access control instead.

---

## 29. Remote / Proxy Support

| Feature | claw-code | tau |
|---------|-----------|-----|
| Remote proxy | Yes (`RemoteSessionContext`, `UpstreamProxyBootstrap`) | No |
| CCR session support | Yes (env vars: `CLAUDE_CODE_REMOTE`, `CCR_SESSION_TOKEN_PATH`) | No |
| TLS certificate management | Yes (`CCR_CA_BUNDLE_PATH`, `CCR_SYSTEM_CA_BUNDLE`) | No |

---

## 30. Miscellaneous Differences

| Detail | claw-code | tau |
|--------|-----------|-----|
| Unsafe code | Forbidden (`#[forbid(unsafe_code)]`) | Not explicitly forbidden (but none present) |
| Clippy config | Pedantic with allow-list | `-D warnings` |
| Cargo resolver | 2 | 2 |
| reqwest TLS | rustls-tls (0.12, feature obsolete in 0.13) | rustls-tls (0.12) |
| Session compaction env var | `CLAUDE_CODE_AUTO_COMPACT_INPUT_TOKENS` | N/A (computed from model context window) |
| Default timeout (bash) | Sandbox-dependent | 120 seconds |
| Grep implementation | Native Rust (runtime/file_ops.rs) | Shells out to ripgrep binary |
| Glob implementation | Native Rust | Native Rust (globset + ignore crate) |
| JSON output | Structured with tool uses/results + usage | Stats-only (`--stats-json`) |
| Model pricing | 3 tiers (Haiku/Sonnet/Opus) | Per-model from catalog (~65 entries) |
| Anthropic API version | `2023-06-01` | `2023-06-01` |
| HTTP client | reqwest (both) | reqwest (both) |

---

## Summary: Where Each Project Excels

### claw-code strengths (things tau lacks)

1. **Claude Code compatibility** -- mirrors the exact config hierarchy, hook schema, permission modes, MCP transport types, and system prompt structure
2. **MCP infrastructure** -- 6 transport types, full JSON-RPC protocol, server bootstrap (even if not fully wired)
3. **Sandbox/container isolation** -- Linux namespace sandboxing for bash execution
4. **Remote proxy support** -- CCR session management, TLS certificate handling
5. **Slash command breadth** -- 20 registered commands covering git workflow (/commit, /pr, /issue), debugging (/bughunter, /debug-tool-call), and session management
6. **Structured patch output** -- file_write and file_edit generate unified diffs
7. **OAuth login flow** -- full PKCE flow with local callback server
8. **CLAUDE.md discovery** -- walks directory tree to find instruction files

### tau strengths (things claw-code lacks)

1. **Multi-provider support** -- Anthropic + OpenAI + OpenRouter + 4 more providers
2. **Orchestration system** -- in-process threads, episodes, virtual documents, query tool, model slots, Python REPL with reverse RPC
3. **Parallel tool execution** -- all tool calls in a turn run concurrently
4. **Hashline editing** -- +8% accuracy average, 10x for weak models
5. **Extended thinking** -- full support for 5 thinking levels across providers
6. **Event system** -- lifecycle events with multiple subscriber support
7. **Always-on tracing** -- JSONL traces for every session, 16 event types
8. **Testing depth** -- 270+ offline tests, property tests, mock LLM injection, CI coverage
9. **Benchmark infrastructure** -- 10 suites, shared Python framework, Terminal-Bench adapter, DuckDB-queryable results
10. **Three-tier compaction** -- mechanical-only, no LLM summarization needed
11. **RPC server mode** -- JSON-RPC over stdio for IDE/orchestrator integration
12. **TUI sophistication** -- Elm-style architecture, sidebar, themes, status bar
13. **Research documentation** -- 180KB+ of comparative analysis, literature review, gap analysis
14. **Prompt caching** -- Anthropic ephemeral + OpenAI 24h retention
15. **Cost tracking** -- per-model pricing for 65+ models with cache read/write costs
