# tau-trace: TUI Trace Viewer

Standalone Python TUI for exploring tau JSONL trace files. Built with
Textual for rich terminal UI. Primary use: demo-quality visualization
of agent thread orchestration and routing behavior.

## Requirements

### View 1: Thread Gantt Chart (hero view, shown on launch)

- Horizontal timeline showing each thread as a colored bar
- Time axis scaled to session duration (0s to Ns)
- Parallel threads visually overlap on the same time range
- Each bar shows: alias, duration, outcome icon (checkmark/X/timeout)
- Main agent activity shown as a distinct "orchestrator" row
- Tool calls rendered as tick marks within each thread bar
- Below the chart: episode injection arrows (e.g., `schema ──▶ tests`)
- Below episodes: document names with R/W indicators

### View 1 interaction: Expand thread inline

- Arrow keys to select a thread bar; Enter to expand/collapse
- Expanded view shows individual tool calls as sub-rows:
  - Tool name, first arg (path/pattern/command), duration
  - Each tool call positioned on the timeline at its actual time
- Expanded tool call can be selected; Enter shows full args + result content in a bottom panel

### View 2: Routing Graph (tab to switch)

- ASCII DAG showing episode and document flow between threads
- Nodes: orchestrator + each thread (with status icon)
- Edges: episode injections (labeled `ep`) and document operations (labeled `doc`)
- Selecting a document node shows content preview in a side panel (first ~30 lines)
- Selecting a thread node shows: task description, model, duration, tool count

### Summary header (always visible)

- Session ID, total duration, model name
- Thread count with outcome icons
- Total tool calls, document count, episode count
- Token usage (if turn_end events have token data)

### Data loading

- Accepts one positional argument: path to `trace.jsonl`
- Parse all event types from the JSONL (see `docs/trace-analysis.md` for schema)
- Gracefully handle missing/unknown event types

## Project Structure

```
tools/tau-trace/
├── SPEC.md
├── pyproject.toml        # uv/pip project, textual dependency
├── tau_trace/
│   ├── __init__.py
│   ├── __main__.py       # CLI entry point
│   ├── app.py            # Textual App class
│   ├── models.py         # Parsed trace data structures
│   ├── gantt.py          # Gantt chart widget
│   ├── routing.py        # Routing graph widget
│   └── detail.py         # Detail panel widget
└── README.md
```

## Verification

- `cd tools/tau-trace && uv run python -m tau_trace ../../../benchmarks/thread-coordination/results/trace-20260401-184131.jsonl` launches and renders
- `uv run python -m pytest tests/ -v` (if tests exist)
- `uvx ruff check tau_trace/`
- `uvx ruff format --check tau_trace/`

## Success Criteria

- All verification commands pass
- Gantt chart renders with correct thread bars, timing, and parallel overlap
- Expanding a thread shows tool calls with names and durations
- Tab switches to routing graph view
- Selecting a document shows content preview
- Summary header shows accurate counts from trace data
- Works with the real trace file at `benchmarks/thread-coordination/results/trace-20260401-184131.jsonl`
