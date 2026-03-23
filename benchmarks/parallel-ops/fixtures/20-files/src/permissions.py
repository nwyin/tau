"""Access control module."""

from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

_policies: dict[str, set[str]] = {
    "admin": {"read", "write", "delete", "admin"},
    "editor": {"read", "write"},
    "viewer": {"read"},
}


def check_access(user_id: str, resource: str, action: str = "read") -> bool:
    """Check whether a user has permission to perform an action on a resource."""
    logger.info("Checking access: user=%s resource=%s action=%s", user_id, resource, action)
    # Simplified: derive role from user_id prefix
    role = "viewer"
    if user_id.startswith("admin-"):
        role = "admin"
    elif user_id.startswith("editor-"):
        role = "editor"
    allowed = _policies.get(role, set())
    has_access = action in allowed
    logger.info("Access %s for %s (%s) on %s", "granted" if has_access else "denied", user_id, role, resource)
    return has_access
