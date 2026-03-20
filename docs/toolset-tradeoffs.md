# Toolset Tradeoffs

How does the choice of tools you expose to a coding agent affect its behavior? This doc compares the toolsets of seven harnesses — tau, kimi-cli, pi-mono, oh-my-pi, crush, codex, and opencode — and analyzes what each choice implies.

## The landscape

| Tool | tau | kimi-cli | pi-mono | oh-my-pi | crush | codex | opencode |
|------|:---:|:--------:|:-------:|:--------:|:-----:|:-----:|:-------:|
| bash/shell | bash | Shell | bash | bash | bash | shell | bash |
| file read | file_read | ReadFile | read | read | view | read_file | read |
| file edit | file_edit | StrReplaceFile | edit | edit | edit | apply_patch | edit *or* apply_patch |
| file write | file_write | WriteFile | write | write | write | (via apply_patch) | write |
| grep | grep | Grep | grep | grep | grep | grep_files | grep |
| glob/find | glob | Glob | find | find | glob | list_dir | glob |
| ls | — | — | ls | — | ls | list_dir | ls |
| **Total core** | **6** | **6** | **7** | **6** | **7** | **6** | **7** |
| sub-agents | — | Task | — | task | agent | spawn_agent | task |
| web fetch | — | FetchURL | — | fetch | fetch | — | webfetch |
| web search | — | SearchWeb | — | web_search | web_search | web_search | websearch |
| code search | — | — | — | — | sourcegraph | — | codesearch |
| browser | — | — | — | browser | — | — | — |
| LSP | — | — | — | lsp | lsp_* (3) | — | lsp |
| notebooks | — | — | — | notebook | — | js_repl | — |
| python REPL | — | — | — | python | — | — | — |
| AST edit | — | — | — | ast_grep/edit | — | — | — |
| multi-edit | — | batched replace | — | — | multiedit | — | multiedit |
| batch exec | — | background tasks | — | — | — | — | batch |
| todos/plan | — | SetTodoList + plan mode | — | todo_write | todos | update_plan | todowrite/read |
| checkpoint | — | — | — | checkpoint | — | — | — |
| ask user | — | AskUserQuestion | — | ask | — | request_user_input | question |
| hashline edit | hash_file_edit | — | — | hash_file_edit | — | — | — |
| **Total** | **8** | **17 (default)** | **7** | **23+** | **23** | **30+** | **~20** |

Every harness converges on the same six core tools: shell execution, file read, file edit, file write, content search, and file search. The divergence is in what else gets added on top.

## Core convergence: the minimal viable toolset

All seven harnesses agree on a base layer:

1. **Shell** — run arbitrary commands. The universal escape hatch.
2. **Read** — read file contents with line numbers.
3. **Edit** — modify existing files (exact match, patch, or AST).
4. **Write** — create new files or overwrite.
5. **Grep** — search file contents by pattern.
6. **Glob/Find** — search for files by name pattern.

This set is sufficient for the full coding loop: discover files, read them, understand the code, modify it, verify with shell commands. Everything else is an optimization or an expansion of the agent's reach.

### Why not just bash?

An earlier version of tau had only bash + read + edit + write — no grep or glob. The model used bash to run `rg`, `find`, `ls`. This works, but has costs:

- **Prompt overhead.** The system prompt has to explain *how* to use bash for search ("use `rg -n` for grep, `find . -name` for file search"). Dedicated tools eliminate this — the tool schema *is* the documentation.
- **Error surface.** Models construct shell commands with quoting bugs, wrong flags, platform-specific syntax. A grep tool with `pattern` and `path` parameters can't have a quoting bug.
- **Observability.** A `grep` tool call in the trace is instantly readable. A `bash` call with `rg -n --color=never --no-heading -C 2 --glob '*.rs' 'fn main' src/` requires parsing.
- **Guardrails.** Dedicated tools can enforce limits (max results, timeout), respect .gitignore, and normalize output format. Bash is unconstrained.

