"""Database migration module."""

from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

_applied: set[str] = set()


def run_migration(version: str, dry_run: bool = False) -> bool:
    """Apply a database migration by version string."""
    logger.info("Running migration %s (dry_run=%s)", version, dry_run)
    if version in _applied:
        logger.info("Migration %s already applied", version)
        return True
    # Simulate migration steps
    steps = ["validate_schema", "alter_tables", "migrate_data", "update_version"]
    for step in steps:
        logger.debug("Migration %s: executing %s", version, step)
        if dry_run:
            logger.info("Dry run: would execute %s", step)
    if not dry_run:
        _applied.add(version)
    logger.info("Migration %s %s", version, "simulated" if dry_run else "applied")
    return True
