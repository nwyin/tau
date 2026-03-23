"""Mine commit history from git repos to generate benchmark fixtures.

Walks git log, filters commits by type, and extracts (input, expected, prompt)
fixture directories suitable for online benchmarks.

Strategies:
    edit     — single-file or few-file changes → fuzzy-e2e fixtures
    refactor — multi-file type/signature propagation → post-edit-diagnostics fixtures

Usage:
    # Mine edit tasks from a Python project
    python -m shared.miner edit ~/projects/fastapi -o ../fuzzy-e2e/fixtures/ \
        --lang python --max-tasks 20

    # Mine refactoring tasks from a Rust project
    python -m shared.miner refactor ~/projects/tokio -o ../post-edit-diagnostics/fixtures/ \
        --lang rust --max-tasks 10

    # Mine from multiple repos
    python -m shared.miner edit ~/projects/fastapi ~/projects/flask \
        -o ../fuzzy-e2e/fixtures/ --lang python --max-tasks 40
"""

from __future__ import annotations

import argparse
import json
import random
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

LANG_EXTENSIONS: dict[str, list[str]] = {
    "python": [".py"],
    "rust": [".rs"],
    "typescript": [".ts", ".tsx"],
    "javascript": [".js", ".jsx"],
    "go": [".go"],
}

# Files/dirs to always skip
SKIP_PATTERNS = [
    "test",
    "tests",
    "spec",
    "__test__",
    "__tests__",
    "vendor",
    "node_modules",
    "target",
    ".git",
    "migrations",
    "generated",
    "snapshots",
    "CHANGELOG",
    "LICENSE",
    "README",
]


@dataclass
class MinedCommit:
    sha: str
    message: str
    files_changed: list[str]
    insertions: int
    deletions: int
    repo_name: str
    repo_path: str


@dataclass
class MinedFixture:
    task_id: str
    commit: MinedCommit
    files: list[dict]  # [{path, input_content, expected_content}]
    prompt: str
    metadata: dict = field(default_factory=dict)


def _run_git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def _repo_name(repo: Path) -> str:
    return repo.resolve().name


def _get_file_at(repo: Path, sha: str, path: str) -> str | None:
    """Get file content at a specific commit."""
    try:
        return _run_git(repo, "show", f"{sha}:{path}")
    except RuntimeError:
        return None


def _list_commits(repo: Path, max_commits: int = 500) -> list[dict]:
    """List commits with stats."""
    log = _run_git(
        repo,
        "log",
        "--format=%H\t%s",
        "--no-merges",
        f"-{max_commits}",
        "--diff-filter=M",
    )
    commits = []
    for line in log.strip().splitlines():
        if "\t" not in line:
            continue
        sha, message = line.split("\t", 1)
        commits.append({"sha": sha, "message": message})
    return commits


def _get_commit_files(repo: Path, sha: str) -> list[dict]:
    """Get changed files for a commit with stats."""
    numstat = _run_git(repo, "diff", "--numstat", f"{sha}~1", sha)
    files = []
    for line in numstat.strip().splitlines():
        parts = line.split("\t")
        if len(parts) != 3:
            continue
        added, deleted, path = parts
        if added == "-" or deleted == "-":  # binary
            continue
        files.append(
            {
                "path": path,
                "insertions": int(added),
                "deletions": int(deleted),
            }
        )
    return files


def _matches_lang(path: str, lang: str | None) -> bool:
    if lang is None:
        return True
    exts = LANG_EXTENSIONS.get(lang, [f".{lang}"])
    return any(path.endswith(ext) for ext in exts)


def _should_skip(path: str) -> bool:
    parts = path.lower().split("/")
    return any(skip in part for part in parts for skip in SKIP_PATTERNS)


def _make_prompt_edit(message: str, files: list[dict]) -> str:
    """Generate a task prompt from a commit message for edit tasks."""
    # Clean up conventional commit prefixes
    cleaned = re.sub(r"^(fix|feat|refactor|chore|docs|style|perf|test|ci|build)(\(.+?\))?:\s*", "", message)
    cleaned = cleaned.strip()
    if not cleaned:
        cleaned = message

    file_list = ", ".join(f"`{f['path']}`" for f in files)
    return f"Apply the following change to {file_list}:\n\n{cleaned}\n\nMake the minimum change necessary."


