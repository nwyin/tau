"""Variant dataclass for A/B test configurations.

Used by benchmark runners to define the configurations being compared.
Each variant specifies overrides to the default tau session settings.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Variant:
    """A named configuration for A/B benchmark comparisons.

    Attributes:
        name: Short identifier used in results and reports (e.g. "baseline", "with-diagnostics").
        description: Human-readable explanation of what this variant tests.
        edit_mode: Historical edit strategy metadata. Only "replace" is currently supported.
        tools: Tool list override. None means use default tools.
        system_prompt_suffix: Extra text appended to the system prompt for this variant.
        tau_config_overrides: Arbitrary tau config key-value overrides.
    """

    name: str
    description: str
    edit_mode: str = "replace"
    tools: list[str] | None = None
    system_prompt_suffix: str = ""
    tau_config_overrides: dict = field(default_factory=dict)