The tradeoff: each dedicated tool narrows the model's decision surface for that task, at the cost of one more tool definition in the context.

## Dimension 1: Edit strategy

The most interesting divergence across harnesses is how they let the model edit files.

### Model-aware tool switching (opencode)

opencode does something no other harness does: it dynamically swaps tools based on which model is running. GPT models get `apply_patch` (a custom patch DSL); Claude/Anthropic models get `edit` + `write` (exact string match). The registry conditionally includes or excludes tools at session start.

This is a pragmatic acknowledgment that different models have different tool-use strengths. GPT models were trained on patch-style editing; Claude models perform better with exact-match replacement. Rather than picking one strategy and forcing all models to adapt, opencode adapts the toolset to the model.

**Tradeoff:** Better per-model performance. Higher implementation complexity — the harness must maintain two edit paths and the system prompt must adapt. Makes cross-model benchmarking harder since the tool surface isn't constant.

### Exact string match (tau, kimi-cli, pi-mono, crush, opencode for Claude)

```json
{"old_string": "fn foo() {", "new_string": "fn foo() -> Result<()> {"}
```

The model provides the exact text to find and its replacement. Simple to implement, simple for the model to understand. Fails when `old_string` appears multiple times or when the model hallucinates whitespace. pi-mono mitigates this with fuzzy matching (whitespace/unicode normalization). tau requires exact match and gives diagnostic context on failure. kimi-cli stays in this family too, but adds one pragmatic twist: `StrReplaceFile` accepts a list of exact replacements in one call, so it can batch a small refactor without committing to a patch DSL.

**Tradeoff:** Low token cost per edit. High failure rate on large or repetitive files.

### Unified diff / patch (codex, opencode for GPT)

```
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,3 @@
-fn foo() {
+fn foo() -> Result<()> {
```

The model produces a patch. More expressive — can handle multi-hunk edits in one call. But models frequently produce malformed patches (wrong line numbers, missing context lines, tab/space confusion). Codex has extensive normalization and repair logic to compensate.

opencode uses a custom patch DSL rather than unified diff, designed to be easier for models to produce correctly:

```
*** Begin Patch
*** Update File: src/main.rs
@@ fn foo() {
-fn foo() {
+fn foo() -> Result<()> {
*** End Patch
```

The custom format uses function signatures as context anchors instead of line numbers, which models get wrong less often. It also supports multi-file operations (Add File, Delete File, Move) in a single tool call.

**Tradeoff:** Higher expressiveness per tool call. Higher implementation complexity and fragility. opencode's custom DSL mitigates some of the fragility but adds a non-standard format the model must learn.

### Hash-anchored lines (oh-my-pi, tau)

```json
{"anchor": "10#A7F2", "operation": "replace", "content": "fn foo() -> Result<()> {"}
```

Invented by Can Boluk for oh-my-pi, hashline mode tags every line with a short content hash anchor. The model references lines by position+hash, which the tool validates. If the file changed since the model read it, stale hashes are rejected. tau ports the concept to Rust as a switchable edit mode alongside exact-match, enabling A/B comparison on the same harness.

**Tradeoff:** Eliminates ambiguity (no string matching). Requires a re-read after every edit (hashes change). Higher token cost from hash-annotated file contents.

### AST-aware edit (oh-my-pi only)

```json
{"pattern": "fn $NAME($$$ARGS) { $$$ }", "replacement": "fn $NAME($$$ARGS) -> Result<()> { $$$ }"}
```

oh-my-pi has `ast_grep` and `ast_edit` tools that operate on syntax tree patterns. Structural matching eliminates whitespace sensitivity entirely and enables semantic transforms (rename all occurrences of a pattern, wrap all function bodies in error handling).

**Tradeoff:** Most precise edit mechanism. Only works for languages with ast-grep support. Models must learn pattern syntax.

### Multi-edit (kimi-cli, crush, opencode)

