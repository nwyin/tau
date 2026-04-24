"""TauSession: context manager wrapping ``tau serve`` JSON-RPC 2.0.

Ported from edit-bench's ``rpc.py`` (TauRpcClient).  The protocol:

1. Spawn ``tau serve --cwd CWD --model MODEL --tools TOOLS``
2. Send JSON-RPC ``initialize``
3. Send ``session/send`` with prompt, and optional system/model overrides
4. Wait for ``session.status`` notification with ``type=idle``
5. Read usage from notification payload

Key additions over the edit-bench client:
- Accepts optional tool list and edit-mode arguments.
- Returns :class:`SessionResult` with token usage, tool-call count,
  and wall-clock timing.
- Tracks turn count (number of send/receive cycles).
"""

from __future__ import annotations

import json
import select
import subprocess
import time
from pathlib import Path

from .result import SessionResult


class TauSession:
    """Context manager for a persistent ``tau serve`` session.

    Parameters:
        model: Model identifier passed to ``tau serve --model``.
        cwd: Working directory for the tau subprocess.
        tools: Comma-separated tool names, or a list of tool names.
            ``None`` means let tau use its defaults.
        edit_mode: Edit strategy — ``"replace"`` or ``"hashline"``.
        trace_output: Directory for tau trace output (``run.json`` + ``trace.jsonl``).
            ``None`` uses tau's default trace location.
        task_id: Optional benchmark task identifier passed to tau for trace metadata.
        timeout: Default timeout in seconds for :meth:`send`.
        tau_binary: Path or name of the tau binary.
    """

    def __init__(
        self,
        model: str,
        cwd: Path,
        tools: list[str] | None = None,
        edit_mode: str = "replace",
        trace_output: Path | None = None,
        task_id: str | None = None,
        timeout: int = 120,
        tau_binary: str = "tau",
    ) -> None:
        self._model = model
        self._cwd = str(cwd)
        self._tools = ",".join(tools) if tools else None
        self._edit_mode = edit_mode
        self._trace_output = str(trace_output) if trace_output is not None else None
        self._task_id = task_id
        self._timeout = timeout
        self._tau_binary = tau_binary
        self._proc: subprocess.Popen[str] | None = None
        self._next_id = 1
        self._turns = 0

    # ── lifecycle ────────────────────────────────────────────────────

    def start(self) -> None:
        """Spawn ``tau serve`` and perform the JSON-RPC ``initialize`` handshake."""
        cmd = [self._tau_binary, "serve", "--cwd", self._cwd, "--model", self._model]
        if self._tools:
            cmd.extend(["--tools", self._tools])
        # NOTE: `tau serve` currently does not expose an `--edit-mode` flag.
        # Keep the constructor parameter for API compatibility across benchmarks,
        # but do not pass it to the serve subprocess.
        if self._trace_output:
            cmd.extend(["--trace-output", self._trace_output])
        if self._task_id:
            cmd.extend(["--task-id", self._task_id])

        self._proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        resp = self._call("initialize")
        if "error" in resp:
            raise RuntimeError(f"tau serve initialize failed: {resp['error']}")

    def send(
        self,
        prompt: str,
        *,
        timeout: int | None = None,
        system: str | None = None,
        model: str | None = None,
    ) -> SessionResult:
        """Send *prompt* and block until the session is idle.

        Returns a :class:`SessionResult` with the assistant output, token
        usage, tool-call count, and wall-clock time for this turn.
        """
        effective_timeout = timeout if timeout is not None else self._timeout

        params: dict[str, str] = {"prompt": prompt}
        if system is not None:
            params["system"] = system
        if model is not None:
            params["model"] = model

        self._call("session/send", params)
        self._turns += 1

        start = time.monotonic()
        deadline = start + effective_timeout
        output_text = ""
        input_tokens = 0
        output_tokens = 0
        tool_calls = 0

        while time.monotonic() < deadline:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            line = self._read_line(timeout=min(1.0, remaining))
            if line is None:
                continue
            msg = json.loads(line)
            method = msg.get("method")
            if method == "session.status":
                params = msg.get("params", {})
                status = params.get("status", {})
                status_type = status.get("type")
                if status_type == "idle":
                    usage = params.get("usage", {})
                    input_tokens = usage.get("input_tokens", 0)
                    output_tokens = usage.get("output_tokens", 0)
                    output_text = params.get("output", "")
                    tool_calls = usage.get("tool_calls", 0)
                    elapsed_ms = int((time.monotonic() - start) * 1000)
                    return SessionResult(
                        output=output_text,
                        input_tokens=input_tokens,
                        output_tokens=output_tokens,
                        tool_calls=tool_calls,
                        wall_clock_ms=elapsed_ms,
                    )
                if status_type == "error":
                    error_msg = params.get("error", "unknown error")
                    elapsed_ms = int((time.monotonic() - start) * 1000)
                    return SessionResult(
                        output=f"error: {error_msg}",
                        input_tokens=input_tokens,
                        output_tokens=output_tokens,
                        tool_calls=tool_calls,
                        wall_clock_ms=elapsed_ms,
                    )

        # Timeout
        elapsed_ms = int((time.monotonic() - start) * 1000)
        return SessionResult(
            output="error: timeout",
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            tool_calls=tool_calls,
            wall_clock_ms=elapsed_ms,
        )

    def shutdown(self) -> None:
        """Gracefully shut down the ``tau serve`` subprocess."""
        if self._proc is None:
            return
        try:
            self._call("shutdown")
            self._proc.wait(timeout=5)
        except Exception:
            self._proc.kill()
            self._proc.wait()
        finally:
            self._proc = None

    @property
    def turns(self) -> int:
        """Number of completed send/receive cycles."""
        return self._turns

    # ── context manager ──────────────────────────────────────────────

    def __enter__(self) -> TauSession:
        self.start()
        return self

    def __exit__(self, exc_type: type | None, exc_val: BaseException | None, exc_tb: object) -> None:
        self.shutdown()

    # ── JSON-RPC internals ───────────────────────────────────────────

    def _call(self, method: str, params: dict | None = None) -> dict:
        """Send a JSON-RPC 2.0 request and read the matching response."""
        req_id = self._next_id
        self._next_id += 1
        msg: dict = {"jsonrpc": "2.0", "method": method, "id": req_id}
        if params:
            msg["params"] = params
        self._write_line(json.dumps(msg))

        deadline = time.time() + 10
        while time.time() < deadline:
            line = self._read_line(timeout=1.0)
            if line is None:
                continue
            parsed = json.loads(line)
            if parsed.get("id") == req_id:
                return parsed.get("result", {})
        return {"error": "timeout waiting for response"}

    def _write_line(self, line: str) -> None:
        assert self._proc and self._proc.stdin
        self._proc.stdin.write(line + "\n")
        self._proc.stdin.flush()

    def _read_line(self, timeout: float = 1.0) -> str | None:
        assert self._proc and self._proc.stdout
        ready, _, _ = select.select([self._proc.stdout], [], [], timeout)
        if ready:
            line = self._proc.stdout.readline().strip()
            return line if line else None
        return None
