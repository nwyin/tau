"""Post-edit diagnostics A/B variants.

Defines configurations for testing whether compiler/linter feedback
after edits reduces edit-error-fix cycle count.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.variants import Variant

# Baseline: no compiler feedback. Model must explicitly invoke
# cargo check / tsc / ruff check itself if it wants diagnostics.
NO_DIAG = Variant(
    name="no-diag",
    description="Baseline — no automatic compiler/linter feedback after edits",
    edit_mode="replace",
    tools=None,
    system_prompt_suffix="",
    tau_config_overrides={},
)

# Prompt-only: system prompt tells the model to run compiler checks after edits.
# Tests whether awareness alone (vs automation) captures the benefit.
PROMPT_CHECK = Variant(
    name="prompt-check",
    description="System prompt instructs model to run compiler/linter after every edit",
    edit_mode="replace",
    tools=None,
    system_prompt_suffix=(
        "\n\nIMPORTANT: After every file edit, immediately run the appropriate "
        "compiler or linter to check for errors:\n"
        "- Rust: `cargo check 2>&1`\n"
        "- TypeScript: `npx tsc --noEmit 2>&1`\n"
        "- Python: `ruff check . 2>&1`\n"
        "If errors are found, fix them before moving on to the next change."
    ),
    tau_config_overrides={},
)

# Future: compiler output automatically appended to edit tool result.
# Requires ~150 LOC diagnostic hook in tau's FileEditTool.
AUTO_CHECK = Variant(
    name="auto-check",
    description="Compiler output appended to edit tool result (requires tau feature)",
    edit_mode="replace",
    tools=None,
    system_prompt_suffix="",
    tau_config_overrides={"post_edit_diagnostics": True},
)

ALL_VARIANTS = [NO_DIAG, PROMPT_CHECK, AUTO_CHECK]
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