kimi-cli, crush, and opencode all reduce round trips for repeated edits, but they do it differently. kimi-cli keeps the edit model simple: `StrReplaceFile` takes a list of exact string replacements and shows a diff for approval. crush and opencode go further with dedicated `multiedit` tools; crush applies edits sequentially with partial success, while opencode treats the batch atomically (all succeed or all fail).

**Tradeoff:** Fewer round trips for multi-site edits. Atomicity vs. partial success is itself a design choice — atomic is safer but wastes work on failure; partial success is more forgiving but leaves the file in a half-edited state. kimi-cli's version is less expressive than a patch tool, but much easier for a model to produce correctly.

## Dimension 2: Toolset breadth

The harnesses cluster into three camps:

### Thin toolset (tau: 8, pi-mono: 7)

Only the tools needed for the core coding loop. Everything else goes through bash.

**Advantages:**
- Smaller tool definition block in context (fewer tokens per turn).
- Simpler decision surface — the model has fewer tools to choose between.
- Faster to reason about correctness: 6-8 tools can be exhaustively tested.
- The system prompt stays small and focused.

**Disadvantages:**
- Forces bash for anything beyond file ops (web requests, git, language servers).
- The model must know how to construct the right shell command for each task.
- No structured output for complex operations (LSP diagnostics, web content).

### Productized mid-size toolset (kimi-cli: 17, opencode: ~20)

kimi-cli and opencode sit between the thin and thick camps, but with different priorities.

kimi-cli's extra tools are mostly about operating safely in an interactive product: ask-user prompts, todos, plan mode, web search/fetch, subagents, and background shell task management. It does not add LSP, browser automation, notebooks, or a patch DSL. The result is a bigger surface than tau, but still a fairly coherent one.

opencode has the core 7, plus web search, LSP, sub-agents, multi-edit, batch execution, and todos — but not browser automation, notebooks, AST editing, or checkpointing. It's selective about which extensions earn a tool.

The most distinctive addition here is opencode's **batch** — a meta-tool that runs up to 25 other tool calls in parallel within a single turn. This is unique across all seven harnesses. Instead of the model issuing tool calls sequentially (read file A, then read file B, then read file C), it wraps them in a batch and gets all results at once.

**Tradeoff:** kimi-cli spends its complexity budget on user-facing workflow primitives; opencode spends it on execution efficiency. batch reduces latency on parallel-safe operations but adds a layer of abstraction the model must reason about. kimi-cli's background task tools are easier to reason about, but only parallelize shell work, not arbitrary tool calls.

### Thick toolset (oh-my-pi: 23+, crush: 23, codex: 30+)

Dedicated tools for many tasks: web search, browser automation, LSP, notebooks, sub-agents, planning.

**Advantages:**
- Structured interfaces for complex operations (LSP gives precise diagnostics vs. parsing compiler output).
- Sub-agent tools enable parallelism and task decomposition.
- Planning tools (todos, checkpoints) give the model explicit state management.
- Web tools give the agent access to documentation, APIs, and external context.

**Disadvantages:**
- Each tool definition consumes context tokens. 30 tools with schemas can be 3-5K tokens.
- More tools = more chances for the model to pick the wrong one. "Should I use `bash` to run the tests, or `exec_command`, or `python`?"
- Testing surface grows combinatorially. Tool interactions create emergent behaviors.
- The system prompt must explain when to use each tool and when not to.

### What the data suggests

pi-mono runs the most mature benchmarks across harnesses. Their production toolset is 7 tools — they explicitly chose to *remove* tools that didn't improve benchmark scores. crush and codex started thick and have been pruning. oh-my-pi is the outlier with maximal tools, but also optimizes for a different use case (interactive assistant with browser, notebooks, SSH — not just coding).

The pattern: **start with the convergent 6, add tools only when bash is measurably worse for a specific task.**

## Dimension 3: Sub-agent delegation

Five harnesses support spawning sub-agents: kimi-cli (`Task`), oh-my-pi (`task`), crush (`agent`), codex (`spawn_agent` + `send_input` + `wait_agent` + `close_agent`), and opencode (`task`).

