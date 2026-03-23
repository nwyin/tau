"""Tests for per-environment configuration loading."""

from src.config import load_config


def test_load_dev_config():
    config = load_config("dev")
    assert config["app_name"] == "myapp"
    assert config["debug"] is True
    assert config["port"] == 8000
    assert config["log_level"] == "DEBUG"


def test_load_prod_config():
    config = load_config("prod")
    assert config["app_name"] == "myapp"
    assert config["debug"] is False
    assert config["port"] == 8443
    assert config["log_level"] == "WARNING"


def test_base_values_present():
    config = load_config("dev")
    assert config["version"] == "1.0.0"


def test_env_overrides_base():
    config = load_config("dev")
    # base has INFO, dev overrides to DEBUG
    assert config["log_level"] == "DEBUG"
