## Workflow: Feature implementation

Use py_repl when asked to build a new feature that touches multiple areas. Adapt `spec` and `scope` to the request.

```python
spec = "Add OAuth2 login flow"
scope = "src/auth/"

# Phase 1: understand (parallel)
tau.parallel(
    tau.Thread("arch", f"Map architecture and data flow around {scope}", tools=["read"]),
    tau.Thread("tests", f"Study existing test patterns for {scope}", tools=["read"]),
)

# Phase 2: plan
plan = tau.query(f"Given the architecture and test patterns, outline an implementation plan for: {spec}")
tau.document("write", name="plan", content=plan)

# Phase 3: implement in isolated worktree
impl = tau.thread("impl", f"Implement per document 'plan': {spec}",
                   episodes=["arch"], tools=["full"], worktree=True)

# Phase 4: review + verify
diff = tau.diff("impl")
result = tau.thread("verify", f"Review this diff and run tests for: {spec}\n\nChanges:\n{diff.stat}",
                     episodes=["arch", "tests", "impl"], tools=["full"])

# Phase 5: merge or fix
if result.completed:
    tau.merge("impl")
else:
    tau.thread("fix", f"Fix failing tests: {result.reason}",
               episodes=["impl", "verify"], tools=["full"], worktree=True)
    tau.merge("fix")
```
