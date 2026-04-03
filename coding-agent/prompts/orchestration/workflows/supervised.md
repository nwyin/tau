## Workflow: Adaptive supervised loop (long-running)

Use py_repl for multi-step tasks with a persistent work queue. The supervisor loop has a **checkpoint** after each phase that evaluates actual project state and adapts the plan — retrying, splitting oversized items, or skipping when appropriate. Independent items run in parallel.

### Workqueue schema

Items support dependencies and retry tracking:

```json
{
  "id": "parser",
  "description": "Implement recursive descent parser...",
  "status": "pending",
  "phase": 2,
  "depends_on": ["scanner", "ast"],
  "max_retries": 2,
  "timeout": 300,
  "attempts": []
}
```

Status values: `pending`, `in_progress`, `done`, `failed`, `split`, `absorbed`, `blocked`.

### The loop

```python
import json, re

def load_queue():
    return json.loads(tau.tool("file_read", path=".tau/workqueue.json"))

def save_queue(q):
    tau.tool("file_write", path=".tau/workqueue.json",
             content=json.dumps(q, indent=2))

def deps_satisfied(item, q):
    for dep_id in item.get("depends_on", []):
        dep = next((i for i in q["items"] if i["id"] == dep_id), None)
        if not dep or dep["status"] not in ("done", "split"):
            return False
    return True

def has_failed_deps(item, q):
    return any(
        any(i["id"] == d and i["status"] == "failed" for i in q["items"])
        for d in item.get("depends_on", [])
    )

def build_task(item):
    """Build the task prompt for a worker thread."""
    task = f"Implement: {item['description']}"
    if item.get("retry_context"):
        task += f"\n\nContext from previous attempt: {item['retry_context']}"
    if item.get("split_from"):
        task += f"\n\n(Split from a larger item — focus only on this piece.)"
    if item.get("last_episode_summary"):
        task += f"\n\nPrior attempt output: {item['last_episode_summary'].get('key_output','')[:500]}"
    return task

def run_batch(ready, q):
    """Run a batch of independent items: parallel workers -> parallel verifiers -> serial merges.
    Returns list of (item, merged, result) tuples."""

    if len(ready) == 1:
        # Single item — use simple sequential path
        item = ready[0]
        w = f"worker-{item['id']}"
        v = f"verifier-{item['id']}"
        timeout = item.get("timeout", 300)

        worker = tau.thread(w, build_task(item), tools=["full"],
                            worktree=True, worktree_include=["_reference"],
                            timeout=timeout)
        if not worker:
            return [(item, False, worker)]

        diff = tau.diff(w)
        verifier = tau.thread(v,
            f"Verify, test, and fix if needed: {item['description']}\nDiff:\n{diff.stat}",
            tools=["full"], worktree=True, worktree_base=w,
            worktree_include=["_reference"], episodes=[w], timeout=timeout)

        if verifier.completed:
            merged = tau.merge(v)
            return [(item, bool(merged), verifier)]
        return [(item, False, verifier)]

    # Multiple items — parallel dispatch
    tau.log(f"Parallel batch: {[i['id'] for i in ready]}")

    # Phase 1: parallel workers
    worker_specs = [
        tau.Thread(f"worker-{item['id']}", build_task(item),
                   tools=["full"], worktree=True,
                   worktree_include=["_reference"],
                   timeout=item.get("timeout", 300))
        for item in ready
    ]
    worker_results = tau.parallel(*worker_specs)

    # Phase 2: parallel verifiers (only for successful workers)
    verifier_items = []
    verifier_specs = []
    failed_results = []
    for item, wr in zip(ready, worker_results):
        if wr.completed:
            w = f"worker-{item['id']}"
            v = f"verifier-{item['id']}"
            diff = tau.diff(w)
            verifier_items.append(item)
            verifier_specs.append(
                tau.Thread(v,
                    f"Verify, test, and fix: {item['description']}\nDiff:\n{diff.stat}",
                    tools=["full"], worktree=True, worktree_base=w,
                    worktree_include=["_reference"], episodes=[w],
                    timeout=item.get("timeout", 300))
            )
        else:
            failed_results.append((item, False, wr))

    verifier_results = tau.parallel(*verifier_specs) if verifier_specs else []

    # Phase 3: serial merges (order matters — each merge changes HEAD)
    results = list(failed_results)
    for item, vr in zip(verifier_items, verifier_results):
        if vr.completed:
            merged = tau.merge(f"verifier-{item['id']}")
            results.append((item, bool(merged), vr))
        else:
            results.append((item, False, vr))

    return results

def checkpoint(q, item, merged, result):
    """Evaluate state and decide next action after a phase."""
    attempt = {
        "attempt": len(item.get("attempts", [])) + 1,
        "status": "done" if merged else (result.status if result else "skipped"),
        "duration_ms": result.duration_ms if result else 0,
    }
    if result and not merged:
        attempt["failure_reason"] = (result.reason or "unknown")[:300]
    item.setdefault("attempts", []).append(attempt)

    # Save episode summary for session resume
    if result:
        item["last_episode_summary"] = {
            "worker": f"worker-{item['id']}",
            "outcome": result.status,
            "key_output": (result.output or "")[:500],
        }

    if merged:
        item["status"] = "done"
        save_queue(q)
        tau.log(f"DONE: {item['id']}")
        return {"action": "DONE"}

    if len(item["attempts"]) > item.get("max_retries", 2):
        item["status"] = "failed"
        save_queue(q)
        tau.log(f"EXHAUSTED: {item['id']}")
        return {"action": "SKIP"}

    # Check actual project state
    state = tau.tool("bash",
        command="cargo build 2>&1 | tail -5; echo '==='; python3 test_runner.py --summary 2>&1 | tail -3")

    pending = [{"id": i["id"], "desc": i["description"][:80],
                "depends_on": i.get("depends_on", [])}
               for i in q["items"] if i["status"] in ("pending",)]

    decision_prompt = f"""A work item failed. Decide the next action.

Failed item: {item['id']}
Description: {item['description'][:200]}
Attempt {len(item['attempts'])} of {item.get('max_retries', 2) + 1}
Status: {attempt['status']} | Duration: {attempt.get('duration_ms', 0) / 1000:.0f}s / timeout {item.get('timeout', 300)}s
Failure: {attempt.get('failure_reason', 'none')[:200]}

Project state on main:
{state}

Remaining pending items:
{json.dumps(pending, indent=2)}

Choose ONE action. Respond with JSON only:
- RETRY: {{"action":"RETRY","reason":"...","new_timeout":<secs>,"extra_context":"hint for retry"}}
- SPLIT: {{"action":"SPLIT","reason":"...","sub_items":[{{"id":"...","description":"...","depends_on":[...]}}]}}
- SKIP: {{"action":"SKIP","reason":"..."}}
- ABSORB: {{"action":"ABSORB","reason":"...","target_id":"<downstream item>","extra_description":"..."}}
"""
    raw = tau.query(decision_prompt, model="reasoning")
    m = re.search(r'\{.*\}', raw, re.DOTALL)
    decision = json.loads(m.group()) if m else {"action": "SKIP", "reason": "parse error"}
    tau.log(f"CHECKPOINT {item['id']}: {decision['action']} — {decision.get('reason','')[:80]}")
    return decision

def apply_decision(q, item, decision):
    action = decision["action"]
    if action == "DONE":
        return
    elif action == "RETRY":
        item["timeout"] = decision.get("new_timeout", item.get("timeout", 300) + 120)
        item["retry_context"] = decision.get("extra_context", "")
        item["status"] = "pending"
    elif action == "SPLIT":
        item["status"] = "split"
        subs = decision["sub_items"]
        item["split_into"] = [s["id"] for s in subs]
        idx = q["items"].index(item)
        for i, sub in enumerate(subs):
            sub.setdefault("status", "pending")
            sub.setdefault("phase", item["phase"])
            sub.setdefault("timeout", item.get("timeout", 300))
            sub.setdefault("attempts", [])
            sub.setdefault("depends_on", [])
            sub.setdefault("max_retries", 2)
            sub["split_from"] = item["id"]
            q["items"].insert(idx + 1 + i, sub)
    elif action == "SKIP":
        item["status"] = "failed"
    elif action == "ABSORB":
        target = next((i for i in q["items"] if i["id"] == decision["target_id"]), None)
        if target:
            target["description"] += f"\n\nNOTE (from {item['id']}): {decision.get('extra_description','')}"
            item["status"] = "absorbed"
    save_queue(q)

# Main loop
queue = load_queue()
max_iter = len(queue["items"]) * 3

for _ in range(max_iter):
    # Handle blocked items first
    for c in queue["items"]:
        if c["status"] not in ("pending", "in_progress"):
            continue
        if has_failed_deps(c, queue):
            c["status"] = "blocked"
            tau.log(f"BLOCKED: {c['id']} (failed deps)")
            decision = checkpoint(queue, c, False, None)
            apply_decision(queue, c, decision)

    # Collect all items with satisfied deps
    ready = [c for c in queue["items"]
             if c["status"] in ("pending",) and deps_satisfied(c, queue)]

    if not ready:
        break

    for item in ready:
        item["status"] = "in_progress"
    tau.log(f"Batch: {[i['id'] for i in ready]}")
    save_queue(queue)

    # Run the batch (parallel if multiple, sequential if single)
    batch_results = run_batch(ready, queue)

    # Checkpoint each item
    for item, merged, result in batch_results:
        decision = checkpoint(queue, item, merged, result)
        apply_decision(queue, item, decision)

# Final report
state = tau.tool("bash",
    command="python3 test_runner.py --summary 2>&1 | tail -5")
done = sum(1 for i in queue["items"] if i["status"] == "done")
total = sum(1 for i in queue["items"] if i["status"] not in ("split", "absorbed"))
tau.log(f"Final: {done}/{total} items done\n{state}")
```

Key design principles:
- **Independent items run in parallel.** Items with satisfied deps and no mutual conflicts dispatch concurrently via `tau.parallel()`. Workers run in separate worktrees; verifiers run after all workers complete.
- **Merges are serial.** Each merge changes HEAD, so they happen one at a time after all verifiers complete.
- **Dependencies prevent cascade failures.** Items with failed deps are `blocked` and trigger a checkpoint decision (usually SPLIT or ABSORB).
- **Checkpoint evaluates real state.** After each batch, `cargo build` + `test_runner.py --summary` reveals what actually works, not what the plan says should work.
- **LLM decides adaptation.** The reasoning model analyzes the failure and project state to choose RETRY (more time), SPLIT (smaller pieces), SKIP (give up), or ABSORB (merge into downstream item).
- **State lives on disk.** The workqueue JSON is the source of truth. Attempts history and episode summaries survive session restarts.
- **Resumable.** Each item's `last_episode_summary` preserves context from prior attempts. On `--resume`, the loop re-reads the queue and continues from checkpoint.
- **Safety bounds.** Max iterations prevent infinite loops from runaway splits.
