"""Variant definitions for fuzzy-e2e benchmark.

Five edit strategy variants tested against the same task fixtures:
- tau-exact:     current replace mode, no fuzzy fallback
- tau-trimws:    exact + trailing-whitespace normalization
- tau-fuzzy-92:  Levenshtein >= 0.92 threshold (candidate from Phase 1)
- tau-hashline:  hashline addressing mode
- baseline-opi:  oh-my-pi with default fuzzy (reference harness)
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

VARIANTS: dict[str, Variant] = {
    "tau-exact": Variant(
        name="tau-exact",
        description="Current replace mode, no fuzzy fallback",
        edit_mode="replace",
        tau_config_overrides={"fuzzy_match": False},
    ),
    "tau-trimws": Variant(
        name="tau-trimws",
        description="Exact + trailing-whitespace normalization",
        edit_mode="replace",
        tau_config_overrides={"fuzzy_match": "trimws"},
    ),
    "tau-fuzzy-92": Variant(
        name="tau-fuzzy-92",
        description="Levenshtein >= 0.92 (candidate threshold from Phase 1)",
        edit_mode="replace",
        tau_config_overrides={"fuzzy_match": True, "fuzzy_threshold": 0.92},
    ),
    "tau-hashline": Variant(
        name="tau-hashline",
        description="Hashline addressing mode",
        edit_mode="hashline",
    ),
    "baseline-opi": Variant(
        name="baseline-opi",
        description="oh-my-pi with default fuzzy (reference harness)",
        edit_mode="replace",
        tau_config_overrides={"binary": "opi", "fuzzy_match": True},
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
