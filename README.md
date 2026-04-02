# tau

A Rust coding-agent harness with a full-screen TUI, headless CLI, JSON-RPC serve mode, reusable worker threads, and structured traces.

## Architecture

```text
ai             provider adapters, model catalog, auth, streaming primitives
agent          generic agent runtime, context compaction, stats, orchestration
coding-agent   tau binary: TUI/CLI/serve modes, tools, permissions, sessions, traces
```

`ai` and `agent` stay generic. `coding-agent` is the built-in coding harness on top.

## What tau has today

- Full-screen TUI with chat, sidebar, slash commands, thread inspection, and inline edit/create diffs
- Headless `--prompt` mode for scripting and benchmarks
- `serve` mode for JSON-RPC orchestration over stdio
- Built-in tool suite for file edits, shell, search, web access, planning, and orchestration
- Reusable in-process threads, episodes, shared documents, and a persistent Python REPL tool
- Tool permissions (`allow` / `deny` / `ask`) plus `--yolo` auto-approve mode
- Session persistence, resume support, and always-on structured trace capture
- Skill discovery from project-local and user-global `.tau/skills/` directories

## Tools

| Category | Tools |
| --- | --- |
| Filesystem + shell | `bash`, `file_read`, `file_edit`, `file_write`, `glob`, `grep` |
| Web | `web_fetch`, `web_search` |
| Planning + delegation | `subagent`, `todo` |
| Orchestration | `thread`, `query`, `document`, `log`, `from_id`, `py_repl` |

`thread`, `query`, `document`, `log`, `from_id`, and `py_repl` are backed by shared in-process orchestration state, so threads can reuse prior episodes and coordinate through virtual documents.

## Installation

### From source

Requires [Rust toolchain](https://rustup.rs/) (1.75+).

```bash
# Clone and install
git clone https://github.com/tnguyen21/tau.git
cd tau
cargo install --path coding-agent

# Or install directly from GitHub without cloning
cargo install --git https://github.com/tnguyen21/tau.git coding-agent
```

This puts `tau` on your `$PATH`.

### Prebuilt Linux binary

Tagged releases publish a static `x86_64-unknown-linux-musl` binary:

```bash
curl -fsSL \
  https://github.com/tnguyen21/tau/releases/latest/download/tau-x86_64-unknown-linux-musl \
  -o /usr/local/bin/tau
chmod +x /usr/local/bin/tau
```

Override the release source with `TAU_BINARY_VERSION`, `TAU_BINARY_REPO`, or `TAU_BINARY_URL` when needed. See [Release and container install](docs/releases.md).

## Quick start

```bash
# Anthropic
export ANTHROPIC_API_KEY=sk-ant-...

# OpenAI-family models
export OPENAI_API_KEY=sk-...
# or use Codex OAuth
codex login

# Interactive TUI
tau

# Choose a model
tau --model claude-sonnet-4-6

# One-shot / headless mode
tau --prompt "Summarize this repo"

# Restrict tools
tau --tools file_read,grep,glob

# List models
tau models --provider anthropic

# Run as a JSON-RPC backend
tau serve --cwd .
```

## Providers and auth

Tau supports:

- Anthropic Messages API
- OpenAI Responses API
- OpenAI-compatible chat backends, including OpenRouter and other compatible providers in the built-in model catalog

Auth comes from provider-specific environment variables such as `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `GROQ_API_KEY`, `TOGETHER_API_KEY`, and `DEEPSEEK_API_KEY`.

For OpenAI-family models, tau can also fall back to Codex OAuth from `~/.codex/auth.json` when `OPENAI_API_KEY` is not set.

## Testing and benchmarking

```bash
cargo test
cargo bench
```

- `cargo test` is offline by default; live provider tests require explicit opt-in
- Criterion benches cover core runtime pieces such as SSE parsing, serde, and agent construction
- Broader harness evals and microbenchmarks live under [`benchmarks/`](benchmarks/) — see [Benchmarking strategy](docs/benchmarking.md)

The Python benchmark suite targets Python 3.12+ and `uv`.

## Configuration

Tau reads global config from `~/.tau/config.toml`:

```toml
model = "claude-sonnet-4-6"
thinking = "medium"
tools = ["file_read", "file_edit", "file_write", "glob", "grep", "bash"]
skills = true

[permissions]
bash = "ask"
file_edit = "ask"
file_write = "ask"
web_search = "allow"

[models]
search = "claude-haiku-4-5"
subagent = "claude-haiku-4-5"
reasoning = "claude-opus-4-6"
```

Model slots let orchestration tools route work to different models for cheap search, deeper reasoning, or subagents.

## Skills

Tau auto-discovers skills from:

- project-local `.tau/skills/` directories (walking up from the current directory toward the git root)
- user-global `~/.tau/skills/`

In the TUI, invoke skills with `/skill:<name>`. You can also load explicit skill files with repeated `--skill PATH` flags.

## Sessions and traces

- Interactive TUI runs create sessions by default in `~/.tau/sessions/`
- Resume with `--resume` or `--session <id>`
- Use `--no-session` for ephemeral runs
- Traces are written to `~/.tau/traces/<session_id>/` by default as `run.json` and `trace.jsonl`
- Override trace output with `--trace-output <dir>`

For trace inspection, see [`tools/tau-trace`](tools/tau-trace/README.md) and [Trace analysis](docs/trace-analysis.md).

## Project structure

```text
ai/
  src/
    providers/            # Anthropic, OpenAI Responses, OpenAI-compatible chat
    catalog.rs            # built-in model catalog
    models.rs             # model registry helpers
    codex_auth.rs         # Codex OAuth / ChatGPT backend auth
    stream.rs, types.rs   # streaming + shared types
agent/
  src/
    agent.rs, loop_.rs    # core agent runtime
    context.rs            # mechanical context compaction
    orchestrator.rs       # shared thread/document state
    thread.rs             # thread + episode types
    stats.rs              # runtime statistics
coding-agent/
  src/
    main.rs               # TUI + headless CLI entrypoint
    serve.rs              # JSON-RPC stdio server
    cli.rs, config.rs     # CLI parsing + config loading
    permissions.rs        # allow/deny/ask tool policy layer
    session.rs            # JSONL session persistence
    trace.rs              # run.json + trace.jsonl output
    skills.rs             # slash-command skill loading
    rpc/                  # serve-mode transport + handlers
    tools/                # built-in tools and orchestration tools
    tui/                  # panes, chat UI, sidebar, thread modal
tools/
  tau-trace/              # TUI viewer for tau trace files
benchmarks/              # eval adapters, microbenchmarks, fixtures
```

## Docs

- [Architecture overview](docs/overview.md)
- [Benchmarking strategy](docs/benchmarking.md)
- [Orchestration design](docs/design-orchestration.md)
- [Context management](docs/context-management.md)
- [Trace analysis](docs/trace-analysis.md)
- [Release and container install](docs/releases.md)
- [Feature comparison](docs/feature-comparison.md)
- [Benchmarks landscape](docs/benchmarks-landscape.md)
- [Harness lit review](docs/harness-lit-review.md)

## Roadmap

Tau is still a hackable research harness. Near-term work is centered on:

1. Daily-driver polish for the TUI, permissions UX, skills, and orchestration workflows
2. Better trace observability and benchmark coverage
3. RL trajectory generation and harness-aware post-training experiments

## License

MIT
