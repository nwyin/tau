# Fuzzy Match Corpus

JSON test corpora for fuzzy match benchmarking. Two types:

## Accuracy corpus (`synthetic.json`)

Near-miss edit perturbations. Each case: `(file_content, old_string, ground_truth)`.

```json
{
  "id": "trailing-ws-001",
  "category": "trailing-ws",
  "corpus_type": "accuracy",
  "file_content": "fn foo() {  \n    bar();\n}\n",
  "old_string": "fn foo() {\n    bar();\n}",
  "ground_truth": {
    "start_line": 0,
    "end_line": 2,
    "matched_text": "fn foo() {  \n    bar();\n}"
  }
}
```

Categories: `trailing-ws`, `indent-shift`, `tabs-vs-spaces`, `unicode-punct`, `partial-block`

## Adversarial corpus (`adversarial.json`)

Repetitive-structure files where fuzzy matching could pick the wrong location.
Records all candidate locations so the runner can score wrong-location matches.

```json
{
  "id": "ambig-struct_i-0001",
  "category": "struct-impls",
  "corpus_type": "adversarial",
  "file_content": "... file with repeated similar blocks ...",
  "old_string": "... perturbed version of block 3 ...",
  "ground_truth": {
    "target_index": 2,
    "all_candidates": [
      { "start_line": 10, "end_line": 18, "similarity": 0.85 },
      { "start_line": 30, "end_line": 38, "similarity": 0.92 }
    ],
    "matched_text": "... exact text of block 3 ..."
  }
}
```

## Generation

```bash
python generate_corpus.py accuracy ../../coding-agent/src -o corpus/synthetic.json
python generate_corpus.py adversarial ../../coding-agent/src -o corpus/adversarial.json
```
