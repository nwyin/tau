"""Tests for the formatting module (renamed from helpers)."""

from datetime import datetime

from src.formatting import format_currency, format_date, truncate_text


def test_format_date_short():
    dt = datetime(2024, 1, 15)
    assert format_date(dt) == "2024-01-15"


def test_format_date_long():
    dt = datetime(2024, 1, 15)
    assert format_date(dt, style="long") == "January 15, 2024"


def test_format_currency_usd():
    assert format_currency(1234.5) == "$1,234.50"


def test_truncate_short():
    assert truncate_text("hello", 10) == "hello"


def test_truncate_long():
    assert truncate_text("a" * 60, 50) == "a" * 47 + "..."
