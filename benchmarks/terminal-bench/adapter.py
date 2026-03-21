"""
Tau adapter for Terminal-Bench evaluation framework.

Wires the tau binary into Terminal-Bench's BaseAgent interface,
enabling tau to be evaluated against Terminal-Bench's curated Docker-based tasks.
"""

import json
import time

try:
    from terminal_bench.agent import BaseAgent, AgentResult
    from terminal_bench.tmux import TmuxSession
except ImportError as e:
    raise ImportError(
        "terminal-bench is required to use this adapter. "
        "Install it with: pip install terminal-bench>=0.1.0\n"
        f"Original error: {e}"
    ) from e


class TauAgent(BaseAgent):
    """
    Terminal-Bench agent adapter for the tau binary.

    The binary is expected to be installed at `binary_path` inside the Docker
    container. Use install.sh to set that up before running evaluations.

    Environment variables forwarded to the container session:
    - TAU_MAX_TURNS: overrides the default max turn count
    - TAU_MODEL:     overrides the model used by the agent
    API keys are passed through automatically by Terminal-Bench from the host env.
    """

    def __init__(
        self,
        model: str = "claude-sonnet-4-20250514",
        max_turns: int = 50,
        binary_path: str = "/usr/local/bin/tau",
        **kwargs,
    ):
        super().__init__(**kwargs)
        self.model = model
        self.max_turns = max_turns
        self.binary_path = binary_path

    def perform_task(self, instruction: str, session: TmuxSession) -> AgentResult:
        """
        Run tau inside the Terminal-Bench tmux session.

        Steps:
        1. Export config env vars into the session.
        2. Run the tau binary with the task instruction.
        3. Poll until the process exits (up to 1 hour).
        4. Read the stats JSON written by the agent.
        5. Return token usage via AgentResult.
        """
        # Step 1 — export config env vars
        session.send_keys(f"export TAU_MAX_TURNS={self.max_turns}", enter=True)
        session.send_keys(f"export TAU_MODEL={self.model}", enter=True)
        # API keys come from the host env — Terminal-Bench passes them through automatically

        # Step 2 — build and run the command
        # Escape single quotes in the instruction so it survives the shell quoting
        escaped_instruction = instruction.replace("'", "'\\\"'\\\"'")
        cmd = (
            f"{self.binary_path} "
            f"--prompt '{escaped_instruction}' "
            f"--model {self.model} "
            f"--stats-json /tmp/tau-stats.json "
            f"--no-session"
        )
        session.send_keys(cmd, enter=True)

        # Step 3 — wait for the agent process to finish
        timeout = 3600  # 1 hour max
        start = time.time()
        while time.time() - start < timeout:
            result = session._run_command("pgrep -f tau")
            if result.strip() == "":
                break
            time.sleep(10)

        # Step 4 — read stats JSON from the container
        stats_output = session._run_command("cat /tmp/tau-stats.json 2>/dev/null || echo \"{}\"")
        try:
            stats = json.loads(stats_output.strip())
        except (json.JSONDecodeError, ValueError):
            stats = {}

        totals = stats.get("totals", {})

        # Step 5 — return token usage
        return AgentResult(
            total_input_tokens=totals.get("input_tokens", 0),
            total_output_tokens=totals.get("output_tokens", 0),
        )
