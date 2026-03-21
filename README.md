# tau

A minimal Rust coding agent harness

## Architecture

```
ai             LLM streaming primitives (providers, models, event streams)
agent          generic agent loop (tools, steering, follow-ups, events)
coding-agent   built-in tools, system prompt, REPL + headless CLI
```

`ai` and `agent` have no opinion about coding — they're generic building blocks. `coding-agent` is one harness built on top; you could build a data-analysis agent or a research agent on the same foundation without importing coding-specific dependencies.

## Tools

| Tool         | What it does                                                  |
| ------------ | ------------------------------------------------------------- |
| `bash`       | Shell execution with timeout, cancellation, output truncation |
| `file_read`  | Read files with line numbers, offset/limit, binary detection  |
| `file_write` | Write/create files, auto-create parent directories            |
| `file_edit`  | Exact-match string replacement with context on miss           |
| `grep`       | Ripgrep-backed content search with glob filtering             |
| `glob`       | Native gitignore-aware file discovery, mtime-sorted           |

**Hashline mode** — tau also ships hash-anchored line editing (invented by [Can Boluk](https://github.com/can1357) for [oh-my-pi](https://github.com/anthropics/omp)). Each line gets a content-hash tag; edits reference tags instead of reproducing text.

## Installation

### From source (recommended for development)

Requires [Rust toolchain](https://rustup.rs/) (1.75+).

```bash
# Clone and install
git clone https://github.com/tnguyen21/tau.git
cd tau
cargo install --path coding-agent

# Or install directly from GitHub without cloning
cargo install --git https://github.com/tnguyen21/tau.git coding-agent
```

This puts `coding-agent` on your `$PATH`.

### Prebuilt Linux binary

CI builds a static `x86_64-unknown-linux-musl` binary on every tagged release. Download it directly:

```bash
curl -fsSL \
  https://github.com/tnguyen21/tau/releases/latest/download/coding-agent-x86_64-unknown-linux-musl \
  -o /usr/local/bin/coding-agent
chmod +x /usr/local/bin/coding-agent
```

Override the release source with `TAU_BINARY_VERSION`, `TAU_BINARY_REPO`, or `TAU_BINARY_URL` when needed. See [Release and container install](docs/releases.md) for details.

### Structural analysis tools (optional)

For call-graph and CFG analysis, install the companion binaries:

```bash
cargo install --git https://github.com/tnguyen21/pycg-rs.git pycg
cargo install --git https://github.com/tnguyen21/pycfg-rs.git pycfg
```

These are only needed if you pass structural tools (`cg_*`, `cfg_*`) via `--tools`.

## Quick start

```bash
# Set a provider key
export ANTHROPIC_API_KEY=sk-ant-...
# or
export OPENAI_API_KEY=sk-...

# Interactive REPL
coding-agent

# Choose a model
coding-agent --model claude-sonnet-4-6

# Headless (for scripting / benchmarks)
coding-agent --prompt "List all Rust files in this repo"

# Restrict tool access
coding-agent --tools file_read,file_write,file_edit,glob,grep,bash

# With stats
coding-agent --stats --prompt "Explain this repo"
```

## Providers

OpenAI (Responses API) and Anthropic (Messages API) are implemented. Both support streaming, tool use, thinking/reasoning, and cost tracking.

## Testing

```bash
cargo test                    # 270+ offline tests, no API keys needed
cargo bench                   # criterion benchmarks (SSE parsing, serde, agent construction)
```

Live provider tests exist behind a double opt-in gate: `OPENAI_API_KEY` + `RUN_LIVE_PROVIDER_TESTS=1`.

## Configuration

tau reads `~/.tau/config.toml` (global) and `.tau/config.toml` (project-local):

```toml
[agent]
model = "claude-sonnet-4-6"
edit_mode = "hashline"    # or "standard"

[agent.thinking]
level = "medium"          # off, low, medium, high
```

## Sessions

Sessions persist as JSONL in `~/.tau/sessions/`. Resume with `--resume` (latest) or `--session <id>`.

## Project structure

```
ai/
  src/
    types.rs              # ContentBlock, Message, Model, streaming types
    stream.rs             # EventStream (mpsc + oneshot channels)
    providers/            # OpenAI Responses, Anthropic Messages
    models.rs, catalog.rs # Model registry (~65 models)
agent/
  src/
    loop_.rs              # Two-level agent loop (outer: follow-ups, inner: stream → tools → steer)
    agent.rs              # Agent struct (state, queues, cancellation, events)
    types.rs              # AgentTool trait, AgentEvent, AgentLoopConfig
coding-agent/
  src/
    tools/                # bash, file_read, file_write, file_edit, grep, glob, hashline
    system_prompt.rs      # Dynamic prompt built from active tools + cwd
    cli.rs, config.rs     # CLI parsing, TOML config
    session.rs            # JSONL session persistence
    main.rs               # REPL + headless modes
```

## Docs

- [Architecture overview](docs/overview.md) — detailed walkthrough of types, patterns, and design decisions
- [Release and container install](docs/releases.md) — tag-driven musl releases and container download flow
- [Feature comparison](docs/feature-comparison.md) — tau vs 6 other harnesses across every dimension
- [Toolset tradeoffs](docs/toolset-tradeoffs.md) — why this toolset, what others chose, and what it means
- [Benchmarks landscape](docs/benchmarks-landscape.md) — harness-sensitive benchmarks and key numbers
- [Harness lit review](docs/harness-lit-review.md) — people, papers, and open questions in harness engineering

## Roadmap

tau is currently a **hackable reference**. The path forward:

1. **Daily driver** — compaction, permissions, sub-agents, MCP, skills. See [feature-comparison.md](docs/feature-comparison.md) for the full gap analysis.
2. **RL trajectory generator** — the daily driver bootstraps itself, then generates trajectories for co-training research.

The long-term thesis: models and harnesses will be co-developed. An RL post-training step where the model does rollouts with harness trajectories and learns better tool use, context management, and file navigation. tau is built to be instrumentable enough to make that possible.

## License

MIT
