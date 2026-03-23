# Microbenchmark Designs

Cheap, fast-feedback benchmarks for evaluating harness feature implementations.
These complement coarse benchmarks (terminal-bench, SWE-bench) by isolating
individual harness engineering decisions.

Ordered by cost and recommended execution sequence.

---

## 1. Fuzzy Edit: Match Accuracy

**Cost:** $0 (no API calls). **Time:** <10 seconds per run.

**What it measures:** Precision and recall of different string-matching strategies
on a corpus of near-miss edit attempts, without any model in the loop.

**Why it matters:** tau currently requires exact string match for `file_edit` in
replace mode. Models frequently produce near-miss `old_string` values (wrong
whitespace, unicode punctuation, stale content). Other harnesses handle this
with fuzzy fallbacks:

- pi-mono: 2-pass (exact, then trailing-ws + unicode normalization)
- codex: 4-pass cascade (exact → trim_end → trim → unicode normalize)
- opencode: 9-strategy chain (simple → line-trimmed → block-anchor →
  whitespace-normalized → indentation-flexible → escape-normalized →
  trimmed-boundary → context-aware → multi-occurrence)
- oh-my-pi: Levenshtein with threshold tuning (0.80-0.95), indent adjustment,
  comment-prefix normalization

### Setup

Curate a corpus of 200-500 triples: `(file_content, old_string_attempt, ground_truth_location)`.

Sources:
1. **Synthetic perturbations**: take real files, extract a block, perturb
   systematically. Tag each triple with its perturbation category:
   - `trailing-ws`: trailing whitespace added/removed
   - `indent-shift`: indentation level differs by 1-3 levels
   - `tabs-vs-spaces`: tab/space substitution
   - `unicode-punct`: smart quotes, em-dashes
   - `stale-content`: content modified since model last read
   - `partial-block`: old_string is a subset of the actual block
   - `ambiguous`: old_string matches multiple locations
   - `hallucinated`: old_string contains content not in the file
2. **Real model failures**: mine trajectories from tau benchmark runs and
   oh-my-pi's edit-benchmark runs. Extract cases where `file_edit` returned
   "old_string not found". The model's `old_string` is the test input.
3. **Adversarial negatives**: files with high structural repetition (React
   component lists, test suites, config files) where the old_string is close
   to 2+ locations.

### Matching strategies to implement

Each as a standalone pure function (no I/O):

| Strategy | Source | Description |
|----------|--------|-------------|
| `exact` | tau current | `content.matches(old_string).count()` |
| `normalized` | pi-mono | trailing-ws strip + unicode normalization |
| `trimmed-cascade` | codex | 4-pass: exact → trim_end → trim → unicode |
| `opencode-9` | opencode | 9-strategy chain, each is a generator |
| `levenshtein-92` | oh-my-pi | Levenshtein similarity >= 0.92 threshold |
| `levenshtein-95` | oh-my-pi | Levenshtein similarity >= 0.95 threshold |
| `levenshtein-80` | oh-my-pi | Levenshtein similarity >= 0.80 (aggressive) |

### Scoring

For each strategy x triple, record: `matched: bool`, `correct: bool`,
`match_location: Option<usize>`, `confidence: f64`.

Metrics:
- **True positive rate** = correct matches / total matchable triples
  (per perturbation category)
- **False positive rate** = wrong-location matches / total matches
- **Category breakdown**: which strategy helps most for which failure mode
- **Net value** = true_positives - (false_positives * penalty_weight)
  where penalty_weight reflects the cost of a wrong-location edit (high)

### Decision it informs

- Should tau add fuzzy matching at all?
- At what threshold?
- Is trailing-ws-trim-only sufficient? (Hypothesis: captures 80% of failures
  at 0% false positive risk.)
- Priority: fuzzy matching vs hashline improvement?

### Implementation notes

Build as a Rust binary in `benchmarks/fuzzy-match/`. Corpus stored as JSON
fixtures. Reusable for regression testing as tau's matching evolves.

