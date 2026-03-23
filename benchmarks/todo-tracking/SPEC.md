# Todo/Plan Tracking: Multi-Step Completion

Phase: 3 | Type: online | Cost: $5-15 | Time: ~1-2 hours

## What it measures

Does explicit plan/todo tracking improve multi-step task completion rate?
And if so, which mechanism (optional tool, mandatory injection, plan mode)
provides the most benefit per token of overhead?

## Why it matters for tau

5 of 7 major harnesses have todo/plan tracking, but nobody benchmarks it.
The implementations span a wide spectrum:

| Harness | Mechanism | Model-visible? | Survives compaction? |
|---------|-----------|----------------|---------------------|
| codex | `update_plan` (client-only render) | No | No |
| opencode | TodoWrite/Read (DB-backed, UI) | No | Yes |
| oh-my-pi | `todo_write` (mandatory every turn) | Yes (forced) | Via history |
| kimi-cli | SetTodoList + plan mode (restricts tools) | Yes (injected) | Yes (file) |

Key insight from research: codex's plan tool is a no-op for the model (it's
purely for the human operator's benefit). oh-my-pi's is the opposite extreme —
mandatory every turn, system prompt enforced. The effectiveness likely comes
from: (a) system prompt mandate, (b) periodic re-injection, and (c) survival
over compaction.

## Prerequisites

- **Compaction built** — todo tracking only matters if state survives
  compaction. Without compaction, the model can just scroll back.
- `compaction-recall` completed — establishes compaction quality baseline
- `shared/` infrastructure

## Fixtures

### Task: 5-step refactoring

A multi-step task with clear sequential dependencies:

1. **Read**: understand existing code structure
2. **Extract**: extract a function into a new module
3. **Update imports**: modify callers to use new module
4. **Add tests**: write tests for the extracted function
5. **Verify**: run tests, fix any failures

Each step has a verifiable outcome. The task is designed so that:
- Skipping step 2 makes step 3 impossible
- Forgetting what was extracted in step 2 causes naming errors in step 3
- The 5-step structure tests whether the model maintains plan awareness

### Error injection variant

Same task, but inject a test failure at step 4 (a deliberate bug in the
extracted function). This tests **recovery**: does the model spiral or
diagnose and fix?

### Fixture layout

```
fixtures/
├── extract-utils/
│   ├── input/
│   │   ├── src/handlers.py     # monolithic, functions to extract
│   │   └── tests/test_handlers.py
│   ├── expected/
│   │   ├── src/handlers.py     # cleaned up
│   │   ├── src/utils.py        # extracted functions
│   │   └── tests/test_utils.py # new tests
│   └── prompt.md
├── extract-utils-error/
│   ├── ...                     # same but expected/ has the bug
│   └── prompt.md               # same task, bug will surface at step 4
```

3 tasks (extract-utils, rename-module, split-config) x 2 variants (normal,
error-injection) = **6 task fixtures**.

## Variants / run matrix

| Variant | Description | Implementation |
|---------|-------------|----------------|
| `baseline` | No todo tracking, standard system prompt | Standard tau |
| `optional-tool` | TodoWrite/Read available but not mandated | Add tools, no prompt change |
| `mandatory-prompt` | System prompt: "Call todo_write before each step" | System prompt + tools |
| `plan-mode` | Read-only exploration first, explicit approval, then execute | Restricted tool sets |
| `periodic-inject` | Re-inject current plan state every 10 turns | System prompt hook |

6 tasks x 5 variants x 3 runs = **90 runs**

### Variant implementation details

**optional-tool**: add `todo_write` and `todo_read` tools. `todo_write` takes
`[{step: str, status: "pending"|"in-progress"|"done"}]` and stores in session
state. `todo_read` returns current plan.

**mandatory-prompt**: add to system prompt:
```
Before starting any implementation step, call todo_write to outline your plan.
Update the plan status as you complete each step. This helps maintain context
across the conversation.
```

**plan-mode**: two-phase execution:
1. Phase 1: model can only use read-only tools (file_read, grep, glob).
   Must produce a plan.
2. User approves plan.
3. Phase 2: all tools available, plan re-injected as context.

**periodic-inject**: every 10 turns, inject a system message:
```
[Reminder] Current plan state: {plan_json}
```

## Procedure

