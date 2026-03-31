# Slate Gap Analysis: Episodes, Threads, and Routing

**Date:** 2026-03-31
**Sources:** Decompiled Slate binary (`@randomlabs/slate` v1.0.24), tau source,
[randomlabs.ai/blog/slate](https://randomlabs.ai/blog/slate)

This document maps the architectural delta between Slate's episode/thread
system and tau's current implementation, identifies gaps worth closing, and
frames everything against the blog's thesis about routing behavior and
knowledge overhang. It supersedes the gap table in `design-orchestration.md`
section 9 (which was written pre-implementation; much of it has since been
addressed).

---

## 0. Why This Matters

The Slate blog argues that the real bottleneck in agent systems is not model
capability but **context management** — what they call the "knowledge
overhang":

> The knowledge that a given model has access to theoretically, but can't
> access tactically without a trick like "think step by step" or by planning
> in files.

Episodes are the mechanism for compressing and routing context across
execution boundaries. The architecture's value is not in running threads —
it's in how the orchestrator **decides what context each thread receives**
and how episode boundaries create natural compression points. The blog notes:

> What's remarkable about Slate is that our routing works at all. The models
> seem to understand how to route context throughout the system in ways that
> are useful and appropriate, without being explicitly trained to do so. We
> leave a formal analysis and benchmarking of this routing behavior as
> future work.

This is what tau aims to study: not benchmark scores, but the emergent
routing behavior itself — how models choose to decompose, delegate, and
compose context across threads. To study routing behavior, we need the
infrastructure to observe it.

---

## 1. Architecture Comparison

### What Tau Has (Implemented on `main`)

| Component | Implementation | Status |
|-----------|---------------|--------|
| In-process thread spawning | `ThreadTool` via `tokio::spawn`, timeout, cancellation | Working |
| Thread reuse by alias | `OrchestratorState::get_or_create_thread()` restores messages | Working |
| Completion tools | `complete`, `abort`, `escalate` with oneshot signaling | Working |
| Episode generation | Two-level traces (full + compact) from message history | Working |
| Episode routing | `episodes` param injects prior compact traces into system prompt | Working |
| Parallel execution | Multiple thread tool calls in one turn → concurrent tokio tasks | Working |
| Event forwarding | Inner tool events forwarded via `EventForwarderCell`; `ThreadStart`/`ThreadEnd` lifecycle events | Working |
| OrchestratorState | Thread-safe shared state, 100-episode FIFO log | Working |
| Virtual documents | `DocumentTool` with read/write/append/list on orchestrator state | Working |
| QueryTool | Single-shot LLM calls without tools | Working |
| LogTool | Progress notes appended to `_orchestration_log` document | Working |
| FromIdTool | Retrieve completed episode by alias | Working |
| Model slots | `search`, `subagent`, `reasoning` slots; thread/query accept slot names | Working |
| Capability aliases | `read`, `write`, `terminal`, `web`, `full` → tool lists | Working |
| PyReplTool | Python subprocess kernel with `tau.*` orchestration API | Working |
| Subprocess subagent | `SubagentTool` spawns fresh `tau` process (no shared state) | Working |
| TUI thread display | Active thread count/aliases, tool calls prefixed with `[alias]` | Working |

### What Slate Has (from Decompilation)

| Component | Implementation |
|-----------|---------------|
| JavaScript DSL | LLM generates JS, compiled via `Function()`, async IIFE |
| system.thread() | Full agent session, capability-gated, returns episode |
| system.query() | Single-shot LLM, JSON schema validation + retry |
| system.allocate() | Named virtual documents for inter-thread sharing |
| system.log() | Progress notes in orchestration sequence |
| system.fromId() | Retrieve prior result by alias |
| Thread reuse | String alias → append to existing session conversation |
| Two-level traces | Full trace (with evidence annotations) + compact trace |
| Episode compression | `kD` (full) and `I9A` (compact) — evidence-annotated |
| Completion tools | complete, abort, escalate, submit (4 tools) |
| Evidence system | `evidence: [tool_call_ids]` on completion, annotated in traces |
| Orchestration transcript | Structured markdown of all steps with timing |
| Orchestration summary | `OrchestrationSummary` with accomplishments, blockers, decisions |
| Rolling compression | Dedicated LLM call for mid-conversation summarization |
| 6 model slots | main, search, subagent, reasoning, vision, image_gen |
| Behavior modes | actor, query, sync, async, compression |
| Permission system | Per-tool, per-pattern rules with persistence |
| File-based KV store | Sessions, messages, permissions persisted to disk |
| SSE event streaming | Session lifecycle events for TUI updates |
| AbortController | Propagates cancellation to all child threads |

---

## 2. Gaps

Organized by relevance to studying routing behavior.

### Tier 1: Critical for Routing Observation

These gaps affect our ability to observe and analyze how models route context
through the system.

#### 2.1 Orchestration Transcript

**Slate:** Generates a structured markdown document (`# Orchestration
Transcript`) recording every step — each thread/query with task, duration,
status, and output. Returned as the `orchestrate` tool result. Format:

```markdown
## Step 1 [query]
**Prompt**: What framework is this?
**Duration**: 450ms
**Output**: "Flask 2.3 with SQLAlchemy"

## Step 2 [thread] [status: completed]
**Task**: Find authentication endpoints
**Duration**: 8200ms
**Output**: "Found /login, /logout, /oauth/callback"
```

**Tau:** Individual episode traces exist but there's no aggregate transcript
of an orchestration run. The parent agent sees each thread's `full_trace` as
a separate tool result. No unified view of the orchestration sequence.

**Why it matters:** To study routing behavior, we need a structured record of
what the orchestrator dispatched, in what order, with what context, and what
came back. Individual tool results are scattered across the conversation;
a transcript is the unit of routing analysis.

**Gap:** No orchestration transcript generation. Need a function that takes
the episode log and produces a structured summary of the orchestration
sequence.

#### 2.2 Orchestration Summary

**Slate:** Auto-generates an `OrchestrationSummary` at the end of a
multi-thread operation:

```typescript
{
  status: "completed" | "partial" | "aborted",
  accomplishments: string[],  // successful thread results (truncated)
  blockers: string[],         // failed/interrupted descriptions
  decisions: Array<{ query: string, decision: any }>,
  results: Array<{ task: string, status: string, result: string }>
}
```

**Tau:** No summary generation. The parent agent sees raw episode traces
and must synthesize a summary itself.

**Why it matters:** The summary is the compressed signal the parent agent
uses to decide next steps. Without it, the parent must re-read full traces
to understand what happened — wasting context window on low-value tokens.
The summary is also useful for post-hoc analysis of routing patterns.

**Gap:** Need a summary generator that runs after orchestration sequences
(e.g., after a py_repl cell completes, or after a batch of thread tool
calls finish).

#### 2.3 Evidence System in Traces

**Slate:** Threads mark specific tool call IDs as evidence for their
conclusions via `complete({result, evidence: ["tc_001", "tc_003"]})`. These
IDs are annotated in traces: `>>> grep(pattern="auth") [EVIDENCE: "Found
the auth module"]`. Downstream consumers see which tool calls supported
which conclusions.

**Tau:** `ThreadOutcome::Completed` has an `evidence: Vec<String>` field,
and the `complete` tool accepts an `evidence` parameter. But evidence is
not annotated in the generated traces — the trace just records tool calls
and results without marking which were cited as evidence.

**Why it matters:** Evidence annotations are a window into the model's
reasoning about what information was important. For routing analysis,
evidence tells us which parts of a thread's work were load-bearing vs.
exploratory. This is the closest thing to observing the model's internal
information routing.

**Gap:** Trace generation (`generate_episode` in `agent/src/episode.rs`)
needs to annotate tool calls that appear in the evidence list. Format:
`TOOL [tc_001] >>> grep(...) [EVIDENCE]` in the trace output.

### Tier 2: Structural Differences Worth Addressing

These affect the system's expressivity and the range of routing patterns
the model can employ.

#### 2.4 Structured Output on Queries

**Slate:** `system.query(prompt, {outputSchema: jsonSchema})` validates
the LLM response against a JSON schema and retries on validation failure.
Enables reliable structured data extraction from single-shot calls.

**Tau:** `QueryTool` returns raw text. No schema validation, no retry.

**Why it matters:** Structured queries are the decision points in routing
logic. When the orchestrator asks "what framework is this?" and routes
subsequent threads based on the answer, structured output ensures the
answer is machine-parseable. Without it, the py_repl code must parse
free-text responses, which is fragile.

**Gap:** Add optional `output_schema` parameter to QueryTool. Validate
response against schema; retry (with error feedback) on failure.

#### 2.5 `submit` Tool

**Slate:** Has a fourth completion tool, `submit`, semantically meaning
"here's a draft for review" rather than "this is complete." Status maps
to `"completed"` but the semantic distinction signals to the orchestrator
that the result may need revision.

**Tau:** Only `complete`, `abort`, `escalate`.

**Why it matters for routing:** A thread that submits (vs. completes) is
signaling uncertainty. The orchestrator can choose to verify, revise, or
accept — a routing decision that `complete` doesn't enable.

**Gap:** Minor. Could add as a fourth completion tool or as a flag on
`complete`.

#### 2.6 Behavioral Modes

**Slate:** Five modes — `actor` (full agent), `query` (structured
extraction), `sync` (synchronous subagent), `async` (async subagent),
`compression` (context summarization). Each mode adjusts system prompt
construction and capabilities.

**Tau:** No explicit modes. Threads are always async agents. Queries are
always single-shot. No compression mode.

**Why it matters:** Modes constrain what a thread can do, which affects
how the orchestrator decomposes work. A compression mode specifically
is useful for long-running orchestrations where episode context exceeds
the window.

**Gap:** Not critical for MVP. Compression mode is the most valuable —
could be added as a specialized use of QueryTool with a compression
prompt.

#### 2.7 Rolling Compression

**Slate:** Long conversations within a thread trigger automatic
summarization via a dedicated LLM call. The conversation is compressed
while preserving semantic content. Uses the `compression` behavior mode.

**Tau:** `compact_messages()` in `agent/src/context.rs` uses mechanical
strategies (truncate large outputs, mask old turns, head+tail
preservation). No model-driven summarization.

**Why it matters:** Long orchestrations accumulate context. Without
rolling compression, threads that do extensive work hit context limits.
Mechanical compression loses semantic content; model-driven compression
preserves what matters.

**Gap:** Add an optional model-driven compression step to the context
management pipeline. Could be triggered when token count exceeds a
threshold, using the `search` model slot for cost efficiency.

### Tier 3: Architectural Differences (Deliberate Divergence)

These are differences where tau has made different design choices, not
necessarily gaps to close.

#### 2.8 DSL Compilation vs Tool-Based Orchestration

**Slate:** The `orchestrate` tool has the LLM generate JavaScript code.
This code is compiled via `Function('system', '...')` and executed as an
async IIFE. Gives `Promise.all()` for parallelism, try/catch for error
handling, variables, loops, conditionals.

**Tau:** Orchestration happens through the `py_repl` tool (Python kernel
with `tau.*` API) or through direct tool calls (LLM calls `thread` tool
multiple times).

**Assessment:** This is a deliberate design choice, not a gap. tau's
py_repl approach provides equivalent expressivity via Python instead of
JavaScript. The `tau.parallel()` primitive handles the main concurrency
pattern. Direct tool calls handle simple cases without the overhead of a
REPL cell. The blog's critique of rigid pipelines applies equally to
over-engineered DSLs — tau's approach lets the model choose the right
level of orchestration complexity.

#### 2.9 Subprocess Subagent (tau-only)

**Slate:** Only in-process threads managed by the orchestrator.

**Tau:** Both in-process threads (`ThreadTool`) AND subprocess subagents
(`SubagentTool`). Subprocess subagents spawn a fresh `tau` process with
no shared state — useful for fully isolated work that shouldn't pollute
the parent's context.

**Assessment:** This is a tau feature Slate doesn't have. The subprocess
path is useful for tasks that are truly independent and don't need
context sharing. It's also a safety valve — subprocess subagents can't
corrupt parent state.

#### 2.10 Persistence Model

**Slate:** File-based KV store with XDG paths. Sessions, messages,
permissions all persisted. SQLite for terminal history.

**Tau:** JSONL session files. Simpler but less structured.

**Assessment:** Slate's persistence is more sophisticated but also more
complex. tau's JSONL approach is adequate for the current use case.
Persistence structure matters more when sessions span days or need to
be queried — not a priority for routing research.

#### 2.11 Permission System

**Slate:** Per-tool, per-pattern rules with globs, persistence, and
configurable defaults. Separate allowlists for subagents.

**Tau:** `PermissionService` with interactive/yolo modes. No per-pattern
rules for subagent threads.

**Assessment:** Slate's permission system is production-grade. tau's is
research-grade. For studying routing behavior, the simpler permission
model is fine — complexity here adds friction without improving
observability.

---

## 3. Notable Implementation Differences

Beyond gaps, these are interesting architectural choices that differ between
the two systems.

### 3.1 Event Propagation Model

**Slate:** SSE (Server-Sent Events) over HTTP. Events are session-scoped.
The TUI subscribes to specific session IDs. Child sessions push events
independently.

**Tau:** In-process `tokio::broadcast` channel with closure-based
subscribers. Thread events forwarded to parent via `EventForwarderCell`.
`ThreadStart`/`ThreadEnd` lifecycle events emitted by the thread tool
itself, not the inner agent.

**Implication:** Tau's model is simpler and lower-latency (no HTTP
overhead). Slate's model is more decoupled (TUI can subscribe to any
session independently). For routing observation, tau's approach is
better — all events flow through a single subscriber, making it easy
to capture a complete trace of orchestration activity.

