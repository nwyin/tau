"""Application configuration - monolithic."""

import os


def get_config() -> dict:
    """Return configuration for the current environment."""
    env = os.environ.get("APP_ENV", "dev")

    base = {
        "app_name": "myapp",
        "version": "1.0.0",
        "log_level": "INFO",
    }

    if env == "dev":
        return {
            **base,
            "debug": True,
            "database_url": "sqlite:///dev.db",
            "host": "localhost",
            "port": 8000,
            "log_level": "DEBUG",
        }
    elif env == "prod":
        return {
            **base,
            "debug": False,
            "database_url": "postgresql://prod-host/myapp",
            "host": "0.0.0.0",
            "port": 8443,
            "log_level": "WARNING",
        }
    else:
        return base
