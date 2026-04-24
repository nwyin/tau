# Token Efficiency Analysis

Analysis of tau's token efficiency against codex, claude-code, oh-my-pi, and
pi-mono, based on the mini-harness-bench runs and the source-level prompt/tool
audit captured in issue #11.

## Executive summary

The headline result from mini-harness-bench is that tau matched codex on solve
rate with roughly half the real tokens and half the cost on the same model:

| Agent | Model | Tasks | Solved | Real tokens | Cache tokens | Cost | Wall time |
|---|---|---:|---:|---:|---:|---:|---:|
| tau | gpt-5.4-mini | 32 | 31/32 | 3.0M | 1.6M | $2.92 | 3264s |
| codex | gpt-5.4-mini | 32 | 31/32 | 6.2M | 5.5M | $5.89 | 2372s |

On the earlier sonnet smoke tier, tau also used far less cached context than
claude-code while solving the same 8/8 tasks:

| Agent | Model | Tasks | Solved | Real tokens | Cache tokens | Cost | Wall time |
|---|---|---:|---:|---:|---:|---:|---:|
| tau | claude-sonnet-4.5 | 8 | 8/8 | 15K | 692K | $0.71 | 377s |
| claude-code | claude-sonnet-4.5 | 8 | 8/8 | 24K | 5.2M | $4.18 | 472s |

The primary cause is not one trick. Tau is efficient because it keeps every
recurring request component small: prompt text, visible tool descriptions,
default tool count, per-turn injected context, and output style.

## Benchmark provenance

The benchmark numbers above come from `nwyin/mini-harness-bench` issue #1 as
referenced by tau issue #11:

- Full 32-task run, gpt-5.4-mini: tau 31/32 at 3.0M real tokens and $2.92;
  codex 31/32 at 6.2M real tokens and $5.89.
- Smoke 8-task run, claude-sonnet-4.5: tau 8/8 at 15K real tokens and $0.71;
  claude-code 8/8 at 24K real tokens and $4.18.

Token accounting note: "real tokens" means input plus output tokens excluding
cache reads/writes where the harness exposes that split. "Cache tokens" means
cache reads plus writes. Cross-harness accounting is close enough for cost and
relative-overhead analysis, but exact provider-level categories are not always
reported identically.

## Per-turn overhead snapshot

This table captures the source audit summarized in issue #11. It is a benchmark
snapshot, not a guarantee that every current harness release still has identical
numbers.

| Dimension | tau | pi-mono | oh-my-pi | codex | claude-code |
|---|---|---|---|---|---|
| System prompt | about 4K chars in the benchmarked lean mode | about 1.5K chars | about 21K chars | about 22K chars | about 17K chars |
| Visible tool set | 10 in the benchmarked lean mode | 7 | 54 plus MCP | 40+ | 8 base plus 184 deferred |
| Tool descriptions | first sentence in prompt listing | one-line snippets | markdown templates with instruction blocks | moderate | multi-paragraph |
| Per-turn injection | none by default | none by default | app-controlled steering callbacks | 30-48K chars when memory is active | about 8.2K chars of reminders |
| Context compaction | mechanical compaction in current tau | token-threshold summaries | multiple strategies | yes | auto-summarization near context limit |
| Cross-session memory | none | session-only | session plus file tracking | cross-session learning | cross-session plus project memory files |

Current tau caveat: `coding-agent/src/agent_builder.rs` now extends the 10
default tools with six orchestration tools (`thread`, `query`, `document`,
`log`, `from_id`, and `py_repl`) and `coding-agent/src/system_prompt.rs`
includes the orchestration prompt whenever `thread` is present. Re-run
mini-harness-bench before treating the 4K/10-tool tau row as current HEAD.

## Source-level causes

### 1. Small static prompt in lean mode

Tau's core static prompt is split across small markdown files in
`coding-agent/prompts/`:

- `identity.md`
- `system.md`
- `doing_tasks.md`
- `executing_with_care.md`
- `tone_and_output.md`

Those five files total about 3.3K characters in the current tree. The prompt
builder then adds tool names, first-sentence tool summaries, tool-use guidance,
skills when loaded, and the current working directory.

This matters because every API call pays for the stable prompt either as direct
input or as cached input. A 15K-20K character prompt delta compounded over
hundreds of calls becomes millions of tokens.

### 2. One-sentence tool listing

`coding-agent/src/system_prompt.rs` defines `first_sentence()` and applies it
to each tool description in the `# Available tools` prompt section. This keeps
the human-readable tool list compact even if individual tool schemas are more
detailed.

Important distinction: the full tool descriptions and JSON schemas are still
sent to the model as tool definitions in `agent/src/loop_.rs`. The
`first_sentence()` optimization reduces prompt text, not the schema payload.

### 3. No default per-turn memory or reminder injection

Tau's loop sends the system prompt, conversation messages, and tools. It can
inject steering messages via `get_steering_messages`, but normal task execution
does not add fresh git status, memory templates, skills catalogs, project-file
instructions, or deferred-tool catalogs on every turn.

That is the dominant efficiency difference. Per-turn injections of 8K-48K
characters look modest once, but across roughly 320 calls they add hundreds of
thousands to millions of tokens.

### 4. Smaller default tool surface

