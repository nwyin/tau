"""Tests for the extracted utility functions."""

import pytest

from src.utils import format_response, parse_query, validate_id


def test_format_response_default():
    result = format_response({"key": "value"})
    assert result == {"status": "ok", "data": {"key": "value"}}


def test_format_response_custom_status():
    result = format_response({"key": "value"}, status="error")
    assert result["status"] == "error"


def test_validate_id_valid():
    assert validate_id("42") == 42


def test_validate_id_with_whitespace():
    assert validate_id(" 7 ") == 7


def test_validate_id_invalid():
    with pytest.raises(ValueError):
        validate_id("abc")


def test_validate_id_empty():
    with pytest.raises(ValueError):
        validate_id("")


def test_parse_query_simple():
    result = parse_query("key1=val1&key2=val2")
    assert result == {"key1": "val1", "key2": "val2"}


def test_parse_query_empty():
    assert parse_query("") == {}


def test_parse_query_single():
    assert parse_query("limit=10") == {"limit": "10"}
