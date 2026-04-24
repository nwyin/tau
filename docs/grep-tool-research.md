# Grep and Glob Tool Research

Issue #7 was opened when tau only had bash plus file tools. That premise is now stale: tau already ships separate `grep` and `glob` tools in the default tool set, marks both read-only in permissions, and tells the model to prefer them over bash `grep`/`rg`/`find`/`ls`.

This document compares the current implementation against the requested agent references and recommends incremental improvements rather than a new first implementation.

## Current tau behavior

Source: `coding-agent/src/tools/grep.rs`, `coding-agent/src/tools/glob.rs`, `coding-agent/src/tools/mod.rs`, `coding-agent/src/system_prompt.rs`, `coding-agent/tests/grep_test.rs`, `coding-agent/tests/glob_test.rs`.

### `grep`

Schema:

```json
{
  "pattern": "string, required",
  "path": "string, optional",
  "glob": "string, optional",
  "ignore_case": "boolean, optional",
  "context": "number, optional",
  "limit": "number, optional"
}
```

Behavior:

- Shells out to `rg` with `-n --color=never --no-heading`.
- Supports regex, optional path, optional ripgrep `--glob`, case-insensitive search, symmetric context via `-C`, and a default display limit of 100 lines.
- Returns text in ripgrep line format plus structured details: `pattern`, `path`, `glob`, `match_count`, `files_with_matches`, `truncated`.
- Times out after 30 seconds and supports cancellation.
- Treats `rg` exit code 1 as "No matches found"; returns stderr for `rg` errors.

Gaps:

- No `output_mode` for `content` vs `files_with_matches` vs `count`.
- No `literal`/fixed-string mode.
- No language/type filter equivalent to `rg --type`.
- Only symmetric `context`, not separate before/after context.
- No multiline search option.
- `--max-count` is per file in ripgrep, so tau still buffers all stdout before applying the global display `limit`.
- Long individual lines are not truncated.
- Requires `rg` on PATH; there is no bundled/downloaded fallback.
- Details count returned output lines, not the true total if ripgrep itself is capped.

### `glob`

Schema:

```json
{
  "pattern": "string, required",
  "path": "string, optional"
}
```

Behavior:

- Uses `globset` plus the `ignore` crate.
- Matches files under an optional root path, returns relative paths, respects `.gitignore`, sorts by mtime descending, and caps display at 1000 results.
- Returns structured details: `pattern`, `root`, `result_count`, `truncated`.

Gaps:

- No explicit `limit` parameter.
- No cancellation token use while walking.
- No option to include hidden/ignored files.
- Pattern semantics are relative to the search root, which is good, but the tool description should say that directly.

## Reference comparison

| Agent | Content search | File search | Notes |
| --- | --- | --- | --- |
| Claude Code / claw-code | Separate grep-like tool with rich shape | Separate glob-like tool | Issue text describes Claude Code `Grep` as ripgrep-based with `output_mode`, context, type filter, and count. Local claw-code/tau comparison notes the same split; local claw-code source has `grep_search` fields for `output_mode`, `-B`, `-A`, `-C`, `-n`, `-i`, `type`, `head_limit`, `offset`, and `multiline`. |
| Codex CLI | No active dedicated content grep in inspected current source | Experimental `list_dir`, shell otherwise | The local Codex checkout at `c4d9887f` registers shell tools and optional `list_dir`; I did not find an active registered `grep_files` handler. Older/local tau comparison docs mention `grep_files`, so this may have changed. Codex also bundles `rg` in npm packages, suggesting shell `rg` remains available. |
| aider | No model-facing grep/glob tools | User commands use glob; model gets repo map | Aider relies on a repo map built from tree-sitter tags via `grep_ast`, ranking symbols/files into prompt context. `/add` and read-only commands use Python glob expansion, but search is user-command/navigation infrastructure rather than a callable model tool. |
| pi-mono | `grep` using `rg --json` | `find` using `fd --glob` | `grep` supports regex/literal, glob, ignore case, context, limit, long-line truncation, byte truncation, and custom operations for remote backends. `find` supports glob pattern, path, limit, gitignore handling, and byte truncation. |
| opencode | `grep` using ripgrep | `glob` using ripgrep files | Separate tools. `grep` supports regex, path, include glob, mtime-sorted matches, 100-match cap, 2000-char line cap, and a description that tells models to use bash `rg` for counts. `glob` returns mtime-sorted paths with a 100-result cap and encourages batching multiple searches. |

Source basis:

- tau worktree: current branch `issue-7`.
- `openai/codex` local reference checkout at `c4d9887f`.
- `badlogic/pi-mono` local reference checkout at `2f8019b6`.
- `anomalyco/opencode` local reference checkout at `0a7dfc0`.
- `instructkr/claw-code` local reference checkout at `9ade3a7`.
- `Aider-AI/aider` shallow source checkout at `0189cf4`.

## Design answers

### One tool or two?

Keep two tools: `grep` for content and `glob` for file discovery.

This matches Claude Code, pi-mono, opencode, and tau's current model-visible names. The concepts differ enough that a single `search` tool would need a mode switch and would make descriptions more ambiguous. Separate names also let the system prompt give simple guidance: use `glob` to find files by pattern, use `grep` to search file contents.

### ripgrep vs built-in search?

Keep ripgrep for `grep`; keep native Rust for `glob`.

Ripgrep gives the behavior agents expect: regex support, `.gitignore`, fast traversal, binary-file handling, file globs, and type filters. Reimplementing content search in Rust would mostly recreate ripgrep with higher maintenance risk. The main downside is the external binary requirement, which can be mitigated by startup checks, clearer errors, or bundling later.

