"""Fixture generator / importer for fuzzy-e2e benchmark.

Two modes:
  1. Import from edit-bench: copies existing fixtures wholesale
  2. Generate new fixtures: applies simple mutations to source files

Usage:
    python generate.py --from-edit-bench ~/projects/edit-bench/fixtures -o fixtures/ [--max-tasks 20]
    python generate.py --source-dir ~/projects/some-code --lang python -o fixtures/ [--max-tasks 20] [--seed 42]
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import shutil
from dataclasses import dataclass
from pathlib import Path


# ── Simplified mutation system (subset of edit-bench) ──────────────────


@dataclass
class MutationMatch:
    line_number: int  # 1-indexed
    original: str
    mutated: str
    indent: int


@dataclass
class Mutation:
    name: str
    category: str
    description_template: str
    fix_hint: str

    def find_candidates(self, lines: list[str]) -> list[MutationMatch]:
        raise NotImplementedError

    def describe(self, match: MutationMatch) -> str:
        return self.description_template.format(original=match.original.strip(), mutated=match.mutated.strip())

    def apply(self, lines: list[str], match: MutationMatch) -> list[str]:
        """Apply mutation to produce the buggy version."""
        result = list(lines)
        original_indent = " " * match.indent
        result[match.line_number - 1] = original_indent + match.mutated.lstrip() + "\n"
        return result


class SwapComparison(Mutation):
    SWAPS = [("<= ", ">= "), (">= ", "<= "), ("< ", "> "), ("> ", "< ")]

    def __init__(self) -> None:
        super().__init__(
            name="swap-comparison",
            category="single-line",
            description_template="A comparison operator has been swapped. `{mutated}` uses the wrong comparison direction.",
            fix_hint="Check the comparison operator direction.",
        )

    def find_candidates(self, lines: list[str]) -> list[MutationMatch]:
        matches = []
        for i, line in enumerate(lines):
            stripped = line.rstrip()
            if stripped.lstrip().startswith(("#", "//", "/*")):
                continue
            for old, new in self.SWAPS:
                if old in stripped and "=>" not in stripped:
                    indent = len(line) - len(line.lstrip())
                    matches.append(MutationMatch(i + 1, stripped, stripped.replace(old, new, 1), indent))
                    break
        return matches


class FlipBoolean(Mutation):
    def __init__(self) -> None:
        super().__init__(
            name="flip-boolean",
            category="single-line",
            description_template="A boolean literal has been flipped. `{mutated}` has the wrong truth value.",
            fix_hint="Check boolean literals for correctness.",
        )

    def find_candidates(self, lines: list[str]) -> list[MutationMatch]:
        matches = []
        for i, line in enumerate(lines):
            stripped = line.rstrip()
            if stripped.lstrip().startswith(("#", "//", "/*")):
                continue
            indent = len(line) - len(line.lstrip())
            if "True" in stripped:
                matches.append(MutationMatch(i + 1, stripped, stripped.replace("True", "False", 1), indent))
            elif "False" in stripped:
                matches.append(MutationMatch(i + 1, stripped, stripped.replace("False", "True", 1), indent))
            elif re.search(r"\btrue\b", stripped):
                matches.append(MutationMatch(i + 1, stripped, re.sub(r"\btrue\b", "false", stripped, count=1), indent))
            elif re.search(r"\bfalse\b", stripped):
                matches.append(MutationMatch(i + 1, stripped, re.sub(r"\bfalse\b", "true", stripped, count=1), indent))
        return matches


class SwapLogical(Mutation):
    def __init__(self) -> None:
        super().__init__(
            name="swap-logical",
            category="single-line",
            description_template="A logical operator has been swapped. `{mutated}` uses the wrong logical connective.",
            fix_hint="Check whether and/or (&&/||) is correct.",
        )

    def find_candidates(self, lines: list[str]) -> list[MutationMatch]:
        matches = []
        for i, line in enumerate(lines):
            stripped = line.rstrip()
            if stripped.lstrip().startswith(("#", "//", "/*")):
                continue
            indent = len(line) - len(line.lstrip())
            if " and " in stripped:
                matches.append(MutationMatch(i + 1, stripped, stripped.replace(" and ", " or ", 1), indent))
            elif " or " in stripped:
                matches.append(MutationMatch(i + 1, stripped, stripped.replace(" or ", " and ", 1), indent))
            elif " && " in stripped:
                matches.append(MutationMatch(i + 1, stripped, stripped.replace(" && ", " || ", 1), indent))
            elif " || " in stripped:
                matches.append(MutationMatch(i + 1, stripped, stripped.replace(" || ", " && ", 1), indent))
        return matches


ALL_MUTATIONS: list[Mutation] = [SwapComparison(), FlipBoolean(), SwapLogical()]


# ── Difficulty scoring ─────────────────────────────────────────────────


def score_difficulty(lines: list[str], match: MutationMatch) -> tuple[int, str]:
    """Score task difficulty based on file and mutation characteristics."""
    score = 0
    n = len(lines)

    if n > 300:
        score += 3
    elif n > 150:
        score += 2
    elif n > 80:
        score += 1

    rel_pos = match.line_number / max(n, 1)
    if 0.33 < rel_pos < 0.66:
        score += 1

    target = match.original.strip()
    repeat_count = sum(1 for ln in lines if ln.strip() == target)
    if repeat_count > 1:
        score += min(repeat_count - 1, 3)

    if match.indent > 12:
        score += 1

    if score <= 2:
        level = "easy"
    elif score <= 4:
        level = "medium"
    elif score <= 6:
        level = "hard"
    else:
        level = "nightmare"

    return score, level


def generate_prompt(filename: str, mutation: Mutation, match: MutationMatch, difficulty: str) -> str:
    """Generate the task prompt based on difficulty level."""
    desc = mutation.describe(match)

    if difficulty == "easy":
        return f"# Fix the bug in `{filename}`\n\n{desc}\n\nThe issue is on line {match.line_number}.\n\n{mutation.fix_hint}\n"
    elif difficulty == "medium":
        return f"# Fix the bug in `{filename}`\n\n{desc}\n\n{mutation.fix_hint}\n"
    elif difficulty == "hard":
        return f"# Fix the bug in `{filename}`\n\n{desc}\n\nFind and fix this issue.\n"
    else:
        return f"# Fix the bug in `{filename}`\n\nThere is a subtle bug in this file.\n\nTrack it down and fix it with a minimal edit.\n"


# ── Source file collection ─────────────────────────────────────────────


LANG_EXTENSIONS: dict[str, list[str]] = {
    "python": [".py"],
    "rust": [".rs"],
    "go": [".go"],
    "typescript": [".ts", ".tsx", ".js", ".jsx"],
    "javascript": [".ts", ".tsx", ".js", ".jsx"],
}

SKIP_DIRS = frozenset(
    {
        "target",
        "node_modules",
        "__pycache__",
        "venv",
        ".venv",
        "vendor",
        "dist",
        "build",
        ".git",
        ".tox",
        ".mypy_cache",
    }
)


def collect_source_files(source_dir: Path, lang: str, min_lines: int = 30, max_lines: int = 500) -> list[Path]:
    """Collect source files of the given language."""
    valid_exts = LANG_EXTENSIONS.get(lang, [".py"])
    files: list[Path] = []
    for root, dirs, filenames in os.walk(source_dir):
        dirs[:] = [d for d in dirs if not d.startswith(".") and d not in SKIP_DIRS]
        for f in filenames:
            if any(f.endswith(ext) for ext in valid_exts):
                path = Path(root) / f
                try:
                    content = path.read_text()
                    line_count = len(content.splitlines())
                    if min_lines <= line_count <= max_lines:
                        files.append(path)
                except Exception:
                    continue
    return files


# ── Import from edit-bench ─────────────────────────────────────────────


def import_from_edit_bench(edit_bench_dir: Path, output_dir: Path, max_tasks: int) -> int:
    """Copy fixtures from edit-bench format into our fixture directory.

    edit-bench fixture layout:
        {task_id}/input/{filename}, expected/{filename}, prompt.md, metadata.json
    """
    imported = 0
    for task_dir in sorted(edit_bench_dir.iterdir()):
        if imported >= max_tasks:
            break
        if not task_dir.is_dir():
            continue
        metadata_path = task_dir / "metadata.json"
        if not metadata_path.exists():
            continue

        # Verify fixture is complete
        input_dir = task_dir / "input"
        expected_dir = task_dir / "expected"
        prompt_file = task_dir / "prompt.md"
        if not (input_dir.exists() and expected_dir.exists() and prompt_file.exists()):
            continue

        dest = output_dir / task_dir.name
        if dest.exists():
            shutil.rmtree(dest)
        shutil.copytree(task_dir, dest)
        imported += 1
        print(f"  imported: {task_dir.name}")

    return imported


# ── Generate new fixtures ──────────────────────────────────────────────


def generate_from_source(source_dir: Path, output_dir: Path, lang: str, max_tasks: int, seed: int) -> int:
    """Generate fixtures by mutating source files."""
    random.seed(seed)

    files = collect_source_files(source_dir, lang)
    if not files:
        print(f"No {lang} files found in {source_dir}")
        return 0

    print(f"Found {len(files)} {lang} source files in {source_dir}")

    # Collect mutation candidates
    candidates: list[tuple[Path, Mutation, MutationMatch]] = []
    for f in files:
        try:
            lines = f.read_text().splitlines()
        except Exception:
            continue
        for mutation in ALL_MUTATIONS:
            for match in mutation.find_candidates(lines):
                candidates.append((f, mutation, match))

    print(f"Found {len(candidates)} mutation candidates")

    if not candidates:
        return 0

    random.shuffle(candidates)

    # Select tasks, spreading across mutation types
    by_type: dict[str, list[tuple[Path, Mutation, MutationMatch]]] = {}
    for c in candidates:
        by_type.setdefault(c[1].name, []).append(c)

    selected: list[tuple[Path, Mutation, MutationMatch]] = []
    while len(selected) < max_tasks and any(by_type.values()):
        for name in list(by_type.keys()):
            if not by_type[name]:
                del by_type[name]
                continue
            if len(selected) >= max_tasks:
                break
            selected.append(by_type[name].pop(0))

    print(f"Selected {len(selected)} tasks")

    generated = 0
    for idx, (source_path, mutation, match) in enumerate(selected):
        lines = source_path.read_text().splitlines(keepends=True)
        filename = source_path.name
        plain_lines = [ln.rstrip() for ln in lines]
        diff_score, difficulty = score_difficulty(plain_lines, match)

        mutated_lines = mutation.apply(lines, match)
        task_id = f"{mutation.name}-{idx + 1:03d}"
        task_dir = output_dir / task_id

        input_dir = task_dir / "input"
        expected_dir = task_dir / "expected"
        input_dir.mkdir(parents=True, exist_ok=True)
        expected_dir.mkdir(parents=True, exist_ok=True)

        (input_dir / filename).write_text("".join(mutated_lines))
        (expected_dir / filename).write_text("".join(lines))

        prompt = generate_prompt(filename, mutation, match, difficulty)
        (task_dir / "prompt.md").write_text(prompt)

        target = match.original.strip()
        repeat_count = sum(1 for ln in plain_lines if ln.strip() == target) if target else 0

        metadata = {
            "mutation_type": mutation.name,
            "mutation_category": mutation.category,
            "difficulty": difficulty,
            "difficulty_score": diff_score,
            "line_number": match.line_number,
            "original_snippet": match.original[:200],
            "mutated_snippet": match.mutated[:200],
            "language": lang if lang != "javascript" else "typescript",
            "file_path": str(source_path),
            "file_name": filename,
            "context": {
                "file_lines": len(lines),
                "is_repeated_line": repeat_count > 1,
                "repeat_count": repeat_count,
                "indent": match.indent,
            },
        }
        (task_dir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
        generated += 1
        print(f"  {task_id}: {difficulty} ({filename}:{match.line_number})")

    return generated


# ── CLI ────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate fixtures for fuzzy-e2e benchmark")

    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--from-edit-bench", type=Path, metavar="DIR", help="Import fixtures from edit-bench fixtures directory")
    mode.add_argument("--source-dir", type=Path, metavar="DIR", help="Generate fixtures by mutating source code")

    parser.add_argument("-o", "--output", type=Path, default=Path("fixtures"), help="Output directory (default: fixtures/)")
    parser.add_argument("--max-tasks", type=int, default=20, help="Maximum number of tasks (default: 20)")
    parser.add_argument("--lang", default="python", choices=["python", "rust", "go", "typescript", "javascript"], help="Source language")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for reproducibility")
    args = parser.parse_args()

    output_dir = args.output
    output_dir.mkdir(parents=True, exist_ok=True)

    if args.from_edit_bench:
        eb_dir = args.from_edit_bench
        if not eb_dir.exists():
            print(f"edit-bench fixtures directory not found: {eb_dir}")
            return
        print(f"Importing from edit-bench: {eb_dir}")
        count = import_from_edit_bench(eb_dir, output_dir, args.max_tasks)
        print(f"\nImported {count} fixtures to {output_dir}")
    else:
        src_dir = args.source_dir
        if not src_dir.exists():
            print(f"Source directory not found: {src_dir}")
            return
        print(f"Generating fixtures from: {src_dir}")
        count = generate_from_source(src_dir, output_dir, args.lang, args.max_tasks, args.seed)
        print(f"\nGenerated {count} fixtures to {output_dir}")


if __name__ == "__main__":
    main()