### 3.2 Thread Identity

**Slate:** Threads get a `sessionId` (UUID) plus an optional `alias`
(string). Session ID is the canonical identifier; alias enables reuse.
Thread roles are cosmetic: `◈ main`, `◎ explore`, `◆ execute`.

**Tau:** Threads get a `thread_id` (`"t-{:04x}"` hex counter) plus a
required `alias`. The alias is the primary identifier for reuse and
episode lookup. No role distinction.

**Implication:** Tau's mandatory alias is cleaner for episode routing —
every thread is addressable by name. Slate's optional alias means some
threads are anonymous and can't be referenced later.

### 3.3 Error Semantics

**Slate:** `abort`/`escalate` outcomes **throw JavaScript exceptions** in
the DSL. The orchestrator must use try/catch for graceful handling. If
uncaught, the entire orchestration fails. `interrupted` (user Ctrl+C) is
returned as a field, not thrown.

**Tau:** All outcomes are returned as `ThreadOutcome` variants in the tool
result. No exceptions. The py_repl can check outcome status in Python code
and branch accordingly. `TimedOut` is an additional outcome variant Slate
doesn't have.

**Implication:** Tau's approach is more explicit — the LLM/code always sees
the outcome and decides how to handle it. Slate's exception-based model is
more idiomatic JS but can cause silent failures if the LLM forgets
try/catch.

