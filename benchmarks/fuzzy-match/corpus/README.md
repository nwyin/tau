# Fuzzy Match Corpus

JSON fixtures of `(file_content, old_string_attempt, ground_truth)` triples.

## File format

Each `.json` file is an array of test cases:

```json
[
  {
    "id": "trailing-ws-001",
    "category": "trailing-ws",
    "file_content": "fn foo() {  \n    bar();\n}\n",
    "old_string": "fn foo() {\n    bar();\n}",
    "ground_truth": {
      "start_line": 0,
      "end_line": 2,
      "matched_text": "fn foo() {  \n    bar();\n}"
    },
    "notes": "Model omitted trailing spaces on line 1"
  }
]
```

## Categories

- `trailing-ws` — trailing whitespace added/removed
- `indent-shift` — indentation level differs by 1-3 levels
- `tabs-vs-spaces` — tab/space substitution
- `unicode-punct` — smart quotes, em-dashes
- `stale-content` — content modified since model last read
- `partial-block` — old_string is a subset of the actual block
- `ambiguous` — old_string matches multiple locations
- `hallucinated` — old_string contains content not in the file

## Sources

1. Synthetic perturbations via `generate_corpus.py`
2. Real model failures mined from tau/oh-my-pi benchmark traces
3. Adversarial cases hand-crafted for false-positive testing
