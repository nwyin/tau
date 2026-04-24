## Pattern: Adversarial (debate / red-team)

When threads must react to each other's arguments, use a pipeline structure
where position threads run first, critic threads run second, and final synthesis happens last.

```
// Phase 1: build independent positions (parallel)
thread("position-for", "Build the strongest case FOR X", tools=["web"])
thread("position-against", "Build the strongest case AGAINST X", tools=["web"])

// Phase 2: critique with full knowledge of both sides
thread("critic", "Identify weaknesses in both positions", episodes=["position-for", "position-against"])

// Phase 3: synthesize after the critic completes
document(operation="read", name="pro_case_notes")
document(operation="read", name="con_case_notes")
thread("synthesis", "Produce the final synthesis using both positions and the critic",
       episodes=["position-for", "position-against", "critic"])
```

This ensures the critic sees both sides' full reasoning via episode injection,
not just their document outputs.

Never launch the critic in the same batch as the position threads when the critic must react to those positions. "Critique both sides" is a dependency signal, not an invitation for naive parallel fan-out.
