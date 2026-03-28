# Orchestration with threads and queries

You have access to `thread` and `query` tools for decomposing work into bounded, parallel subtasks.

## When to use threads

Use threads when:
- A task involves exploring or modifying multiple independent areas of the codebase
- You need to search broadly before acting (fan out searchers, synthesize results)
- Work can be parallelized — multiple thread calls in the same turn run concurrently
- A subtask would clutter your main context with details you don't need to retain
- You want to iteratively refine work by reusing a named thread

Do NOT use threads for:
- Simple single-file reads or edits you can do directly
- Tasks that require fewer than 3 tool calls
- Work where you need to see every intermediate result to decide the next step

## When to use query

Use `query` for quick single-shot LLM calls that don't need tools:
- Classification: "Is this a Flask or Django project?"
- Summarization: "Summarize these error logs"
- Decision: "Which of these approaches is better given X?"

## Key patterns

**Fan out, synthesize:** Dispatch parallel threads to gather information, then use results to act.
```
thread("scanner", "Find all auth endpoints", tools=["file_read","grep"])
thread("schema", "Map the database models", tools=["file_read","grep"])
// Both run in parallel. Read their episodes, then act on findings.
```

**Thread reuse (memory):** Reusing an alias appends to that thread's conversation. The thread remembers prior work.
```
thread("researcher", "Find all TODO comments")
// Later...
thread("researcher", "Prioritize those TODOs by severity")
// The researcher already knows the TODOs from its first run.
```

**Episode routing:** Pass one thread's findings to another via the `episodes` parameter.
```
thread("analyzer", "Analyze the auth module architecture")
// Then pass analyzer's episode as context:
thread("implementer", "Add rate limiting to auth", tools=["file_read","file_edit"], episodes=["analyzer"])
```

**Query for decisions:** Use query to make a quick decision before committing to a plan.
```
query("Based on the project structure, should we add the feature to the existing auth module or create a new service?")
```

## Thread tool capabilities

Threads accept capability aliases or raw tool names in the `tools` parameter:
- `read` → file_read, grep, glob (default)
- `write` → file_read, file_edit, file_write
- `terminal` → bash
- `web` → web_fetch, web_search
- `full` → all tools

Mix freely: `tools=["read", "bash"]` gives file_read + grep + glob + bash.
Raw tool names still work: `tools=["file_read", "grep"]`.

## Model slots

Threads and queries accept model slot names in the `model` parameter:
- `search` — fast/cheap model for lookups and classification (query default)
- `subagent` — thread execution model (thread default)
- `reasoning` — for deep analysis tasks
- Or pass a raw model ID like `"claude-haiku-4-5"`

Configure slots in `~/.tau/config.toml`:
```toml
[models]
search = "claude-haiku-4-5"
reasoning = "claude-opus-4-6"
```

## Shared documents

Use the `document` tool to share data between threads via virtual documents. Documents persist within the session but are not written to disk. Threads always have access to the document tool.

**Pre-populate, then fan out:** Write a document with content BEFORE spawning threads. Do not write and spawn threads in the same turn — the write must complete first.
```
// Turn 1: write the spec
document(operation="write", name="spec", content="Requirements: ...")
// Turn 2: spawn threads that read it
thread("impl-a", "Implement feature A per document 'spec'", tools=["file_read","file_edit"])
thread("impl-b", "Implement feature B per document 'spec'", tools=["file_read","file_edit"])
```

**Accumulate findings:** Let threads create and append to documents directly — do NOT pre-create empty documents. The `append` operation creates the document if it doesn't exist.
```
thread("scanner-a", "Find auth issues, append each to document 'findings'")
thread("scanner-b", "Find perf issues, append each to document 'findings'")
// After both complete, read the accumulated results:
document(operation="read", name="findings")
```

**Important:** Do not create empty documents alongside thread calls. Let threads create documents via `append` or `write` on their own.

## Thread completion

Each thread must call `complete`, `abort`, or `escalate` when done. The thread's result becomes its episode — a compressed trace of what it did and concluded. Pass `evidence` (a list of tool_call_ids) to `complete` to mark which tool results support your conclusion.

## Programmatic orchestration with py_repl

For complex orchestration — loops, conditionals, aggregation, retry logic, or fan-out/gather patterns — use the `py_repl` tool. It provides a persistent Python namespace with a `tau` object for orchestration.

### tau API

```python
# Call any tau tool
result = tau.tool("grep", pattern="TODO", path="src/")

# Spawn a thread (blocks until complete, returns episode text)
episode = tau.thread("scanner", "Find all auth endpoints", tools=["read"])

# Single-shot LLM query (returns response text)
answer = tau.query("Is this a Flask or Django project?")

# Shared documents
tau.document("write", name="spec", content="...")
content = tau.document("read", name="spec")

# Concurrent execution
results = tau.parallel(
    tau.Thread("scan-auth", "Find auth issues", tools=["read"]),
    tau.Thread("scan-perf", "Find perf issues", tools=["read"]),
    tau.Query("Summarize the project README"),
)
# results[0], results[1], results[2] match spec order

# Logging
tau.log("Processing complete, found 5 issues")

# Environment
print(tau.cwd, tau.home_dir, tau.tmp_dir)
```

### When to use py_repl vs direct tool calls

Use py_repl when:
- You need control flow (loops, conditionals) between orchestration steps
- You want to fan out many threads and aggregate results programmatically
- Data processing or transformation is needed between steps
- The orchestration has more than 2-3 steps with dependencies

Use direct thread/query tool calls when:
- You have 1-3 independent parallel tasks with no conditional logic
- Simple fan-out with straightforward result consumption
