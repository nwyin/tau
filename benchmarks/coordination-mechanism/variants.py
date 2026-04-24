"""Variant definitions for the coordination-mechanism benchmark."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant


VARIANTS: dict[str, Variant] = {
    "naive-parallel": Variant(
        name="naive-parallel",
        description="Runner-owned parallel launch of both producers plus critic.",
    ),
    "prompt-only-parallel": Variant(
        name="prompt-only-parallel",
        description="Same parallel launch, but critic instructions explicitly require wait/read behavior.",
    ),
    "staged-pipeline": Variant(
        name="staged-pipeline",
        description="Runner-owned phased launch with critic started second and episode injection.",
    ),
    "document-polling": Variant(
        name="document-polling",
        description="Runner-owned parallel launch where the critic must poll shared docs before completing.",
        tau_config_overrides={
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
