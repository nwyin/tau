#!/usr/bin/env python3
"""Generate test corpora for fuzzy match benchmarking.

Two modes:
  - accuracy: synthetic perturbations of real code blocks (near-miss edits)
  - adversarial: repetitive-structure files where fuzzy matching could pick the wrong location

Usage:
    # Accuracy corpus (near-miss edits)
    python generate_corpus.py accuracy ../../coding-agent/src -o corpus/synthetic.json

    # Adversarial corpus (false-positive stress test)
    python generate_corpus.py adversarial ../../coding-agent/src ~/projects/oh-my-pi/packages \
        -o corpus/adversarial.json --max-cases 150
"""

from __future__ import annotations

import argparse
import difflib
import json
import random
import re
from collections import Counter
from pathlib import Path

LANG_EXTENSIONS: dict[str, list[str]] = {
    "rust": [".rs"],
    "python": [".py"],
    "typescript": [".ts", ".tsx"],
    "javascript": [".js", ".jsx"],
    "sql": [".sql"],
    "css": [".css", ".scss"],
    "toml": [".toml"],
    "yaml": [".yaml", ".yml"],
    "json": [".json"],
}

ALL_EXTENSIONS: list[str] = [ext for exts in LANG_EXTENSIONS.values() for ext in exts]

SKIP_DIRS = {"target", "node_modules", "__pycache__", "dist", "build", "venv", ".venv", "vendor"}


# ---------------------------------------------------------------------------
# Block extraction
# ---------------------------------------------------------------------------


def extract_blocks(content: str, min_lines: int = 3, max_lines: int = 15) -> list[tuple[int, int, str]]:
    """Extract contiguous non-blank blocks from file content.

    Returns list of (start_line, end_line, block_text). Lines are 0-indexed.
    """
    lines = content.splitlines(keepends=True)
    blocks = []
    i = 0
    while i < len(lines):
        if not lines[i].strip():
            i += 1
            continue
        j = i
        while j < len(lines) and lines[j].strip():
            j += 1
        block_len = j - i
        if min_lines <= block_len <= max_lines:
            block_text = "".join(lines[i:j])
            blocks.append((i, j, block_text))
        i = j
    return blocks


def extract_blocks_newline_join(content: str, min_lines: int = 3, max_lines: int = 30) -> list[tuple[int, int, str]]:
    """Extract blocks using newline join (for adversarial corpus)."""
    lines = content.split("\n")
    blocks: list[tuple[int, int, str]] = []
    i = 0
    while i < len(lines):
        if not lines[i].strip():
            i += 1
            continue
        j = i
        while j < len(lines) and lines[j].strip():
            j += 1
        block_len = j - i
        if min_lines <= block_len <= max_lines:
            block_text = "\n".join(lines[i:j])
            blocks.append((i, j, block_text))
        i = j
    return blocks


# ---------------------------------------------------------------------------
# Source file collection
# ---------------------------------------------------------------------------


def collect_source_files(source_dirs: list[Path], extensions: list[str] | None = None) -> list[tuple[Path, Path]]:
    """Collect source files from directories. Returns (abs_path, source_dir) pairs."""
    if extensions is None:
        extensions = ALL_EXTENSIONS
    files: list[tuple[Path, Path]] = []
    for src_dir in source_dirs:
        src_dir = src_dir.resolve()
        if not src_dir.is_dir():
            continue
        for ext in extensions:
            for fp in sorted(src_dir.rglob(f"*{ext}")):
                parts = fp.relative_to(src_dir).parts
                if any(p.startswith(".") or p in SKIP_DIRS for p in parts):
                    continue
                files.append((fp, src_dir))
    return files


# ===========================================================================
# ACCURACY CORPUS — synthetic perturbations of real code
# ===========================================================================


def perturb_trailing_whitespace(block: str) -> tuple[str, str, str]:
    lines = block.splitlines(keepends=True)
    stripped = [line.rstrip() + "\n" if line.endswith("\n") else line.rstrip() for line in lines]
    result = "".join(stripped)
    if result == block:
        result = "".join(line.rstrip("\n") + "  \n" if line.endswith("\n") else line + "  " for line in lines)
    return result, "trailing-ws", "Trailing whitespace stripped/added"


