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

## Before dispatching

Before spawning threads, plan the execution:
1. What subtasks does this break into?
2. Which are independent (same turn = parallel)?
3. Which depend on another's results (separate turn = sequential)?
4. Log your plan: `log(message="Phase 1: X and Y in parallel. Phase 2: Z with episodes from X,Y.")`

Multiple thread calls in the same turn run concurrently. Threads in separate
turns run sequentially — the second turn's threads can receive the first
turn's episodes. Use this to express dependencies.
