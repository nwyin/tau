# Compaction: Memory Retrieval

Phase: 3 | Type: online | Cost: $10-25 | Time: ~2-3 hours

## What it measures

Can the model recall specific facts after context compaction? Does the
compacted summary introduce hallucinations? This isolates compaction quality
from task performance — a unit test for compaction strategies.

## Why it matters for tau

Every harness except tau has auto-compaction. No harness benchmarks compaction
quality in isolation — they all just run their full suite with/without it and
report the delta. This benchmark fills that gap by seeding known facts at
specific conversation turns and testing recall after compaction.

Compaction quality directly affects multi-step task completion. If the model
"forgets" a decision made 30 turns ago, it may redo work, contradict itself,
or spiral. This benchmark quantifies that risk per strategy.

## Prerequisites

- **Compaction built in tau.** tau has the `transform_context` hook in the
  agent loop — this benchmark requires at least one compaction strategy
  implemented behind it.
- `shared/` infrastructure (TauSession, BenchConfig, Reporter)

## Fixtures

Synthetic multi-turn conversations with **planted facts** at known positions.

### Conversation template

A coding task that naturally spans 50+ turns:

```
Turn  1-5:   Setup — explore project structure, read files
Turn  6-10:  Define helper functions (FACT: function names, signatures)
Turn 11-15:  Encounter errors (FACT: error messages, root causes)
Turn 16-20:  Make design decisions (FACT: "chose X because Y")
Turn 21-30:  Implementation — edits, tests, iteration
Turn 31:     ** COMPACTION TRIGGER ** (context exceeds threshold)
Turn 32-40:  Continue implementation
Turn 41-50:  RECALL QUESTIONS — direct queries about planted facts
```

### Planted fact categories

| Category | Example | Difficulty |
|----------|---------|-----------|
| Function names | "What were the 3 helpers we defined in turns 6-8?" | Easy |
| Error messages | "What was the compilation error in turn 12?" | Medium |
| Design decisions | "Why did we choose HashMap over BTreeMap in turn 17?" | Medium |
| File paths | "Which files did we modify in turns 21-25?" | Easy |
| Constraints | "What constraint did the user specify in turn 15?" | Hard |
| Rejected alternatives | "What approach did we reject and why?" | Hard |

### Fixture format

```json
{
  "id": "recall-001",
  "conversation": [
    { "turn": 1, "role": "user", "content": "..." },
    { "turn": 1, "role": "assistant", "content": "...", "tool_calls": [...] },
    ...
  ],
  "planted_facts": [
    {
      "turn": 7,
      "category": "function-names",
      "fact": "Defined process_batch(), validate_input(), format_output()",
      "recall_question": "What were the 3 helper functions we defined earlier?",
      "expected_answer_contains": ["process_batch", "validate_input", "format_output"]
    }
  ],
  "compaction_trigger_turn": 31
}
```

Target: 10 conversation scripts x 5-8 planted facts each = 50-80 recall tests.

### Conversation generation

Conversations should be **realistic** — not artificial Q&A but actual coding
task trajectories. Approaches:

1. **Record real sessions**: run tau on a task, record the trace, then inject
   planted facts at specific turns. Most realistic but labor-intensive.

2. **Generate with LLM**: prompt a model to generate a plausible 50-turn
   coding conversation, specifying where facts should appear. Cheaper but
   may not capture realistic tool-use patterns.

3. **Hybrid**: use real session traces as templates, swap in specific planted
   facts at key turns.

Start with approach 2 (LLM-generated), validate with approach 1.

## Variants / run matrix

| Variant | Description | Implementation |
|---------|-------------|----------------|
| `truncation` | Drop oldest turns, keep last N | Keep last 60% of tokens |
| `observation-mask` | Replace old tool outputs with `[omitted]` | Keep tool names visible |
| `llm-summary` | Structured LLM summary (goal/progress/decisions/next/files) | LLM call at compaction |
| `progressive` | OpenDev-style: mask at 80%, prune at 85%, summarize at 95% | Multi-stage |

10 conversations x 4 variants x 1 run = 40 compaction runs.
Plus 50-80 recall questions evaluated per run.

## Procedure

```bash
# 1. Generate conversation scripts
uv run python generate.py --conversations 10 --facts-per 6 -o fixtures/

# 2. Run all variants
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants truncation,observation-mask,llm-summary,progressive \
    -o results/

# 3. Analyze recall accuracy
uv run python score.py results/report.json
```

## Metrics

### Primary

- **Recall accuracy**: % of planted facts correctly recalled after compaction.
  Measured by checking if `expected_answer_contains` terms appear in the
  model's response.
  Target: >85% for the best strategy.

- **Hallucination rate**: facts in model's recall response that were NOT
  planted (false memories from the compacted summary).
  Target: 0%.

### Secondary

- **Category breakdown**: which fact types survive compaction best?
  Hypothesis: function names (concrete) survive better than design rationale
  (abstract).

- **Token efficiency**: original_tokens / compacted_tokens (compression ratio)

- **Compaction latency**: wall-clock time for the compaction step itself.
  LLM summary is slow; truncation is instant.

- **Task continuation**: after compaction + recall questions, can the model
  continue the original task? (Binary: did it pick up where it left off?)

### Scoring matrix

```
Variant           Recall%  Halluc%  Compression  Latency(s)  Continue?
truncation           55%      0%        0.40         0.0        yes
observation-mask     72%      0%        0.55         0.0        yes
llm-summary          88%      2%        0.35         8.5        yes
progressive          82%      1%        0.45         4.2        yes
```

## Decision it informs

1. **Which compaction strategy to implement first.** If observation masking
   gets >80% recall with 0% hallucination, it's the clear winner (zero cost,
   zero latency).

2. **Compaction trigger threshold.** Run variants at different trigger points
   (70%, 80%, 90% of context) to find where recall starts degrading.

3. **JetBrains finding replication.** Their research found masking beats
   summarization on coding tasks. Do we see the same?

4. **LLM summary prompt tuning.** If LLM summary has high recall but >0%
   hallucination, the summary prompt needs refinement.

## Architecture

```
compaction-recall/
├── SPEC.md
├── generate.py       # Conversation + planted fact generator
├── run.py            # Runner: replay conversation, trigger compaction, ask recall
├── score.py          # Recall accuracy scorer (string matching)
├── fixtures/         # Generated conversation scripts
└── results/          # Output (gitignored)
```

Uses shared: `TauSession`, `BenchConfig`, `TaskResult`, `Reporter`.

Does NOT use `Verifier` — this benchmark scores recall answers, not file output.

Estimated LOC: ~500 (generate.py: ~200, run.py: ~200, score.py: ~100)

### Special runner logic

The runner has a unique flow compared to other benchmarks:

1. Replay the pre-compaction conversation turns (turns 1-30) by sending
   them as user messages through TauSession
2. Trigger compaction (via tau's compaction API or by reaching token threshold)
3. Continue with turns 32-40
4. Send recall questions (turns 41-50)
5. Score recall responses against planted facts

This replay-then-test pattern is specific to this benchmark.
