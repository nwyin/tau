# Trace Analysis Guide

Tau writes JSONL traces to `~/.tau/traces/<session_id>/trace.jsonl`.
Each line is a JSON object with a `ts` (RFC3339 timestamp) and `event`
field. This guide covers how to query traces with `jq` and `rg` to
understand agent routing behavior.

## Event Types

| Event | Source | Fields |
|-------|--------|--------|
| `agent_start` | Agent loop | — |
| `agent_end` | Agent loop | `status` |
| `turn_start` | Agent loop | — |
| `turn_end` | Agent loop | `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens` |
| `tool_start` | Tool execution | `tool_call_id`, `tool_name`, `args`, `thread_id?`, `thread_alias?` |
| `tool_end` | Tool execution | `tool_call_id`, `tool_name`, `duration_ms`, `is_error`, `result_content`, `thread_id?`, `thread_alias?` |
| `thinking` | MessageUpdate | `content`, `thread_id?`, `thread_alias?` |
| `thread_start` | ThreadTool | `thread_id`, `alias`, `task`, `model` |
| `thread_end` | ThreadTool | `thread_id`, `alias`, `outcome`, `duration_ms` |
| `episode_inject` | ThreadTool | `source_aliases`, `target_alias`, `target_thread_id` |
| `evidence_cite` | ThreadTool | `thread_alias`, `thread_id`, `tool_call_ids` |
| `document_op` | DocumentTool | `thread_alias?`, `op`, `name`, `content` |
| `query_start` | QueryTool | `query_id`, `prompt`, `model` |
| `query_end` | QueryTool | `query_id`, `output`, `duration_ms` |
| `context_compact` | transform_context | `thread_alias?`, `before_tokens`, `after_tokens`, `strategy` |

Fields marked `?` are present only when the event occurs inside a thread.

---

## Basic Queries

### Event distribution

What happened during this session?

```bash
jq -r '.event' trace.jsonl | sort | uniq -c | sort -rn
```

### Session duration

```bash
jq -r 'select(.event == "agent_start" or .event == "agent_end") | "\(.event): \(.ts)"' trace.jsonl
```

### Token usage per turn

```bash
jq -r 'select(.event == "turn_end") | "\(.input_tokens) in / \(.output_tokens) out"' trace.jsonl
```

### Total token usage

```bash
jq -s '[.[] | select(.event == "turn_end")] | {
  total_in: (map(.input_tokens) | add),
  total_out: (map(.output_tokens) | add),
  turns: length
}' trace.jsonl
```

---

## Thread Analysis

### What threads were spawned?

```bash
jq -r 'select(.event == "thread_start") | "\(.alias) | model=\(.model) | \(.task[:100])"' trace.jsonl
```

### Thread outcomes and duration

```bash
jq -r 'select(.event == "thread_end") | "\(.alias): \(.outcome) (\(.duration_ms)ms)"' trace.jsonl
```

### Thread timeline (lifecycle events in order)

```bash
jq -r 'select(
  .event == "thread_start" or
  .event == "thread_end" or
  .event == "episode_inject" or
  .event == "document_op"
) | "\(.ts[11:19]) \(.event) \(.alias // .target_alias // .name // "")"' trace.jsonl
```

### Were threads parallel or sequential?

If two `thread_start` events have the same timestamp (or close), they ran
in parallel. Look for overlapping time windows:

```bash
jq -r 'select(.event == "thread_start" or .event == "thread_end") |
  "\(.ts[11:19]) \(.event) \(.alias)"' trace.jsonl
```

### Tool calls per thread

```bash
jq -r 'select(.event == "tool_start" and .thread_alias != null) | .thread_alias' trace.jsonl \
  | sort | uniq -c | sort -rn
```

### Tool breakdown per thread

```bash
ALIAS="db-schema"  # change this
jq -r "select(.event == \"tool_start\" and .thread_alias == \"$ALIAS\") | .tool_name" trace.jsonl \
  | sort | uniq -c | sort -rn
```

### Untagged tool calls (main agent, no thread)

```bash
jq -r 'select(.event == "tool_start" and .thread_alias == null) | .tool_name' trace.jsonl \
  | sort | uniq -c | sort -rn
```

### Tools that ran in a specific thread with timing

```bash
ALIAS="tests"
jq -r "select(.event == \"tool_end\" and .thread_alias == \"$ALIAS\") |
  \"\(.ts[11:19]) \(.tool_name) \(.duration_ms)ms\"" trace.jsonl
```

---

## Routing Analysis

### Episode injection graph (who got context from whom)

```bash
jq -r 'select(.event == "episode_inject") |
  "\(.source_aliases | join(",")) → \(.target_alias)"' trace.jsonl
```

This shows the dependency graph: which prior thread episodes were
injected into which new threads.

### Document operations (inter-thread data sharing)

```bash
jq -r 'select(.event == "document_op") |
  "\(.op) \(.name) (\(.content | length) chars)"' trace.jsonl
```

### Document content (what was shared)

```bash
jq -r 'select(.event == "document_op" and .op == "write") | .content' trace.jsonl
```

### Evidence citations (what tool calls supported conclusions)

```bash
jq -r 'select(.event == "evidence_cite") |
  "\(.thread_alias): \(.tool_call_ids | length) tool calls cited"' trace.jsonl
```

To see which specific tool calls were cited as evidence:

