"""Configuration for the data pipeline."""

from dataclasses import dataclass


@dataclass
class Config:
    """Pipeline configuration."""

    cache_size: int = 128
