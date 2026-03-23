"""Workspace fixture generator for subagent-decomposition benchmark.

Creates codebases at 3 difficulty levels where common code should be
extracted into a shared utility module. Each level has handlers with
duplicated utility functions, and the expected output shows those
functions extracted into utils.py with proper imports.

Difficulty levels:
  - easy:   3 handlers, 1 duplicated function (~30 lines each)
  - medium: 5 handlers, 2 duplicated functions, slight variations
  - hard:   8 handlers, 3 duplicated functions, callers have differences
            requiring parameter handling

Usage:
    python generate.py -o fixtures/ [--seed 42]
"""

from __future__ import annotations

import argparse
import json
import textwrap
from pathlib import Path


# ── Utility function templates ─────────────────────────────────────────
# These are the common functions that appear duplicated across handlers.
# Each has a "canonical" form (for utils.py) and per-handler variations.


def _indent(code: str, level: int = 0) -> str:
    """Dedent then re-indent code block."""
    dedented = textwrap.dedent(code).strip()
    prefix = "    " * level
    return "\n".join(prefix + line if line.strip() else "" for line in dedented.split("\n"))


# ── parse_header: appears in all handlers ──────────────────────────────

PARSE_HEADER_CANONICAL = '''\
def parse_header(raw_header: str) -> dict[str, str]:
    """Parse a colon-separated header string into key-value pairs.

    Handles quoted values, strips whitespace, and lowercases keys.
    """
    result: dict[str, str] = {}
    if not raw_header or not raw_header.strip():
        return result
    for part in raw_header.split(";"):
        part = part.strip()
        if ":" not in part:
            continue
        key, _, value = part.partition(":")
        key = key.strip().lower()
        value = value.strip()
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        if key:
            result[key] = value
    return result
'''

PARSE_HEADER_VARIATIONS: dict[str, str] = {
    "default": PARSE_HEADER_CANONICAL,
    "with_encoding": '''\
def parse_header(raw_header: str, default_encoding: str = "utf-8") -> dict[str, str]:
    """Parse a colon-separated header string into key-value pairs.

    Handles quoted values, strips whitespace, and lowercases keys.
    Adds encoding if not present.
    """
    result: dict[str, str] = {}
    if not raw_header or not raw_header.strip():
        return result
    for part in raw_header.split(";"):
        part = part.strip()
        if ":" not in part:
            continue
        key, _, value = part.partition(":")
        key = key.strip().lower()
        value = value.strip()
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        if key:
            result[key] = value
    if "encoding" not in result:
        result["encoding"] = default_encoding
    return result
''',
}


# ── validate_token: appears in auth, api, webhook, admin ───────────────

VALIDATE_TOKEN_CANONICAL = '''\
def validate_token(token: str, secret: str) -> tuple[bool, str]:
    """Validate an API token against a secret.

    Returns (is_valid, error_message). Error message is empty on success.
    Token format: base64(payload).base64(signature)
    """
    import hashlib
    import hmac

    if not token or "." not in token:
        return False, "malformed token: missing separator"
    parts = token.split(".", 1)
    if len(parts) != 2:
        return False, "malformed token: expected payload.signature"
    payload, signature = parts
    if not payload or not signature:
        return False, "malformed token: empty segment"
    expected = hmac.new(
        secret.encode(), payload.encode(), hashlib.sha256
    ).hexdigest()[:32]
    if not hmac.compare_digest(signature, expected):
        return False, "invalid signature"
    return True, ""
'''


# ── format_response: appears in api, webhook, admin, health ────────────

FORMAT_RESPONSE_CANONICAL = '''\
def format_response(status: int, data: dict | list | None = None, error: str | None = None) -> dict:
    """Build a standardized API response envelope.

    Format: {"status": int, "ok": bool, "data": ..., "error": ...}
    """
    ok = 200 <= status < 300
    response: dict = {
        "status": status,
        "ok": ok,
    }
    if data is not None:
        response["data"] = data
    if error is not None:
        response["error"] = error
    elif not ok:
        response["error"] = "unknown error"
    return response
'''

