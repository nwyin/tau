# Context Window Management

Survey of how coding agent harnesses manage context windows, and the
design for tau's implementation.

## Field survey (March 2026)

### Codex CLI

Source: `/Users/tau/projects/harnesses/codex/codex-rs/core/src/`

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

Source: `/Users/tau/projects/harnesses/opencode/packages/opencode/src/`

- **Token counting**: chars/4 heuristic (`Math.round(text.length / 4)`)
- **Overflow strategy**: proactive detection with 20K token buffer
  1. Prune old tool call outputs (walk backwards, protect last 40K tokens)
  2. LLM-based structured summary (goal/instructions/discoveries/accomplished/files)
  3. Strip media (images/PDFs → text placeholders)
- **Tool result handling**: 2000 lines / 50KB cap. Full output written to disk,
  truncated preview returned with file path reference
- **Trigger**: when `total_tokens >= usable_context`
- **Config**: `opencode.json` with `compaction.auto`, `compaction.prune`,
  `compaction.reserved`
- **Known issue**: asymmetric overflow detection with `limit.input` models

### Claude Code

Source: web research (closed source)

- **Overflow strategy**: triggers at ~64-75% capacity (conservative)
  1. Tool outputs cleared first (largest tokens, least semantic value)
  2. Conversation summarized via Claude itself (server-side)
  3. Critical context preserved (code patterns, file paths, decisions)
- **User experience**: notification on auto-compact, `/compact` manual command
  with optional instructions ("preserve all file paths"), `/context` to inspect
- **Persistent rules**: `CLAUDE.md` survives compaction; in-conversation
  instructions may be lost
- **Manual controls**: `Esc+Esc` or `/rewind` to select checkpoint

### aider

Source: github.com/Aider-AI/aider

- **Different philosophy**: repo map as primary context strategy
  - Graph-ranked dependency map of codebase (class/method signatures)
  - `--map-tokens` flag (default 1000 tokens) controls map size
  - LLM requests specific files when deeper understanding needed
- **Chat history**: `ChatSummary` class auto-summarizes when approaching limits
  (background thread)
- **Token counting**: tiktoken (only harness using a real tokenizer)
- **Reactive**: reports provider token limit errors rather than preventing them

### pi-mono

Source: github.com/badlogic/pi-mono

- **Token counting**: token-level counting (not just estimation)
- **Overflow strategy**: structured compaction
  1. Walk backwards from newest, accumulate until `keepRecentTokens` (20K default)
  2. Everything before cut point → LLM structured summary
     (goal/constraints/progress/decisions/next steps/critical context with paths)
  3. `CompactionEntry` marks where preserved messages begin
- **Reserve**: 16,384 token reserve before triggering
- **Split turns**: single turns exceeding budget get split at assistant message,
  two summaries generated and merged
- **Session storage**: append-only JSONL with tree structure (id/parentId),
  enabling conversation branches

### crush

Source: github.com/charmbracelet/crush

- **Auto-compaction**: monitors token usage, summarizes on approach
- **LSP integration**: uses language servers for symbol tables (gopls,
  rust-analyzer, pyright) — structured context rather than raw file dumps
- **Prompt caching**: minimizes redundant tokens on supporting providers
- **Project awareness**: `.crushignore` for excluding large fixtures

## Common patterns

1. **Token estimation**: chars/4 heuristic is universal. Nobody integrates a
   real tokenizer for pre-flight checks. Actual counts come from API responses.

2. **Three-tier strategy**:
   - Tier 1: Truncate tool results (biggest wins, least semantic loss)
   - Tier 2: Drop/prune old messages (keep recent N turns)
   - Tier 3: LLM summarization (structured summary of progress)

3. **Structured summary templates** beat free-form. Common structure:
   goal, progress (done/in-progress/blocked), decisions, next steps,
   relevant files.

4. **Observation masking**: hide old tool outputs but keep tool call names
   visible. Research shows this often outperforms LLM summarization in
   efficiency and reliability.

5. **Safety margin**: trigger at 64-95% of context window, not at 100%.

6. **Proactive, not reactive**: everyone does pre-flight checks. Nobody
   waits for the API to reject and retries with fewer messages.

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
- If messages fit → pass through unchanged
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

### Phase 3: refinements

- Per-tool truncation policies (bash output vs file read vs grep)
- Post-hoc calibration: compare estimated tokens to actual API usage,
  adjust estimator
- `/context` command to inspect what's using space
- Prompt cache awareness (don't evict cached prefixes)
