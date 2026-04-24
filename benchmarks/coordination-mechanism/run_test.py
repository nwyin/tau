from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
sys.path.insert(0, str(Path(__file__).parent.parent))

from run import build_variant_scaffold
from shared.coordination import CoordinationExpectations, scaffold_hash


EXPECTATIONS = CoordinationExpectations(
    pro_alias="builder-api",
    con_alias="builder-tests",
    critic_alias="reviewer",
    pro_doc="api_build_notes",
    con_doc="test_build_notes",
    pro_markers=["API_ANCHOR_SCHEMA_18"],
    con_markers=["TEST_ANCHOR_RETRY_44"],
    pro_task="Write API artifact",
    con_task="Write test artifact",
    critic_task="Review both artifacts",
    final_doc="final_review",
)


def test_build_variant_scaffold_is_deterministic_and_hash_stable() -> None:
    first = build_variant_scaffold(EXPECTATIONS, "staged-pipeline")
    second = build_variant_scaffold(EXPECTATIONS, "staged-pipeline")

    assert first == second
    assert scaffold_hash(first) == scaffold_hash(second)
    assert "episodes=[pro_alias, con_alias]" in first
    assert "name=final_doc" in first


def test_document_polling_scaffold_owns_parallel_topology() -> None:
    scaffold = build_variant_scaffold(EXPECTATIONS, "document-polling")

    assert "results = tau.parallel(" in scaffold
    assert "tau.Thread(critic_alias, critic_task, max_turns=16)" in scaffold
    assert "tau.document(operation='write', name=final_doc, content=final_text)" in scaffold