FORMAT_RESPONSE_VARIATIONS: dict[str, str] = {
    "default": FORMAT_RESPONSE_CANONICAL,
    "with_meta": '''\
def format_response(
    status: int, data: dict | list | None = None, error: str | None = None, meta: dict | None = None,
) -> dict:
    """Build a standardized API response envelope.

    Format: {"status": int, "ok": bool, "data": ..., "error": ..., "meta": ...}
    """
    ok = 200 <= status < 300
    response: dict = {
        "status": status,
        "ok": ok,
    }
    if data is not None:
        response["data"] = data
    if error is not None:
        response["error"] = error
    elif not ok:
        response["error"] = "unknown error"
    if meta is not None:
        response["meta"] = meta
    return response
''',
}


# ── Handler templates ──────────────────────────────────────────────────
# Each handler is an HTTP-style handler with business logic + duplicated utils.


def _make_auth_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    """Generate auth_handler.py with inline utility functions."""
    return f'''\
"""Authentication handler for user login and token verification."""

from __future__ import annotations


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


AUTH_SECRET = "supersecret-auth-key-2024"


def handle_login(request: dict) -> dict:
    """Handle user login request."""
    headers = parse_header(request.get("headers", ""))
    content_type = headers.get("content-type", "application/json")

    username = request.get("username", "")
    password = request.get("password", "")

    if not username or not password:
        return format_response(400, error="username and password required")

    # Simplified auth check
    if password == "correct-password":
        token = f"{{username}}.abcdef1234567890abcdef1234567890"
        return format_response(200, data={{"token": token, "user": username}})

    return format_response(401, error="invalid credentials")


def handle_verify(request: dict) -> dict:
    """Verify an existing token."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    if not token:
        return format_response(401, error="no token provided")

    valid, err = validate_token(token, AUTH_SECRET)
    if not valid:
        return format_response(401, error=err)

    payload = token.split(".")[0]
    return format_response(200, data={{"user": payload, "valid": True}})
'''


def _make_api_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    """Generate api_handler.py with inline utility functions."""
    return f'''\
"""API handler for CRUD operations on resources."""

from __future__ import annotations


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


API_SECRET = "api-service-key-2024"

# In-memory store for demo purposes
_resources: dict[str, dict] = {{}}


def handle_list(request: dict) -> dict:
    """List all resources."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, API_SECRET)
    if not valid:
        return format_response(401, error=err)

    items = list(_resources.values())
    return format_response(200, data={{"items": items, "count": len(items)}})


def handle_create(request: dict) -> dict:
    """Create a new resource."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, API_SECRET)
    if not valid:
        return format_response(401, error=err)

    body = request.get("body", {{}})
    name = body.get("name", "")
    if not name:
        return format_response(400, error="name is required")

    resource_id = f"res-{{len(_resources) + 1:04d}}"
    resource = {{"id": resource_id, "name": name, "status": "active"}}
    _resources[resource_id] = resource
    return format_response(201, data=resource)


def handle_delete(request: dict) -> dict:
    """Delete a resource by ID."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, API_SECRET)
    if not valid:
        return format_response(401, error=err)

    resource_id = request.get("resource_id", "")
    if resource_id not in _resources:
        return format_response(404, error=f"resource {{resource_id}} not found")

    del _resources[resource_id]
    return format_response(200, data={{"deleted": resource_id}})
'''


def _make_webhook_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    """Generate webhook_handler.py with inline utility functions."""
    return f'''\
"""Webhook handler for receiving and processing external events."""

from __future__ import annotations

import json


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


WEBHOOK_SECRET = "webhook-signing-secret-2024"

# Event log
_event_log: list[dict] = []


def handle_webhook(request: dict) -> dict:
    """Process an incoming webhook event."""
    headers = parse_header(request.get("headers", ""))
    signature = headers.get("x-signature", "")

    # Verify webhook signature
    valid, err = validate_token(signature, WEBHOOK_SECRET)
    if not valid:
        return format_response(401, error=f"webhook signature invalid: {{err}}")

    event_type = headers.get("x-event-type", "unknown")
    payload = request.get("body", {{}})

    event = {{
        "type": event_type,
        "payload": payload,
        "processed": False,
    }}

    # Process known event types
    if event_type == "user.created":
        event["processed"] = True
        _event_log.append(event)
        return format_response(200, data={{"event_id": len(_event_log), "status": "processed"}})
    elif event_type == "user.deleted":
        event["processed"] = True
        _event_log.append(event)
        return format_response(200, data={{"event_id": len(_event_log), "status": "processed"}})
    else:
        _event_log.append(event)
        return format_response(202, data={{"event_id": len(_event_log), "status": "queued"}})


def get_event_log() -> list[dict]:
    """Return all recorded events."""
    return list(_event_log)
'''


