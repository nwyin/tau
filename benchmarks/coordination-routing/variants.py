"""Variant definitions for the coordination-routing benchmark.

This benchmark isolates routing behavior when a critic thread must consume
upstream thread outputs before producing a synthesis.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant


VARIANTS: dict[str, Variant] = {
    "naive-parallel": Variant(
        name="naive-parallel",
        description="Launch position-for, position-against, and critic in one parallel batch.",
    ),
    "prompt-only-parallel": Variant(
        name="prompt-only-parallel",
        description="Keep parallel launch shape, but add stronger critic instructions to wait/read.",
    ),
    "staged-pipeline": Variant(
        name="staged-pipeline",
        description="Launch position threads first, then critic with episode injection.",
    ),
    "document-polling": Variant(
        name="document-polling",
        description="Launch critic in parallel, but force polling shared docs before completion.",
        tau_config_overrides={
            # Polling variants can legitimately run longer than other orchestration shapes.
            "timeout": 240,
        },
    ),
}

ALL_VARIANT_NAMES: list[str] = list(VARIANTS.keys())


def get_variants(names: list[str] | None = None) -> list[Variant]:
    """Return variant objects for *names*, or all variants when omitted."""
    if names is None:
        return list(VARIANTS.values())

    out: list[Variant] = []
    for name in names:
        if name not in VARIANTS:
            raise ValueError(f"Unknown variant {name!r}, expected one of {ALL_VARIANT_NAMES}")
        out.append(VARIANTS[name])
    return out