def perturb_indent_shift(block: str, levels: int = 1) -> tuple[str, str, str]:
    indent = "    " * levels
    lines = block.splitlines(keepends=True)
    shifted = [indent + line if line.strip() else line for line in lines]
    return "".join(shifted), "indent-shift", f"Indentation shifted +{levels} levels"


def perturb_indent_unshift(block: str) -> tuple[str, str, str]:
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
    replacements = [
        ('"', "\u201c"),
        ("'", "\u2018"),
        ("--", "\u2014"),
        ("-", "\u2013"),
    ]
    result = block
    applied = []
    for ascii_char, unicode_char in replacements[:2]:
        if ascii_char in result:
            result = result.replace(ascii_char, unicode_char, 2)
            applied.append(f"{ascii_char} -> {unicode_char}")
    if not applied:
        return block, "unicode-punct", "No punctuation to replace (skip)"
    return result, "unicode-punct", f"Unicode replacement: {', '.join(applied)}"


def perturb_partial_block(block: str) -> tuple[str, str, str]:
    lines = block.splitlines(keepends=True)
    if len(lines) < 4:
        return block, "partial-block", "Block too short (skip)"
    partial = "".join(lines[1:-1])
    return partial, "partial-block", "First and last lines removed"


ACCURACY_PERTURBATIONS = [
    perturb_trailing_whitespace,
    lambda b: perturb_indent_shift(b, 1),
    perturb_indent_unshift,
    perturb_tabs_to_spaces,
    perturb_unicode_punct,
    perturb_partial_block,
]


