"""API layer that uses the database."""

from src.db import get_user


def user_profile(user_id: int) -> dict:
    """Get user profile for display."""
    user = get_user(user_id)
    if user is None:
        return {"error": "not found"}
    return {"profile": user}


def user_exists(user_id: int) -> bool:
    """Check if a user exists."""
    return get_user(user_id) is not None
