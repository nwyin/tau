# Orchestration Prompt Structure

```
orchestration/
├── overview.md              # When to use threads vs queries vs direct tools
├── tools.md                 # Thread capabilities, model slots, completion protocol
├── documents.md             # Shared document patterns for inter-thread coordination
├── patterns/
│   ├── fanout.md            # Parallel fan-out, synthesize results
│   ├── pipeline.md          # Phased execution with episode dependency injection
│   ├── adversarial.md       # Debate / red-team — threads that react to each other
│   ├── reuse.md             # Thread memory via alias reuse
│   └── programmatic.md      # py_repl for complex control flow and DAGs
└── README.md                # This file
```

## Why this structure

The orchestration prompt was a single 166-line file. We split it so that each
**pattern** is a separate file for three reasons:

1. **Discoverability.** Scanning the directory tells you what orchestration
   strategies exist. Adding a new pattern means adding a file, not editing a
   monolith. Both humans and models benefit from the filesystem being
   self-documenting.

2. **Testability.** Each pattern file maps to a class of trace behavior. When
   a trace shows the model didn't use phased execution, we can check whether
   `pipeline.md` was included, whether its example was clear, and iterate on
   that file alone without risking regressions in other patterns.

3. **Composability.** Down the line we may want to selectively include patterns
   based on task type, A/B test which patterns improve routing quality, or
   weight certain patterns more heavily. Separate files make this trivial —
   include or exclude at the `system_prompt.rs` wiring level.

## How it's wired

`system_prompt.rs` concatenates these files (in order: overview, tools,
documents, then all pattern files) into the orchestration section of the system
prompt. The section is only included when the `thread` tool is enabled.

## Adding a new pattern

1. Create `patterns/<name>.md` with a `## Pattern: <Name>` heading.
2. Include: a one-line description of when to use it, a code example showing
   the tool calls, and a brief explanation of why this pattern exists (what
   failure mode it addresses).
3. Add the `include_str!` line in `system_prompt.rs`.
4. Find or create a test trace that exercises the pattern and verify the model
   uses it correctly.

## Notes

- Pattern files should be concise — the model's system prompt has a token
  budget. Each pattern should be 10-30 lines, not a tutorial.
- The `pipeline` and `adversarial` patterns are stubs. They were identified
  from a trace analysis (session 6a118256) where the model launched all threads
  simultaneously instead of phasing them by dependency. The tools to support
  phased execution already exist (`episodes` parameter, documents); the model
  just needs the pattern made salient.
