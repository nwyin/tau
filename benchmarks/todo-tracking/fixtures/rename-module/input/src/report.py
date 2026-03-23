"""Report generation using helper formatting."""

from src.helpers import format_currency, format_date, truncate_text


def generate_report(title: str, items: list[dict]) -> str:
    """Generate a text report."""
    lines = [truncate_text(title, 40), "=" * 40, ""]

    for item in items:
        date = format_date(item["date"], style="long")
        amount = format_currency(item["amount"])
        desc = truncate_text(item.get("description", ""), 30)
        lines.append(f"  {date}: {amount} - {desc}")

    return "\n".join(lines)