```bash
# 1. Fixtures (hand-crafted, committed)

# 2. Run all variants
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants baseline,optional-tool,mandatory-prompt,plan-mode,periodic-inject \
    --runs 3 \
    -o results/

# 3. Score step completion
uv run python score.py results/report.json
```

## Metrics

### Primary

- **Task completion rate**: did all 5 steps succeed? Binary per task.
- **Step completion rate**: out of 5 steps, how many completed? Partial credit.
- **Step ordering correctness**: did the model follow a logical sequence?
  (1->2->3->4->5, not 1->3->2->4->5)

### Secondary

- **Recovery (error variant)**: when step 4 fails, how many extra turns to
  fix? Did the model spiral (>10 recovery turns) or diagnose quickly (<3)?
- **Turns to completion**: total turns for successful runs. Lower is better.
- **Token cost**: mandatory todo adds ~200 tokens/turn. Is the completion
  rate improvement worth it?
- **Plan adherence**: for variants with plans, did the model follow its own
  plan? (Measured by comparing planned steps to actual execution order.)
- **Post-compaction recovery**: does the model maintain plan awareness after
  context compaction? (Only measurable for tasks long enough to trigger
  compaction.)

### Expected results sketch

```
Variant           Complete%  Steps(avg)  Recovery(turns)  Tokens
baseline              60%       3.8           8.5          65K
optional-tool         65%       4.0           7.0          68K
mandatory-prompt      80%       4.5           4.2          78K
plan-mode             85%       4.7           3.5          72K
periodic-inject       82%       4.6           3.8          82K
```

Hypothesis: `mandatory-prompt` and `plan-mode` will significantly beat
`baseline`, while `optional-tool` will barely differ from `baseline`
(the model won't use the tool unless forced).

## Decision it informs

1. **Is mandatory injection worth the token cost?** If `mandatory-prompt`
   adds 20% tokens but improves completion by 20%, it's ROI-positive.

2. **Model-visible vs UI-only?** If `optional-tool` (model-visible but not
   forced) doesn't beat `baseline`, the benefit comes from the mandate,
   not the visibility.

3. **Plan mode vs mandatory prompt?** If `plan-mode` matches `mandatory-prompt`
   with fewer tokens (the read-only phase is cheap), it's the better approach.

4. **Periodic re-injection value?** If `periodic-inject` beats `mandatory-prompt`,
   the key factor is re-injection, not initial planning.

5. **Should this be a harness feature or system prompt technique?** If
   `mandatory-prompt` (pure system prompt, no code changes) matches
   `plan-mode` (requires tool restrictions + phase management), skip the
   feature work.

## Architecture

```
todo-tracking/
├── SPEC.md
├── run.py            # Runner with variant-specific configs
├── variants.py       # Variant definitions (tools, prompts, phases)
├── score.py          # Step completion scorer
├── fixtures/         # 6 task fixtures (committed)
│   └── {task}/
│       ├── input/
│       ├── expected/
│       └── prompt.md
└── results/          # Output (gitignored)
```

Uses shared: `TauSession`, `BenchConfig`, `TaskResult`, `Reporter`, `Verifier`.

Estimated LOC: ~500 (run.py: ~200, variants.py: ~100, score.py: ~100,
fixtures: ~100 lines of prompts)

### Special runner logic

**Plan-mode variant** requires two-phase execution:
1. Start session with restricted tool set (file_read, grep, glob only)
2. Send task prompt, wait for plan output
3. Parse plan from model response
4. Restart session (or inject plan as context) with full tool set
5. Continue with implementation

**Periodic-inject variant** requires turn counting:
- After every 10th model response, inject a system message with current
  plan state before the next user message.

**Step scoring** parses the final workspace state:
- Step 1 (read): always "done" if model produced any output
- Step 2 (extract): check if `utils.py` exists with expected functions
- Step 3 (imports): check if callers import from `utils`
- Step 4 (tests): check if `test_utils.py` exists
- Step 5 (verify): run `pytest`, check exit code

### Simplest tau implementation (for the feature variants)

```
1. TodoWrite tool: takes [{step, status}], writes to .tau-plan.json
2. TodoRead tool: reads .tau-plan.json, returns current plan
3. System prompt hook: "Before implementing, outline steps with todo_write"
4. After compaction: re-inject "Current plan: {contents of .tau-plan.json}"
```

~200 LOC in tau. The benchmark tests whether this is worth building.
