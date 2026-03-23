"""Variant definitions for subagent-decomposition benchmark.

Four execution strategy variants:
- single-agent:  one TauSession does everything
- sub-msg:       agent 1 extracts, summary passed to agents 2-N for caller updates
- sub-discover:  agent 1 extracts, agents 2-N discover changes by reading files
- hive:          Hive orchestrator coordinates (placeholder)
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

VARIANTS: dict[str, Variant] = {
    "single-agent": Variant(
        name="single-agent",
        description="One TauSession does everything",
        edit_mode="replace",
    ),
    "sub-msg": Variant(
        name="sub-msg",
        description="Agent 1 extracts, summary passed to agents 2-N for caller updates",
        edit_mode="replace",
        tau_config_overrides={"orchestration": "message-passing"},
    ),
    "sub-discover": Variant(
        name="sub-discover",
        description="Agent 1 extracts, agents 2-N discover changes by reading files",
        edit_mode="replace",
        tau_config_overrides={"orchestration": "discovery"},
    ),
    "hive": Variant(
        name="hive",
        description="Hive orchestrator coordinates (placeholder)",
        edit_mode="replace",
        tau_config_overrides={"orchestration": "hive"},
    ),
}


def get_variants(names: list[str] | None = None) -> list[Variant]:
    """Return requested variants, or all if none specified."""
    if names is None:
        return list(VARIANTS.values())
    result = []
    for name in names:
        if name not in VARIANTS:
            raise ValueError(f"Unknown variant: {name!r}. Available: {', '.join(VARIANTS.keys())}")
        result.append(VARIANTS[name])
    return result
