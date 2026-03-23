"""Basic app test."""

from src.config import get_config


def test_get_config_returns_dict():
    config = get_config()
    assert isinstance(config, dict)
    assert "app_name" in config
