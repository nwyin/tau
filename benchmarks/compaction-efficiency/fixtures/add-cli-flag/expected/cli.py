"""Command-line interface for the data processor."""

import argparse


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Process data files")
    parser.add_argument("input", help="Input file path")
    parser.add_argument("-o", "--output", default="out.json", help="Output file path")
    parser.add_argument("--format", choices=["json", "csv"], default="json", help="Output format")
    parser.add_argument("--verbose", action="store_true", help="Print verbose output")
    return parser


def run(args: argparse.Namespace) -> None:
    print(f"Processing {args.input} -> {args.output} (format={args.format})")


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    if args.verbose:
        print(f"Verbose: args={args}")
    run(args)


if __name__ == "__main__":
    main()