Native Rust remains fine for `glob` because tau already has a small gitignore-aware implementation with deterministic sorting and good tests. Switching file glob to `rg --files --glob` is optional, not urgent.

### Output format and truncation

Recommended default:

- `grep` should default to content lines, because that is what agents need before deciding what to read.
- Add `output_mode` with `content`, `files_with_matches`, and `count`.
- Keep `limit`, but implement it as a global streaming cap rather than `rg --max-count`.
- Truncate long lines to a fixed width, around 500-2000 chars.
- Include a final notice that says what cap was hit and how to refine or raise the limit.
- Do not add hashline annotations to grep output. Grep output is navigation evidence; the model should still call `file_read` before editing. Hashes belong in `file_read`/`file_edit`, not search snippets.

### Tool descriptions

The current tau descriptions are accurate but too thin. The peer tools that look strongest tell the model when to use the tool, what the cap is, and when to switch tools.

Recommended `grep` description:

> Search file contents using ripgrep. Use for finding symbols, strings, errors, or references before reading files. Supports regex patterns, optional path and file glob filters, case-insensitive search, context lines, and result limits. Returns matching lines with file paths and line numbers. Prefer this over bash grep/rg for code search; use bash rg only for unsupported ripgrep flags or custom pipelines.

Recommended `glob` description:

> Find files by glob pattern relative to a search root. Use for locating files by name or extension before reading them. Respects `.gitignore`, returns matching file paths sorted newest first, and caps large result sets. Prefer this over bash find/ls for file discovery.

## Recommendation

Do not replace tau's grep/glob pair. Open a follow-up implementation issue to harden and enrich the existing tools.

Priority order:

1. Improve `grep` output modes: add `output_mode: "content" | "files_with_matches" | "count"`, keeping `content` as the default.
2. Stream ripgrep output using `--json`, stop once the global `limit` is reached, and avoid buffering unbounded stdout.
3. Add `literal`, `type`, `before_context`, `after_context`, and `multiline` fields.
4. Add long-line truncation and clearer truncation notices in both text and `details`.
5. Add `limit` to `glob` and wire cancellation into the blocking walk.
6. Expand tests around true global limiting, output modes, literal search, type filtering, long-line truncation, invalid regex, and missing `rg`.
7. Optionally add an `rg` availability check at startup or an actionable error message that tells the user how to install ripgrep.

## Proposed follow-up schema

`grep`:

```json
{
  "type": "object",
  "properties": {
    "pattern": { "type": "string", "description": "Regex pattern to search for, or literal text when literal=true." },
    "path": { "type": "string", "description": "Directory or file to search in. Defaults to cwd." },
    "glob": { "type": "string", "description": "File glob filter, e.g. '*.rs' or '*.{ts,tsx}'." },
    "type": { "type": "string", "description": "Ripgrep file type filter, e.g. 'rust', 'ts', or 'py'." },
    "output_mode": { "type": "string", "enum": ["content", "files_with_matches", "count"], "description": "Result shape. Defaults to content." },
    "ignore_case": { "type": "boolean", "description": "Case-insensitive search. Defaults to false." },
    "literal": { "type": "boolean", "description": "Treat pattern as fixed text instead of regex. Defaults to false." },
    "context": { "type": "number", "description": "Lines before and after each match. Defaults to 0." },
    "before_context": { "type": "number", "description": "Lines before each match. Overrides context before side." },
    "after_context": { "type": "number", "description": "Lines after each match. Overrides context after side." },
    "multiline": { "type": "boolean", "description": "Enable multiline regex search. Defaults to false." },
    "limit": { "type": "number", "description": "Maximum results to return. Defaults to 100." }
  },
  "required": ["pattern"]
}
```

`glob`:

```json
{
  "type": "object",
  "properties": {
    "pattern": { "type": "string", "description": "Glob pattern relative to the search root, e.g. '**/*.rs' or 'src/**/*.ts'." },
    "path": { "type": "string", "description": "Root directory to search from. Defaults to cwd." },
    "limit": { "type": "number", "description": "Maximum file paths to return. Defaults to 1000." }
  },
  "required": ["pattern"]
}
```

## Implementation plan

1. Extend `GrepTool` parameters and parse defaults without breaking existing calls.
2. Build ripgrep args from schema fields:
   - `--json --line-number --color=never`
   - `--fixed-strings` for `literal`
   - `--ignore-case` for `ignore_case`
   - `--glob <glob>` and `--type <type>` when provided
   - `-C`, `-B`, and `-A` for context controls
   - `-U` for multiline if needed
   - `--files-with-matches` or `--count-matches` for non-content modes, or parse JSON for content mode
3. Stream stdout line-by-line and stop the child once the global display cap is reached.
4. Track true displayed counts separately from "limit reached" metadata.
5. Format content output as `path:line: text` and context output as `path-line- text`, matching familiar grep conventions.
6. Cap long lines and include a notice such as `[100 matches limit reached. Refine pattern or raise limit.]`.
7. Add `limit` and cancellation checks to `GlobTool`.
8. Update system prompt/tool descriptions and tests.

## Acceptance checks for the follow-up

- Existing `grep` and `glob` tests continue to pass.
- New tests cover output modes, literal matching, type filters, before/after context, global limit semantics across many files, long-line truncation, and cancellation.
- A prompt-mode smoke test can find a symbol with `grep`, locate candidate files with `glob`, and then read the relevant file without using bash search commands.
