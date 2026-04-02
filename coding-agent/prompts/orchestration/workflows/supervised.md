## Workflow: Supervised loop (long-running)

Use py_repl for multi-step tasks with a persistent work queue. The py_repl program acts as a lightweight supervisor — it consumes minimal context while worker and verifier threads each get fresh, bounded context windows. Progress is checkpointed to disk so work survives session restarts.

The pattern: iterate a work queue, dispatch a worker thread per item, verify with a separate thread, revert on failure, checkpoint after each item.

```python
import json

# Load work queue from disk (survives session restarts)
queue = json.loads(tau.tool("file_read", path=".tau/workqueue.json"))

for item in queue["items"]:
    if item["status"] == "done":
        continue

    desc = item["description"]
    item["status"] = "in_progress"
    tau.log(f"Starting: {desc}")

    # Worker: implement in bounded context (fresh thread each item)
    worker = tau.thread(f"worker-{item['id']}", f"Implement: {desc}",
                         tools=["full"])

    # Verifier: separate thread, adversarial perspective
    verify = tau.thread(f"verify-{item['id']}", f"Test and verify: {desc}",
                         episodes=[f"worker-{item['id']}"], tools=["full"])

    if verify.completed:
        item["status"] = "done"
        tau.tool("bash", command=f"git add -A && git commit -m 'feat: {desc}'")
    else:
        item["status"] = "failed"
        item["failure_reason"] = verify.reason[:200]
        tau.tool("bash", command="git checkout .")
        tau.log(f"FAILED: {desc} — {verify.reason[:100]}")

    # Checkpoint after every item — if session dies, we resume here
    tau.tool("file_write", path=".tau/workqueue.json",
             content=json.dumps(queue, indent=2))

done = sum(1 for i in queue["items"] if i["status"] == "done")
tau.log(f"Progress: {done}/{len(queue['items'])} items complete")
```

Key design principles:
- **State lives on disk, not in memory.** The work queue file is the source of truth. Virtual documents and thread episodes are ephemeral — use `file_write` for anything that must survive.
- **Each item gets fresh threads.** Don't reuse thread aliases across items — old context accumulates and degrades quality. Use unique aliases like `worker-{id}`.
- **Revert on failure.** Use `git checkout .` to undo partial work. Never leave the repo in a broken state between items.
- **The supervisor is stateless.** The py_repl loop itself holds no important state. If the session restarts, re-run the same program — it skips completed items automatically.
