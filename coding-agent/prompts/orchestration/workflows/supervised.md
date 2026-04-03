## Workflow: Supervised loop (long-running)

Use py_repl for multi-step tasks with a persistent work queue. The py_repl program acts as a lightweight supervisor — it consumes minimal context while worker and verifier threads each get fresh, bounded context windows. Progress is checkpointed to disk so work survives session restarts.

The pattern: iterate a work queue, dispatch a worker thread per item in an isolated worktree, verify with a separate thread, merge on success, checkpoint after each item.

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

    # Worker: implement in isolated worktree (fresh thread each item)
    worker = tau.thread(f"worker-{item['id']}", f"Implement: {desc}",
                         tools=["full"], worktree=True)

    if not worker:
        item["status"] = "failed"
        item["failure_reason"] = worker.reason[:200]
        tau.log(f"FAILED (worker): {desc}")
        tau.tool("file_write", path=".tau/workqueue.json",
                 content=json.dumps(queue, indent=2))
        continue

    # Verifier: review the diff and run tests
    diff = tau.diff(f"worker-{item['id']}")
    verify = tau.thread(f"verify-{item['id']}",
        f"Review and test changes for: {desc}\n\nDiff summary:\n{diff.stat}",
        episodes=[f"worker-{item['id']}"], tools=["full"])

    if verify.completed:
        merged = tau.merge(f"worker-{item['id']}")
        if merged:
            item["status"] = "done"
            tau.log(f"DONE: {desc} ({worker.files_changed} files)")
        else:
            item["status"] = "conflict"
            item["failure_reason"] = f"Merge conflicts: {merged.conflicts}"
            tau.log(f"CONFLICT: {desc}")
    else:
        item["status"] = "failed"
        item["failure_reason"] = verify.reason[:200]
        tau.log(f"FAILED (verify): {desc}")

    # Checkpoint after every item — if session dies, we resume here
    tau.tool("file_write", path=".tau/workqueue.json",
             content=json.dumps(queue, indent=2))

done = sum(1 for i in queue["items"] if i["status"] == "done")
tau.log(f"Progress: {done}/{len(queue['items'])} items complete")
```

Key design principles:
- **Worktree isolation.** Each worker thread gets its own branch via `worktree=True`. No clobbering between parallel items, and failed work is discarded by simply not merging.
- **State lives on disk, not in memory.** The work queue file is the source of truth. Virtual documents and thread episodes are ephemeral — use `file_write` for anything that must survive.
- **Each item gets fresh threads.** Don't reuse thread aliases across items — old context accumulates and degrades quality. Use unique aliases like `worker-{id}`.
- **Merge on success, skip on failure.** Use `tau.merge()` to integrate verified work. Failed items leave their branch intact for later inspection or retry.
- **The supervisor is stateless.** The py_repl loop itself holds no important state. If the session restarts, re-run the same program — it skips completed items automatically.
