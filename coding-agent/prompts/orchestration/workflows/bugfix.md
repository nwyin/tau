## Workflow: Bug investigation and fix

Use py_repl when debugging a bug that requires investigation across multiple files or components.

```python
bug = "Users get 500 error when submitting the contact form"

# Phase 1: investigate (parallel — cast a wide net)
tau.parallel(
    tau.Thread("repro", f"Find the error path and reproduce: {bug}", tools=["read", "terminal"]),
    tau.Thread("context", f"Find related code, recent changes, and test coverage for: {bug}", tools=["read"]),
)

# Phase 2: root cause (has investigation context)
rca = tau.thread("rca", f"Identify root cause given investigation: {bug}",
                  episodes=["repro", "context"], tools=["read"])
tau.document("write", name="root_cause", content=str(rca))

# Phase 3: fix (has full context chain)
fix = tau.thread("fix", f"Fix the root cause described in document 'root_cause'",
                  episodes=["repro", "rca"], tools=["full"])

# Phase 4: verify the fix
result = tau.thread("verify", f"Run tests and confirm the fix for: {bug}",
                     episodes=["rca", "fix"], tools=["full"])

if not result:
    tau.thread("fix", f"Tests still failing: {result.reason}",
               episodes=["fix", "verify"], tools=["full"])
```
