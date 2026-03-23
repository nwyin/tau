"""Database access layer."""

_users = {
    1: {"id": 1, "name": "Alice", "email": "alice@example.com", "role": "admin"},
    2: {"id": 2, "name": "Bob", "email": "bob@example.com", "role": "user"},
}


def get_user(user_id: int, fields: list[str] | None = None) -> dict | None:
    """Fetch a user by ID, optionally filtering to specific fields."""
    user = _users.get(user_id)
    if user is None:
        return None
    if fields is not None:
        return {k: v for k, v in user.items() if k in fields}
    return user


def list_users() -> list[dict]:
    """Return all users."""
    return list(_users.values())