Reference implementations to port:
- oh-my-pi fuzzy: `oh-my-pi/packages/coding-agent/src/patch/fuzzy.ts`
- oh-my-pi normalize: `oh-my-pi/packages/coding-agent/src/patch/normalize.ts`
- codex seek_sequence: `codex/codex-rs/apply-patch/src/seek_sequence.rs`
- opencode replacers: `opencode/packages/opencode/src/tool/edit.ts`
- pi-mono fuzzy: `pi-mono/packages/coding-agent/src/core/tools/edit-diff.ts`

---

## 2. Fuzzy Edit: False Positive Audit

**Cost:** $0. **Time:** <10 seconds per run.

**What it measures:** How often fuzzy matching applies an edit to the wrong
location in files with repetitive structure (the worst-case scenario).

### Setup

Curate 50 adversarial files with high structural repetition:
- React component lists (similar JSX blocks)
- Database migration files (similar ALTER TABLE blocks)
- Test suites with similar test cases
- Config files with repeated blocks (YAML, TOML)
- CSS files with similar selectors

For each file, create 3-5 edit attempts where the `old_string` is a slightly
perturbed version of one block but could plausibly match 2+ locations. Record
intended target and all candidate locations.

### Scoring

For each strategy x attempt:
- **Correct**: matched the intended location
- **Wrong location**: matched a different block (dangerous)
- **Rejected**: no match found (safe failure — model retries)
- **Ambiguous-rejected**: multiple matches, correctly refused

Key metric: **wrong-location rate** per strategy. If >1% for any common
strategy, tau should stay exact-only or limit to trailing-ws normalization.

**Safety ratio** = (correct + rejected) / total

### Decision it informs

Whether fuzzy matching is safe enough for tau. Sets the threshold floor.

---

## 3. Fuzzy Edit: End-to-End Model-in-Loop

**Cost:** $60-100. **Time:** ~8 hours.

**What it measures:** Task completion rate as a function of edit strategy,
using real model calls.

### Setup

Use oh-my-pi's mutation generator (`react-edit-benchmark`) to create ~80
fixtures from real JS/TS/Rust/Python files. Each: `input/` (mutated file),
`expected/` (original), `prompt.md`.

Port runner to work with tau's `serve` RPC interface.

### Run matrix (4 configurations)

| Config | Description |
|--------|-------------|
| `tau-exact` | Current replace mode, no fuzzy |
| `tau-exact+trimws` | Exact + trailing-whitespace normalization |
| `tau-hashline` | Hashline mode |
| `baseline` | oh-my-pi with fuzzy at default threshold (0.95) |

Same 80 tasks x 1 run, same model (claude-sonnet-4-6), same temperature.

### Scoring