def _make_admin_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    """Generate admin_handler.py with inline utility functions."""
    return f'''\
"""Admin handler for system management operations."""

from __future__ import annotations


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


ADMIN_SECRET = "admin-master-key-2024"


def handle_system_status(request: dict) -> dict:
    """Return system status information."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, ADMIN_SECRET)
    if not valid:
        return format_response(403, error=f"admin access denied: {{err}}")

    status_data = {{
        "uptime_seconds": 86400,
        "active_connections": 42,
        "memory_mb": 512,
        "cpu_percent": 23.5,
    }}
    return format_response(200, data=status_data)


def handle_user_management(request: dict) -> dict:
    """Manage user accounts (admin only)."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, ADMIN_SECRET)
    if not valid:
        return format_response(403, error=f"admin access denied: {{err}}")

    action = request.get("action", "")
    target_user = request.get("target_user", "")

    if not action or not target_user:
        return format_response(400, error="action and target_user required")

    if action == "suspend":
        return format_response(200, data={{"user": target_user, "status": "suspended"}})
    elif action == "activate":
        return format_response(200, data={{"user": target_user, "status": "active"}})
    else:
        return format_response(400, error=f"unknown action: {{action}}")
'''


def _make_health_handler(parse_header_fn: str, format_response_fn: str) -> str:
    """Generate health_handler.py with inline utility functions."""
    return f'''\
"""Health check handler for monitoring and liveness probes."""

from __future__ import annotations


{parse_header_fn}


{format_response_fn}


def handle_health(request: dict) -> dict:
    """Basic health check endpoint."""
    return format_response(200, data={{"status": "healthy"}})


def handle_ready(request: dict) -> dict:
    """Readiness probe -- checks dependencies."""
    headers = parse_header(request.get("headers", ""))
    verbose = headers.get("x-verbose", "false").lower() == "true"

    checks = {{
        "database": True,
        "cache": True,
        "queue": True,
    }}

    all_ok = all(checks.values())

    if verbose:
        return format_response(
            200 if all_ok else 503,
            data={{"checks": checks, "all_ok": all_ok}},
        )
    return format_response(200 if all_ok else 503, data={{"ready": all_ok}})


def handle_metrics(request: dict) -> dict:
    """Return basic service metrics."""
    headers = parse_header(request.get("headers", ""))
    fmt = headers.get("accept", "application/json")

    metrics = {{
        "requests_total": 15432,
        "requests_error": 23,
        "latency_p50_ms": 12,
        "latency_p99_ms": 145,
    }}

    if "text/plain" in fmt:
        # Prometheus-style text output
        lines = [f"{{k}} {{v}}" for k, v in metrics.items()]
        return format_response(200, data={{"text": "\\n".join(lines)}})

    return format_response(200, data=metrics)
'''


# ── Additional handlers for hard difficulty ────────────────────────────


def _make_billing_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    return f'''\
"""Billing handler for invoice and payment operations."""

from __future__ import annotations


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


BILLING_SECRET = "billing-api-key-2024"


def handle_invoice(request: dict) -> dict:
    """Generate an invoice for a user."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, BILLING_SECRET)
    if not valid:
        return format_response(401, error=err)

    user_id = request.get("user_id", "")
    amount = request.get("amount", 0)

    if not user_id or amount <= 0:
        return format_response(400, error="user_id and positive amount required")

    invoice = {{
        "invoice_id": f"inv-{{user_id}}-001",
        "user_id": user_id,
        "amount": amount,
        "currency": "USD",
        "status": "pending",
    }}
    return format_response(201, data=invoice)
'''