def _make_prompt_refactor(message: str, files: list[dict]) -> str:
    """Generate a task prompt from a commit message for refactoring tasks."""
    cleaned = re.sub(r"^(fix|feat|refactor|chore|docs|style|perf|test|ci|build)(\(.+?\))?:\s*", "", message)
    cleaned = cleaned.strip()
    if not cleaned:
        cleaned = message

    file_list = "\n".join(f"- `{f['path']}`" for f in files)
    return (
        f"Perform the following refactoring:\n\n{cleaned}\n\n"
        f"Files involved:\n{file_list}\n\n"
        "Propagate all type changes and update all call sites. "
        "Make the minimum change necessary."
    )


def mine_edit_tasks(
    repos: list[Path],
    lang: str | None = None,
    max_tasks: int = 20,
    max_files_per_commit: int = 3,
    max_diff_lines: int = 100,
    max_commits_scan: int = 500,
    seed: int = 42,
) -> list[MinedFixture]:
    """Mine single-file or few-file edit commits as fuzzy-e2e fixtures."""
    random.seed(seed)
    fixtures: list[MinedFixture] = []
    task_counter = 0

    for repo in repos:
        repo_name = _repo_name(repo)
        commits = _list_commits(repo, max_commits=max_commits_scan)

        for commit_info in commits:
            if len(fixtures) >= max_tasks:
                break

            sha = commit_info["sha"]
            message = commit_info["message"]

            try:
                changed_files = _get_commit_files(repo, sha)
            except RuntimeError:
                continue

            # Filter to language-relevant, non-skipped files
            relevant = [f for f in changed_files if _matches_lang(f["path"], lang) and not _should_skip(f["path"])]

            if not relevant:
                continue

            # Edit tasks: small, focused changes
            total_diff = sum(f["insertions"] + f["deletions"] for f in relevant)
            if len(relevant) > max_files_per_commit or total_diff > max_diff_lines:
                continue

            # Get file contents at parent and current commit
            file_data = []
            valid = True
            for f in relevant:
                input_content = _get_file_at(repo, f"{sha}~1", f["path"])
                expected_content = _get_file_at(repo, sha, f["path"])
                if input_content is None or expected_content is None:
                    valid = False
                    break
                if input_content == expected_content:
                    continue  # no actual change
                file_data.append(
                    {
                        "path": f["path"],
                        "input_content": input_content,
                        "expected_content": expected_content,
                    }
                )

            if not valid or not file_data:
                continue

            task_id = f"edit-{repo_name}-{task_counter:04d}"
            task_counter += 1

            mined = MinedCommit(
                sha=sha,
                message=message,
                files_changed=[f["path"] for f in relevant],
                insertions=sum(f["insertions"] for f in relevant),
                deletions=sum(f["deletions"] for f in relevant),
                repo_name=repo_name,
                repo_path=str(repo),
            )

            fixture = MinedFixture(
                task_id=task_id,
                commit=mined,
                files=file_data,
                prompt=_make_prompt_edit(message, file_data),
                metadata={
                    "source": "mined",
                    "repo": repo_name,
                    "sha": sha,
                    "language": lang or "mixed",
                    "difficulty": _score_difficulty_edit(file_data, total_diff),
                    "files_changed": len(file_data),
                    "total_diff_lines": total_diff,
                },
            )
            fixtures.append(fixture)

    random.shuffle(fixtures)
    return fixtures[:max_tasks]


