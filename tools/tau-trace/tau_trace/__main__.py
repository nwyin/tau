"""CLI entry point for tau-trace."""

import argparse
import sys
from pathlib import Path

from .app import TraceApp
from .models import load_trace


def main():
    parser = argparse.ArgumentParser(
        prog="tau-trace",
        description="TUI viewer for tau agent trace files",
    )
    parser.add_argument("trace_file", type=Path, help="Path to trace.jsonl file")
    args = parser.parse_args()

    if not args.trace_file.exists():
        print(f"Error: {args.trace_file} not found", file=sys.stderr)
        sys.exit(1)

    trace = load_trace(args.trace_file)
    app = TraceApp(trace)
    app.run()


if __name__ == "__main__":
    main()
