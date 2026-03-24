---
name: sitrep
description: Produce a fast situational report on the current repo — what it is, where it's at, what's in flight, and what the likely next moves are. Use when the user says "sitrep", "what's going on here", "catch me up", "where was I", "what's the state of this repo", "orient me", or when context-switching into a project. Also useful as a pre-step before planning work to understand current state.
---

# Repo Sitrep

Produce a concise situational report on the repository rooted at the current working directory. The goal is to orient a developer who is context-switching into this project — or to prime an agent with the context it needs before starting work.

You have a token budget but no time pressure. Be thorough in your research, concise in your output.

## Phase 1: Gather (parallel where possible)

Run all of the following in parallel using subagents or parallel tool calls. Do not skip any category — missing data is itself a signal.

### 1a. Identity and purpose
- Read README.md (or README.rst, README.txt) if it exists
- Read CLAUDE.md / .claude/CLAUDE.md if it exists
- Read pyproject.toml, package.json, Cargo.toml, or equivalent project manifest
- Read any ARCHITECTURE.md, CONTRIBUTING.md, or docs/index.md
- Identify: project name, one-line description, language/stack, intended audience

### 1b. Structure
- List top-level directory contents
- Glob for source entry points (main.py, src/lib.rs, src/index.ts, etc.)
- Count files and rough LOC by language (use `tokei` if available, otherwise `find | wc -l`)
- Identify test locations and test framework
- Note any monorepo structure (workspaces, packages/)

### 1c. Git state
- `git log --oneline -20` — recent commit history
- `git log --oneline --since="2 weeks ago"` — velocity/recency
- `git branch -a` — all branches, identify which look active vs stale
- `git status` — uncommitted changes (staged and unstaged)
- `git stash list` — any stashed work
- `git diff --stat` — what's dirty right now
- If there are uncommitted changes, `git diff` to understand what's in flight

### 1d. CI / GitHub state
- Check for .github/workflows/ — what CI exists?
- If `gh` CLI is available and repo has a remote:
  - `gh pr list` — open PRs
  - `gh issue list` — open issues
  - `gh run list -L 5` — recent CI runs and their status

### 1e. Dependencies and health
- Check for lockfiles (uv.lock, Cargo.lock, bun.lockb, package-lock.json, etc.)
- If Rust: `cargo outdated` or check Cargo.toml versions
- If Python/uv: `uv pip list --outdated` or similar
- If JS/TS: check for outdated deps
- Look for TODO/FIXME/HACK/XXX comments: `rg -c "TODO|FIXME|HACK|XXX"` (just counts per file)

### 1f. Documentation and specs
- Glob for any spec files, design docs, or ADRs (docs/, specs/, decisions/, *.spec.md)
- Check for CHANGELOG.md or HISTORY.md
- Look for any .md files in the repo root beyond README

## Phase 2: Synthesize

From the gathered data, produce a structured report. Do NOT just dump raw tool output. Interpret and synthesize.

### Output format

```
# Sitrep: {project name}

## What this is
{One paragraph. What the project does, who it's for, what stack it uses.
Mention the repo's maturity: greenfield, active development, mature/stable, stale.}

## Current state
- **Last commit**: {date, relative} — "{message}"
- **Branch**: {current branch} ({N total branches})
- **Dirty**: {yes/no — if yes, summarize what's uncommitted}
- **CI**: {passing/failing/none}
- **Open PRs**: {count, with titles if few}
- **Open issues**: {count, with titles if few}

## What's in flight
{Based on recent commits, branch names, uncommitted changes, and open PRs,
deduce what work is currently happening. What was the developer working on
when they last touched this? What branches look like active feature work?}

## Architecture snapshot
{Brief description of how the code is organized. Entry points, main modules,
key abstractions. Not a full architecture doc — just enough to orient.}

## Health signals
- **Dependencies**: {clean / N outdated — list names if <10}
- **TODOs**: {count across codebase, hotspot files if clustered}
- **Test coverage**: {exists/doesn't exist, framework, rough extent}
- **Stale branches**: {list any that look mergeable or deletable}

## Likely next moves
{Based on everything above, what are the 2-4 most natural next actions?
Be specific and opinionated. Examples:
- "Finish the feature on branch X and merge"
- "The uncommitted changes look like a half-done refactor of Y — decide whether to commit or discard"
- "Bump the 3 outdated deps and run tests"
- "This has no CI — adding a basic GitHub Actions workflow would be high-value"
Don't suggest things that aren't grounded in what you found.}
```

## Guidelines

- If a section has no data (e.g., no CI, no issues), say so in one line and move on. Don't pad.
- "Likely next moves" is the most valuable section. Spend your reasoning budget there.
- If you find CLAUDE.md or PM memory files, use them to inform your interpretation — they contain the developer's own context about the project.
- Be dense, not verbose. No padding, no filler — but don't truncate meaningful findings to hit an arbitrary word count. Let the complexity of the repo dictate the length.
- Do not use emoji.
- Write in second person ("you were working on..." not "the developer was working on...").
