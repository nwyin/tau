#!/usr/bin/env python3
"""Run a self-improvement eval matrix and compute one composite score.

This is intentionally a thin harness around existing benchmarks. Eval commands
own their task setup and scoring. This runner only repeats them across models,
parses an objective pass ratio, and writes a comparable experiment report.
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCORE_JSON_RE = re.compile(r"^SELF_IMPROVE_EVAL\s+({.*})\s*$", re.MULTILINE)
LEGACY_RESULT_RE = re.compile(r"Result:\s*(\d+)\s*/\s*(\d+)\s+passed", re.IGNORECASE)


@dataclass(frozen=True)
class EvalSpec:
    name: str
    command: str


def parse_eval(value: str) -> EvalSpec:
    if "=" not in value:
        raise argparse.ArgumentTypeError("eval must be NAME=COMMAND")
    name, command = value.split("=", 1)
    name = name.strip()
    command = command.strip()
    if not name or not command:
        raise argparse.ArgumentTypeError("eval must include both NAME and COMMAND")
    return EvalSpec(name=name, command=command)


def git_value(args: list[str], default: str) -> str:
    try:
        return subprocess.check_output(["git", *args], text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return default


def format_command(spec: EvalSpec, model: str, run_index: int, output_dir: Path) -> list[str]:
    raw_command = spec.command
    command = (
        raw_command.replace("{model}", model)
        .replace("{run}", str(run_index))
        .replace("{output_dir}", str(output_dir))
    )
    argv = shlex.split(command)
    if "{model}" not in raw_command:
        argv.extend(["--model", model])
    return argv


def parse_score_json(stdout: str) -> dict[str, Any] | None:
    match = SCORE_JSON_RE.search(stdout)
    if not match:
        return None

    data = json.loads(match.group(1))
    if "passed" not in data or "total" not in data:
        raise ValueError("SELF_IMPROVE_EVAL JSON must include passed and total")
    return data


def parse_report_json(path: Path) -> dict[str, Any] | None:
    report_path = path / "report.json"
    if not report_path.exists():
        return None

    data = json.loads(report_path.read_text())
    summary = data.get("summary", data)

    if "passed" in summary and "total" in summary:
        return {"passed": summary["passed"], "total": summary["total"], "source": str(report_path)}

    if "pass_rate" in summary:
        return {"passed": float(summary["pass_rate"]), "total": 1, "source": str(report_path)}

    if "resolved" in summary and "total_tasks" in summary:
        return {"passed": summary["resolved"], "total": summary["total_tasks"], "source": str(report_path)}

    return None


def parse_legacy_result(stdout: str) -> dict[str, Any] | None:
    matches = LEGACY_RESULT_RE.findall(stdout)
    if not matches:
        return None
    passed, total = matches[-1]
    return {"passed": int(passed), "total": int(total), "source": "stdout"}


def normalize_score(score: dict[str, Any]) -> tuple[float, float, float]:
    passed = float(score["passed"])
    total = float(score["total"])
    if total <= 0:
        raise ValueError("score total must be greater than zero")
    return passed, total, passed / total


def run_one(
    spec: EvalSpec,
    model: str,
    run_index: int,
    output_root: Path,
    timeout: int,
) -> dict[str, Any]:
    cell_name = f"{spec.name}__{model.replace('/', '_')}__run-{run_index}"
    cell_dir = output_root / cell_name
    cell_dir.mkdir(parents=True, exist_ok=True)

    argv = format_command(spec, model, run_index, cell_dir)
    started = time.monotonic()
    proc = subprocess.run(
        argv,
        cwd=repo_root(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = int((time.monotonic() - started) * 1000)

    (cell_dir / "stdout.log").write_text(proc.stdout)
    (cell_dir / "stderr.log").write_text(proc.stderr)

    score = parse_score_json(proc.stdout) or parse_report_json(cell_dir) or parse_legacy_result(proc.stdout)
    if score is None:
        raise RuntimeError(
            f"{spec.name} for {model} did not emit a parseable score; "
            f"see {cell_dir / 'stdout.log'} and {cell_dir / 'stderr.log'}"
        )

    passed, total, pass_rate = normalize_score(score)
    return {
        "eval": spec.name,
        "model": model,
        "run_index": run_index,
        "argv": argv,
        "exit_code": proc.returncode,
        "elapsed_ms": elapsed_ms,
        "passed": passed,
        "total": total,
        "pass_rate": pass_rate,
        "score_source": score.get("source", "SELF_IMPROVE_EVAL"),
        "stdout_log": str(cell_dir / "stdout.log"),
        "stderr_log": str(cell_dir / "stderr.log"),
    }


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def load_baseline(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n")


def build_report(label: str, results: list[dict[str, Any]]) -> dict[str, Any]:
    composite = sum(result["pass_rate"] for result in results)
    max_score = len(results)
    normalized = composite / max_score if max_score else 0.0
    return {
        "label": label,
        "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "git": {
            "commit": git_value(["rev-parse", "--short", "HEAD"], "unknown"),
            "dirty": bool(git_value(["status", "--porcelain"], "")),
        },
        "summary": {
            "cells": max_score,
            "composite_score": round(composite, 6),
            "max_score": max_score,
            "normalized_score": round(normalized, 6),
        },
        "results": results,
    }


def compare_or_exit(
    report: dict[str, Any],
    baseline_path: Path | None,
    min_delta: float,
    allow_regression: bool,
) -> None:
    if baseline_path is None:
        return

    baseline = load_baseline(baseline_path)
    current_score = float(report["summary"]["composite_score"])
    baseline_score = float(baseline["summary"]["composite_score"])
    delta = current_score - baseline_score

    print(f"BASELINE_SCORE composite={baseline_score:.6f} path={baseline_path}")
    print(f"DELTA composite={delta:+.6f} min_delta={min_delta:.6f}")

    if not allow_regression and delta < min_delta:
        raise SystemExit(2)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", default="experiment", help="Report label, e.g. baseline or experiment")
    parser.add_argument("--eval", dest="evals", type=parse_eval, action="append", required=True, help="Eval command as NAME=COMMAND")
    parser.add_argument("--model", dest="models", action="append", required=True, help="Model to run; repeat for a matrix")
    parser.add_argument("--runs", type=int, default=1, help="Runs per eval/model cell")
    parser.add_argument("--timeout", type=int, default=900, help="Seconds before killing one eval process")
    parser.add_argument("--baseline", type=Path, help="Prior self-improve-report.json to compare against")
    parser.add_argument("--min-delta", type=float, default=0.0, help="Required composite score delta over baseline")
    parser.add_argument("--allow-regression", action="store_true", help="Do not fail when score is below baseline")
    parser.add_argument("-o", "--output", type=Path, default=Path("results/self-improve"), help="Output directory")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)

    if args.runs < 1:
        raise SystemExit("--runs must be at least 1")

    output_root = args.output
    output_root.mkdir(parents=True, exist_ok=True)

    results: list[dict[str, Any]] = []
    for spec in args.evals:
        for model in args.models:
            for run_index in range(args.runs):
                print(f"Running eval={spec.name} model={model} run={run_index}")
                results.append(run_one(spec, model, run_index, output_root, args.timeout))

    report = build_report(args.label, results)
    report_path = output_root / "self-improve-report.json"
    write_report(report_path, report)

    summary = report["summary"]
    print(
        "SELF_IMPROVE_SCORE "
        f"composite={summary['composite_score']:.6f} "
        f"max={summary['max_score']} "
        f"normalized={summary['normalized_score']:.6f} "
        f"report={report_path}"
    )

    compare_or_exit(report, args.baseline, args.min_delta, args.allow_regression)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except subprocess.TimeoutExpired as exc:
        print(f"ERROR: timed out after {exc.timeout}s: {exc.cmd}", file=sys.stderr)
        raise SystemExit(124)
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
