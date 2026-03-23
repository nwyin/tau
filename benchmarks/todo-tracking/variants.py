"""Variant definitions for the todo-tracking benchmark.

Five variants spanning the spectrum from no tracking to mandatory plan injection:

  - baseline: no todo tracking
  - optional-tool: TodoWrite/Read available but not mandated
  - mandatory-prompt: system prompt forces todo_write before each step
  - plan-mode: read-only exploration phase, then execution phase
  - periodic-inject: re-inject plan state every 10 turns
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

# ---------------------------------------------------------------------------
# System prompt additions
# ---------------------------------------------------------------------------

_MANDATORY_PROMPT_SUFFIX = """
Before starting any implementation step, call todo_write to outline your plan.
Update the plan status as you complete each step. This helps maintain context
across the conversation.
""".strip()

_PERIODIC_INJECT_SUFFIX = """
You have access to todo_write and todo_read tools for tracking your progress.
Use them to maintain awareness of your plan as you work.
""".strip()

# ---------------------------------------------------------------------------
# Tool definitions (for variants that need todo tools)
# ---------------------------------------------------------------------------

TODO_TOOLS = ["todo_write", "todo_read"]

# ---------------------------------------------------------------------------
# Variant definitions
# ---------------------------------------------------------------------------

VARIANTS: dict[str, Variant] = {
    "baseline": Variant(
        name="baseline",
        description="No todo tracking, standard system prompt",
        tau_config_overrides={},
    ),
    "optional-tool": Variant(
        name="optional-tool",
        description="TodoWrite/Read available but not mandated",
        tools=TODO_TOOLS,
        tau_config_overrides={
            # TODO: requires todo tracking feature in tau
            "todo_tracking": "optional",
        },
    ),
    "mandatory-prompt": Variant(
        name="mandatory-prompt",
        description="System prompt forces todo_write before each step",
        tools=TODO_TOOLS,
        system_prompt_suffix=_MANDATORY_PROMPT_SUFFIX,
        tau_config_overrides={
            # TODO: requires todo tracking feature in tau
            "todo_tracking": "mandatory",
        },
    ),
    "plan-mode": Variant(
        name="plan-mode",
        description="Read-only exploration first, explicit approval, then execute",
        tau_config_overrides={
            # TODO: requires todo tracking feature in tau
            "todo_tracking": "plan-mode",
            "plan_mode_readonly_tools": ["file_read", "grep", "glob"],
        },
    ),
    "periodic-inject": Variant(
        name="periodic-inject",
        description="Re-inject current plan state every 10 turns",
        tools=TODO_TOOLS,
        system_prompt_suffix=_PERIODIC_INJECT_SUFFIX,
        tau_config_overrides={
            # TODO: requires todo tracking feature in tau
            "todo_tracking": "periodic-inject",
            "inject_interval": 10,
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
