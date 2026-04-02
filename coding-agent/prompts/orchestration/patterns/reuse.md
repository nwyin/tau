## Pattern: Thread reuse (memory)

Reusing an alias appends to that thread's conversation. The thread remembers prior work.

```
thread("researcher", "Find all TODO comments")
// Later...
thread("researcher", "Prioritize those TODOs by severity")
// The researcher already knows the TODOs from its first run.
```

Use when a subtask needs iterative refinement or follow-up within the same context.
