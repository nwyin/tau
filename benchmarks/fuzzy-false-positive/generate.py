#!/usr/bin/env python3
"""Adversarial corpus generator for fuzzy-match false-positive auditing.

Scans source directories for files with HIGH STRUCTURAL REPETITION — similar
blocks that repeat within a single file (match arms, test functions, JSX
components, SQL migrations, config sections, CSS rules, struct impls).

For each cluster of similar blocks, creates perturbations that target one
block but could plausibly match others.  Records ALL candidate locations
and their similarity scores so the runner can score whether a matcher
picked the right one.

Usage:
    python generate.py <source_dirs...> -o corpus/adversarial.json
    python generate.py ../../coding-agent/src ~/projects/oh-my-pi/packages \
        -o corpus/adversarial.json --max-cases 150 --seed 42
"""

from __future__ import annotations

import argparse
import difflib
import json
import random
import re
from pathlib import Path

# ---------------------------------------------------------------------------
# Language / extension map
# ---------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------
# Category heuristics
# ---------------------------------------------------------------------------

# Patterns that hint at which adversarial category a file belongs to.
# Each entry: (category, filename_regex, block_hint_regex)
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
    """Guess an adversarial category from the file path and block contents."""
    joined = "\n".join(block_texts[:5])
    for category, path_re, content_re in CATEGORY_HINTS:
        if path_re.search(filepath):
            if content_re is None or content_re.search(joined):
                return category
    return "config-blocks"  # safe fallback


# ---------------------------------------------------------------------------
# Block extraction (AST-free, heuristic)
# ---------------------------------------------------------------------------


def extract_blocks(content: str, min_lines: int = 3, max_lines: int = 30) -> list[tuple[int, int, str]]:
    """Extract contiguous non-blank blocks from file content.

    Returns list of (start_line, end_line, block_text).
    Lines are 0-indexed.  block_text preserves original newlines.
    """
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
# Similarity clustering
# ---------------------------------------------------------------------------


def block_similarity(a: str, b: str) -> float:
    """Compute SequenceMatcher ratio between two block strings."""
    return difflib.SequenceMatcher(None, a, b).ratio()


def find_similar_clusters(
    blocks: list[tuple[int, int, str]],
    min_similarity: float = 0.55,
    min_cluster_size: int = 2,
) -> list[list[int]]:
    """Find clusters of mutually similar blocks within a file.

    Returns list of clusters, each cluster being a list of indices into
    *blocks*.  Only keeps clusters with >= min_cluster_size members.
    """
    n = len(blocks)
    if n < min_cluster_size:
        return []

    # Build adjacency based on similarity
    adj: dict[int, list[int]] = {i: [] for i in range(n)}
    for i in range(n):
        for j in range(i + 1, n):
            sim = block_similarity(blocks[i][2], blocks[j][2])
            if sim >= min_similarity:
                adj[i].append(j)
                adj[j].append(i)

    # Simple connected-component clustering
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


# ---------------------------------------------------------------------------
# Perturbation strategies
# ---------------------------------------------------------------------------


def perturb_rename_identifier(block: str) -> tuple[str, str]:
    """Find a short identifier and rename it slightly."""
    # Look for words that appear multiple times, rename one occurrence
    words = re.findall(r"\b([a-zA-Z_]\w{2,20})\b", block)
    if not words:
        return block, "no identifiers found"
    # Pick most common word
    from collections import Counter

    counts = Counter(words)
    target = counts.most_common(1)[0][0]
    # Small rename: append/remove a char
    renamed = target + "_v2" if len(target) < 18 else target[:-1]
    result = block.replace(target, renamed, 1)
    if result == block:
        return block, "rename had no effect"
    return result, f"renamed '{target}' -> '{renamed}'"


