"""Parallel file operations A/B variants.

Defines configurations for testing whether parallel tool execution
(reading multiple files in a single turn) saves wall-clock time.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

SEQUENTIAL = Variant(
    name="sequential",
    description="Model reads files one at a time, sequentially",
    edit_mode="replace",
    tools=None,
    system_prompt_suffix=(
        "\n\nIMPORTANT: When reading files, read them ONE AT A TIME. "
        "Read a single file, process its contents, then read the next file. "
        "Do not read multiple files in the same turn."
    ),
    tau_config_overrides={},
)

PARALLEL = Variant(
    name="parallel",
    description="Model reads all files in a single turn using parallel tool calls",
    edit_mode="replace",
    tools=None,
    system_prompt_suffix=(
        "\n\nIMPORTANT: When you need to read multiple files, read ALL of them "
        "in a SINGLE turn by issuing multiple read tool calls at once. Do not "
        "read files one at a time — batch all reads together."
    ),
    tau_config_overrides={},
)

NATURAL = Variant(
    name="natural",
    description="No instruction — model decides its own read strategy",
    edit_mode="replace",
    tools=None,
    system_prompt_suffix="",
    tau_config_overrides={},
)

ALL_VARIANTS = [SEQUENTIAL, PARALLEL, NATURAL]
VARIANT_MAP: dict[str, Variant] = {v.name: v for v in ALL_VARIANTS}


def get_variants(names: list[str] | None = None) -> list[Variant]:
    """Return requested variants by name, or all if names is None."""
    if names is None:
        return list(ALL_VARIANTS)
    result = []
    for name in names:
        if name not in VARIANT_MAP:
            valid = ", ".join(VARIANT_MAP)
            raise ValueError(f"Unknown variant {name!r}. Valid: {valid}")
        result.append(VARIANT_MAP[name])
    return result