def _make_notification_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    return f'''\
"""Notification handler for sending alerts and messages."""

from __future__ import annotations


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


NOTIFY_SECRET = "notification-key-2024"


def handle_send_notification(request: dict) -> dict:
    """Send a notification to a user."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, NOTIFY_SECRET)
    if not valid:
        return format_response(401, error=err)

    user_id = request.get("user_id", "")
    message = request.get("message", "")
    channel = request.get("channel", "email")

    if not user_id or not message:
        return format_response(400, error="user_id and message required")

    notification = {{
        "notification_id": f"notif-{{user_id}}-001",
        "user_id": user_id,
        "message": message,
        "channel": channel,
        "status": "sent",
    }}
    return format_response(200, data=notification)
'''


def _make_search_handler(parse_header_fn: str, validate_token_fn: str, format_response_fn: str) -> str:
    return f'''\
"""Search handler for querying indexed resources."""

from __future__ import annotations


{parse_header_fn}


{validate_token_fn}


{format_response_fn}


SEARCH_SECRET = "search-api-key-2024"


def handle_search(request: dict) -> dict:
    """Search for resources matching a query."""
    headers = parse_header(request.get("headers", ""))
    token = headers.get("authorization", "")

    valid, err = validate_token(token, SEARCH_SECRET)
    if not valid:
        return format_response(401, error=err)

    query = request.get("query", "")
    limit = request.get("limit", 10)

    if not query:
        return format_response(400, error="query is required")

    results = [
        {{"id": f"result-{{i}}", "title": f"Result {{i}} for '{{query}}'", "score": 1.0 - i * 0.1}}
        for i in range(min(limit, 5))
    ]
    return format_response(200, data={{"results": results, "total": len(results), "query": query}})
'''


# ── Expected output (utils.py + updated handlers) ─────────────────────


def _make_utils_py(functions: list[str]) -> str:
    """Generate utils.py with extracted canonical functions."""
    parts = [
        '"""Shared utility functions extracted from handler modules."""\n',
        "from __future__ import annotations\n",
    ]

    if any("hashlib" in f or "hmac" in f for f in functions):
        parts.append("import hashlib")
        parts.append("import hmac\n")

    for fn in functions:
        parts.append("")
        parts.append(textwrap.dedent(fn).strip())
        parts.append("")

    return "\n".join(parts) + "\n"


def _update_handler_imports(handler_code: str, extracted_functions: list[str]) -> str:
    """Remove inlined utility functions and add import from utils."""
    lines = handler_code.split("\n")
    result_lines: list[str] = []
    fn_names = [_extract_fn_name(fn) for fn in extracted_functions]

    current_indent = 0

    i = 0
    added_import = False
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()

        # Check if this line starts one of the functions to remove
        is_target_def = False
        for fn_name in fn_names:
            if stripped.startswith(f"def {fn_name}("):
                is_target_def = True
                break

        if is_target_def:
            # Skip the entire function (def line + body)
            current_indent = len(line) - len(line.lstrip())
            i += 1
            # Skip docstring and body
            while i < len(lines):
                next_line = lines[i]
                next_stripped = next_line.strip()
                if not next_stripped:
                    i += 1
                    continue
                next_indent = len(next_line) - len(next_line.lstrip())
                if (
                    next_indent <= current_indent
                    and next_stripped
                    and not next_stripped.startswith('"""')
                    and not next_stripped.startswith("'")
                ):
                    # Check if it is another def at same level or content
                    break
                i += 1
            # Remove trailing blank lines
            while result_lines and not result_lines[-1].strip():
                result_lines.pop()
            continue

        # Add import after the module docstring and existing imports
        if not added_import and stripped.startswith(
            ("def ", "class ", "AUTH_", "API_", "ADMIN_", "WEBHOOK_", "BILLING_", "NOTIFY_", "SEARCH_", "_resources", "_event_log")
        ):
            import_line = f"from utils import {', '.join(fn_names)}"
            result_lines.append(import_line)
            result_lines.append("")
            result_lines.append("")
            added_import = True

        result_lines.append(line)
        i += 1

    return "\n".join(result_lines)