tau and pi-mono do not have sub-agent tools.

**Arguments for sub-agents:**
- Enables parallelism — search multiple files simultaneously, run tests while editing.
- Natural decomposition of complex tasks.
- Isolates failures — a sub-agent crash doesn't kill the parent.

**Arguments against:**
- Multiplies cost — each sub-agent is a separate LLM call chain.
- Coordination complexity — the parent must track sub-agent state, handle partial failures.
- The model must learn *when* to delegate vs. do it directly.
- Codex needs 5 tools just for agent lifecycle (spawn, send, wait, resume, close). That's toolset bloat for coordination overhead.

kimi-cli is interesting here because it splits parallelism into two separate concepts: subagents for isolated reasoning, and background shell tasks for long-running commands. That is a more opinionated decomposition than codex or opencode, where "parallel work" is mostly one bucket.

**tau's position:** No sub-agent tool. The hive orchestrator handles parallelism at a higher level — the agent itself stays single-threaded. This is a deliberate architectural choice: delegation lives in the harness infrastructure, not in the model's tool surface.

## Dimension 4: Web and external access

| | tau | kimi-cli | pi-mono | oh-my-pi | crush | codex | opencode |
|---|:---:|:--------:|:-------:|:--------:|:-----:|:-----:|:-------:|
| HTTP fetch | — | FetchURL | — | fetch | fetch, agentic_fetch | — | webfetch |
| Web search | — | SearchWeb | — | web_search | web_search | web_search | websearch |
| Code search | — | — | — | — | sourcegraph | — | codesearch |
| Browser | — | — | — | browser (Puppeteer) | — | — | — |

**Arguments for web tools:**
- Models can look up API docs, Stack Overflow answers, library changelogs.
- Reduces hallucination when the model is uncertain about an API.
- Enables tasks that require external data (fetching schemas, checking deployment status).

**Arguments against:**
- Latency — web fetches add seconds to tool execution.
- Token cost — web pages are large. Even cleaned, a docs page is 2-10K tokens.
- Security surface — the agent can now exfiltrate code to arbitrary URLs or fetch malicious content.
- For benchmarking, web access introduces non-determinism.

kimi-cli's web tools are more service-backed than agentic: `SearchWeb` calls a configured Moonshot search endpoint, and `FetchURL` prefers a configured fetch service but can fall back to local fetching. That is different from oh-my-pi's browser-first stance or opencode's generalized web/code search. It keeps the harness simple, but it also means web capability quality depends heavily on the provider-side service.

**tau's position:** No web tools. Coding agents operating on local codebases rarely need web access. When they do, `bash` + `curl` works. If web access becomes important for benchmarks, it's a candidate for a dedicated tool.

## Dimension 5: Structured code intelligence (LSP)

oh-my-pi, crush, and opencode all expose LSP tools:

- **oh-my-pi:** Single `lsp` tool with multiple capabilities.
- **crush:** Three tools: `lsp_diagnostics`, `lsp_references`, `lsp_restart`.
- **opencode:** Single `lsp` tool with 9 operations (goToDefinition, findReferences, hover, documentSymbol, workspaceSymbol, goToImplementation, prepareCallHierarchy, incomingCalls, outgoingCalls). Gated behind an experimental flag, suggesting they're still evaluating its value.

opencode also integrates LSP *implicitly* — the edit and write tools automatically trigger LSP diagnostics after modifying a file, feeding type errors back to the model without a separate tool call. This is a clever middle ground: the model doesn't need to know about LSP to benefit from it.

**Arguments for LSP:**
- Precise "find all references" — grep finds string matches, LSP finds semantic references.
- Real-time diagnostics — the model sees type errors and warnings without running the compiler.
- Rename refactoring with LSP is more reliable than grep-and-replace.

