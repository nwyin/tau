from __future__ import annotations

import json
import os
import shlex
from pathlib import Path

from harbor.agents.installed.base import BaseInstalledAgent, ExecInput
from harbor.models.agent.context import AgentContext
from harbor.models.trial.paths import EnvironmentPaths


class TauAgent(BaseInstalledAgent):
    @staticmethod
    def name() -> str:
        return "tau"

    def version(self) -> str | None:
        return None

    @property
    def _install_agent_template_path(self) -> Path:
        return Path(__file__).parent / "install-tau.sh.j2"

    def _setup_env(self) -> dict[str, str]:
        env = {"DEBIAN_FRONTEND": "noninteractive"}
        if "TAU_BINARY_URL" in os.environ:
            env["TAU_BINARY_URL"] = os.environ["TAU_BINARY_URL"]
        return env

    def create_run_agent_commands(self, instruction: str) -> list[ExecInput]:
        model = self._parsed_model_name or "gpt-4o-mini"
        max_turns = os.environ.get("TAU_MAX_TURNS", "200")
        env: dict[str, str] = {"TAU_MAX_TURNS": max_turns, "TAU_MODEL": model}

        for key in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY"):
            if key in os.environ:
                env[key] = os.environ[key]

        stats_path = f"{EnvironmentPaths.agent_dir}/tau-stats.json"

        return [
            ExecInput(command=f"mkdir -p {EnvironmentPaths.agent_dir}", env=env),
            ExecInput(
                command=(
                    f"/usr/local/bin/coding-agent"
                    f" --prompt {shlex.quote(instruction)}"
                    f" --model {model}"
                    f" --stats-json {stats_path}"
                    f" --no-session"
                ),
                env=env,
                timeout_sec=3600,
            ),
        ]

    def populate_context_post_run(self, context: AgentContext) -> None:
        stats_file = self.logs_dir / "tau-stats.json"
        if not stats_file.exists():
            return
        stats = json.loads(stats_file.read_text())
        totals = stats.get("totals", {})
        context.n_input_tokens = totals.get("input_tokens")
        context.n_output_tokens = totals.get("output_tokens")
        cache_tokens = (totals.get("cache_read_tokens", 0) or 0) + (
            totals.get("cache_write_tokens", 0) or 0
        )
        context.n_cache_tokens = cache_tokens or None
        context.cost_usd = totals.get("total_cost") or None
        context.metadata = {"tau_stats": stats}
