"""Entry point for the data pipeline."""

from src.processor import process


def main() -> None:
    record_ids = [1, 2, 3, 1, 2, 4, 5, 1]
    results = process(record_ids)
    for r in results:
        print(r)


if __name__ == "__main__":
    main()