def _extract_fn_name(fn_code: str) -> str:
    """Extract function name from a function definition string."""
    for line in fn_code.strip().split("\n"):
        stripped = line.strip()
        if stripped.startswith("def "):
            name = stripped[4:].split("(")[0].strip()
            return name
    return "unknown"


# ── Test generation ────────────────────────────────────────────────────


def _make_test_file(handler_name: str, handler_functions: list[str]) -> str:
    """Generate a simple pytest test file for a handler."""
    parts = [
        f'"""Tests for {handler_name}."""\n',
        "from __future__ import annotations\n",
        f"from src.{handler_name} import {', '.join(handler_functions)}\n",
        "",
    ]

    for fn_name in handler_functions:
        parts.append(f"""
def test_{fn_name}_basic():
    \"\"\"Basic smoke test for {fn_name}.\"\"\"
    request = {{"headers": "content-type:application/json"}}
    result = {fn_name}(request)
    assert isinstance(result, dict)
    assert "status" in result
""")

    return "\n".join(parts) + "\n"


# ── Fixture generation per difficulty ──────────────────────────────────


def generate_easy(output_dir: Path) -> None:
    """Easy: 3 handlers, 1 duplicated function (parse_header)."""
    level_dir = output_dir / "easy"
    input_dir = level_dir / "input" / "src"
    expected_dir = level_dir / "expected" / "src"
    test_input_dir = level_dir / "input" / "tests"
    test_expected_dir = level_dir / "expected" / "tests"

    for d in (input_dir, expected_dir, test_input_dir, test_expected_dir):
        d.mkdir(parents=True, exist_ok=True)

    ph = PARSE_HEADER_CANONICAL
    fr = FORMAT_RESPONSE_CANONICAL

    # Input: 3 handlers with parse_header duplicated
    handlers = {
        "auth_handler": (_make_auth_handler(ph, VALIDATE_TOKEN_CANONICAL, fr), ["handle_login", "handle_verify"]),
        "api_handler": (_make_api_handler(ph, VALIDATE_TOKEN_CANONICAL, fr), ["handle_list", "handle_create", "handle_delete"]),
        "health_handler": (_make_health_handler(ph, fr), ["handle_health", "handle_ready", "handle_metrics"]),
    }

    extracted_fns = [PARSE_HEADER_CANONICAL]
    fn_names = ["parse_header"]

    for name, (code, test_fns) in handlers.items():
        (input_dir / f"{name}.py").write_text(code)
        (input_dir / "__init__.py").write_text("")

        # Expected: handlers with parse_header removed, importing from utils
        updated = _update_handler_imports(code, extracted_fns)
        (expected_dir / f"{name}.py").write_text(updated)
        (expected_dir / "__init__.py").write_text("")

        # Tests
        test_code = _make_test_file(name, test_fns)
        (test_input_dir / f"test_{name}.py").write_text(test_code)
        (test_expected_dir / f"test_{name}.py").write_text(test_code)
        (test_input_dir / "__init__.py").write_text("")
        (test_expected_dir / "__init__.py").write_text("")

    # Expected utils.py
    utils_code = _make_utils_py(extracted_fns)
    (expected_dir / "utils.py").write_text(utils_code)

    # Prompt
    prompt = textwrap.dedent("""\
        # Extract common utility functions

        The `src/` directory contains three handler modules. Each one has its own
        copy of the `parse_header` function. This duplication should be eliminated.

        ## Task
        1. Create `src/utils.py` containing the shared `parse_header` function
        2. Update all three handlers to import `parse_header` from `utils` instead
           of defining it inline
        3. Ensure all tests still pass after the refactoring

        Do not change any business logic. Only extract the duplicated function
        and update imports.
    """)
    (level_dir / "prompt.md").write_text(prompt)

    # Metadata
    metadata = {
        "difficulty": "easy",
        "handlers": 3,
        "duplicated_functions": 1,
        "function_names": fn_names,
        "description": "Extract parse_header from 3 handlers into utils.py",
    }
    (level_dir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")


def generate_medium(output_dir: Path) -> None:
    """Medium: 5 handlers, 2 duplicated functions (parse_header, format_response)."""
    level_dir = output_dir / "medium"
    input_dir = level_dir / "input" / "src"
    expected_dir = level_dir / "expected" / "src"
    test_input_dir = level_dir / "input" / "tests"
    test_expected_dir = level_dir / "expected" / "tests"

    for d in (input_dir, expected_dir, test_input_dir, test_expected_dir):
        d.mkdir(parents=True, exist_ok=True)

    ph = PARSE_HEADER_CANONICAL
    vt = VALIDATE_TOKEN_CANONICAL
    fr = FORMAT_RESPONSE_CANONICAL

    handlers = {
        "auth_handler": (_make_auth_handler(ph, vt, fr), ["handle_login", "handle_verify"]),
        "api_handler": (_make_api_handler(ph, vt, fr), ["handle_list", "handle_create", "handle_delete"]),
        "webhook_handler": (_make_webhook_handler(ph, vt, fr), ["handle_webhook", "get_event_log"]),
        "admin_handler": (_make_admin_handler(ph, vt, fr), ["handle_system_status", "handle_user_management"]),
        "health_handler": (_make_health_handler(ph, fr), ["handle_health", "handle_ready", "handle_metrics"]),
    }

    extracted_fns = [PARSE_HEADER_CANONICAL, FORMAT_RESPONSE_CANONICAL]
    fn_names = ["parse_header", "format_response"]

    for name, (code, test_fns) in handlers.items():
        (input_dir / f"{name}.py").write_text(code)
        (input_dir / "__init__.py").write_text("")

        updated = _update_handler_imports(code, extracted_fns)
        (expected_dir / f"{name}.py").write_text(updated)
        (expected_dir / "__init__.py").write_text("")

        test_code = _make_test_file(name, test_fns)
        (test_input_dir / f"test_{name}.py").write_text(test_code)
        (test_expected_dir / f"test_{name}.py").write_text(test_code)
        (test_input_dir / "__init__.py").write_text("")
        (test_expected_dir / "__init__.py").write_text("")

    utils_code = _make_utils_py(extracted_fns)
    (expected_dir / "utils.py").write_text(utils_code)

    prompt = textwrap.dedent("""\
        # Extract common utility functions

        The `src/` directory contains five handler modules. They share two duplicated
        utility functions: `parse_header` and `format_response`. This duplication
        should be eliminated.

        ## Task
        1. Create `src/utils.py` containing the shared `parse_header` and
           `format_response` functions
        2. Update all five handlers to import these functions from `utils`
        3. Ensure all tests still pass after the refactoring

        Do not change any business logic. Only extract the duplicated functions
        and update imports.
    """)
    (level_dir / "prompt.md").write_text(prompt)

    metadata = {
        "difficulty": "medium",
        "handlers": 5,
        "duplicated_functions": 2,
        "function_names": fn_names,
        "description": "Extract parse_header and format_response from 5 handlers into utils.py",
    }
    (level_dir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")


def generate_hard(output_dir: Path) -> None:
    """Hard: 8 handlers, 3 duplicated functions, callers have variations requiring parameter handling."""
    level_dir = output_dir / "hard"
    input_dir = level_dir / "input" / "src"
    expected_dir = level_dir / "expected" / "src"
    test_input_dir = level_dir / "input" / "tests"
    test_expected_dir = level_dir / "expected" / "tests"

    for d in (input_dir, expected_dir, test_input_dir, test_expected_dir):
        d.mkdir(parents=True, exist_ok=True)

    ph = PARSE_HEADER_CANONICAL
    ph_enc = PARSE_HEADER_VARIATIONS["with_encoding"]
    vt = VALIDATE_TOKEN_CANONICAL
    fr = FORMAT_RESPONSE_CANONICAL
    fr_meta = FORMAT_RESPONSE_VARIATIONS["with_meta"]

    # Some handlers use variations of the functions
    handlers = {
        "auth_handler": (_make_auth_handler(ph, vt, fr), ["handle_login", "handle_verify"]),
        "api_handler": (_make_api_handler(ph, vt, fr_meta), ["handle_list", "handle_create", "handle_delete"]),
        "webhook_handler": (_make_webhook_handler(ph_enc, vt, fr), ["handle_webhook", "get_event_log"]),
        "admin_handler": (_make_admin_handler(ph, vt, fr), ["handle_system_status", "handle_user_management"]),
        "health_handler": (_make_health_handler(ph, fr), ["handle_health", "handle_ready", "handle_metrics"]),
        "billing_handler": (_make_billing_handler(ph, vt, fr), ["handle_invoice"]),
        "notification_handler": (_make_notification_handler(ph_enc, vt, fr_meta), ["handle_send_notification"]),
        "search_handler": (_make_search_handler(ph, vt, fr), ["handle_search"]),
    }

    extracted_fns = [PARSE_HEADER_CANONICAL, VALIDATE_TOKEN_CANONICAL, FORMAT_RESPONSE_CANONICAL]
    fn_names = ["parse_header", "validate_token", "format_response"]

    for name, (code, test_fns) in handlers.items():
        (input_dir / f"{name}.py").write_text(code)
        (input_dir / "__init__.py").write_text("")

        updated = _update_handler_imports(code, extracted_fns)
        (expected_dir / f"{name}.py").write_text(updated)
        (expected_dir / "__init__.py").write_text("")

        test_code = _make_test_file(name, test_fns)
        (test_input_dir / f"test_{name}.py").write_text(test_code)
        (test_expected_dir / f"test_{name}.py").write_text(test_code)
        (test_input_dir / "__init__.py").write_text("")
        (test_expected_dir / "__init__.py").write_text("")

    utils_code = _make_utils_py(extracted_fns)
    (expected_dir / "utils.py").write_text(utils_code)

    prompt = textwrap.dedent("""\
        # Extract common utility functions

        The `src/` directory contains eight handler modules. They share three
        duplicated utility functions: `parse_header`, `validate_token`, and
        `format_response`. Some handlers have slight variations of these functions
        (e.g., `parse_header` with a `default_encoding` parameter, `format_response`
        with a `meta` parameter).

        ## Task
        1. Create `src/utils.py` containing the canonical versions of all three
           shared functions: `parse_header`, `validate_token`, and `format_response`
        2. Update all eight handlers to import these functions from `utils`
        3. Where handlers use variations (extra parameters, additional logic),
           adapt the callers to work with the canonical version, or extend the
           canonical version to support the variations via optional parameters
        4. Ensure all tests still pass after the refactoring

        Do not change any business logic beyond what is needed for the extraction.
    """)
    (level_dir / "prompt.md").write_text(prompt)

    metadata = {
        "difficulty": "hard",
        "handlers": 8,
        "duplicated_functions": 3,
        "function_names": fn_names,
        "has_variations": True,
        "description": "Extract parse_header, validate_token, format_response from 8 handlers with variations",
    }
    (level_dir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")


# ── CLI ────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate fixtures for subagent-decomposition benchmark")
    parser.add_argument("-o", "--output", type=Path, default=Path("fixtures"), help="Output directory (default: fixtures/)")
    parser.add_argument("--seed", type=int, default=42, help="Random seed (for future use)")
    args = parser.parse_args()

    output_dir = args.output
    if output_dir.exists():
        import shutil

        shutil.rmtree(output_dir)

    print("Generating subagent-decomposition fixtures...")

    generate_easy(output_dir)
    print("  easy:   3 handlers, 1 duplicated function")

    generate_medium(output_dir)
    print("  medium: 5 handlers, 2 duplicated functions")

    generate_hard(output_dir)
    print("  hard:   8 handlers, 3 duplicated functions (with variations)")

    print(f"\nGenerated fixtures to {output_dir}/")


if __name__ == "__main__":
    main()