def mine_refactor_tasks(
    repos: list[Path],
    lang: str | None = None,
    max_tasks: int = 10,
    min_files: int = 2,
    max_files: int = 8,
    max_diff_lines: int = 300,
    max_commits_scan: int = 500,
    seed: int = 42,
) -> list[MinedFixture]:
    """Mine multi-file refactoring commits as post-edit-diagnostics fixtures."""
    random.seed(seed)
    fixtures: list[MinedFixture] = []
    task_counter = 0

    # Patterns that suggest type/signature changes
    refactor_patterns = [
        re.compile(r"\brename\b", re.IGNORECASE),
        re.compile(r"\brefactor\b", re.IGNORECASE),
        re.compile(r"\bextract\b", re.IGNORECASE),
        re.compile(r"\bpropagate\b", re.IGNORECASE),
        re.compile(r"\bsignature\b", re.IGNORECASE),
        re.compile(r"\btype\b", re.IGNORECASE),
        re.compile(r"\bmove\b", re.IGNORECASE),
        re.compile(r"\breorganize\b", re.IGNORECASE),
        re.compile(r"\bsplit\b", re.IGNORECASE),
    ]

    for repo in repos:
        repo_name = _repo_name(repo)
        commits = _list_commits(repo, max_commits=max_commits_scan)

        for commit_info in commits:
            if len(fixtures) >= max_tasks:
                break

            sha = commit_info["sha"]
            message = commit_info["message"]

            # Prefer commits with refactoring-related messages
            has_refactor_signal = any(p.search(message) for p in refactor_patterns)

            try:
                changed_files = _get_commit_files(repo, sha)
            except RuntimeError:
                continue

            relevant = [f for f in changed_files if _matches_lang(f["path"], lang) and not _should_skip(f["path"])]

            total_diff = sum(f["insertions"] + f["deletions"] for f in relevant)

            # Refactor tasks: multi-file, moderate size
            if len(relevant) < min_files or len(relevant) > max_files:
                continue
            if total_diff > max_diff_lines:
                continue

            # Without a refactoring signal in the message, require stronger
            # evidence: multiple files with small, coordinated changes
            if not has_refactor_signal:
                avg_diff = total_diff / len(relevant)
                if avg_diff > 30:  # large per-file changes unlikely to be coordinated refactoring
                    continue

            file_data = []
            valid = True
            for f in relevant:
                input_content = _get_file_at(repo, f"{sha}~1", f["path"])
                expected_content = _get_file_at(repo, sha, f["path"])
                if input_content is None or expected_content is None:
                    valid = False
                    break
                if input_content == expected_content:
                    continue
                file_data.append(
                    {
                        "path": f["path"],
                        "input_content": input_content,
                        "expected_content": expected_content,
                    }
                )

            if not valid or len(file_data) < min_files:
                continue

            task_id = f"refactor-{repo_name}-{task_counter:04d}"
            task_counter += 1

            mined = MinedCommit(
                sha=sha,
                message=message,
                files_changed=[f["path"] for f in relevant],
                insertions=sum(f["insertions"] for f in relevant),
                deletions=sum(f["deletions"] for f in relevant),
                repo_name=repo_name,
                repo_path=str(repo),
            )

            fixture = MinedFixture(
                task_id=task_id,
                commit=mined,
                files=file_data,
                prompt=_make_prompt_refactor(message, file_data),
                metadata={
                    "source": "mined",
                    "repo": repo_name,
                    "sha": sha,
                    "language": lang or "mixed",
                    "difficulty": _score_difficulty_refactor(file_data, total_diff, has_refactor_signal),
                    "files_changed": len(file_data),
                    "total_diff_lines": total_diff,
                    "has_refactor_signal": has_refactor_signal,
                },
            )
            fixtures.append(fixture)

    random.shuffle(fixtures)
    return fixtures[:max_tasks]


def _score_difficulty_edit(files: list[dict], total_diff: int) -> str:
    score = 0
    if total_diff > 30:
        score += 2
    elif total_diff > 10:
        score += 1
    if len(files) > 1:
        score += 1
    # Check if changes are spread across the file (harder to find)
    for f in files:
        lines = f["input_content"].splitlines()
        if len(lines) > 200:
            score += 1
            break
    if score <= 1:
        return "easy"
    if score <= 2:
        return "medium"
    return "hard"


def _score_difficulty_refactor(files: list[dict], total_diff: int, has_signal: bool) -> str:
    score = 0
    if len(files) >= 4:
        score += 2
    elif len(files) >= 2:
        score += 1
    if total_diff > 100:
        score += 2
    elif total_diff > 50:
        score += 1
    if not has_signal:
        score += 1  # harder without clear message
    if score <= 2:
        return "easy"
    if score <= 4:
        return "medium"
    return "hard"


