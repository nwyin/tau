# Terminal-Bench Adapter for tau

This directory contains the Python adapter that wires tau's `coding-agent` binary into
[Terminal-Bench](https://github.com/terminal-bench/terminal-bench)'s evaluation framework,
enabling tau to be benchmarked against Terminal-Bench's 89 curated Docker-based coding tasks.

## Prerequisites

1. **terminal-bench** installed:
   ```bash
   pip install terminal-bench>=0.1.0
   # or from this directory:
   pip install -r benchmarks/terminal-bench/requirements.txt
   ```

2. **coding-agent binary** available inside the evaluation Docker container.
   Use `install.sh` to set that up (see [Binary Installation](#binary-installation)).

3. **API key** set in your host environment (e.g. `ANTHROPIC_API_KEY`).
   Terminal-Bench passes host env vars through to the container automatically.

## Quick Start

Run tau against a single task to verify the setup:

```bash
tb run \
  --agent-import-path benchmarks.terminal_bench.adapter:TauAgent \
  --dataset terminal-bench-core==head \
  --task-id hello-world
```

## Full Evaluation

```bash
tb run \
  --agent-import-path benchmarks.terminal_bench.adapter:TauAgent \
  --dataset terminal-bench-core@0.1.1 \
  --model anthropic/claude-sonnet-4-20250514
```

## Binary Installation

`install.sh` installs the `coding-agent` binary inside the Docker container using one
of three fallback paths (tried in order):

| Method | How to trigger |
|--------|---------------|
| **Mount** (dev) | `docker run -v /path/to/coding-agent:/mnt/coding-agent ...` |
| **Release download** (default) | Publish a musl binary release; installer uses `latest` automatically |
| **URL download** (override) | Set `TAU_BINARY_URL=https://...` |
| **Build from source** | Have `cargo` installed in the container |

Run it as part of your Docker image build or as a pre-eval hook:

```bash
bash benchmarks/terminal-bench/install.sh
```

## Configuration

All configuration is via environment variables set on the host before running `tb run`:

| Variable | Default | Description |
|----------|---------|-------------|
| `TAU_MAX_TURNS` | `50` | Maximum agent turns per task |
| `TAU_MODEL` | `claude-sonnet-4-20250514` | Model used by coding-agent |
| `TAU_BINARY_VERSION` | `latest` | Release tag to install from (`latest` or `v0.1.0`) |
| `TAU_BINARY_REPO` | `tnguyen21/tau` | GitHub repo used for release downloads |
| `TAU_BINARY_URL` | _(computed)_ | Explicit URL override for the pre-built binary |

You can also pass constructor kwargs directly when instantiating `TauAgent` in a custom
evaluation script:

```python
from benchmarks.terminal_bench.adapter import TauAgent

agent = TauAgent(model="claude-opus-4-20250514", max_turns=100)
```

## Known Limitations

- **Interactive tasks time out naturally** — tasks requiring interactive terminal programs
  (vim, less, ncurses apps) will run until the 1-hour timeout rather than failing fast.
  There is no vim/less support in the current coding-agent implementation.
- **No streaming progress** — the adapter polls every 10 seconds; there is no real-time
  output while the agent is running.
- **Single-session only** — `--no-session` disables persistent session state; each task
  starts fresh.
