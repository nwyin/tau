"""Application entry point."""

from src.config import get_config


def start() -> None:
    config = get_config()
    print(f"Starting {config['app_name']} on {config.get('host', 'localhost')}:{config.get('port', 8000)}")
    print(f"Debug: {config.get('debug', False)}")
