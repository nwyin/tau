# Fuzzy Edit: Match Accuracy

Phase: 1 | Type: offline | Cost: $0 | Time: <10 seconds per run

## What it measures

Precision and recall of string-matching strategies on a corpus of near-miss
edit attempts. No model in the loop — pure computation on pre-generated triples.

## Why it matters for tau

tau requires exact string match for `file_edit` in replace mode. Models
frequently produce near-miss `old_string` values (wrong whitespace, unicode
punctuation, stale content). This benchmark quantifies how much each fuzzy
strategy recovers vs how much false-positive risk it introduces.

## Status

**Scaffolded.** Core files exist:
- `generate_corpus.py` — synthetic perturbation generator
- `matchers.py` — 6 matching strategies
- `run.py` — runner with scorecard output
- `corpus/README.md` — fixture format docs

## Prerequisites

None. Zero-cost, runs locally.

## Fixtures

JSON corpus of `(file_content, old_string, ground_truth)` triples.
See `corpus/README.md` for format.

### Corpus sources (in priority order)

1. **Synthetic perturbations** (implemented): extract blocks from real source
   files, apply systematic perturbations. Categories: `trailing-ws`,
   `indent-shift`, `tabs-vs-spaces`, `unicode-punct`, `partial-block`.

2. **Real model failures** (not yet): mine tau and oh-my-pi benchmark traces
   for cases where `file_edit` returned "old_string not found". The model's
   `old_string` attempt becomes the test input.

3. **Adversarial negatives** (not yet): files with high structural repetition
   where old_string is close to 2+ locations. Shared with `fuzzy-false-positive`.

### Corpus generation

```bash
# Generate from tau's own source
uv run python generate_corpus.py ../../coding-agent/src -o corpus/synthetic.json --lang rust

# Generate from a TypeScript project
uv run python generate_corpus.py ~/projects/oh-my-pi/packages -o corpus/ts.json --lang typescript
```

Target: 200-500 cases across all categories.

## Matchers

Each matcher: `(content: str, old_string: str) -> list[Match]`

| Matcher | Source | Implemented |
|---------|--------|-------------|
| `exact` | tau current | yes |
| `normalized` | pi-mono | yes |
| `trimmed-cascade` | codex | yes |
| `levenshtein-80` | oh-my-pi (aggressive) | yes |
| `levenshtein-92` | oh-my-pi (balanced) | yes |
| `levenshtein-95` | oh-my-pi (conservative) | yes |
| `opencode-9` | opencode 9-strategy chain | no |
| `indent-aware` | oh-my-pi indent normalization | no |
| `comment-strip` | oh-my-pi comment prefix strip | no |

## Procedure

```bash
# 1. Generate corpus
uv run python generate_corpus.py ../../coding-agent/src -o corpus/synthetic.json

# 2. Run all matchers
uv run python run.py corpus/synthetic.json

# 3. Run specific matchers
uv run python run.py corpus/synthetic.json --matchers exact normalized levenshtein-92

# 4. JSON output for analysis
uv run python run.py corpus/synthetic.json --json -o results/report.json
```

## Metrics

Per matcher x category:
- **True positive rate** = correct matches / total matchable cases
- **False positive rate** = wrong-location matches / total matches
- **Ambiguous rate** = multi-match cases / total matches
- **Net value** = TP - (FP * penalty_weight), penalty_weight >> 1

Aggregate:
- **Category breakdown**: which strategy helps most for which failure mode
- **Timing**: mean, P50, P99 microseconds per case

## Decision it informs

1. Should tau add fuzzy matching at all?
2. Which strategies have zero (or near-zero) false positive rate?
3. Is trailing-ws-only sufficient? (Hypothesis: captures 80% of recoverable
   failures at 0% FP risk.)
4. Priority ordering: fuzzy matching vs hashline improvements

## Architecture

```
fuzzy-match/
├── SPEC.md
├── generate_corpus.py   # Corpus generation from source files
├── matchers.py          # Matching strategy implementations
├── run.py               # Runner + scorecard
└── corpus/
    ├── README.md
    └── *.json           # Generated corpora (gitignored)
```

All Python, no shared infrastructure needed. ~400 LOC total (currently ~350).

## Next steps

1. Add `opencode-9` matcher (port 9-strategy chain from opencode)
2. Add `indent-aware` and `comment-strip` matchers from oh-my-pi
3. Mine real model failure cases from benchmark traces
4. Expand corpus to TypeScript and Python sources
5. Cross-reference results with `fuzzy-false-positive` audit
