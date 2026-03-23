"""Database access layer."""

_users = {
    1: {"id": 1, "name": "Alice", "email": "alice@example.com", "role": "admin"},
    2: {"id": 2, "name": "Bob", "email": "bob@example.com", "role": "user"},
}


def get_user(user_id: int) -> dict | None:
    """Fetch a user by ID."""
    return _users.get(user_id)


def list_users() -> list[dict]:
    """Return all users."""
    return list(_users.values())