- **Task success rate**: file matches expected after all edits
- **Edit tool success rate**: fraction of edit calls that succeed first attempt
- **Retry overhead**: extra turns caused by edit failures
- **Token cost per task**: hashline re-reads vs fuzzy saved retries
- **Indent score**: how much a formatter must fix (oh-my-pi's metric)
- **False edit rate**: edits that "succeeded" but changed wrong location

### Decision it informs

The central question: does fuzzy matching improve end-to-end scores enough to
justify false-positive risk? And: hashline re-read cost vs fuzzy retry savings?

Only run this after Benchmarks 1 and 2 have narrowed candidate strategies.

---

## 4. Post-Edit Diagnostics: Edit-Cycle Reduction

**Cost:** $2-5 per model. **Time:** ~45-120 minutes.

**What it measures:** Number of edit-test-fix cycles needed with and without
immediate compiler diagnostics.

**Why it matters:** opencode, oh-my-pi, and crush wire diagnostics into edit
tool results. The model sees type errors without a separate tool call. tau
currently requires the model to manually run `cargo check` or `tsc` via bash.

### Setup

4-6 small refactoring tasks where edits commonly introduce type errors:
- Rename a type parameter across a module (Rust)
- Update an API signature; call sites must follow (TypeScript)
- Extract a function with a slightly wrong return type (Python/Rust)
- Change a struct field type and propagate (Rust)

### Variants

| Variant | Description |
|---------|-------------|
| A (baseline) | No compiler feedback after edit |
| B (post-edit check) | Shell out to `cargo check`/`tsc --noEmit`/`ruff check` after each edit, append output to tool result |
| C (full LSP) | Run LSP diagnostics on changed file (if we build this) |

### Implementation for variant B

~150 lines of changes to tau:

```
After FileEditTool::execute() succeeds:
1. Detect project type from file extension + nearby config files
2. Shell out: cargo check --message-format=json, tsc --noEmit --pretty false, ruff check --output-format=json
3. Parse errors/warnings into structured list: {file, line, message, severity}
4. Append to edit tool result: "Edit applied. Diagnostics: 2 errors — line 45: expected u32, found String"
```

### Scoring

- **Cycle count**: turns from "edit introduced error" to "error resolved"
- **Total turns**: per task across all variants
- **Total tokens**: per task (variant B adds diagnostic output but may save
  retry turns)
- **Final correctness**: did the task pass at the end?

### Decision it informs

- Is post-edit compiler check sufficient, or is full LSP needed?
  (Hypothesis: compiler check captures 90% of the benefit at ~0 complexity.)
- Which languages benefit most? (Rust's strict types likely benefit more
  than Python.)
- Token tradeoff: diagnostic output tokens vs saved retry turns.

---

## 5. Compaction: Memory Retrieval

**Cost:** $10-25. **Time:** ~2-3 hours.

**What it measures:** Can the model recall specific facts after context
compaction? Does the compacted summary introduce hallucinations?

**Why it matters:** Every harness except tau has auto-compaction. No harness
benchmarks compaction quality in isolation — they all just run their full
suite with/without it and report the delta. This benchmark isolates the signal.

**Dependency:** Requires building compaction first. tau already has the
`transform_context` hook in the agent loop.

### Setup

Generate synthetic multi-turn conversations with known facts seeded at
specific turns. 50+ turns covering:

- Variable/function definitions (10 helper functions described in turns 5-15)
- Error messages (compilation/runtime errors in turns 8-12)
- File paths and structure (project layout in turns 3-6)
- Intermediate results (test outputs in turns 18-25)
- Decisions and constraints ("we chose X because Y" in turns 10-20)

### Run procedure

1. Turns 1-30: agent works on a coding task, accumulating history (~50K tokens)
2. Turn 31: trigger compaction (compress to ~20K tokens)
3. Turns 32-45: agent continues the task
4. Turn 46-50: ask direct recall questions:
   - "What were the 3 functions we defined in turns 5-8?"
   - "What was the error in the test output from turn 12?"
   - "What constraint did we decide on in turn 15?"

### Compaction variants

| Variant | Description |
|---------|-------------|
| Truncation | Drop oldest turns, keep last N |
| Observation masking | Replace old tool outputs with `[output from <tool_name> omitted]`, keep tool call names visible |
| LLM summarization | Structured summary (goal/progress/decisions/next steps/files) |
| Progressive | OpenDev-style: mask at 80%, prune at 85%, summarize at 95% |

### Scoring

- **Recall accuracy**: % of direct questions answered correctly (target: >85%)
- **Task continuation**: did the agent complete the original task? (binary)
- **False positive rate**: facts in the summary not in original history (should be 0%)
- **Token efficiency**: original_tokens / compacted_tokens
- **Compaction latency**: time spent in compaction step

### Decision it informs

- Which compaction strategy to implement first.
- Compaction trigger threshold (70% vs 80% vs 90% of context window).
- Whether observation masking (cheap, no LLM call) is sufficient vs
  LLM summarization (expensive, higher recall).
- JetBrains finding replication: does masking really beat summarization?

---

## 6. Compaction: Token Efficiency Curve

**Cost:** $8-12. **Time:** ~2-3 hours.

**What it measures:** The Pareto frontier of token savings vs task success rate
across compaction strategies.

### Setup

5 representative tasks at different complexity levels:
- Simple (20 turns, ~30K tokens)
- Medium (35 turns, ~60K tokens)
- Complex (50+ turns, ~100K tokens)

### Run matrix

For each task, run with different compaction strategies:

| Strategy | Description |
|----------|-------------|
| None | Full history, no compaction |
| Aggressive (80%) | Compress to 20% of original |
| Conservative (50%) | Compress to 50% |
| Dynamic | Trigger only when approaching context window |

### Scoring

Plot 2D scatter: X = tokens saved (compression ratio), Y = success rate.
Each point is a strategy x task combination. Identify the "knee" — where
token savings flatten without hurting success.

5 tasks x 4 strategies x 3 runs = 60 runs.

### Decision it informs

- Default compaction threshold.
- Whether LLM summarization is worth the latency + cost vs simple truncation.
- Model-specific tuning: do small models (Haiku) need more careful
  summarization than large models (Opus)?

---

## 7. Sub-agents: Parallel File Operations

**Cost:** $2-3. **Time:** ~30 minutes.

**What it measures:** Does running N independent file reads in parallel
(within a single turn) save wall-clock time vs sequential execution?

**Why it matters:** tau's architecture delegates orchestration to Hive. The
question is whether harness-native parallel tool execution (the "80% case")
is worth adding to the agent loop itself.

### Setup

Small codebase: 10 independent files with isolated functions.
Task: "Read these 10 files and identify which ones export a function named
`process_data`."

### Variants

| Variant | Description |
|---------|-------------|
| Sequential | Agent reads file1, then file2, ... (10 turns) |
| Parallel | Agent calls file_read for all 10 in one batch (1 turn) |
| Baseline harness | codex or opencode (which support parallel tool calls) |

### Scoring

- **Wall-clock time**: end-to-end
- **Token count**: input + output across all turns
- **API calls**: number of LLM round-trips

Win criterion: parallel >= 20% faster with <= 10% token increase.

10 runs per condition x 3 conditions x 5 models = 150 runs.

### Decision it informs

- Should tau add parallel tool execution to the agent loop?
- Is this sufficient, or are full sub-agents needed?
- Validates/invalidates the Hive delegation architecture.

---

## 8. Sub-agents: Decomposition vs Coordination Overhead

**Cost:** ~$18. **Time:** ~2 hours.

**What it measures:** At what task complexity does spawning separate agents
(with reduced context) hurt more than it helps?

### Setup

Task: "Refactor a small service: extract common utilities into a new module,
then update 5 callers." This task wants decomposition but has coordination
overhead — sub-agents don't know what was extracted.

### Variants

| Variant | Description |
|---------|-------------|
| Sequential single-agent | One agent does extract + all 5 updates |
| Sub-agents with messaging | Agent 1 extracts, sends results to agents 2-5 |
| Sub-agents without messaging | Agent 1 extracts, agents 2-5 re-discover changes |
| Hive orchestration | Hive coordinates extraction, spawns 5 tau workers |

### Scoring

- **Task success rate**: does final code compile and pass tests?
- **Total tokens**: across all agents
- **Wall-clock time**: end-to-end
- **Re-work**: did sub-agents redo discovery because they lacked context?

4 conditions x 3 models x 3 trials = 36 runs.

### Decision it informs

- Whether harness-native sub-agents help on coordinated tasks.
- Whether Hive-style orchestration matches or beats embedded sub-agents.
- The coherence loss question: does splitting context hurt more than
  parallelism helps?

---

## 9. Todo/Plan Tracking: Multi-Step Completion

**Cost:** $5-15. **Time:** ~1-2 hours.

**What it measures:** Does explicit plan/todo tracking improve multi-step
task completion rate?

**Why it matters:** 5 of 7 major harnesses have this. But nobody benchmarks
it. The implementations vary wildly:

| Harness | Mechanism | Model sees it? | Survives compaction? |
|---------|-----------|----------------|---------------------|
| codex | `update_plan` — client-rendered only. Source comment: "it's the *inputs* that matter, not the outputs" | No | No |
| opencode | TodoWrite/Read — stored in DB, rendered in UI | No (UI only) | Yes (separate store) |
| oh-my-pi | `todo_write` — mandatory system prompt: "call todo_write FIRST every turn" | Yes (forced) | Via history scan |
| kimi-cli | SetTodoList + plan mode — restricts tools to read-only, forces plan artifact, requires user approval | Yes (periodic system reminders) | Yes (separate file) |

Key insight: codex's plan tool is basically a no-op for the model. oh-my-pi's
is the opposite extreme — mandatory injection. The effectiveness likely comes
from (a) system prompt mandate, (b) periodic re-injection, and (c) survival
over compaction.

**Dependency:** Depends on compaction existing first. Todo tracking only helps
if state survives compaction.

### Setup

5-step refactoring task: read existing code → extract function → update
imports → add tests → verify tests pass.

### Variants

| Variant | Description |
|---------|-------------|
| Baseline | No todo tracking, standard system prompt |
| Optional tool | TodoWrite available but not mandated |
| Mandatory prompt | System prompt: "call todo_write before each step" |
| Plan mode | Read-only exploration phase, explicit approval before execution |

### Scoring

- **Task completion rate**: did all 5 steps succeed?
- **Step ordering**: did the model follow a logical sequence?
- **Recovery**: when step 3 fails, does the model recover or spiral?
- **Turns to completion**: efficiency
- **Token cost**: mandatory todo adds tokens — is it worth it?

### Additional benchmark: recovery from mid-task errors

Same task, but deliberately inject a test failure at step 3/5.
Measure: steps wasted re-exploring work, time to fix, spiral vs recover.

### Simplest implementation for tau

```
1. TodoWrite tool: takes [{step, status}], writes to .tau-plan.json in session dir
2. System prompt hook: "Before implementing, outline steps with todo_write"
3. After compaction: inject "Current plan: [from .tau-plan.json]"
4. TodoRead tool: returns current plan state
```

~500 LOC total.

### Decision it informs

- Is mandatory prompt injection worth the token cost?
- Does the model-visible approach (oh-my-pi) beat the UI-only approach (codex)?
- Is plan-mode restriction (kimi-cli) worth the complexity?
- Should this be a harness feature or just a system prompt technique?

---

## Implementation Roadmap

### Phase 1: Zero-cost benchmarks (build corpus, no API spend)

1. Fuzzy match accuracy corpus + matcher implementations (Benchmark 1)
2. Fuzzy false-positive audit corpus (Benchmark 2)

### Phase 2: Cheap A/B tests ($5-10)

3. Post-edit diagnostics implementation + edit-cycle benchmark (Benchmark 4)
4. Parallel file operations benchmark (Benchmark 7)

### Phase 3: Feature builds + evaluation ($20-40)

5. Build compaction (using `transform_context` hook)
6. Compaction memory retrieval + efficiency curve (Benchmarks 5-6)
7. Build todo tracking
8. Todo multi-step completion benchmark (Benchmark 9)

### Phase 4: Full model-in-loop evaluation ($60-100)

9. Fuzzy edit end-to-end (Benchmark 3)
10. Sub-agent decomposition (Benchmark 8)

### Total estimated cost

| Phase | Cost | Time |
|-------|------|------|
| Phase 1 | $0 | 2-3 days |
| Phase 2 | $5-10 | 1-2 days |
| Phase 3 | $20-40 | 3-5 days |
| Phase 4 | $60-100 | 2-3 days |
| **Total** | **$85-150** | **~2 weeks** |