def perturb_swap_line(block: str) -> tuple[str, str]:
    """Swap two adjacent lines in the block."""
    lines = block.split("\n")
    if len(lines) < 3:
        return block, "too short for line swap"
    # Swap lines in the middle (avoid first/last which are often structural)
    idx = len(lines) // 2
    lines[idx], lines[idx + 1] = lines[idx + 1], lines[idx]
    return "\n".join(lines), f"swapped lines {idx} and {idx + 1}"


def perturb_change_literal(block: str) -> tuple[str, str]:
    """Change a string literal or number in the block."""
    # Try string literals first
    match = re.search(r'"([^"]{1,40})"', block)
    if match:
        old_lit = match.group(0)
        inner = match.group(1)
        new_inner = inner + "_modified" if len(inner) < 30 else inner[:10]
        new_lit = f'"{new_inner}"'
        return block.replace(old_lit, new_lit, 1), f"changed literal {old_lit} -> {new_lit}"

    # Try numbers
    match = re.search(r"\b(\d{1,6})\b", block)
    if match:
        old_num = match.group(1)
        new_num = str(int(old_num) + 1)
        return block.replace(old_num, new_num, 1), f"changed number {old_num} -> {new_num}"

    return block, "no literals found"


def perturb_add_line(block: str) -> tuple[str, str]:
    """Insert a comment or blank statement in the middle."""
    lines = block.split("\n")
    if len(lines) < 2:
        return block, "too short"
    mid = len(lines) // 2
    # Detect indentation from surrounding lines
    indent = re.match(r"(\s*)", lines[mid]).group(1) if lines[mid] else "    "
    inserted = f"{indent}// added line"
    lines.insert(mid, inserted)
    return "\n".join(lines), f"inserted comment at line {mid}"


def perturb_delete_line(block: str) -> tuple[str, str]:
    """Remove a line from the middle of the block."""
    lines = block.split("\n")
    if len(lines) < 4:
        return block, "too short for deletion"
    mid = len(lines) // 2
    removed = lines.pop(mid)
    return "\n".join(lines), f"deleted line {mid}: {removed[:40]}"


def perturb_trailing_ws(block: str) -> tuple[str, str]:
    """Add or strip trailing whitespace."""
    lines = block.split("\n")
    result = "\n".join(line.rstrip() + "  " for line in lines)
    if result == block:
        result = "\n".join(line.rstrip() for line in lines)
    if result == block:
        return block, "no-op"
    return result, "trailing whitespace modified"


ALL_PERTURBATIONS: list[callable] = [
    perturb_rename_identifier,
    perturb_swap_line,
    perturb_change_literal,
    perturb_add_line,
    perturb_delete_line,
    perturb_trailing_ws,
]


# ---------------------------------------------------------------------------
# Case generation
# ---------------------------------------------------------------------------


def make_candidates(
    blocks: list[tuple[int, int, str]],
    cluster_indices: list[int],
    old_string: str,
) -> list[dict]:
    """Build the all_candidates list with similarity scores against old_string."""
    candidates = []
    for idx in cluster_indices:
        start_line, end_line, text = blocks[idx]
        sim = block_similarity(old_string, text)
        candidates.append(
            {
                "start_line": start_line,
                "end_line": end_line,
                "similarity": round(sim, 4),
                "block_index": idx,
            }
        )
    return candidates


