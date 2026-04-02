## Pattern: Programmatic orchestration with py_repl

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
