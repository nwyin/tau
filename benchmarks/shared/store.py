"""Result storage: local files + optional S3-compatible remote via rclone.

Local results are written to benchmarks/{benchmark}/results/ (gitignored).
Remote results are synced to an S3-compatible store (R2, MinIO, etc.) via rclone.

Setup:
    # Install rclone and configure an R2 remote (one-time)
    brew install rclone
    rclone config create r2 s3 provider=Cloudflare \
        access_key_id=<KEY> secret_access_key=<SECRET> \
        endpoint=https://<ACCOUNT_ID>.r2.cloudflarestorage.com

    # Set the remote path (add to shell profile)
    export TAU_BENCH_REMOTE=r2:tau-bench-results

Library usage:
    from shared.store import ResultStore
    store = ResultStore(benchmark="fuzzy-match")
    run_id = store.save(report_dict)
    store.push(run_id)

CLI usage:
    python -m shared.store save <report.json> --benchmark fuzzy-match
    python -m shared.store push <run_id|report.json> --benchmark fuzzy-match
    python -m shared.store ls [benchmark]
    python -m shared.store pull [benchmark/run_id]
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

BENCHMARKS_DIR = Path(__file__).parent.parent


def _git_sha() -> str | None:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            capture_output=True,
            text=True,
            cwd=BENCHMARKS_DIR,
        )
        return result.stdout.strip() or None
    except FileNotFoundError:
        return None


def _git_dirty() -> bool:
    try:
        result = subprocess.run(
            ["git", "status", "--porcelain"],
            capture_output=True,
            text=True,
            cwd=BENCHMARKS_DIR,
        )
        return bool(result.stdout.strip())
    except FileNotFoundError:
        return False


def _remote() -> str | None:
    """Get remote path from env. Returns e.g. 'r2:tau-bench-results'."""
    return os.environ.get("TAU_BENCH_REMOTE")


def _has_rclone() -> bool:
    try:
        subprocess.run(["rclone", "version"], capture_output=True)
        return True
    except FileNotFoundError:
        return False


class ResultStore:
    """Manages benchmark result storage and sync."""

    def __init__(self, benchmark: str):
        self.benchmark = benchmark
        self.results_dir = BENCHMARKS_DIR / benchmark / "results"

    def save(self, report: dict | str) -> str:
        """Save a report locally. Returns the run_id.

        Enriches the report with standard metadata (run_id, timestamp, host,
        git info) if not already present.
        """
        if isinstance(report, str):
            report = json.loads(report)
        if not isinstance(report, dict):
            raise TypeError("report must be a dict or JSON object string")

        self.results_dir.mkdir(parents=True, exist_ok=True)

        now = datetime.now(timezone.utc)
        run_id = report.get("run_id") or f"{self.benchmark}-{now.strftime('%Y%m%d-%H%M%S')}"

        # Enrich with metadata
        report.setdefault("run_id", run_id)
        report.setdefault("benchmark", self.benchmark)
        report.setdefault("timestamp", now.isoformat())
        report.setdefault("host", platform.node())
        report.setdefault("git_sha", _git_sha())
        report.setdefault("git_dirty", _git_dirty())

        path = self.results_dir / f"{run_id}.json"
        path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        print(f"Saved: {path}", file=sys.stderr)
        return run_id

    def push(self, run_id: str | None = None) -> bool:
        """Push result(s) to remote. If run_id is None, push all."""
        remote = _remote()
        if not remote:
            print("TAU_BENCH_REMOTE not set, skipping push", file=sys.stderr)
            return False
        if not _has_rclone():
            print("rclone not found, skipping push", file=sys.stderr)
            return False

        if run_id:
            src = self.results_dir / f"{run_id}.json"
            dst = f"{remote}/{self.benchmark}/{run_id}.json"
            cmd = ["rclone", "copyto", str(src), dst]
        else:
            src = str(self.results_dir) + "/"
            dst = f"{remote}/{self.benchmark}/"
            cmd = ["rclone", "sync", src, dst, "--include", "*.json"]

        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            print(f"Push failed: {result.stderr}", file=sys.stderr)
            return False
        print(f"Pushed: {src} -> {dst}", file=sys.stderr)
        return True

    def pull(self, run_id: str | None = None) -> bool:
        """Pull result(s) from remote."""
        remote = _remote()
        if not remote:
            print("TAU_BENCH_REMOTE not set", file=sys.stderr)
            return False
        if not _has_rclone():
            print("rclone not found", file=sys.stderr)
            return False

        self.results_dir.mkdir(parents=True, exist_ok=True)

        if run_id:
            src = f"{remote}/{self.benchmark}/{run_id}.json"
            dst = self.results_dir / f"{run_id}.json"
            cmd = ["rclone", "copyto", src, str(dst)]
        else:
            src = f"{remote}/{self.benchmark}/"
            dst = str(self.results_dir) + "/"
            cmd = ["rclone", "sync", src, dst, "--include", "*.json"]

        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            print(f"Pull failed: {result.stderr}", file=sys.stderr)
            return False
        print(f"Pulled: {src} -> {dst}", file=sys.stderr)
        return True

    @staticmethod
    def ls(benchmark: str | None = None) -> list[str]:
        """List runs, locally or remotely."""
        runs: list[str] = []

        # Local
        if benchmark:
            dirs = [BENCHMARKS_DIR / benchmark / "results"]
        else:
            dirs = list(BENCHMARKS_DIR.glob("*/results"))

        for d in dirs:
            if d.is_dir():
                for f in sorted(d.glob("*.json")):
                    runs.append(f"{d.parent.name}/{f.stem}")

        # Remote (if configured)
        remote = _remote()
        if remote and _has_rclone():
            path = f"{remote}/{benchmark}/" if benchmark else f"{remote}/"
            result = subprocess.run(
                ["rclone", "lsf", path, "--include", "*.json"],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                prefix = benchmark or ""
                for line in result.stdout.strip().splitlines():
                    name = line.strip().removesuffix(".json")
                    full = f"{prefix}/{name}" if prefix else name
                    if full not in runs:
                        runs.append(f"{full} (remote)")

        return runs


def main():
    parser = argparse.ArgumentParser(description="Benchmark result storage")
    sub = parser.add_subparsers(dest="command", required=True)

    # save
    p_save = sub.add_parser("save", help="Save a report JSON locally")
    p_save.add_argument("report", type=Path, help="Path to report JSON file")
    p_save.add_argument("--benchmark", "-b", required=True)

    # push
    p_push = sub.add_parser("push", help="Push results to remote")
    p_push.add_argument("--benchmark", "-b", required=True)
    p_push.add_argument("--run-id", help="Specific run to push (default: all)")

    # pull
    p_pull = sub.add_parser("pull", help="Pull results from remote")
    p_pull.add_argument("--benchmark", "-b", required=True)
    p_pull.add_argument("--run-id", help="Specific run to pull (default: all)")

    # ls
    p_ls = sub.add_parser("ls", help="List runs")
    p_ls.add_argument("benchmark", nargs="?", help="Filter by benchmark")

    args = parser.parse_args()

    if args.command == "save":
        report = json.loads(args.report.read_text())
        store = ResultStore(benchmark=args.benchmark)
        store.save(report)

    elif args.command == "push":
        store = ResultStore(benchmark=args.benchmark)
        store.push(run_id=args.run_id)

    elif args.command == "pull":
        store = ResultStore(benchmark=args.benchmark)
        store.pull(run_id=args.run_id)

    elif args.command == "ls":
        runs = ResultStore.ls(benchmark=args.benchmark)
        if not runs:
            print("No runs found.", file=sys.stderr)
        for r in runs:
            print(r)


if __name__ == "__main__":
    main()