### 3.4 Context Window as RAM

The blog draws an explicit analogy to operating systems:

> Instead of letting RAM fill until the process crashes, each thread return
> is a natural opportunity to decide what gets retained, what gets
> compressed, and what gets discarded.

Both architectures implement this through episodes — the thread's full
context window is compressed into an episode at the completion boundary.
But Slate has rolling compression *within* threads (for long-running work),
while tau only compresses at thread boundaries.

**Implication:** Tau handles short-to-medium threads well but may struggle
with threads that do extensive multi-turn work before completing. The gap
in rolling compression (2.7) matters here.

### 3.5 Cross-Model Composition

The blog highlights an unexpected finding:

> Using Sonnet and Codex together across the same task works, with the
> episode boundary acting as a clean handoff.

Both architectures support this via model slots. Tau's implementation
(`ModelSlots` with `search`, `subagent`, `reasoning` slots) enables
heterogeneous model routing. The episode boundary is the key — it
normalizes the interface between models, regardless of their native
format or capabilities.

### 3.6 Parallel Thread Dispatch

**Slate:** Explicit via `Promise.all([system.thread(...), system.thread(...)])`.
The DSL makes parallelism a first-class syntactic construct.

**Tau:** Implicit — if the LLM emits multiple `thread` tool calls in one
assistant message, they execute concurrently via tokio. Or explicit via
`tau.parallel()` in py_repl. Also naturally concurrent when the LLM calls
multiple tools in a single turn.

