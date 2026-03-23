from __future__ import annotations

from report import generate_detail, generate_summary


def run() -> None:
    entries = [
        {"name": "revenue", "value": 1234.5},
        {"name": "users", "value": 42},
        {"name": "conversion", "value": 0.156, "notes": "Up from last month"},
    ]
    print(generate_summary(entries))
    print()
    print(generate_detail(entries))


if __name__ == "__main__":
    run()
