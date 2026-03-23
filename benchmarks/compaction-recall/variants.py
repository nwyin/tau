"""Compaction strategy variant definitions for compaction-recall benchmark.

Each variant describes a different approach to context compaction.  The runner
applies the selected variant's configuration when triggering compaction.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

# ---------------------------------------------------------------------------
# Strategy variants
# ---------------------------------------------------------------------------

VARIANTS: dict[str, Variant] = {
    "truncation": Variant(
        name="truncation",
        description="Drop oldest turns, keep last 60% of tokens",
        tau_config_overrides={
            # TODO: requires compaction feature in tau
            "compaction_strategy": "truncation",
            "compaction_keep_ratio": 0.60,
        },
    ),
    "observation-mask": Variant(
        name="observation-mask",
        description="Replace old tool outputs with [omitted], keep tool names visible",
        tau_config_overrides={
            # TODO: requires compaction feature in tau
            "compaction_strategy": "observation-mask",
            "compaction_keep_ratio": 0.55,
        },
    ),
    "llm-summary": Variant(
        name="llm-summary",
        description="Structured LLM summary (goal/progress/decisions/next/files)",
        tau_config_overrides={
            # TODO: requires compaction feature in tau
            "compaction_strategy": "llm-summary",
            "compaction_keep_ratio": 0.35,
        },
    ),
    "progressive": Variant(
        name="progressive",
        description="OpenDev-style: mask at 80%, prune at 85%, summarize at 95%",
        tau_config_overrides={
            # TODO: requires compaction feature in tau
            "compaction_strategy": "progressive",
            "compaction_thresholds": {"mask": 0.80, "prune": 0.85, "summarize": 0.95},
        },
    ),
}

ALL_VARIANT_NAMES: list[str] = list(VARIANTS.keys())


def get_variants(names: list[str] | None = None) -> list[Variant]:
    """Return variant objects for the given names, or all if *names* is None."""
    if names is None:
        return list(VARIANTS.values())
    out: list[Variant] = []
    for n in names:
        if n not in VARIANTS:
            raise ValueError(f"Unknown variant {n!r}, expected one of {ALL_VARIANT_NAMES}")
        out.append(VARIANTS[n])
    return out
