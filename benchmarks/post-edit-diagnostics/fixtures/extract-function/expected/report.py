from __future__ import annotations


def format_entry(entry: dict) -> str:
    """Validate entry and return formatted 'name: value' string."""
    if "name" not in entry or "value" not in entry:
        raise ValueError(f"Entry missing required keys: {entry}")
    value = entry["value"]
    if isinstance(value, float):
        formatted = f"{value:.2f}"
    else:
        formatted = str(value)
    return f"{entry['name']}: {formatted}"


def generate_summary(entries: list[dict]) -> str:
    """Generate a summary report from entries."""
    lines = [format_entry(entry) for entry in entries]
    return "Summary\n" + "\n".join(lines)


def generate_detail(entries: list[dict], include_header: bool = True) -> str:
    """Generate a detailed report from entries."""
    lines = []
    if include_header:
        lines.append("Detailed Report")
        lines.append("=" * 40)
    for entry in entries:
        lines.append(format_entry(entry))
        if "notes" in entry:
            lines.append(f"  Notes: {entry['notes']}")
    return "\n".join(lines)


def generate_csv(entries: list[dict]) -> str:
    """Generate CSV output from entries."""
    lines = ["name,value"]
    for entry in entries:
        lines.append(f"{entry['name']},{entry['value']}")
    return "\n".join(lines)
