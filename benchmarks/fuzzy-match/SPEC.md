# Fuzzy Match Benchmark

Phase: 1 | Type: offline | Cost: $0 | Time: <10 seconds per run

## What it measures

Precision, recall, and safety of string-matching strategies for `file_edit`'s
replace mode. Two corpus types test different properties:

- **Accuracy corpus**: synthetic near-miss perturbations (trailing whitespace,
  indent shifts, tabs-vs-spaces, unicode punctuation, partial blocks). Measures
  how well each matcher recovers from common model mistakes.

- **Adversarial corpus**: files with high structural repetition (similar match
  arms, test functions, config blocks). Measures false-positive risk — whether
  a matcher picks the wrong location in ambiguous code.

No model in the loop — pure computation on pre-generated corpora.

## Why it matters for tau

tau's `file_edit` uses a trimmed-cascade fuzzy matcher (exact -> trim_end ->
trim_both -> unicode normalize). This benchmark validates that the strategy
recovers near-miss edits without introducing wrong-location false positives.

## Matchers

| Matcher | Source | Strategy |
|---------|--------|----------|
| `exact` | tau (pre-fuzzy) | Pure string match |
| `normalized` | pi-mono | Trailing-ws + unicode normalization, 2-pass |
| `trimmed-cascade` | codex / tau current | 4-pass: exact -> trim_end -> trim_both -> unicode |
| `levenshtein-80` | oh-my-pi (aggressive) | Sliding window Levenshtein, 80% threshold |
| `levenshtein-92` | oh-my-pi (balanced) | Sliding window Levenshtein, 92% threshold |
| `levenshtein-95` | oh-my-pi (conservative) | Sliding window Levenshtein, 95% threshold |

## Corpus generation

```bash
# Accuracy corpus (near-miss perturbations)
python generate_corpus.py accuracy ../../coding-agent/src -o corpus/synthetic.json --lang rust

# Adversarial corpus (repetitive-structure stress test)
python generate_corpus.py adversarial ../../coding-agent/src ~/projects/oh-my-pi/packages \
    -o corpus/adversarial.json --max-cases 150
```

## Running

```bash
# Run one corpus
python run.py corpus/synthetic.json

# Run both (prints accuracy scorecard then safety scorecard)
python run.py corpus/synthetic.json corpus/adversarial.json

# Specific matchers
python run.py corpus/synthetic.json --matchers exact trimmed-cascade

# JSON output
python run.py corpus/synthetic.json --json -o results/report.json
```

## Metrics

**Accuracy scorecard** (per matcher x category):
- True positive rate: correct / total
- False positive rate: wrong-location / total
- Category breakdown: which strategy helps for which failure mode

**Safety scorecard** (per matcher):
- Wrong-location rate: if >1% for any strategy, it's unsafe for production
- Safety ratio: (correct + rejected) / total. Target: >99%

## Architecture

```
fuzzy-match/
├── SPEC.md
├── generate_corpus.py   # Both accuracy and adversarial corpus generation
├── matchers.py          # Matching strategy implementations
├── run.py               # Unified runner with auto-detected scorecards
└── corpus/
    ├── README.md
    ├── synthetic.json    # Accuracy corpus (gitignored)
    └── adversarial.json  # Adversarial corpus (gitignored)
```
