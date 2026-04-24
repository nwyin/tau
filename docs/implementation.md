# Implementation Status

This page records implementation-level status for the current tau runtime. It is a short, current-state companion to the architecture overview and benchmark docs.

## Fixed Correctness Issues

These issues were fixed after the April 24, 2026 implementation audit.

### `py_repl` Tool Dispatch Respects Permissions

`tau.tool()` and `tau.parallel(tau.Tool(...))` now dispatch through the configured, permission-wrapped direct tool set for the current session. They no longer reconstruct tools from `tools::all_known_tools()`.

Implications:

- Allowlisting `py_repl` no longer reopens built-in tools that the session did not otherwise expose.
- Permission wrappers are preserved for generic tool calls made from Python.
- First-class orchestration operations remain explicit through `tau.thread`, `tau.query`, `tau.document`, `tau.launch`, `tau.poll`, and `tau.wait`.

### Reused-Thread Episode Injection Applies Immediately

Reusing a thread alias with valid `episodes` now applies the newly built system prompt to the current invocation. Previously, the stored prompt was updated, but the current reused-thread call still received the old prompt, making episode context appear one reuse late.

The stored prompt is only updated when prior episodes were actually injected, so missing episode aliases do not rewrite the prompt.

### Serve-Mode Session Results Match Benchmark Expectations

`tau serve` terminal `session.status` notifications now include the fields expected by `benchmarks/shared/session.py`:

```json
{
  "status": { "type": "idle" },
  "usage": {
    "input_tokens": 11,
    "output_tokens": 7,
    "tool_calls": 2
  },
  "output": "assistant text"
}
```

Serve mode now reports per-send usage deltas and counts observed tool executions. Error notifications include `error` plus the same usage shape.

## Verification

The fixes are covered by targeted regression tests for:

- permission-aware `py_repl` generic dispatch;
- reused-thread prompt selection with injected episodes;
- serve-mode output, token usage, and tool-call serialization.

Current verification commands:

```sh
cargo test --workspace
uvx pytest benchmarks/shared/session_test.py
```
