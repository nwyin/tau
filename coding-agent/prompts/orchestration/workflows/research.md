## Workflow: Deep research with synthesis

Use py_repl for research tasks that need multiple angles of investigation and a synthesized conclusion.

```python
question = "How is AI impacting the labor market in Southeast Asia?"
angles = [
    ("manufacturing", "Impact on manufacturing employment and automation"),
    ("services", "Impact on service sector and knowledge work"),
    ("policy", "Government policy responses and retraining programs"),
    ("startups", "AI startup ecosystem and new job creation"),
]

# Phase 1: parallel research threads (fan-out only because these angles are independent)
specs = [
    tau.Thread(alias, f"Research: {desc}. Write findings to document '{alias}_notes'.",
               tools=["web"])
    for alias, desc in angles
]
tau.parallel(*specs)

# Phase 2: synthesis (receives all research episodes)
aliases = [a for a, _ in angles]
synthesis = tau.thread("synthesis",
    f"Synthesize all research into an integrated analysis of: {question}",
    episodes=aliases, tools=["read"])

tau.document("write", name="final_report", content=str(synthesis))
```

Scale the number of angles to match the complexity. For broad topics, 4-8 parallel threads work well. For narrow questions, 2-3 is sufficient.

If the synthesis or critique must wait for intermediate artifacts before launch, switch to `tau.launch()` + `tau.wait()` and gate on document readiness instead of batching everything with `tau.parallel()`.