def generate_accuracy_corpus(source_dirs: list[Path], lang: str | None = None, max_cases: int = 200, seed: int = 42) -> list[dict]:
    """Generate accuracy corpus: synthetic perturbations of real code blocks."""
    random.seed(seed)

    extensions = []
    if lang:
        extensions = LANG_EXTENSIONS.get(lang, [f".{lang}"])
    else:
        for exts in LANG_EXTENSIONS.values():
            extensions.extend(exts)

    source_files = []
    for src_dir in source_dirs:
        for ext in extensions:
            source_files.extend(src_dir.rglob(f"*{ext}"))

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
            for perturb_fn in ACCURACY_PERTURBATIONS:
                perturbed, category, notes = perturb_fn(block_text)
                if perturbed == block_text or "(skip)" in notes:
                    continue

                cases.append(
                    {
                        "id": f"{category}-{case_id:04d}",
                        "category": category,
                        "corpus_type": "accuracy",
                        "source_file": str(source_file.relative_to(source_dirs[0])),
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


# ===========================================================================
# ADVERSARIAL CORPUS — repetitive-structure false-positive stress test
# ===========================================================================

CATEGORY_HINTS: list[tuple[str, re.Pattern[str], re.Pattern[str] | None]] = [
    ("similar-jsx", re.compile(r"\.(tsx|jsx)$"), re.compile(r"<\w+[\s/>]")),
    ("migration-sql", re.compile(r"\.sql$"), re.compile(r"(?i)ALTER\s+TABLE|CREATE\s+TABLE")),
    ("test-suite", re.compile(r"(test|spec)\.", re.IGNORECASE), re.compile(r"(?:fn test_|def test_|it\(|describe\(|#\[test\])")),
    ("config-blocks", re.compile(r"\.(toml|yaml|yml|json)$"), None),
    ("css-selectors", re.compile(r"\.(css|scss)$"), re.compile(r"[.#]\w+\s*\{")),
    ("match-arms", re.compile(r"\.rs$"), re.compile(r"^\s*(match |=>)")),
    ("struct-impls", re.compile(r"\.rs$"), re.compile(r"^\s*impl\b")),
]


def infer_category(filepath: str, block_texts: list[str]) -> str:
    joined = "\n".join(block_texts[:5])
    for category, path_re, content_re in CATEGORY_HINTS:
        if path_re.search(filepath):
            if content_re is None or content_re.search(joined):
                return category
    return "config-blocks"


def block_similarity(a: str, b: str) -> float:
    return difflib.SequenceMatcher(None, a, b).ratio()


def find_similar_clusters(
    blocks: list[tuple[int, int, str]],
    min_similarity: float = 0.55,
    min_cluster_size: int = 2,
) -> list[list[int]]:
    n = len(blocks)
    if n < min_cluster_size:
        return []

    adj: dict[int, list[int]] = {i: [] for i in range(n)}
    for i in range(n):
        for j in range(i + 1, n):
            sim = block_similarity(blocks[i][2], blocks[j][2])
            if sim >= min_similarity:
                adj[i].append(j)
                adj[j].append(i)

    visited: set[int] = set()
    clusters: list[list[int]] = []
    for i in range(n):
        if i in visited or not adj[i]:
            continue
        component: list[int] = []
        stack = [i]
        while stack:
            node = stack.pop()
            if node in visited:
                continue
            visited.add(node)
            component.append(node)
            for nb in adj[node]:
                if nb not in visited:
                    stack.append(nb)
        if len(component) >= min_cluster_size:
            clusters.append(sorted(component))

    return clusters


# Adversarial perturbations (more structural than accuracy perturbations)


def adv_rename_identifier(block: str) -> tuple[str, str]:
    words = re.findall(r"\b([a-zA-Z_]\w{2,20})\b", block)
    if not words:
        return block, "no identifiers found"
    counts = Counter(words)
    target = counts.most_common(1)[0][0]
    renamed = target + "_v2" if len(target) < 18 else target[:-1]
    result = block.replace(target, renamed, 1)
    if result == block:
        return block, "rename had no effect"
    return result, f"renamed '{target}' -> '{renamed}'"


def adv_swap_line(block: str) -> tuple[str, str]:
    lines = block.split("\n")
    if len(lines) < 3:
        return block, "too short for line swap"
    idx = len(lines) // 2
    lines[idx], lines[idx + 1] = lines[idx + 1], lines[idx]
    return "\n".join(lines), f"swapped lines {idx} and {idx + 1}"


def adv_change_literal(block: str) -> tuple[str, str]:
    match = re.search(r'"([^"]{1,40})"', block)
    if match:
        old_lit = match.group(0)
        inner = match.group(1)
        new_inner = inner + "_modified" if len(inner) < 30 else inner[:10]
        new_lit = f'"{new_inner}"'
        return block.replace(old_lit, new_lit, 1), f"changed literal {old_lit} -> {new_lit}"
    match = re.search(r"\b(\d{1,6})\b", block)
    if match:
        old_num = match.group(1)
        new_num = str(int(old_num) + 1)
        return block.replace(old_num, new_num, 1), f"changed number {old_num} -> {new_num}"
    return block, "no literals found"


def adv_add_line(block: str) -> tuple[str, str]:
    lines = block.split("\n")
    if len(lines) < 2:
        return block, "too short"
    mid = len(lines) // 2
    indent = re.match(r"(\s*)", lines[mid]).group(1) if lines[mid] else "    "
    lines.insert(mid, f"{indent}// added line")
    return "\n".join(lines), f"inserted comment at line {mid}"


def adv_delete_line(block: str) -> tuple[str, str]:
    lines = block.split("\n")
    if len(lines) < 4:
        return block, "too short for deletion"
    mid = len(lines) // 2
    removed = lines.pop(mid)
    return "\n".join(lines), f"deleted line {mid}: {removed[:40]}"


def adv_trailing_ws(block: str) -> tuple[str, str]:
    lines = block.split("\n")
    result = "\n".join(line.rstrip() + "  " for line in lines)
    if result == block:
        result = "\n".join(line.rstrip() for line in lines)
    if result == block:
        return block, "no-op"
    return result, "trailing whitespace modified"


ADVERSARIAL_PERTURBATIONS: list[callable] = [
    adv_rename_identifier,
    adv_swap_line,
    adv_change_literal,
    adv_add_line,
    adv_delete_line,
    adv_trailing_ws,
]


def generate_adversarial_from_file(
    filepath: Path,
    rel_path: str,
    content: str,
    max_per_file: int = 10,
    rng: random.Random | None = None,
) -> list[dict]:
    if rng is None:
        rng = random.Random()

    blocks = extract_blocks_newline_join(content)
    if len(blocks) < 2:
        return []

    clusters = find_similar_clusters(blocks)
    if not clusters:
        return []

    category = infer_category(rel_path, [blocks[i][2] for c in clusters for i in c[:3]])
    cases: list[dict] = []

    for cluster in clusters:
        if len(cases) >= max_per_file:
            break
        targets = rng.sample(cluster, min(len(cluster), 3))
        perturbations = rng.sample(ADVERSARIAL_PERTURBATIONS, min(len(ADVERSARIAL_PERTURBATIONS), 3))

        for target_idx in targets:
            if len(cases) >= max_per_file:
                break
            target_block = blocks[target_idx]
            for perturb_fn in perturbations:
                if len(cases) >= max_per_file:
                    break
                perturbed, notes = perturb_fn(target_block[2])
                if perturbed == target_block[2] or "no-op" in notes or "skip" in notes:
                    continue

                # Build candidates with similarity scores
                candidates = []
                target_candidate_idx = None
                for ci, idx in enumerate(cluster):
                    start_line, end_line, text = blocks[idx]
                    sim = block_similarity(perturbed, text)
                    candidates.append({"start_line": start_line, "end_line": end_line, "similarity": round(sim, 4)})
                    if idx == target_idx:
                        target_candidate_idx = ci

                if target_candidate_idx is None:
                    continue

                # Only keep cases with meaningful adversarial similarity
                other_sims = [c["similarity"] for ci, c in enumerate(candidates) if ci != target_candidate_idx]
                if not other_sims or max(other_sims) < 0.40:
                    continue

                cases.append(
                    {
                        "category": category,
                        "corpus_type": "adversarial",
                        "source_file": rel_path,
                        "file_content": content,
                        "old_string": perturbed,
                        "ground_truth": {
                            "target_index": target_candidate_idx,
                            "all_candidates": candidates,
                            "matched_text": target_block[2],
                        },
                        "notes": notes,
                    }
                )

    return cases


def generate_adversarial_corpus(source_dirs: list[Path], max_cases: int = 150, seed: int = 42) -> list[dict]:
    """Generate adversarial corpus from files with repetitive structure."""
    rng = random.Random(seed)
    source_files = collect_source_files(source_dirs)
    rng.shuffle(source_files)

    all_cases: list[dict] = []
    files_used = 0

    for fp, src_dir in source_files:
        if len(all_cases) >= max_cases:
            break
        try:
            content = fp.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        if len(content) < 200 or len(content) > 200_000:
            continue

        rel_path = str(fp.relative_to(src_dir))
        cases = generate_adversarial_from_file(
            fp,
            rel_path,
            content,
            max_per_file=max(2, (max_cases - len(all_cases)) // max(1, len(source_files) - files_used)),
            rng=rng,
        )
        if cases:
            files_used += 1
            all_cases.extend(cases)

    rng.shuffle(all_cases)
    all_cases = all_cases[:max_cases]
    for i, case in enumerate(all_cases):
        case["id"] = f"ambig-{case['category'][:8]}-{i:04d}"
    return all_cases


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(description="Generate fuzzy match test corpus")
    sub = parser.add_subparsers(dest="mode", required=True)

    acc = sub.add_parser("accuracy", help="Near-miss edit perturbations")
    acc.add_argument("source_dirs", nargs="+", type=Path, help="Source directories")
    acc.add_argument("-o", "--output", type=Path, default=Path("corpus/synthetic.json"))
    acc.add_argument("--lang", choices=["rust", "python", "typescript", "javascript"])
    acc.add_argument("--max-cases", type=int, default=200)
    acc.add_argument("--seed", type=int, default=42)

    adv = sub.add_parser("adversarial", help="Repetitive-structure false-positive stress test")
    adv.add_argument("source_dirs", nargs="+", type=Path, help="Source directories")
    adv.add_argument("-o", "--output", type=Path, default=Path("corpus/adversarial.json"))
    adv.add_argument("--max-cases", type=int, default=150)
    adv.add_argument("--seed", type=int, default=42)

    args = parser.parse_args()

    if args.mode == "accuracy":
        cases = generate_accuracy_corpus(args.source_dirs, lang=getattr(args, "lang", None), max_cases=args.max_cases, seed=args.seed)
    else:
        cases = generate_adversarial_corpus(args.source_dirs, max_cases=args.max_cases, seed=args.seed)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(cases, indent=2, ensure_ascii=False))
    print(f"Generated {len(cases)} {args.mode} cases -> {args.output}")

    counts = Counter(c["category"] for c in cases)
    for cat, n in counts.most_common():
        print(f"  {cat}: {n}")


if __name__ == "__main__":
    main()
