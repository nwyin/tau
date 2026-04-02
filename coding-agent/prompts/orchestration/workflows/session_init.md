## Workflow: Session initialization

Use py_repl to set up a long-running session before the supervised loop begins. This workflow analyzes the codebase, generates a structured work queue, and creates any setup scripts needed. Run once, then hand off to the supervised loop workflow.

```python
import json, os

# Phase 1: Analyze the codebase (parallel)
tau.parallel(
    tau.Thread("arch", "Map the project structure, entry points, and key modules",
               tools=["read"]),
    tau.Thread("tests", "Identify test framework, existing test coverage, and how to run tests",
               tools=["read"]),
    tau.Thread("gaps", "Identify missing features, TODOs, and areas needing work",
               tools=["read"]),
)

# Phase 2: Generate the work queue
plan = tau.thread("planner",
    "Create a prioritized list of work items based on the codebase analysis. "
    "Each item should be a self-contained unit of work (implementable in one session). "
    "Output valid JSON matching this schema: "
    '{"items": [{"id": "1", "description": "...", "priority": 1, "status": "pending"}]}',
    episodes=["arch", "tests", "gaps"], tools=["read"])

# Extract JSON from the planner's output
tau.tool("file_write", path=".tau/workqueue.json", content=plan.output)

# Phase 3: Create init script (environment setup)
init = tau.thread("setup",
    "Create an init.sh script that starts the dev environment. "
    "Include: install dependencies, start dev server, wait for it to be ready. "
    "Write it to init.sh.",
    episodes=["arch", "tests"], tools=["full"])

tau.tool("bash", command="chmod +x init.sh 2>/dev/null; true")

# Phase 4: Verify the setup works
verify = tau.thread("verify-setup", "Run init.sh and verify the dev environment starts. "
                     "Run any existing tests to establish a baseline.",
                     episodes=["setup", "tests"], tools=["full"])

items = json.loads(tau.tool("file_read", path=".tau/workqueue.json"))
tau.log(f"Session initialized: {len(items.get('items', []))} work items queued")
```

After initialization, the supervised loop workflow takes over and iterates through the work queue.
