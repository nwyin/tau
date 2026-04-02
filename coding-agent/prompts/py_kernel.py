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


# Keep references to the real stdin/stdout before any redirects.
# exec_cell uses contextlib.redirect_stdout which replaces sys.stdout,
# but RPC must always go through the actual pipe to the host.
_real_stdin = sys.stdin
_real_stdout = sys.stdout


class ThreadResult:
    """Structured result from tau.thread()."""

    def __init__(self, data):
        if isinstance(data, str):
            self.status = "completed"
            self.output = data
            self.trace = data
            self.alias = None
            self.duration_ms = None
            self.turns = None
        else:
            self.status = data.get("status", "completed")
            self.output = data.get("output", "")
            self.trace = data.get("trace", "")
            self.alias = data.get("alias")
            self.duration_ms = data.get("duration_ms")
            self.turns = data.get("turns")

    @property
    def completed(self):
        return self.status == "completed"

    @property
    def aborted(self):
        return self.status == "aborted"

    @property
    def escalated(self):
        return self.status == "escalated"

    @property
    def timed_out(self):
        return self.status == "timed_out"

    @property
    def reason(self):
        return self.output

    def __str__(self):
        return self.trace

    def __repr__(self):
        return f"ThreadResult(status={self.status!r}, output={self.output[:60]!r})"

    def __bool__(self):
        return self.completed

    def __contains__(self, item):
        return item in self.trace


class WorkflowHandle:
    def __init__(self, name, module, path, tau):
        self.name = name
        self._module = module
        self._tau = tau
        self.path = path
        self.description = (module.__doc__ or "").strip().split("\n")[0]

    def run(self, **params):
        if not hasattr(self._module, "run"):
            raise RuntimeError(f"Workflow '{self.name}' has no run() function")
        return self._module.run(self._tau, **params)

    def info(self):
        import inspect

        sig = inspect.signature(self._module.run)
        params = [p for p in sig.parameters if p != "tau"]
        return {"name": self.name, "description": self.description, "parameters": params, "path": self.path}


class WorkflowRegistry:
    def __init__(self, tau_proxy):
        self._tau = tau_proxy
        self._cache = None
        self._modules = {}

    def _scan(self):
        if self._cache is not None:
            return
        self._cache = {}
        for d in self._workflow_dirs():
            if not os.path.isdir(d):
                continue
            for fname in sorted(os.listdir(d)):
                if fname.endswith(".py") and fname[:-3] not in self._cache:
                    self._cache[fname[:-3]] = os.path.join(d, fname)

    def _workflow_dirs(self):
        dirs = []
        current = self._tau.cwd
        while True:
            candidate = os.path.join(current, ".tau", "workflows")
            if os.path.isdir(candidate):
                dirs.append(candidate)
            if os.path.isdir(os.path.join(current, ".git")):
                break
            parent = os.path.dirname(current)
            if parent == current:
                break
            current = parent
        dirs.append(os.path.join(self._tau.home_dir, ".tau", "workflows"))
        return dirs

    def get(self, name):
        self._scan()
        if name not in self._cache:
            raise RuntimeError(f"Unknown workflow: '{name}'. Use tau.workflows() to list.")
        if name not in self._modules:
            import importlib.util

            spec = importlib.util.spec_from_file_location(f"tau_workflow_{name}", self._cache[name])
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            self._modules[name] = mod
        return WorkflowHandle(name, self._modules[name], self._cache[name], self._tau)

    def list_all(self):
        self._scan()
        result = []
        for name in sorted(self._cache):
            try:
                handle = self.get(name)
                import inspect

                params = [p for p in inspect.signature(handle._module.run).parameters if p != "tau"]
                result.append({"name": name, "description": handle.description, "parameters": params})
            except Exception as e:
                result.append({"name": name, "description": f"(error: {e})", "parameters": []})
        return result

    def save(self, name, code):
        d = os.path.join(self._tau.home_dir, ".tau", "workflows")
        os.makedirs(d, exist_ok=True)
        path = os.path.join(d, f"{name}.py")
        with open(path, "w") as f:
            f.write(code)
        self._cache = None
        self._modules.pop(name, None)
        return path

    def refresh(self):
        self._cache = None
        self._modules.clear()


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
        _real_stdout.write(json.dumps(msg, default=str) + "\n")
        _real_stdout.flush()
        # Block reading stdin until we get the matching rpc_result
        while True:
            line = _real_stdin.readline()
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
        """Spawn a thread. Blocks until complete. Returns a ThreadResult."""
        params = {"alias": alias, "task": task}
        if tools is not None:
            params["tools"] = tools
        if model is not None:
            params["model"] = model
        if episodes is not None:
            params["episodes"] = episodes
        if timeout is not None:
            params["timeout"] = timeout
        return ThreadResult(self._rpc("thread", params))

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
        raw = self._rpc("parallel", {"specs": list(specs)})
        specs_list = list(specs)
        wrapped = []
        for i, val in enumerate(raw):
            if i < len(specs_list) and specs_list[i].get("method") == "thread":
                wrapped.append(ThreadResult(val))
            else:
                wrapped.append(val)
        return wrapped

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

    # --- Workflow template API ---

    @property
    def _workflows(self):
        if not hasattr(self, "_workflow_registry") or self._workflow_registry is None:
            self._workflow_registry = WorkflowRegistry(self)
        return self._workflow_registry

    def workflow(self, name):
        """Load a workflow template by name. Returns a WorkflowHandle."""
        return self._workflows.get(name)

    def workflows(self):
        """List all available workflow templates."""
        return self._workflows.list_all()

    def save_workflow(self, name, code):
        """Save a workflow template to ~/.tau/workflows/."""
        return self._workflows.save(name, code)

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
    namespace = {"tau": TauProxy(), "ThreadResult": ThreadResult, "__name__": "__main__"}

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
