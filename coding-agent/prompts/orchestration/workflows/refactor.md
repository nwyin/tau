## Workflow: Safe refactoring

Use py_repl for refactors that touch many files. The key is verifying no breakage at each step.

```python
goal = "Extract auth middleware into its own module"
scope = "src/"

# Phase 1: map the blast radius (parallel)
tau.parallel(
    tau.Thread("usages", f"Find all usages and call sites for: {goal}", tools=["read"]),
    tau.Thread("deps", f"Map dependency graph and imports for: {goal}", tools=["read"]),
    tau.Thread("tests", f"Identify all tests covering: {goal}", tools=["read"]),
)

# Phase 2: plan the refactor (has full map)
plan = tau.thread("plan", f"Design a step-by-step refactoring plan for: {goal}",
                   episodes=["usages", "deps", "tests"], tools=["read"])
tau.document("write", name="refactor_plan", content=str(plan))

# Phase 3: execute in isolated worktree
tau.thread("refactor", f"Execute refactoring per document 'refactor_plan'",
            episodes=["usages", "deps", "plan"], tools=["full"], worktree=True)

# Phase 4: verify nothing broke
result = tau.thread("verify", f"Run full test suite, check imports, verify no regressions",
                     episodes=["tests", "refactor"], tools=["full"])

if result.completed:
    tau.merge("refactor")
else:
    tau.thread("fixup", f"Fix regressions: {result.reason}",
               episodes=["refactor", "verify"], tools=["full"], worktree=True)
    tau.merge("fixup")
```
