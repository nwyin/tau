# Fuzzy Edit: False Positive Audit

Phase: 1 | Type: offline | Cost: $0 | Time: <10 seconds per run

## What it measures

How often fuzzy matching applies an edit to the **wrong location** in files
with repetitive structure. This is the worst-case scenario for fuzzy matching:
a confident match at the wrong place silently corrupts code.

## Why it matters for tau

A false positive in fuzzy matching is strictly worse than a failed match.
A failed match causes a retry (the model re-reads the file and tries again).
A false positive causes a silent wrong edit that may not surface until tests
run (or worse, production). This benchmark establishes the **safety floor**
for any fuzzy strategy before tau adopts it.

## Prerequisites

- `fuzzy-match/matchers.py` (imports matching strategies from there)

## Fixtures

### Adversarial corpus format

Same JSON format as `fuzzy-match/corpus/`, but cases are specifically designed
to have multiple plausible match locations.

```json
{
  "id": "ambig-react-001",
  "category": "similar-jsx",
  "file_content": "... file with repeated JSX blocks ...",
  "old_string": "... perturbed version of block 3 ...",
  "ground_truth": {
    "target_index": 2,
    "all_candidates": [
      { "start_line": 10, "end_line": 18, "similarity": 0.85 },
      { "start_line": 30, "end_line": 38, "similarity": 0.92 },
      { "start_line": 50, "end_line": 58, "similarity": 0.88 }
    ],
    "matched_text": "... exact text of block 3 ..."
  }
}
```

The `all_candidates` field records every location that a fuzzy matcher might
plausibly match, with the ground-truth `target_index` indicating the intended
one. This lets us score whether a matcher picked the right candidate.

### Adversarial file categories

| Category | Description | Why it's dangerous |
|----------|-------------|-------------------|
| `similar-jsx` | React components with near-identical JSX blocks | Common in component libraries |
| `migration-sql` | ALTER TABLE blocks differing by table name | Database migrations |
| `test-suite` | Similar test cases with different assertions | Test files |
| `config-blocks` | Repeated TOML/YAML/JSON blocks | Configuration |
| `css-selectors` | Similar CSS rules with different selectors | Stylesheets |
| `match-arms` | Rust match arms or switch cases | Pattern matching |
| `struct-impls` | Similar impl blocks for different types | Rust trait impls |

### Corpus generation

```bash
uv run python generate.py \
    --adversarial \
    --sources ../../coding-agent/src ~/projects/oh-my-pi/packages \
    -o corpus/adversarial.json \
    --max-cases 150
```

The generator:
1. Scans for files with high structural repetition (repeated similar blocks)
2. For each file, identifies clusters of similar blocks
3. Creates 3-5 perturbations per cluster targeting one block but plausibly
   matching others
4. Records all candidate locations and their similarity scores

Target: 100-200 adversarial cases, 50+ files.

## Procedure

```bash
# 1. Generate adversarial corpus
uv run python generate.py --adversarial -o corpus/adversarial.json

# 2. Run audit
uv run python run.py corpus/adversarial.json

# 3. Detailed results
uv run python run.py corpus/adversarial.json --json -o results/audit.json
```

## Metrics

Per matcher:
- **Correct**: matched the intended location
- **Wrong location**: matched a different block (**the critical failure**)
- **Rejected**: no match found (safe — model retries)
- **Ambiguous-rejected**: multiple matches found, correctly refused

Key metrics:
- **Wrong-location rate** = wrong / total. Threshold: if >1% for any
  strategy, that strategy is unsafe for production use.
- **Safety ratio** = (correct + rejected) / total. Target: >99%.
- **Discrimination**: for each case, did the matcher pick the most-similar
  candidate? (Tests whether similarity scoring works correctly.)

Scorecard format:

```
Matcher             Cases  Correct  Wrong  Rejected  Ambig   Safety%
exact                 150      60       0        90      0    100.0%
normalized            150      85       2        63      0     98.7%
levenshtein-92        150     110       8        30      2     94.7%
levenshtein-95        150      95       3        50      2     98.0%
```

## Decision it informs

- **Is fuzzy matching safe enough?** If all strategies show >1% wrong-location
  rate on adversarial cases, tau should stay exact-only or limit to
  trailing-ws normalization.
- **Safety thresholds**: what Levenshtein threshold keeps wrong-location rate
  at 0%? This sets the floor for any fuzzy strategy we ship.
- **Combined with fuzzy-match results**: a strategy is viable only if it has
  high TP rate (from fuzzy-match) AND low FP rate (from this audit).

## Architecture

```
fuzzy-false-positive/
├── SPEC.md
├── generate.py       # Adversarial corpus generator
├── run.py            # Runner + safety scorecard
└── corpus/
    ├── README.md
    └── *.json        # Generated corpora (gitignored)
```

Imports matchers from `fuzzy-match/matchers.py`:
```python
sys.path.insert(0, str(Path(__file__).parent.parent / "fuzzy-match"))
from matchers import MATCHERS
```

Estimated LOC: ~300 (generate.py: ~200, run.py: ~100)
