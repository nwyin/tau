from __future__ import annotations

import json
import os
import shlex
from pathlib import Path

from harbor.agents.installed.base import BaseInstalledAgent, ExecInput
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext
from harbor.models.trial.paths import EnvironmentPaths


_CODEX_AUTH_PATH = Path.home() / ".codex" / "auth.json"
_BINARY_NAME = "coding-agent"
_BINARY_DEST = f"/usr/local/bin/{_BINARY_NAME}"


def _find_binary() -> Path | None:
    """Locate the coding-agent Linux binary, checking common locations."""
    candidates = [
        # Explicit env override
        os.environ.get("TAU_BINARY_PATH"),
        # Native release build in repo
        Path(__file__).resolve().parents[2] / "target" / "release" / _BINARY_NAME,
        # Musl cross-compiled release build in repo
        Path(__file__).resolve().parents[2]
        / "target"
        / "x86_64-unknown-linux-musl"
        / "release"
        / _BINARY_NAME,
    ]
    for c in candidates:
        if c is None:
            continue
        p = Path(c)
        if p.is_file():
            return p
    return None


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
        for key, value in os.environ.items():
            if key.startswith("TAU_BINARY_"):
                env[key] = value
        return env

    async def setup(self, environment: BaseEnvironment) -> None:
        """Upload the binary directly, falling back to the install script."""
        binary = _find_binary()
        if binary is not None:
            await environment.upload_file(
                source_path=binary, target_path=_BINARY_DEST
            )
            await environment.exec(command=f"chmod +x {_BINARY_DEST}")
            return
        # Fall back to original install script (URL download, etc.)
        await super().setup(environment)

    def _read_codex_auth(self) -> str | None:
        """Read ~/.codex/auth.json from host if it exists and no OPENAI_API_KEY is set."""
        if "OPENAI_API_KEY" in os.environ:
            return None
        try:
            return _CODEX_AUTH_PATH.read_text()
        except OSError:
            return None

    def create_run_agent_commands(self, instruction: str) -> list[ExecInput]:
        model = self._parsed_model_name or "gpt-4o-mini"
        max_turns = os.environ.get("TAU_MAX_TURNS", "200")
        env: dict[str, str] = {"TAU_MAX_TURNS": max_turns, "TAU_MODEL": model}

        for key in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY"):
            if key in os.environ:
                env[key] = os.environ[key]

        codex_auth = self._read_codex_auth()
        if codex_auth is not None:
            env["CODEX_AUTH_JSON"] = codex_auth

        stats_path = f"{EnvironmentPaths.agent_dir}/tau-stats.json"

        setup_parts = [f"mkdir -p {EnvironmentPaths.agent_dir}"]
        if codex_auth is not None:
            setup_parts.append(
                "mkdir -p ~/.codex && printenv CODEX_AUTH_JSON > ~/.codex/auth.json"
            )

        return [
            ExecInput(command=" && ".join(setup_parts), env=env),
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
