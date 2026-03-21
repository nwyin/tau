# Context Window Management

Survey of how coding agent harnesses manage context windows, and the
design for tau's implementation.

## Field survey (March 2026)

### Codex CLI

Source: `/harnesses/codex/codex-rs/core/src/`

- **Token counting**: chars/4 heuristic (`APPROX_BYTES_PER_TOKEN = 4`), no tiktoken
- **Overflow strategy**: three tiers
  1. Truncate individual tool outputs (head/tail preservation, `[omitted N items]`)
  2. Drop oldest history items (`history.remove_first_item()`)
  3. Auto-compact via LLM summarization when token usage hits threshold
- **LLM summarization**: inline (sub-task with same model) or remote (OpenAI
  `compact_conversation_history()` API). Prompt asks for handoff summary:
  progress, decisions, constraints, next steps
- **Safety margin**: `effective_context_window_percent` per model (e.g. 95%)
- **Recent message preservation**: last 20K tokens of user messages kept
  post-compaction
- **User feedback**: warning after compaction ("long threads can cause the
  model to be less accurate")

### OpenCode

Source: `/harnesses/opencode/packages/opencode/src/`

- **Token counting**: chars/4 heuristic (`Math.round(text.length / 4)`)
- **Overflow strategy**: proactive detection with 20K token buffer
  1. Prune old tool call outputs (walk backwards, protect last 40K tokens)
  2. LLM-based structured summary (goal/instructions/discoveries/accomplished/files)
  3. Strip media (images/PDFs -> text placeholders)
- **Tool result handling**: 2000 lines / 50KB cap. Full output written to disk,
  truncated preview returned with file path reference
- **Trigger**: when `total_tokens >= usable_context`
- **Config**: `opencode.json` with `compaction.auto`, `compaction.prune`,
  `compaction.reserved`
- **Known issue**: asymmetric overflow detection with `limit.input` models

### Claude Code (deobfuscated source)

Source: `/harnesses/claude-code-source-code-deobfuscation/`

Note: the deobfuscated source is an older version with a stateless
single-turn architecture. The production Claude Code (2026) has
full compaction — see web research section below.

- **Older deobfuscated version**: no compaction, no token counting,
  `maxHistoryLength: 20` defined but unused, stateless per-request design,
  5MB buffer cap on command output
- **Production version (2026, from docs/blogs)**: triggers at ~64-75%
  capacity, clears tool outputs first, LLM-based summarization via Claude,
  `/compact` command with custom instructions, `CLAUDE.md` survives
  compaction, `/context` to inspect usage

### kimi-cli

Source: `/harnesses/kimi-cli/src/kimi_cli/`

- **Token counting**: chars/4 heuristic, updated with actual usage from API
- **Overflow strategy**: LLM-based compaction
  - Trigger at 85% of max context OR when `tokens + 50K reserved >= max`
  - Summarize oldest messages via LLM, keep last 2 conversation turns
  - System prompt rewritten post-compaction
- **Tool result handling**: dual truncation — 50K chars AND 2000 chars/line,
  max 1000 lines for file reads, 100KB file size cap
- **Message transforms**: adjacent user messages merged, dynamic system
  reminders injected before each step
- **Session persistence**: JSON append-only with checkpoints, revert support

### OpenDev

Source: `/harnesses/opendev/crates/opendev-context/src/`

- **Token counting**: cl100k_base-style heuristic (whitespace/punct splitting,
  ~0.75 tokens/word ratio, long words at ~1 token per 4 chars). More accurate
  than naive chars/4. Fallback: `text.len() / 4`
- **Overflow strategy**: **6-stage progressive compaction**
  - 70%: warning logged
  - 80%: observation masking (old tool results -> `[ref: tool_id]`)
  - 85%: fast pruning (protect last 40K tokens)
  - 90%: aggressive masking (keep only 3 recent tool results)
  - 99%: full LLM compaction (summarize middle messages)
- **Sliding window**: for 500+ message sessions, keep system prompt + last 50
  messages + compressed summary of middle
- **Tool result handling**: 2000 lines / 50KB, overflow saved to
  `~/.opendev/tool-output/` with 7-day retention
- **Tool output summarization**: >500 chars -> 2-3 line summary, protected
  tools (read_file, skill) skip summarization
- **Artifact tracking**: `ArtifactIndex` survives compaction (tracks file
  create/modify/read/delete operations)
- **Config default**: 100K tokens

### smolagents (HuggingFace)

Source: `/harnesses/smolagents/src/smolagents/`

- **Token counting**: post-hoc from API responses, no pre-flight estimation
- **Overflow strategy**: **none** — no context management
  - `MAX_LENGTH_TRUNCATE_CONTENT = 20000` chars for individual outputs
  - Truncation: head + tail with ellipsis in middle
  - `max_steps = 20` prevents unbounded growth (step limit, not token limit)
- **Summary mode**: planning steps use `summary_mode=True` which omits model
  outputs from context (reduces planning input, not persistent)
- **No eviction policy**: `AgentMemory.steps` grows unbounded

### pi-mono

Source: `/harnesses/pi-mono/packages/coding-agent/`

- **Token counting**: token-level counting
- **Overflow strategy**: structured compaction
  1. Walk backwards from newest, accumulate until `keepRecentTokens` (20K)
  2. Summarize rest via LLM (goal/constraints/progress/decisions/next steps)
  3. `CompactionEntry` marks preserved message boundary
- **Reserve**: 16,384 token reserve
- **Split turns**: single turns exceeding budget split at assistant message,
  two summaries generated and merged
- **Session storage**: append-only JSONL with tree structure (id/parentId),
  conversation branching support

### oh-my-pi

Source: `/harnesses/oh-my-pi/packages/coding-agent/src/`

- **Token counting**: chars/4 heuristic, verified against LLM usage with
  adaptive ratio adjustment
- **Overflow strategy**: threshold + overflow-retry compaction
  - Reserve: `max(15% of window, 16,384 tokens)`
  - Trigger: `contextTokens > contextWindow - effectiveReserve`
  - Also triggers on overflow API error with auto-retry
- **Pre-compaction pruning**: walk backwards, protect newest 40K tokens of
  tool output, prune rest to `[Output truncated - N tokens]`, minimum 20K
  savings threshold
- **Cut point algorithm**: adaptive walkback with ratio-based budget adjustment
  (`keepRecentTokens / (promptTokens / estimatedTokens)`)
- **Split turn handling**: dual summaries (history + turn-prefix)
- **Branch summarization**: optional summarization when switching `/tree`
  branches
- **File tracking**: cumulative read/modified file lists survive compaction,
  injected into summary context, capped at 20 files per list
- **Summary budget allocation**: 80% for main summary, 20% for short summary,
  50% for turn-prefix

### pi_agent_rust

Source: `/harnesses/pi_agent_rust/src/`

- **Token counting**: 3 chars/token (more conservative than chars/4), 1200
  tokens per image. Falls back to assistant usage from API
- **Overflow strategy**: LLM-based compaction with cut-point detection
  - Trigger: `context_tokens > context_window - reserve_tokens`
  - Reserve: 16,384 tokens (8% of context window)
  - Keep recent: 20,000 tokens (10% of context window)
  - Cut points preserve message integrity (never cut mid-tool-call sequence)
- **Tool result caps**: 2000 lines / 50KB, grep lines capped at 500 chars,
  100 grep results, 1000 find results, 500 ls entries
- **Background compaction**: dedicated thread (`pi-compaction-bg`), 60s
  cooldown, 120s timeout, max 100 compactions per session
- **Structured summary**: goal/constraints/progress/decisions/next steps/
  critical context. Iterative updates preserve prior summary
- **Session storage**: DAG-based (parent-child links), path-based context
  reconstruction, compaction entries mark boundaries
- **File tracking**: cumulative read/modified file lists across compactions

### crush (Charm)

Source: `/harnesses/crush/` (limited exploration)

- **Auto-compaction**: monitors token usage, summarizes on approach
- **LSP integration**: uses language servers for structured context (symbol
  tables, diagnostics) rather than raw file dumps
- **Prompt caching**: minimizes redundant tokens on supporting providers
- **Project awareness**: `.crushignore` for excluding large fixtures

### aider

Source: web research (github.com/Aider-AI/aider)

- **Different philosophy**: repo map as primary context strategy
  - Graph-ranked dependency map of codebase (class/method signatures)
  - `--map-tokens` flag (default 1000 tokens) controls map size
  - LLM requests specific files when deeper understanding needed
- **Chat history**: `ChatSummary` class auto-summarizes when approaching limits
  (background thread)
- **Token counting**: tiktoken (only harness using a real tokenizer)
- **Reactive**: reports provider token limit errors rather than preventing them

## Comparative matrix

| Harness | Token Est. | Trigger | Tier 1 (Truncate) | Tier 2 (Drop/Mask) | Tier 3 (Summarize) |
|---------|-----------|---------|-------------------|-------------------|--------------------|
| Codex | chars/4 | threshold | head/tail, markers | drop oldest | LLM handoff summary |
| OpenCode | chars/4 | budget check | 2K lines/50KB, disk | prune old outputs | structured template |
| Claude Code | unknown | 64-75% | tool outputs first | observation mask | LLM via Claude |
| kimi-cli | chars/4 | 85% or reserve | 50K/2K line, 1K lines | keep last 2 turns | LLM summary |
| OpenDev | cl100k heuristic | 6 stages (70-99%) | 2K lines/50KB, disk | progressive masking | LLM at 99% |
| smolagents | none | none | 20K chars | none | none |
| pi-mono | token count | reserve threshold | — | keep 20K recent | structured template |
| oh-my-pi | chars/4 + adaptive | 85% or overflow | prune >40K protected | keep recent budget | LLM, split turns |
| pi_agent_rust | chars/3 | reserve threshold | 2K lines/50KB | cut-point detection | LLM, iterative |
| crush | unknown | threshold | — | — | auto-compact |
| aider | tiktoken | reactive | — | — | background summary |

## Common patterns

1. **Token estimation**: chars/4 is near-universal. pi_agent_rust uses chars/3
   (more conservative for code). OpenDev uses a slightly smarter word-based
   heuristic. Only aider uses a real tokenizer (tiktoken).

2. **Three-tier strategy** is universal among serious harnesses:
   - Tier 1: Truncate tool results (biggest wins, least semantic loss)
   - Tier 2: Drop/prune/mask old messages (keep recent N turns)
   - Tier 3: LLM summarization (structured summary of progress)

3. **Structured summary templates** beat free-form. Common structure:
   goal, progress (done/in-progress/blocked), decisions, next steps,
   relevant files. Used by: pi-mono, oh-my-pi, pi_agent_rust, OpenCode.

4. **Observation masking**: hide old tool outputs but keep tool call names
   visible. OpenDev does this progressively. Research shows this often
   outperforms LLM summarization in efficiency and reliability.

5. **Safety margin**: trigger at 64-95% of context window. Typical reserve
   is 15-20K tokens or 8-15% of window.

6. **Proactive, not reactive**: everyone except aider does pre-flight checks.
   Nobody waits for the API to reject and retries with fewer messages
   (except oh-my-pi as a fallback path).

7. **Tool result caps converge**: 2000 lines / 50KB is the de facto standard
   across codex, opencode, opendev, pi_agent_rust. Overflow goes to disk.

8. **File tracking survives compaction**: opendev, oh-my-pi, and pi_agent_rust
   all track read/modified files across compaction boundaries.

9. **Split turn handling**: oh-my-pi and pi-mono both handle the case where
   a single turn exceeds the budget by splitting and generating dual summaries.

10. **Background compaction**: pi_agent_rust runs compaction on a dedicated
    thread with cooldown (60s) and timeout (120s). Others run inline.

## Research notes

- NoLiMa (2025): 11 of 12 LLMs drop below 50% of short-context performance
  at 32K tokens — intelligent context management matters more than large windows
- Anthropic engineering blog: recommends <40% context utilization for optimal
  performance
- JetBrains research: observation masking (98% accuracy at 3300+ tokens/sec
  with zero hallucination) vs LLM summarization (slower, can hallucinate)
- Verbatim compaction: delete tokens instead of rewriting — simpler, faster,
  no hallucination risk

## tau design

### Extension point

tau already has `transform_context: Option<TransformContextFn>` in the agent
loop config. This is an async callback that receives the full message history
and returns a (possibly modified) history. It runs before every LLM call.
Currently unused.

Models in the catalog already have `context_window: u64` and `max_tokens: u64`.

### Phase 1: mechanical compaction (no LLM calls)

- chars/4 token estimation
- Budget: `model.context_window * 0.80 - max_tokens - system_prompt_estimate`
- If messages fit -> pass through unchanged
- If over budget:
  1. Truncate large tool results (>50KB or >2000 lines) to head+tail with
     `[truncated N lines]` marker
  2. Re-check budget
  3. If still over: drop oldest turns from the front, keeping:
     - First user message (original task context)
     - Last N turns (recent working context)
  4. Observation masking on dropped turns: replace tool result content with
     `[output from <tool_name> omitted]` but keep the tool call visible

- Estimated scope: ~300 lines in `agent` crate

### Phase 2: LLM-assisted compaction

- Structured summarization prompt (goal/progress/decisions/next steps/files)
- Uses the session's current model
- `/compact` manual command
- Auto-compact at configurable threshold (default 75%)
- Summary prefixed with marker so model knows it's working from a summary
- File tracking: cumulative read/modified file lists across compactions

### Phase 3: refinements

- Progressive stages (OpenDev-style: mask at 80%, prune at 85%, compact at 95%)
- Per-tool truncation policies (bash output vs file read vs grep)
- Post-hoc calibration: compare estimated tokens to actual API usage,
  adjust estimator
- Split turn handling for single turns exceeding budget
- `/context` command to inspect what's using space
- Background compaction thread with cooldown
- Prompt cache awareness (don't evict cached prefixes)
