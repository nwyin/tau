#!/usr/bin/env python3
"""tau Python kernel — persistent REPL with reverse RPC for orchestration.

Runs as a subprocess. Communicates with the Rust host via JSON-lines over
stdin (host→kernel) and stdout (kernel→host). Stderr is for kernel diagnostics.

Protocol:
  Host→Kernel: {"type": "exec", "cell_id": "...", "code": "..."}
  Host→Kernel: {"type": "rpc_result", "id": "...", "result": ..., "error": ...}
  Host→Kernel: {"type": "shutdown"}
  Kernel→Host: {"type": "rpc", "id": "...", "method": "...", "params": {...}}
  Kernel→Host: {"type": "result", "cell_id": "...", "output": ..., "error": ..., "stdout": ..., "stderr": ...}
"""

import ast
import contextlib
import io
import json
import os
import sys
import traceback


class TauProxy:
    """The `tau` object available in the kernel namespace.

    All methods are blocking — they issue an RPC to the Rust host and wait
    for the response. No asyncio needed.
    """

    def __init__(self):
        self.cwd = os.getcwd()
        self.home_dir = os.path.expanduser("~")
        self.tmp_dir = os.environ.get("TMPDIR", "/tmp")
        self._rpc_counter = 0

    def _rpc(self, method, params):
        """Send an RPC request to the host and block until the response."""
        self._rpc_counter += 1
        rpc_id = f"rpc-{self._rpc_counter}"
        msg = {"type": "rpc", "id": rpc_id, "method": method, "params": params}
        sys.stdout.write(json.dumps(msg, default=str) + "\n")
        sys.stdout.flush()
        # Block reading stdin until we get the matching rpc_result
        while True:
            line = sys.stdin.readline()
            if not line:
                raise RuntimeError("Host closed connection")
            resp = json.loads(line)
            if resp.get("type") == "rpc_result" and resp.get("id") == rpc_id:
                if resp.get("error"):
                    raise RuntimeError(resp["error"])
                return resp.get("result")

    def tool(self, name, **kwargs):
        """Call a tau tool by name. Returns the tool result text."""
        return self._rpc("tool", {"name": name, "args": kwargs})

    def thread(self, alias, task, tools=None, model=None, episodes=None, timeout=None):
        """Spawn a thread. Blocks until complete. Returns episode text."""
        params = {"alias": alias, "task": task}
        if tools is not None:
            params["tools"] = tools
        if model is not None:
            params["model"] = model
        if episodes is not None:
            params["episodes"] = episodes
        if timeout is not None:
            params["timeout"] = timeout
        return self._rpc("thread", params)

    def query(self, prompt, alias=None, model=None):
        """Single-shot LLM query. Returns response text."""
        params = {"prompt": prompt}
        if alias is not None:
            params["alias"] = alias
        if model is not None:
            params["model"] = model
        return self._rpc("query", params)

    def parallel(self, *specs):
        """Execute multiple operations concurrently.

        Each spec should be created via tau.Thread(...), tau.Query(...), or tau.Tool(...).
        Returns a list of results in the same order as the specs.
        """
        return self._rpc("parallel", {"specs": list(specs)})

    def document(self, operation, name=None, content=None):
        """Access shared virtual documents.

        Operations: read, write, append, list.
        """
        params = {"operation": operation}
        if name is not None:
            params["name"] = name
        if content is not None:
            params["content"] = content
        return self._rpc("document", params)

    def log(self, message):
        """Record a message in the orchestration trace."""
        self._rpc("log", {"message": str(message)})

    # --- Spec factories for tau.parallel() ---

    @staticmethod
    def Thread(alias, task, **kwargs):
        """Create a thread spec for use with tau.parallel()."""
        return {"method": "thread", "alias": alias, "task": task, **kwargs}

    @staticmethod
    def Query(prompt, **kwargs):
        """Create a query spec for use with tau.parallel()."""
        return {"method": "query", "prompt": prompt, **kwargs}

    @staticmethod
    def Tool(name, **kwargs):
        """Create a tool spec for use with tau.parallel()."""
        return {"method": "tool", "name": name, "args": kwargs}


def exec_cell(code, namespace):
    """Execute a code cell in the given namespace.

    Returns (output, error, stdout, stderr).
    If the last statement is an expression, its repr is returned as output.
    """
    stdout_buf = io.StringIO()
    stderr_buf = io.StringIO()
    output = None
    error = None

    try:
        tree = ast.parse(code)
        last_expr = None
        # If the last statement is a bare expression, extract it
        # so we can eval it and return its value (REPL-style).
        if tree.body and isinstance(tree.body[-1], ast.Expr):
            last_expr = tree.body.pop()

        with contextlib.redirect_stdout(stdout_buf), contextlib.redirect_stderr(stderr_buf):
            if tree.body:
                compiled = compile(tree, "<cell>", "exec")
                exec(compiled, namespace)
            if last_expr:
                expr_code = compile(
                    ast.Expression(body=last_expr.value), "<cell>", "eval"
                )
                result = eval(expr_code, namespace)
                if result is not None:
                    output = repr(result)
    except Exception:
        error = traceback.format_exc()

    return output, error, stdout_buf.getvalue(), stderr_buf.getvalue()


def main():
    namespace = {"tau": TauProxy(), "__name__": "__main__"}

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        msg_type = msg.get("type")

        if msg_type == "shutdown":
            break

        if msg_type == "exec":
            cell_id = msg["cell_id"]
            code = msg["code"]
            output, error, stdout, stderr = exec_cell(code, namespace)
            result = {
                "type": "result",
                "cell_id": cell_id,
                "output": output,
                "error": error,
                "stdout": stdout,
                "stderr": stderr,
            }
            sys.stdout.write(json.dumps(result) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