```bash
# Get the cited tool_call_ids
CITED=$(jq -r 'select(.event == "evidence_cite") | .tool_call_ids[]' trace.jsonl)

# Look up what those tool calls did
for id in $CITED; do
  jq -r "select(.event == \"tool_start\" and .tool_call_id == \"$id\") |
    \"  \(.tool_name) \(.args | tostring[:80])\"" trace.jsonl
done
```

---

## Query Tool Analysis

### What queries were made (decision points)?

```bash
jq -r 'select(.event == "query_start") |
  "\(.query_id): \(.prompt[:80])..."' trace.jsonl
```

### Query results

```bash
jq -r 'select(.event == "query_end") |
  "\(.query_id) (\(.duration_ms)ms): \(.output[:100])"' trace.jsonl
```

---

## Context Management

### Context compaction events

```bash
jq -r 'select(.event == "context_compact") |
  "before=\(.before_tokens) after=\(.after_tokens) saved=\(.before_tokens - .after_tokens) strategy=\(.strategy)"' trace.jsonl
```

### Thinking content (model reasoning)

```bash
jq -r 'select(.event == "thinking") | .content[:200]' trace.jsonl
```

### Thinking per thread

```bash
jq -r 'select(.event == "thinking" and .thread_alias != null) |
  "\(.thread_alias): \(.content[:100])..."' trace.jsonl
```

---

## Performance Analysis

### Slowest tool calls

```bash
jq -r 'select(.event == "tool_end") |
  "\(.duration_ms)ms \(.tool_name) \(.thread_alias // "main")"' trace.jsonl \
  | sort -rn | head -10
```

### Tool call duration by type

```bash
jq -s 'group_by(.tool_name) | map(select(.[0].event == "tool_end")) | map({
  tool: .[0].tool_name,
  count: length,
  total_ms: (map(.duration_ms) | add),
  avg_ms: ((map(.duration_ms) | add) / length | round)
}) | sort_by(-.total_ms)' trace.jsonl
```

### Thread wall-clock vs tool-call time

Helps identify LLM thinking time vs tool execution time:

```bash
jq -s '{
  threads: [.[] | select(.event == "thread_end")] | map({
    alias: .alias,
    wall_ms: .duration_ms,
    tool_ms: (
      [.alias as $a | .. | select(.event == "tool_end" and .thread_alias == $a) | .duration_ms] | add // 0
    )
  })
}' trace.jsonl
```

---

## Compound Queries

### Full orchestration summary

```bash
TRACE=trace.jsonl
echo "Threads: $(jq -r 'select(.event == "thread_start")' $TRACE | wc -l | tr -d ' ')"
echo "Episodes injected: $(jq -r 'select(.event == "episode_inject")' $TRACE | wc -l | tr -d ' ')"
echo "Document ops: $(jq -r 'select(.event == "document_op")' $TRACE | wc -l | tr -d ' ')"
echo "Tool calls: $(jq -r 'select(.event == "tool_start")' $TRACE | wc -l | tr -d ' ')"
echo "Queries: $(jq -r 'select(.event == "query_start")' $TRACE | wc -l | tr -d ' ')"
echo "Evidence citations: $(jq -r 'select(.event == "evidence_cite")' $TRACE | wc -l | tr -d ' ')"
echo "Context compactions: $(jq -r 'select(.event == "context_compact")' $TRACE | wc -l | tr -d ' ')"
```

### Routing pattern visualization (ASCII dependency graph)

```bash
echo "=== Routing Graph ==="
echo ""
jq -r 'select(.event == "episode_inject") |
  .source_aliases[] as $src | "  \($src) --> \(.target_alias)"' trace.jsonl | sort -u

echo ""
echo "=== Document Flow ==="
jq -r 'select(.event == "document_op") |
  "  \(.thread_alias // "orchestrator") \(if .op == "write" or .op == "append" then "==>" else "<--" end) [\(.name)]"' trace.jsonl
```

### Detect parallel vs sequential thread execution

```bash
jq -s '
  [.[] | select(.event == "thread_start")] as $starts |
  [.[] | select(.event == "thread_end")] as $ends |
  [$starts[] | {alias, start: .ts}] as $s |
  [$ends[] | {alias, end: .ts}] as $e |
  [range($s | length)] | map(
    {alias: $s[.].alias, start: $s[.].start, end: ($e[] | select(.alias == $s[.].alias) | .end)}
  ) | sort_by(.start) |
  . as $threads |
  if length < 2 then "all sequential"
  else
    [range(length - 1) | 
      if $threads[. + 1].start < $threads[.].end 
      then "\($threads[.].alias) || \($threads[. + 1].alias)"
      else "\($threads[.].alias) >> \($threads[. + 1].alias)"
      end
    ] | join("\n")
  end
' -r trace.jsonl
```

Output uses `||` for parallel and `>>` for sequential:
```
db-schema >> api-endpoints
api-endpoints || html-frontend
html-frontend >> tests
```

---

## Tips

- **Pipe to `less`** for large traces: `jq ... trace.jsonl | less`
- **Use `rg`** for quick keyword searches: `rg "episode_inject" trace.jsonl`
- **Filter by time window**: `jq 'select(.ts > "2026-03-31T15:28:00" and .ts < "2026-03-31T15:30:00")'`
- **Count events fast**: `rg -c '"event":"thread_start"' trace.jsonl`
- **Pretty-print one event type**: `jq 'select(.event == "document_op")' trace.jsonl | jq .`
- **Export to CSV** for spreadsheet analysis:
  ```bash
  jq -r 'select(.event == "tool_end") | [.ts, .tool_name, .thread_alias, .duration_ms] | @csv' trace.jsonl
  ```
