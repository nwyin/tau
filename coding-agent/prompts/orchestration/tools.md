## Thread tool capabilities

Threads accept capability aliases or raw tool names in the `tools` parameter:
- `read` → file_read, grep, glob (default)
- `write` → file_read, file_edit, file_write
- `terminal` → bash
- `web` → web_fetch, web_search
- `full` → all tools

Mix freely: `tools=["read", "bash"]` gives file_read + grep + glob + bash.
Raw tool names still work: `tools=["file_read", "grep"]`.

## Model slots

Threads and queries accept model slot names in the `model` parameter:
- `search` — fast/cheap model for lookups and classification (query default)
- `subagent` — thread execution model (thread default)
- `reasoning` — for deep analysis tasks
- Or pass a raw model ID like `"claude-haiku-4-5"`

Configure slots in `~/.tau/config.toml`:
```toml
[models]
search = "claude-haiku-4-5"
reasoning = "claude-opus-4-6"
```

## Thread completion

Each thread must call `complete`, `abort`, or `escalate` when done. The thread's result becomes its episode — a compressed trace of what it did and concluded. Pass `evidence` (a list of tool_call_ids) to `complete` to mark which tool results support your conclusion.

## Worktree isolation

Threads that modify files can use `worktree=True` for git-level isolation:
```python
worker = tau.thread("impl", "Implement feature X", tools=["full"], worktree=True)
# worker.branch → "tau/impl"
# worker.diff_stat → "3 files changed, 45 insertions(+), 12 deletions(-)"
```

After a worktree thread completes, use the merge API:
```python
diff = tau.diff("impl")          # Inspect changes before merging
print(diff.stat)                  # "3 files changed, 45(+), 12(-)"
result = tau.merge("impl")       # Merge tau/impl into current branch
if not result:
    tau.log(f"Conflicts: {result.conflicts}")
```

## Helper tools

**Logging:** Record progress notes between orchestration steps.
```
log(message="Scanned 3 modules, found 5 issues. Proceeding to fix phase.")
```

**Retrieve prior results:** Look up a completed thread/query episode by alias.
```
from_id(alias="scanner")
// Returns the compact trace of the scanner thread's last run
```

**Worktree management:**
```python
tau.diff(alias)       # DiffResult: stat, diff, files_changed
tau.merge(alias)      # MergeResult: success, conflicts
tau.branches()        # List active tau/* branches
```