def write_fixtures(fixtures: list[MinedFixture], output_dir: Path) -> None:
    """Write mined fixtures to disk in standard fixture format."""
    output_dir.mkdir(parents=True, exist_ok=True)

    for fixture in fixtures:
        task_dir = output_dir / fixture.task_id
        input_dir = task_dir / "input"
        expected_dir = task_dir / "expected"

        for f in fixture.files:
            fpath = Path(f["path"])
            (input_dir / fpath.parent).mkdir(parents=True, exist_ok=True)
            (expected_dir / fpath.parent).mkdir(parents=True, exist_ok=True)
            (input_dir / fpath).write_text(f["input_content"])
            (expected_dir / fpath).write_text(f["expected_content"])

        (task_dir / "prompt.md").write_text(fixture.prompt + "\n")

        meta = dict(fixture.metadata)
        meta["commit_sha"] = fixture.commit.sha
        meta["commit_message"] = fixture.commit.message
        meta["repo"] = fixture.commit.repo_name
        (task_dir / "metadata.json").write_text(json.dumps(meta, indent=2, ensure_ascii=False) + "\n")

    print(f"Wrote {len(fixtures)} fixtures to {output_dir}", file=sys.stderr)

    # Print summary
    by_difficulty: dict[str, int] = {}
    by_repo: dict[str, int] = {}
    for f in fixtures:
        d = f.metadata.get("difficulty", "unknown")
        by_difficulty[d] = by_difficulty.get(d, 0) + 1
        r = f.commit.repo_name
        by_repo[r] = by_repo.get(r, 0) + 1

    print("\nBy difficulty:", file=sys.stderr)
    for d, n in sorted(by_difficulty.items()):
        print(f"  {d}: {n}", file=sys.stderr)
    print("\nBy repo:", file=sys.stderr)
    for r, n in sorted(by_repo.items()):
        print(f"  {r}: {n}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(
        description="Mine commit history for benchmark fixtures",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    python -m shared.miner edit ~/projects/fastapi -o ../fuzzy-e2e/fixtures/mined/
    python -m shared.miner refactor ~/projects/hive -o ../post-edit-diagnostics/fixtures/mined/
        """,
    )
    sub = parser.add_subparsers(dest="strategy", required=True)

    # edit subcommand
    p_edit = sub.add_parser("edit", help="Mine single/few-file edit commits")
    p_edit.add_argument("repos", type=Path, nargs="+", help="Git repo paths")
    p_edit.add_argument("-o", "--output", type=Path, required=True, help="Output fixtures directory")
    p_edit.add_argument("--lang", choices=list(LANG_EXTENSIONS.keys()), help="Filter by language")
    p_edit.add_argument("--max-tasks", type=int, default=20)
    p_edit.add_argument("--max-files", type=int, default=3, help="Max files per commit")
    p_edit.add_argument("--max-diff", type=int, default=100, help="Max diff lines per commit")
    p_edit.add_argument("--scan", type=int, default=500, help="Max commits to scan per repo")
    p_edit.add_argument("--seed", type=int, default=42)

    # refactor subcommand
    p_refactor = sub.add_parser("refactor", help="Mine multi-file refactoring commits")
    p_refactor.add_argument("repos", type=Path, nargs="+", help="Git repo paths")
    p_refactor.add_argument("-o", "--output", type=Path, required=True, help="Output fixtures directory")
    p_refactor.add_argument("--lang", choices=list(LANG_EXTENSIONS.keys()), help="Filter by language")
    p_refactor.add_argument("--max-tasks", type=int, default=10)
    p_refactor.add_argument("--min-files", type=int, default=2, help="Min files per commit")
    p_refactor.add_argument("--max-files", type=int, default=8, help="Max files per commit")
    p_refactor.add_argument("--max-diff", type=int, default=300, help="Max diff lines per commit")
    p_refactor.add_argument("--scan", type=int, default=500, help="Max commits to scan per repo")
    p_refactor.add_argument("--seed", type=int, default=42)

    args = parser.parse_args()

    # Validate repos exist
    for repo in args.repos:
        if not (repo / ".git").is_dir():
            print(f"Error: {repo} is not a git repository", file=sys.stderr)
            sys.exit(1)

    if args.strategy == "edit":
        fixtures = mine_edit_tasks(
            repos=args.repos,
            lang=args.lang,
            max_tasks=args.max_tasks,
            max_files_per_commit=args.max_files,
            max_diff_lines=args.max_diff,
            max_commits_scan=args.scan,
            seed=args.seed,
        )
    elif args.strategy == "refactor":
        fixtures = mine_refactor_tasks(
            repos=args.repos,
            lang=args.lang,
            max_tasks=args.max_tasks,
            min_files=args.min_files,
            max_files=args.max_files,
            max_diff_lines=args.max_diff,
            max_commits_scan=args.scan,
            seed=args.seed,
        )
    else:
        parser.error(f"Unknown strategy: {args.strategy}")

    if not fixtures:
        print("No qualifying commits found.", file=sys.stderr)
        sys.exit(1)

    write_fixtures(fixtures, args.output)


if __name__ == "__main__":
    main()
