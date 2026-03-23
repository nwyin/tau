"""Application configuration - loads from per-environment JSON files."""

import json
import os
from pathlib import Path

CONFIG_DIR = Path(__file__).parent.parent / "config"


def load_config(env: str | None = None) -> dict:
    """Load configuration by merging base + environment-specific settings."""
    if env is None:
        env = os.environ.get("APP_ENV", "dev")

    # Load base config
    base_path = CONFIG_DIR / "base.json"
    base = json.loads(base_path.read_text()) if base_path.exists() else {}

    # Load environment-specific config
    env_path = CONFIG_DIR / f"{env}.json"
    env_config = json.loads(env_path.read_text()) if env_path.exists() else {}

    # Merge: environment overrides base
    return {**base, **env_config}


def get_config() -> dict:
    """Return configuration for the current environment."""
    return load_config()
