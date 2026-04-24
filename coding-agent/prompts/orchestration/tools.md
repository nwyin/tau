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

## Long-running threads

Use `max_turns` when a thread needs a larger conversation budget than the default:
```python
thread("researcher", "Keep iterating until both fixtures are mapped",
       tools=["read"], max_turns=80)
```

Increase `max_turns` deliberately for reactive or research threads. Keep short, bounded threads on the default.

## Worktree isolation

Threads that modify files can use `worktree=True` for git-level isolation:
```python
thread("impl", "Implement feature X", tools=["full"], worktree=True)
# result branch: "tau/impl"
# result diff_stat: "3 files changed, 45 insertions(+), 12 deletions(-)"
```

After a worktree thread completes, inspect the recorded branch and diff summary
before integrating. Programmatic diff and merge APIs are documented with the
Python orchestration addendum when that tool is enabled.

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

**Worktree management:** Worktree threads return branch and diff summary
metadata. Use that metadata to review changes before merging branches.
