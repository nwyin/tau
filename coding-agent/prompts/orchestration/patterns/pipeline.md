## Pattern: Pipeline (phased execution)

When subtasks have data dependencies, dispatch them in phases. Threads in a
later phase receive prior threads' episodes as context.

```
// Phase 1: independent research (parallel)
thread("research-a", "Investigate component A", tools=["read"])
thread("research-b", "Investigate component B", tools=["read"])

// Phase 2: dependent work (next turn — Phase 1 results are now episodes)
thread("implementer", "Refactor based on findings", tools=["write"], episodes=["research-a", "research-b"])
```

The key insight: **turn boundaries are synchronization points.** All threads
in a turn complete before the next turn begins. Use this to express "B needs
A's results" — put A in turn 1, B in turn 2 with `episodes=["A"]`.
