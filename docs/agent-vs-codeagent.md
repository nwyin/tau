Yes. Your read is basically correct, but the split is more precise than “core loop vs more
coding tools.”

High-level
packages/agent/README.md describes a small reusable library for running an LLM agent loop with
tools and streamed events.
packages/coding-agent/README.md describes a full coding harness built on top of that library:
CLI, TUI, sessions, settings, auth, model selection, compaction, extensions, skills, package
system, export, and RPC.

What packages/agent actually is
packages/agent/src/index.ts:1 exports only four things: the Agent class, loop functions, proxy
helpers, and types.
packages/agent/src/agent-loop.ts:28 is the core loop: add prompt messages, stream assistant
output, execute tool calls, emit events, inject steering/follow-up messages, and end.
packages/agent/src/agent.ts:96 is a thin stateful wrapper around that loop. It manages
messages, isStreaming, pendingToolCalls, sessionId, thinkingBudgets, transport, and queues for
steer() / followUp().
The README matches that scope: event flow, convertToLlm, transformContext, tool definition,
custom message types, proxy usage, and the low-level loop API. It is an SDK-style README, not
an app README.

So agent is the generic engine you embed into something else.

What packages/coding-agent adds on top
packages/coding-agent/src/core/sdk.ts:165 literally constructs a pi-agent-core Agent, then
wraps it in an AgentSession.
packages/coding-agent/src/core/agent-session.ts:1 is the real center of gravity. It adds:

- session persistence and replay
- model/thinking-level restore
- auto-retry and auto-compaction
- bash execution recording
- branch/tree navigation and forked sessions
- extension event wiring
- prompt template / skill / system prompt integration

packages/coding-agent/src/core/session-manager.ts:663 manages JSONL session trees, branching,
summaries, model-change entries, and thinking-level entries.
packages/coding-agent/src/core/settings-manager.ts:195 manages persisted settings like steering
mode, follow-up mode, transport, theme, retry, image blocking, enabled models, and thinking
budgets.
packages/coding-agent/src/core/model-registry.ts:223 handles available models, provider lookup,
API key resolution, OAuth state, and provider registration.
packages/coding-agent/src/core/tools/index.ts:81 defines the built-in coding tool sets:

- default coding tools: read, bash, edit, write
- read-only tools: read, grep, find, ls

So yes, coding tools are part of it, but they’re only one subsystem.

README difference in specifics
packages/agent/README.md:34 focuses on:

- message model vs LLM message model
- event sequencing
- Agent options and state
- prompting / continuing
- steering and follow-up queues
- custom message types
- defining tools
- low-level loop and proxy transport

packages/coding-agent/README.md:49 focuses on:

- installing and running pi
- provider/auth setup
- interactive UI behavior
- slash commands and keybindings
- session storage, branching, and compaction
- settings and context files
- prompt templates, skills, extensions, themes, packages
- SDK embedding and RPC mode
- product philosophy

That is: agent explains “how to build an agent runtime”; coding-agent explains “how to use and
extend the finished agent application.”

Concrete architecture relationship
packages/coding-agent/package.json:41 depends on @mariozechner/pi-agent-core.
packages/agent/package.json:19 depends only on @mariozechner/pi-ai.

So the stack is:

1. pi-ai: provider/model streaming layer
2. pi-agent-core: generic agent loop and tool execution
3. pi-coding-agent: coding harness/product around that loop

One practical way to think about it
Use packages/agent if you want to build your own agent app, UI, or workflow engine.
Use packages/coding-agent if you want an opinionated but extensible coding assistant product
with sessions, TUI/CLI/RPC, built-in file/shell tools, and extension hooks.

There’s also a scale difference: agent has 5 source files, while coding-agent has about 120
source files, 26 docs files, 113 example files, and 76 tests, which matches the “library vs
full application platform” split.

If you want, I can do a second pass that maps packages/coding-agent file-by-file onto the
subsystems it layers over packages/agent.
