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

Threads get a restricted tool set (default: `file_read`, `grep`, `glob`). Override with the `tools` parameter:
- Read-only exploration: `["file_read", "grep", "glob"]`
- Implementation: `["file_read", "file_edit", "file_write", "bash"]`
- Full access: `["bash", "file_read", "file_edit", "file_write", "glob", "grep", "web_fetch", "web_search"]`

## Thread completion

Each thread must call `complete`, `abort`, or `escalate` when done. The thread's result becomes its episode — a compressed trace of what it did and concluded.
