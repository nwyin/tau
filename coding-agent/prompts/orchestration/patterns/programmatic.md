## Pattern: Programmatic orchestration with py_repl

For complex orchestration — loops, conditionals, aggregation, retry logic, phased pipelines, or reactive coordination — use the `py_repl` tool. It provides a persistent Python namespace with a `tau` object for orchestration.

### tau API

```python
# Call any tau tool
result = tau.tool("grep", pattern="TODO", path="src/")

# Spawn a thread (blocks until complete, returns episode text)
episode = tau.thread("scanner", "Find all auth endpoints", tools=["read"])

# Launch without blocking, then poll or wait later
worker = tau.launch("producer", "Write findings incrementally to 'producer_notes'",
                    tools=["read"], max_turns=60)
status = tau.poll(worker)
batch = tau.wait([worker], timeout=30)

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

# Thread results are structured — use for conditional branching
result = tau.thread("tests", "Run the test suite", tools=["full"])
result.status      # "completed", "aborted", "escalated", "timed_out"
result.output      # result text / abort reason / escalation problem
result.completed   # True if status == "completed"
if not result:     # __bool__ returns completed
    tau.thread("fix", f"Fix: {result.reason}", episodes=["tests"], tools=["full"])

# Logging
tau.log("Processing complete, found 5 issues")

# Worktree isolation and merge (for parallel write threads)
worker = tau.thread("impl", "Build the feature", tools=["full"], worktree=True)
# worker.branch → "tau/impl", worker.diff_stat, worker.files_changed
diff = tau.diff("impl")           # DiffResult: stat, diff, files_changed
merged = tau.merge("impl")        # MergeResult: success, conflicts
branches = tau.branches()         # list active tau/* branches

# Environment
print(tau.cwd, tau.home_dir, tau.tmp_dir)
```

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
