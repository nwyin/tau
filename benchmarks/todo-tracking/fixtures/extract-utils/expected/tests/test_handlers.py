"""Tests for the handler module."""

from src.handlers import handle_create, handle_get, handle_list


def test_handle_get():
    result = handle_get("1")
    assert result["status"] == "ok"
    assert result["data"]["id"] == 1


def test_handle_list():
    result = handle_list("limit=3")
    assert result["status"] == "ok"
    assert len(result["data"]["items"]) == 3


def test_handle_create():
    result = handle_create({"name": "New Item"})
    assert result["status"] == "created"
    assert result["data"]["name"] == "New Item"


def test_handle_create_missing_name():
    result = handle_create({})
    assert result["status"] == "error"
