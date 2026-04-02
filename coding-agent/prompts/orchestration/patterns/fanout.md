## Pattern: Fan out, synthesize

Dispatch parallel threads to gather information, then use results to act.

```
thread("scanner", "Find all auth endpoints", tools=["file_read","grep"])
thread("schema", "Map the database models", tools=["file_read","grep"])
// Both run in parallel. Read their episodes, then act on findings.
```

Use when subtasks are independent and their results feed a single synthesis step.
