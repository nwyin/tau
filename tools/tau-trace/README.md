# tau-trace

TUI viewer for tau agent trace files. Visualizes thread orchestration,
routing behavior, and tool execution from `.jsonl` traces.

## Install

```bash
cd tools/tau-trace
uv venv && uv pip install -e .
```

## Usage

```bash
# View a specific trace
uv run tau-trace path/to/trace.jsonl

# View the most recent trace
uv run tau-trace ~/.tau/traces/$(ls -t ~/.tau/traces/ | head -1)/trace.jsonl

# View a benchmark trace
uv run tau-trace ../../benchmarks/thread-coordination/results/trace-*.jsonl
```

## Views

### Timeline (default)

Gantt chart showing thread execution over time.

```
            0s   39s  78s  118s 157s 196s
            ────────────────────────────────
▶ api       ████                      ✓ 14.0s
▶ schema    ████████████              ✓ 38.4s
▶ html      █                         ✓ 5.3s
                     tests ██████     ✓ 20.1s

Episodes
  schema, api, html ──▶ tests

Documents
  recipe_book_spec (1257 chars)  W:orch  R:api,html,schema
```

- **Arrow keys** — navigate between threads
- **Enter** — expand/collapse thread to show individual tool calls
- **Enter on tool call** — show full args and result in the detail panel

When expanded:

```
▼ schema (38.4s, 9 tools, ✓)
  ├ document recipe_book_spec           │ 0ms
  ├ bash pwd && ls -la && find ...      │ 30ms
  ├ file_write db.py                    │ 2ms
  ├ file_write main.py                  │ 0ms
  ├ file_write templates/index.html     │ 2ms
  └ complete                            │ 0ms
```

### Routing

ASCII DAG showing how context flows between threads via episodes
and shared documents.

```
  ◈ orchestrator
  │ write recipe_book_spec
  ├────────┬────────┐
  ✓ api    ✓ schema ✓ html     (parallel)
  └────────┼────────┘
  │ episodes → tests
  ✓ tests  20.1s
    ← episodes from schema, api, html

Documents
  ▶ recipe_book_spec  (1257 chars)
      written by: orchestrator  read by: api, html, schema
      ────────────────────────────────────────
      Project: FastAPI + SQLite recipe book app.

      Database:
      - recipes(id, title, description, ...)
      - ingredients(id, recipe_id, name, ...)
      ...
```

- **Arrow keys** — navigate between documents
- Selected document shows a content preview inline

### Detail Panel (bottom)

Always visible. Shows context for the currently selected item:

- **Thread selected** — alias, outcome, duration, model, task description, tool breakdown
- **Tool call selected** — tool name, args (JSON), result content, duration, error status

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Up/Down` | Navigate threads or documents |
| `Enter` | Expand/collapse thread, or show tool detail |
| `Tab` | Switch between Timeline and Routing views |
| `q` | Quit |

## Trace Format

Reads `.jsonl` files produced by tau's `TraceSubscriber` (written to
`~/.tau/traces/<session_id>/trace.jsonl`). Each line is a JSON object
with `ts` (RFC3339) and `event` fields. Supported event types:

| Event | What it shows |
|-------|---------------|
| `thread_start/end` | Thread bars on Gantt chart |
| `tool_start/end` | Tool calls within threads |
| `episode_inject` | Episode routing arrows |
| `document_op` | Document coordination |
| `evidence_cite` | Evidence citation count |
| `turn_end` | Token usage |
| `agent_start/end` | Session duration |

See `docs/trace-analysis.md` in the tau repo for the full event schema
and `jq` query recipes for CLI-based analysis.