**Arguments against:**
- LSP servers are heavy — spinning up `rust-analyzer` or `typescript-language-server` adds startup latency and memory.
- LSP is stateful — the server must index the project before results are useful. Race conditions abound.
- For most edits, grep + compiler output (via bash) is sufficient.
- crush needs a dedicated `lsp_restart` tool, which hints at reliability issues.

**tau's position:** No LSP tools. Bash can run compilers and linters. Grep handles most "find references" cases. If benchmarks show that LSP-guided edits have meaningfully higher success rates, it's worth adding — but the operational complexity is high.

## Dimension 6: Planning and state management

| | tau | kimi-cli | pi-mono | oh-my-pi | crush | codex | opencode |
|---|:---:|:--------:|:-------:|:--------:|:-----:|:-----:|:-------:|
| Todo/plan | — | SetTodoList + plan mode | — | todo_write | todos | update_plan | todowrite/read |
| Checkpoint | — | — | — | checkpoint + rewind | — | — | — |

These tools let the model explicitly manage its own state: create task lists, save progress checkpoints, rewind on failure.

**Arguments for:**
- Complex tasks benefit from explicit decomposition before execution.
- Checkpoints enable safe exploration — try an approach, rewind if it fails.
- Makes the model's plan visible in the trace for debugging.

**Arguments against:**
- Models already plan implicitly in their reasoning. Externalizing it adds tool-call overhead.
- Checkpoints are complex to implement correctly (file system state, git state, agent state).
- In practice, git provides checkpointing. `git stash` / `git checkout` via bash is equivalent.

The distinctive move here is kimi-cli's **plan mode**. This is not just a todo list: it is an enforced read-only phase where the model only gets `Glob`, `Grep`, and `ReadFile`, writes a plan artifact, and asks the user to approve or revise it before execution. That is much stronger than codex's `update_plan` or opencode's todo list because it changes the available tool surface, not just the trace.

**tau's position:** No planning tools in the agent itself. Planning happens at the hive level (the queen decomposes tasks into issues). The agent is a worker that executes a well-scoped task — it shouldn't need to plan.

## Spotlight: kimi-cli's interesting choices

kimi-cli (Python, 17 default tools) is the clearest example here of a harness that is more product than benchmark scaffold. The interesting part is not raw tool count, but which problems it chose to productize.

### Plan mode as a hard phase boundary

kimi-cli's plan mode is not advisory. It shrinks the tool surface to read-only exploration, forces the model to write a plan file, and routes the next step through user approval or revision. That makes planning auditable and safe, but also slows down short tasks where an experienced model could have just started coding.

### Background shell as a first-class primitive

Instead of a general batch tool, kimi-cli treats long-running shell commands as background jobs with IDs, progress inspection, stop controls, and automatic completion notifications. This matches interactive development work very well: builds, tests, watchers, and servers are the long pole more often than file reads.

### Sessions as workflow state, not just transcripts

kimi-cli persists not just chat history, but approval state, plan mode, dynamic subagents, and additional workspace directories. That makes resuming a session feel like reopening a workbench, not replaying a transcript. Most benchmark harnesses stop at message history.

### Multiple frontends over one core

The same harness runs in terminal mode, Web UI, ACP server mode for IDEs, and a separate trace visualizer. That pushes design decisions toward approval UX, structured questions, session search/fork/archive, and interoperable wire logs. tau is currently much closer to a core harness than a user-facing product.

## Spotlight: opencode's interesting choices

opencode (TypeScript, ~20 tools) deserves special attention because it makes several architectural bets that differ from the other harnesses:

### Model-aware toolset composition

The tool registry dynamically includes/excludes tools based on the model. GPT gets `apply_patch`; Claude gets `edit` + `write`. LSP and batch are behind experimental flags. The `question` tool only appears for interactive clients (app/cli/desktop), not headless mode. This is the most sophisticated tool filtering across all seven harnesses — everyone else gives every model the same tools.

This raises an interesting benchmarking question: should a harness optimize per-model, or should it provide a uniform interface and let the model adapt? Per-model optimization likely wins on benchmarks but makes the harness harder to reason about.