The benchmarked tau configuration used the 10 `default_tools()` in
`coding-agent/src/tools/mod.rs`:

- `bash`
- `file_read`
- `file_edit`
- `file_write`
- `glob`
- `grep`
- `web_fetch`
- `web_search`
- `subagent`
- `todo`

Current agent construction adds six orchestration tools by default. Even with
that addition, tau still avoids MCP catalogs, large deferred tool catalogs,
Notebook tools, LSP tools, browser automation, and shell variants unless they
are deliberately added.

### 5. Cache points are intentionally narrow

Tau marks a small number of cache boundaries:

- Anthropic: system prompt block, last tool definition, and the last user
  content block in `ai/src/providers/anthropic.rs`.
- OpenAI Responses: `prompt_cache_key` and optional 24-hour retention in
  `ai/src/providers/openai_responses.rs`.

Small cached content is cheaper to write and cheaper to read. Codex and
claude-code can use caching effectively, but larger prompt, tool, and injection
payloads create larger cache volumes.

### 6. Output style is terse

`coding-agent/prompts/tone_and_output.md` asks the model to lead with the answer
or action and avoid unnecessary reasoning. That reduces output tokens and can
also reduce follow-up turns caused by verbose intermediate narration.

Codex and claude-code also ask for concise output, but their broader protocols
often require preambles, progress updates, reminder handling, and richer final
answer formatting.

## What tau gives up

Tau's minimalism is a tradeoff. The overhead in other harnesses buys useful
capabilities:

| Capability | What higher-overhead harnesses gain | Tau tradeoff |
|---|---|---|
| File-change awareness | Per-turn notifications about external edits, hooks, or formatters | Tau must explicitly inspect `git diff`, reread files, or rely on tool results |
| Project convention grounding | Repeated injection of project memory files such as CLAUDE.md | Tau can load instructions once, but does not repeatedly refresh them |
| Dynamic tool discovery | Deferred catalogs and loaders for rare tools | Tau has a smaller fixed surface unless a tool is explicitly enabled |
| Cross-session memory | Learned project preferences and recurring failure modes | Tau starts each session without persistent learned memory |
| Long-context handling | LLM summaries or rolling compaction | Current tau has deterministic mechanical compaction, but not LLM handoff summaries |
| Git orientation | Current branch, dirty state, and recent commits stay visible | Tau generally needs explicit git-status calls |

These are real capabilities. The recommendation is not to reject them; it is to
make them conditional, lazy, and measurable.

## Recommendations

1. Preserve zero default per-turn injection.

   Any recurring reminder, memory, git-state, or skills catalog should have a
   token budget and a trigger condition. If it appears in every call, it should
   be treated as part of the core prompt and benchmarked that way.

2. Re-baseline current HEAD.

   The issue #11 numbers are valid for the benchmarked configuration, but
   current tau has orchestration tools and mechanical compaction. Re-run the
   32-task mini-harness-bench matrix before using the 4K prompt and 10-tool
   figures in release notes.

3. Add prompt/tool budget checks.

   A lightweight test or script should print and optionally threshold:
   core prompt characters, full built prompt characters, tool count, total tool
   description characters, and tool schema JSON size. Track both lean mode and
   default orchestration mode.

4. Prefer once-loaded project instructions over per-turn reinjection.

   A `.tau` project instruction file or config-derived guidance can be included
   in the system prompt at session start. Avoid reinjecting the same file every
   turn unless a freshness check shows it changed and the model needs to know.

5. Make file-change awareness lazy.

   Track file mtimes or git state internally, but notify the model only when it
   is about to use stale information, for example when rereading a file that was
   modified externally after the last read.

6. Keep advanced tools on demand.

   If LSP, notebooks, MCP, browser automation, or specialized search tools are
   added, keep the default prompt free of large catalogs. Let the model request
   discovery through a small tool-loading primitive.

7. Extend compaction only when mechanical compaction fails.

   Current mechanical compaction is zero-token and deterministic. LLM summaries
   are useful for very long sessions, but should trigger near context pressure
   and should report their own token and latency cost.

## Local references

These paths are the local source anchors for the analysis:

| Topic | Path |
|---|---|
| Prompt builder and `first_sentence()` | `coding-agent/src/system_prompt.rs` |
| Static prompt files | `coding-agent/prompts/` |
| Default and orchestration tools | `coding-agent/src/tools/mod.rs` |
| Agent construction and compaction hook | `coding-agent/src/agent_builder.rs` |
| Agent loop and tool-schema conversion | `agent/src/loop_.rs` |
| Mechanical compaction | `agent/src/context.rs` |
| Anthropic cache boundaries | `ai/src/providers/anthropic.rs` |
| OpenAI Responses cache key handling | `ai/src/providers/openai_responses.rs` |
| Benchmarking strategy | `docs/benchmarking.md` |
| Harness feature comparison | `docs/feature-comparison.md` |
| Context-management tradeoffs | `docs/context-management.md` |

## Bottom line

Tau's efficiency comes from disciplined restraint: small prompts, short visible
tool descriptions, a limited default tool surface, no routine per-turn memory
injection, and terse output instructions. That restraint is why tau can match
codex's solve rate at about half the real tokens in the 32-task benchmark.

The durable design rule is to treat every recurring prompt addition as a
product decision with a measured token budget, not as free context.
