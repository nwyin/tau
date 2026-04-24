"""Variant dataclass for A/B test configurations.

Used by benchmark runners to define the configurations being compared.
Each variant specifies overrides to the default tau session settings.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Variant:
    """A named configuration for A/B benchmark comparisons.

    Attributes:
        name: Short identifier used in results and reports (e.g. "baseline", "with-diagnostics").
        description: Human-readable explanation of what this variant tests.
        edit_mode: Legacy metadata field retained for older runners; Tau only supports replace.
        tools: Tool list override. None means use default tools.
        system_prompt_suffix: Legacy prompt text field retained for older runners.
        tau_config_overrides: Runtime knobs consumed by benchmark runners.
    """

    name: str
    description: str
    edit_mode: str = "replace"
    tools: list[str] | None = None
    system_prompt_suffix: str = ""
    tau_config_overrides: dict[str, Any] = field(default_factory=dict)

    def timeout(self, default: int) -> int:
        """Return this variant's session timeout override, or *default*."""
        return int(self.tau_config_overrides.get("timeout", default))
