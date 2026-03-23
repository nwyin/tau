"""Strategy + compression level variant definitions for compaction-efficiency.

Two dimensions:
  - Strategy: none, truncation, observation-mask, llm-summary, progressive
  - Compression: conservative (keep 60%), moderate (keep 40%), aggressive (keep 20%)

Some combinations are intentionally excluded (see SPEC.md):
  - truncation/aggressive — known to be destructive
  - llm-summary/conservative — not worth the LLM cost for light compression
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

# ---------------------------------------------------------------------------
# Compression levels
# ---------------------------------------------------------------------------

COMPRESSION_LEVELS: dict[str, float] = {
    "conservative": 0.60,  # keep 60% of tokens
    "moderate": 0.40,  # keep 40% of tokens
    "aggressive": 0.20,  # keep 20% of tokens
}

# ---------------------------------------------------------------------------
# Strategies
# ---------------------------------------------------------------------------

STRATEGIES: list[str] = [
    "none",
    "truncation",
    "observation-mask",
    "llm-summary",
    "progressive",
]

# ---------------------------------------------------------------------------
# Excluded combinations
# ---------------------------------------------------------------------------

EXCLUDED: set[tuple[str, str]] = {
    ("truncation", "aggressive"),  # known to be destructive
    ("llm-summary", "conservative"),  # not worth the LLM cost
}

# ---------------------------------------------------------------------------
# Build variant matrix
# ---------------------------------------------------------------------------


def _build_variant(strategy: str, compression: str | None, keep_ratio: float | None) -> Variant:
    """Build a single variant from strategy + compression level."""
    if strategy == "none":
        return Variant(
            name="none",
            description="Full history, no compaction (baseline)",
            tau_config_overrides={
                # TODO: requires compaction feature in tau
                "compaction_strategy": "none",
            },
        )

    assert compression is not None and keep_ratio is not None
    name = f"{strategy}/{compression}"

    return Variant(
        name=name,
        description=f"{strategy} strategy at {compression} compression (keep {keep_ratio:.0%})",
        tau_config_overrides={
            # TODO: requires compaction feature in tau
            "compaction_strategy": strategy,
            "compaction_keep_ratio": keep_ratio,
        },
    )


def build_variant_matrix(
    strategies: list[str] | None = None,
    compressions: list[str] | None = None,
) -> list[Variant]:
    """Build the full variant matrix, excluding known-bad combinations.

    Returns a list of Variant objects suitable for the runner.
    """
    strats = strategies or STRATEGIES
    comps = compressions or list(COMPRESSION_LEVELS.keys())

    variants: list[Variant] = []

    for strategy in strats:
        if strategy == "none":
            variants.append(_build_variant("none", None, None))
            continue

        if strategy not in STRATEGIES:
            raise ValueError(f"Unknown strategy {strategy!r}, expected one of {STRATEGIES}")

        for compression in comps:
            if compression not in COMPRESSION_LEVELS:
                raise ValueError(f"Unknown compression {compression!r}, expected one of {list(COMPRESSION_LEVELS.keys())}")

            if (strategy, compression) in EXCLUDED:
                continue

            keep_ratio = COMPRESSION_LEVELS[compression]
            variants.append(_build_variant(strategy, compression, keep_ratio))

    return variants


ALL_STRATEGIES = STRATEGIES
ALL_COMPRESSIONS = list(COMPRESSION_LEVELS.keys())