### Implicit LSP feedback

Rather than exposing LSP only as a tool the model calls explicitly, opencode's edit and write tools trigger LSP diagnostics automatically and include them in the tool result. The model sees "your edit introduced 2 type errors" without needing to know LSP exists. This is arguably the right level of abstraction — LSP as infrastructure, not as a user-facing tool.

If tau were to adopt LSP, this implicit approach is worth copying: wire diagnostics into the edit tool result rather than adding a separate `lsp_diagnostics` tool.

### Batch tool as parallel execution primitive

opencode's `batch` tool lets the model explicitly parallelize up to 25 tool calls. Most harnesses either do this implicitly (the LLM API supports parallel tool calls natively) or not at all. Making it an explicit tool is unusual — it gives the model control over parallelism at the cost of one more tool to reason about.

The value depends on the LLM API: if the API already supports parallel tool calls (Anthropic does), batch is redundant. If the API is sequential (some OpenAI modes), batch adds real value.

### Custom patch DSL

opencode's `apply_patch` uses a purpose-built format instead of unified diff. Context anchors use function signatures rather than line numbers, which models get wrong less often. The format supports multi-file operations (add, update, delete, move) in one call. This is a pragmatic recognition that unified diff is a format designed for humans and `patch(1)`, not for LLMs.

### File time locking

Edit and write tools track file modification times and reject edits to files that changed since the model last read them. This is similar in spirit to tau's hashline approach (reject stale state) but coarser-grained (whole-file timestamp vs. per-line hash).

## Summary: tau's design philosophy

tau's toolset is deliberately thin: the convergent 6 core tools plus hashline variants. The reasoning:

1. **Minimize the model's decision surface.** Fewer tools = fewer wrong choices = more predictable behavior. The model should spend tokens on the *task*, not on deciding *which tool to use*.

2. **Bash is the escape hatch.** Anything not worth a dedicated tool goes through bash. The threshold for adding a tool: it must be measurably better than the bash equivalent across benchmarks.

3. **Delegation lives outside the agent.** Sub-agents, planning, and coordination are handled by the hive orchestrator, not by giving the model tools to manage its own complexity.

4. **Edit strategy as a variable, not a constant.** tau implements both exact-match and hashline editing (ported from Can Boluk's oh-my-pi) as switchable modes. The bet is that having both in one harness enables direct A/B comparison — and that better edit reliability matters more than more tool variety.

5. **Benchmarking decides.** The toolset should grow based on measured impact, not feature parity with other harnesses. If LSP, web search, or sub-agents move benchmark scores, they earn their place.

### What to consider adding next

Based on convergence across harnesses and likely benchmark impact:

- **Implicit LSP diagnostics** — opencode's approach of wiring LSP feedback into the edit tool result (not as a separate tool) is compelling. The model gets type error feedback for free, without tool-choice overhead. Worth prototyping as an enhancement to file_edit rather than a standalone tool.
- **ls** — pi-mono, crush, and opencode have it. Listing directory contents is common enough that a structured tool (with depth control, type labels) might outperform `bash ls -la`. Low cost to add.
- **multiedit / batched replace** — kimi-cli, crush, and opencode all reduce round trips here. Worth testing whether tau should add a low-risk batched exact-replace tool before jumping straight to patch mode.
- **Model-aware tool filtering** — opencode's dynamic tool composition is worth watching. If tau supports multiple models with different edit strengths, conditional tool selection could help. But adds complexity and makes benchmarking less apples-to-apples.
- **ask user** — kimi-cli, oh-my-pi, codex, and opencode let the agent ask clarifying questions. Useful for interactive mode, irrelevant for headless benchmarks.
- **plan mode** — kimi-cli shows a stronger version of planning than a todo tool: read-only exploration plus explicit approval before writing. Worth studying if tau wants a safe "research first" mode.
- **web search** — five of seven harnesses have it. Might help on tasks requiring API knowledge the model lacks. Adds non-determinism.
