"""BenchConfig dataclass with CLI argument helpers.

Provides a single source of truth for standard benchmark configuration.
The ``add_cli_args`` / ``from_cli`` pair gives every runner the same
flags without duplicating argument definitions.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path


@dataclass
class BenchConfig:
    """Standard configuration shared by all online benchmark runners.

    Attributes:
        model: Model identifier passed to ``tau serve --model``.
        edit_mode: Historical result field. Tau currently supports only ``"replace"``.
        runs_per_task: Number of independent runs per task per variant.
        timeout: Seconds allowed per task before it is killed.
        concurrency: Maximum number of parallel task executions.
        max_attempts: Verification retry attempts per run.
        tau_binary: Path or name of the tau binary.
        output_dir: Directory for report output files.
    """

    model: str = "claude-sonnet-4-6"
    edit_mode: str = "replace"
    runs_per_task: int = 1
    timeout: int = 120
    concurrency: int = 4
    max_attempts: int = 1
    tau_binary: str = "tau"
    output_dir: Path = Path("results")

    @staticmethod
    def add_cli_args(parser: argparse.ArgumentParser) -> None:
        """Add standard benchmark flags to *parser*.

        Call this in every benchmark's ``run.py`` argument setup so that
        all runners expose the same knobs.
        """
        parser.add_argument("--model", default="claude-sonnet-4-6", help="Model identifier (default: claude-sonnet-4-6)")
        parser.add_argument("--edit-mode", default="replace", choices=["replace"], help="Edit strategy metadata; only replace is currently supported")
        parser.add_argument("--runs", type=int, default=1, help="Runs per task per variant (default: 1)")
        parser.add_argument("--timeout", type=int, default=120, help="Seconds per task (default: 120)")
        parser.add_argument("--concurrency", "-j", type=int, default=4, help="Parallel task execution (default: 4)")
        parser.add_argument("--max-attempts", type=int, default=1, help="Verification retry attempts per run (default: 1)")
        parser.add_argument("--tau", default="tau", help="Path to tau binary (default: tau)")
        parser.add_argument("-o", "--output", default="results", help="Output directory for reports (default: results)")

    @classmethod
    def from_cli(cls, args: argparse.Namespace) -> BenchConfig:
        """Construct a ``BenchConfig`` from parsed CLI arguments.

        Expects the namespace produced by a parser that had
        ``add_cli_args`` called on it.
        """
        return cls(
            model=args.model,
            edit_mode=args.edit_mode,
            runs_per_task=args.runs,
            timeout=args.timeout,
            concurrency=args.concurrency,
            max_attempts=args.max_attempts,
            tau_binary=args.tau,
            output_dir=Path(args.output),
        )
