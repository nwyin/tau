#!/usr/bin/env python3
"""Generate synthetic near-miss edit corpus from real source files.

Reads source files, extracts blocks, and applies systematic perturbations
to create (file_content, old_string_attempt, ground_truth) triples.

Usage:
    python generate_corpus.py <source_dir> -o corpus/synthetic.json
    python generate_corpus.py ../../coding-agent/src -o corpus/synthetic.json --lang rust
"""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path

LANG_EXTENSIONS = {
    "rust": [".rs"],
    "python": [".py"],
    "typescript": [".ts", ".tsx"],
    "javascript": [".js", ".jsx"],
}


def extract_blocks(content: str, min_lines: int = 3, max_lines: int = 15) -> list[tuple[int, int, str]]:
    """Extract contiguous blocks of code from file content.

    Returns list of (start_line, end_line, block_text).
    """
    lines = content.splitlines(keepends=True)
    blocks = []
    i = 0
    while i < len(lines):
        # Skip blank lines
        if not lines[i].strip():
            i += 1
            continue
        # Find end of non-blank block
        j = i
        while j < len(lines) and lines[j].strip():
            j += 1
        block_len = j - i
        if min_lines <= block_len <= max_lines:
            block_text = "".join(lines[i:j])
            blocks.append((i, j, block_text))
        i = j
    return blocks


# --- Perturbation functions ---
# Each takes a block string and returns (perturbed_string, category, notes).


def perturb_trailing_whitespace(block: str) -> tuple[str, str, str]:
    """Strip trailing whitespace from each line."""
    lines = block.splitlines(keepends=True)
    stripped = []
    for line in lines:
        stripped.append(line.rstrip() + "\n" if line.endswith("\n") else line.rstrip())
    result = "".join(stripped)
    if result == block:
        # Add trailing whitespace instead
        lines = block.splitlines(keepends=True)
        result = "".join(line.rstrip("\n") + "  \n" if line.endswith("\n") else line + "  " for line in lines)
    return result, "trailing-ws", "Trailing whitespace stripped/added"


def perturb_indent_shift(block: str, levels: int = 1) -> tuple[str, str, str]:
    """Shift indentation by N levels (4 spaces per level)."""
    indent = "    " * levels
    lines = block.splitlines(keepends=True)
    shifted = [indent + line if line.strip() else line for line in lines]
    return "".join(shifted), "indent-shift", f"Indentation shifted +{levels} levels"


def perturb_indent_unshift(block: str) -> tuple[str, str, str]:
    """Remove one level of indentation."""
    lines = block.splitlines(keepends=True)
    unshifted = []
    for line in lines:
        if line.startswith("    "):
            unshifted.append(line[4:])
        elif line.startswith("\t"):
            unshifted.append(line[1:])
        else:
            unshifted.append(line)
    result = "".join(unshifted)
    if result == block:
        return block, "indent-shift", "No indentation to remove (skip)"
    return result, "indent-shift", "Indentation removed 1 level"


def perturb_tabs_to_spaces(block: str) -> tuple[str, str, str]:
    """Convert leading tabs to spaces or vice versa."""
    if "\t" in block:
        return block.replace("\t", "    "), "tabs-vs-spaces", "Tabs converted to spaces"
    lines = block.splitlines(keepends=True)
    converted = []
    for line in lines:
        stripped = line.lstrip(" ")
        n_spaces = len(line) - len(stripped)
        if n_spaces >= 4:
            tabs = "\t" * (n_spaces // 4) + " " * (n_spaces % 4)
            converted.append(tabs + stripped)
        else:
            converted.append(line)
    result = "".join(converted)
    if result == block:
        return block, "tabs-vs-spaces", "No conversion possible (skip)"
    return result, "tabs-vs-spaces", "Spaces converted to tabs"


def perturb_unicode_punct(block: str) -> tuple[str, str, str]:
    """Replace ASCII quotes/dashes with Unicode equivalents."""
    replacements = [
        ('"', "\u201c"),  # left double quote
        ("'", "\u2018"),  # left single quote
        ("--", "\u2014"),  # em-dash
        ("-", "\u2013"),  # en-dash (only standalone hyphens)
    ]
    result = block
    applied = []
    for ascii_char, unicode_char in replacements[:2]:  # just quotes
        if ascii_char in result:
            result = result.replace(ascii_char, unicode_char, 2)  # max 2 replacements
            applied.append(f"{ascii_char} → {unicode_char}")
    if not applied:
        return block, "unicode-punct", "No punctuation to replace (skip)"
    return result, "unicode-punct", f"Unicode replacement: {', '.join(applied)}"


def perturb_partial_block(block: str) -> tuple[str, str, str]:
    """Return only the middle portion of the block."""
    lines = block.splitlines(keepends=True)
    if len(lines) < 4:
        return block, "partial-block", "Block too short (skip)"
    # Drop first and last line
    partial = "".join(lines[1:-1])
    return partial, "partial-block", "First and last lines removed"


ALL_PERTURBATIONS = [
    perturb_trailing_whitespace,
    lambda b: perturb_indent_shift(b, 1),
    perturb_indent_unshift,
    perturb_tabs_to_spaces,
    perturb_unicode_punct,
    perturb_partial_block,
]


def generate_cases(source_dir: Path, lang: str | None = None, max_cases: int = 200) -> list[dict]:
    """Generate test cases from source files in a directory."""
    extensions = []
    if lang:
        extensions = LANG_EXTENSIONS.get(lang, [f".{lang}"])
    else:
        for exts in LANG_EXTENSIONS.values():
            extensions.extend(exts)

    source_files = []
    for ext in extensions:
        source_files.extend(source_dir.rglob(f"*{ext}"))

    cases = []
    case_id = 0

    for source_file in sorted(source_files):
        try:
            content = source_file.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue

        blocks = extract_blocks(content)
        if not blocks:
            continue

        for start_line, end_line, block_text in blocks:
            for perturb_fn in ALL_PERTURBATIONS:
                perturbed, category, notes = perturb_fn(block_text)

                # Skip no-ops
                if perturbed == block_text or "(skip)" in notes:
                    continue

                cases.append(
                    {
                        "id": f"{category}-{case_id:04d}",
                        "category": category,
                        "source_file": str(source_file.relative_to(source_dir)),
                        "file_content": content,
                        "old_string": perturbed,
                        "ground_truth": {
                            "start_line": start_line,
                            "end_line": end_line,
                            "matched_text": block_text,
                        },
                        "notes": notes,
                    }
                )
                case_id += 1

                if len(cases) >= max_cases:
                    random.shuffle(cases)
                    return cases

    random.shuffle(cases)
    return cases


def main():
    parser = argparse.ArgumentParser(description="Generate fuzzy match test corpus")
    parser.add_argument("source_dir", type=Path, help="Directory of source files to extract blocks from")
    parser.add_argument("-o", "--output", type=Path, default=Path("corpus/synthetic.json"), help="Output JSON file")
    parser.add_argument("--lang", choices=list(LANG_EXTENSIONS.keys()), help="Filter by language")
    parser.add_argument("--max-cases", type=int, default=200, help="Maximum number of test cases")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    args = parser.parse_args()

    random.seed(args.seed)
    cases = generate_cases(args.source_dir, lang=args.lang, max_cases=args.max_cases)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(cases, indent=2, ensure_ascii=False))
    print(f"Generated {len(cases)} test cases → {args.output}")

    # Print category breakdown
    from collections import Counter

    counts = Counter(c["category"] for c in cases)
    for cat, n in counts.most_common():
        print(f"  {cat}: {n}")


if __name__ == "__main__":
    main()
