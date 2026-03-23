"""Main application that uses helper formatting."""

from src.formatting import format_currency, format_date


def display_order(order: dict) -> str:
    """Format an order for display."""
    date_str = format_date(order["created_at"])
    total_str = format_currency(order["total"])
    return f"Order #{order['id']} - {date_str} - {total_str}"


def display_summary(orders: list[dict]) -> str:
    """Format a summary of orders."""
    total = sum(o["total"] for o in orders)
    return f"{len(orders)} orders, total: {format_currency(total)}"
