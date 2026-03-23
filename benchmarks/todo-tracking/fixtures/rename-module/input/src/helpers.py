"""Helper functions for text and date formatting."""

from datetime import datetime


def format_date(dt: datetime, style: str = "short") -> str:
    """Format a datetime for display."""
    if style == "short":
        return dt.strftime("%Y-%m-%d")
    elif style == "long":
        return dt.strftime("%B %d, %Y")
    return dt.isoformat()


def format_currency(amount: float, currency: str = "USD") -> str:
    """Format a number as currency."""
    symbols = {"USD": "$", "EUR": "E", "GBP": "L"}
    symbol = symbols.get(currency, currency)
    return f"{symbol}{amount:,.2f}"


def truncate_text(text: str, max_length: int = 50) -> str:
    """Truncate text to max_length, adding ellipsis if needed."""
    if len(text) <= max_length:
        return text
    return text[: max_length - 3] + "..."
