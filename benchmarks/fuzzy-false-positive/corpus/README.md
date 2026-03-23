# Adversarial Corpus Format

JSON files in this directory contain adversarial test cases designed to
trigger false positives in fuzzy string matchers. Each case features a
file with structurally repetitive blocks where a perturbation of one block
could plausibly match a different block.

## Schema

Each corpus file is a JSON array of case objects:

```json
{
  "id": "ambig-similar--0042",
  "category": "similar-jsx",
  "source_file": "components/Card.tsx",
  "file_content": "... full file with repeated similar blocks ...",
  "old_string": "... perturbed version of target block ...",
  "ground_truth": {
    "target_index": 2,
    "all_candidates": [
      { "start_line": 10, "end_line": 18, "similarity": 0.85 },
      { "start_line": 30, "end_line": 38, "similarity": 0.92 },
      { "start_line": 50, "end_line": 58, "similarity": 0.88 }
    ],
    "matched_text": "... exact text of the intended target block ..."
  },
  "notes": "renamed 'onClick' -> 'onClick_v2'"
}
```

## Fields

| Field | Description |
|-------|-------------|
| `id` | Unique case identifier |
| `category` | Adversarial category (similar-jsx, migration-sql, test-suite, config-blocks, css-selectors, match-arms, struct-impls) |
| `source_file` | Relative path to the original source file |
| `file_content` | Full file content containing the repetitive blocks |
| `old_string` | Perturbed version of one block — the "search string" a matcher receives |
| `ground_truth.target_index` | Index into `all_candidates` identifying the intended match |
| `ground_truth.all_candidates` | Every block in the file that a fuzzy matcher might plausibly match, with line ranges and similarity scores relative to `old_string` |
| `ground_truth.matched_text` | Exact text of the intended target block |
| `notes` | Description of the perturbation applied |

## Generation

```bash
python generate.py <source_dirs...> -o corpus/adversarial.json --max-cases 150 --seed 42
```

The generator scans source directories for files with high structural
repetition, clusters similar blocks within each file, and creates
perturbations that target one block but could plausibly match others.

## Categories

| Category | Source pattern | Why it is dangerous |
|----------|---------------|---------------------|
| similar-jsx | React components with near-identical JSX blocks | Common in component libraries |
| migration-sql | ALTER TABLE / CREATE TABLE blocks differing by name | Database migrations |
| test-suite | Similar test cases with different assertions | Test files |
| config-blocks | Repeated TOML/YAML/JSON sections | Configuration |
| css-selectors | Similar CSS rules with different selectors | Stylesheets |
| match-arms | Rust match arms or switch cases | Pattern matching code |
| struct-impls | Similar impl blocks for different types | Rust trait implementations |
