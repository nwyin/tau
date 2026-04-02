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

# Phase 3: implement (receives architecture context)
impl = tau.thread("impl", f"Implement per document 'plan': {spec}",
                   episodes=["arch"], tools=["full"])

# Phase 4: verify (receives all prior context)
result = tau.thread("verify", f"Write and run tests for: {spec}",
                     episodes=["arch", "tests", "impl"], tools=["full"])

# Phase 5: retry if needed
if not result:
    tau.thread("fix", f"Fix failing tests: {result.reason}",
               episodes=["impl", "verify"], tools=["full"])
```
