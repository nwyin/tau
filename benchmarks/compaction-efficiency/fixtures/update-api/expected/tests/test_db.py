"""Tests for the database layer."""

from src.db import get_user, list_users


def test_get_user_found():
    user = get_user(1, fields=["id", "name"])
    assert user is not None
    assert user["name"] == "Alice"


def test_get_user_not_found():
    user = get_user(999, fields=["id", "name"])
    assert user is None


def test_list_users():
    users = list_users()
    assert len(users) == 2
