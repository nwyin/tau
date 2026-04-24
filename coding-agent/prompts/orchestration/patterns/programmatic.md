## Pattern: Programmatic orchestration with py_repl

For complex orchestration — loops, conditionals, aggregation, retry logic, phased pipelines, or reactive coordination — use the `py_repl` tool. It provides a persistent Python namespace with a `tau` object for orchestration.

### tau API

The complete `tau` Python API reference is generated from
`prompts/py_tau_api.json` and included below this pattern. Treat that generated
reference as authoritative for method signatures and return shapes.

### When to use py_repl vs direct tool calls

Use py_repl when:
- You need control flow (loops, conditionals) between orchestration steps
- You want to fan out many threads and aggregate results programmatically
- You need reactive coordination: launch producers now, then wait or poll before launching a dependent reviewer/critic
- Data processing or transformation is needed between steps
- The orchestration has more than 2-3 steps with dependencies

Use direct thread/query tool calls when:
- You have 1-3 independent parallel tasks with no conditional logic
- Simple fan-out with straightforward result consumption

### Checkpoint pattern (for adaptive loops)

After each phase in a long-running loop, evaluate actual state and decide next action:

```python
# Check real state, not plan assumptions
state = tau.tool("bash", command="cargo build 2>&1 | tail -3; python3 test_runner.py --summary 2>&1")

# Ask the reasoning model to decide
decision = json.loads(tau.query(f"""
Phase '{item_id}' failed: {reason}
Project state: {state}
Choose: RETRY (more time), SPLIT (smaller items), SKIP, or ABSORB (merge into downstream).
Respond with JSON: {{"action": "...", "reason": "...", ...}}
""", model="reasoning"))

# Apply the decision
if decision["action"] == "SPLIT":
    # Insert sub-items into workqueue with dependencies
    ...
elif decision["action"] == "RETRY":
    item["timeout"] += 120
    item["status"] = "pending"
```

See the supervised loop workflow for the full implementation.