def generate_cases_from_file(
    filepath: Path,
    rel_path: str,
    content: str,
    max_per_file: int = 10,
    rng: random.Random | None = None,
) -> list[dict]:
    """Generate adversarial cases from a single file."""
    if rng is None:
        rng = random.Random()

    blocks = extract_blocks(content)
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

        # Pick perturbation targets from the cluster
        targets = rng.sample(cluster, min(len(cluster), 3))
        perturbations = rng.sample(ALL_PERTURBATIONS, min(len(ALL_PERTURBATIONS), 3))

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

                candidates = make_candidates(blocks, cluster, perturbed)
                # Find the target in candidates
                target_candidate_idx = None
                for ci, cand in enumerate(candidates):
                    if cand["block_index"] == target_idx:
                        target_candidate_idx = ci
                        break

                if target_candidate_idx is None:
                    continue

                # Only keep cases where at least one OTHER candidate has
                # meaningful similarity (otherwise it's not adversarial)
                other_sims = [c["similarity"] for ci, c in enumerate(candidates) if ci != target_candidate_idx]
                if not other_sims or max(other_sims) < 0.40:
                    continue

                # Clean up block_index from candidates (internal bookkeeping)
                clean_candidates = [
                    {"start_line": c["start_line"], "end_line": c["end_line"], "similarity": c["similarity"]} for c in candidates
                ]

                cases.append(
                    {
                        "category": category,
                        "source_file": rel_path,
                        "file_content": content,
                        "old_string": perturbed,
                        "ground_truth": {
                            "target_index": target_candidate_idx,
                            "all_candidates": clean_candidates,
                            "matched_text": target_block[2],
                        },
                        "notes": notes,
                    }
                )

    return cases


# ---------------------------------------------------------------------------
# Directory scanning
# ---------------------------------------------------------------------------


def collect_source_files(source_dirs: list[Path], extensions: list[str] | None = None) -> list[tuple[Path, Path]]:
    """Collect source files from multiple directories.

    Returns list of (absolute_path, source_dir) tuples.
    """
    if extensions is None:
        extensions = ALL_EXTENSIONS

    files: list[tuple[Path, Path]] = []
    for src_dir in source_dirs:
        src_dir = src_dir.resolve()
        if not src_dir.is_dir():
            continue
        for ext in extensions:
            for fp in sorted(src_dir.rglob(f"*{ext}")):
                # Skip hidden dirs, node_modules, target, etc.
                parts = fp.relative_to(src_dir).parts
                if any(p.startswith(".") or p in ("node_modules", "target", "__pycache__", "dist", "build") for p in parts):
                    continue
                files.append((fp, src_dir))

    return files


def generate_corpus(
    source_dirs: list[Path],
    max_cases: int = 150,
    seed: int = 42,
) -> list[dict]:
    """Generate the full adversarial corpus by scanning source directories."""
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

        # Skip very small or very large files
        if len(content) < 200 or len(content) > 200_000:
            continue

        rel_path = str(fp.relative_to(src_dir))
        cases = generate_cases_from_file(
            fp,
            rel_path,
            content,
            max_per_file=max(2, (max_cases - len(all_cases)) // max(1, len(source_files) - files_used)),
            rng=rng,
        )

        if cases:
            files_used += 1
            all_cases.extend(cases)

    # Trim to max and assign IDs
    rng.shuffle(all_cases)
    all_cases = all_cases[:max_cases]

    for i, case in enumerate(all_cases):
        case["id"] = f"ambig-{case['category'][:8]}-{i:04d}"

    return all_cases


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate adversarial corpus for fuzzy-match false-positive auditing",
    )
    parser.add_argument("source_dirs", nargs="+", type=Path, help="Source directories to scan for repetitive files")
    parser.add_argument("-o", "--output", type=Path, default=Path("corpus/adversarial.json"), help="Output JSON file")
    parser.add_argument("--max-cases", type=int, default=150, help="Maximum number of test cases")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for reproducibility")
    args = parser.parse_args()

    cases = generate_corpus(args.source_dirs, max_cases=args.max_cases, seed=args.seed)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(cases, indent=2, ensure_ascii=False))
    print(f"Generated {len(cases)} adversarial cases -> {args.output}")

    # Category breakdown
    from collections import Counter

    counts = Counter(c["category"] for c in cases)
    for cat, n in counts.most_common():
        print(f"  {cat}: {n}")

    # Source file breakdown
    source_files = {c["source_file"] for c in cases}
    print(f"  From {len(source_files)} source files")


if __name__ == "__main__":
    main()