**Implication:** Both achieve the same result. Slate's approach is more
visible to the LLM (it writes the parallelism). Tau's implicit approach
relies on the model's understanding that multiple tool calls in one turn
run concurrently — which models generally handle well.

---

## 4. Recommendations for Routing Research

To study routing behavior effectively, the following priorities emerge:

### Priority 1: Observability Infrastructure

1. **Orchestration transcript generation** — aggregate episode log into
   structured markdown after orchestration sequences. This is the primary
   data source for routing analysis.

2. **Evidence annotation in traces** — wire the existing `evidence` field
   through trace generation. Minimal code change, high observability value.

3. **Orchestration summary** — structured JSON summary of what happened.
   Useful both for the parent agent's decision-making and for post-hoc
   analysis.

### Priority 2: Routing Expressivity

4. **Structured output on queries** — schema validation + retry on
   QueryTool. Enables reliable structured decision points that affect
   routing.

5. **Rolling compression** — model-driven summarization for long threads.
   Without this, threads hit context limits and the routing patterns we
   can study are bounded by context window size.

### Priority 3: Analysis Tooling

6. **Trace export** — structured JSON export of orchestration runs
   (episodes, traces, timing, evidence) for offline analysis. Currently
   traces are ephemeral — they exist in the tool result and episode log
   but aren't exported for research use.

