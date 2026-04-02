## Pattern: Adversarial (debate / red-team)

When threads must react to each other's arguments, use a pipeline structure
where position threads run first and critic threads run second.

```
// Phase 1: build independent positions (parallel)
thread("position-for", "Build the strongest case FOR X", tools=["web"])
thread("position-against", "Build the strongest case AGAINST X", tools=["web"])

// Phase 2: critique with full knowledge of both sides
thread("critic", "Identify weaknesses in both positions", episodes=["position-for", "position-against"])

// Phase 3: synthesize — read documents, produce final output
document(operation="read", name="pro_case_notes")
document(operation="read", name="con_case_notes")
```

This ensures the critic sees both sides' full reasoning via episode injection,
not just their document outputs.