7. **Routing metrics** — instrument context routing decisions: how many
   episodes were passed to each thread, which episodes were reused vs.
   discarded, context utilization per thread, etc.

---

## 5. What's *Not* a Gap

Things Slate has that tau deliberately doesn't need:

- **JavaScript DSL compilation** — py_repl serves the same purpose with
  Python. Design choice, not a gap.
- **File-based KV persistence** — JSONL sessions are adequate for research.
- **Production permission system** — yolo mode is fine for controlled
  experiments.
- **6 model slots** — 3-4 slots cover the research use cases. Vision and
  image_gen are orthogonal to routing research.
- **SSE event transport** — in-process broadcast is simpler and faster.
- **`view_tool_call`** — inspecting inner tool calls is covered by full
  trace access.

---

## 6. Connection to the Blog's Thesis

The blog argues against benchmark optimization and for studying agent
behavior:

> Our goal at Random Labs is to build generalized, non-benchmaxxed,
> end-to-end agents for software engineering.

And on expressivity:

> An agent harness has high expressivity when it enables many possible
> end states with relatively few output operations.

tau's architecture already supports high expressivity — py_repl gives
arbitrary Python control flow over orchestration primitives, and direct
tool calls give simplicity for straightforward tasks. The model chooses
the right level.

The blog identifies the open question:

> We leave a formal analysis and benchmarking of this routing behavior
> as future work.

This is tau's opportunity. The infrastructure for thread spawning, episode
generation, and context routing exists. What's missing is the
**observability layer** — transcripts, summaries, evidence annotations,
and trace export — that would let us study how models route context and
whether routing patterns correlate with task success.

The thesis is not "build a better agent harness." It's "build
infrastructure to observe and understand agent routing behavior, so we
can learn what makes orchestration work." Closing the gaps in this
document — particularly the Tier 1 observability gaps — is the path
to that research.
