"""Generate synthetic codebases for the parallel-ops benchmark.

Creates workspaces with N independent Python modules, each exporting a
function. Exactly one module exports `process_data` -- the target function
the model must locate.

Usage:
    uv run python generate.py -o fixtures/ --file-counts 5,10,15,20 --seed 42
"""

from __future__ import annotations

import argparse
import random
import sys
from pathlib import Path

# Each entry is (module_name, func_name, full_source).
# Every module is self-contained, lint-clean, and ~30-50 lines.

MODULES: list[tuple[str, str, str]] = [
    (
        "auth",
        "authenticate",
        '''\
"""User authentication module."""
from __future__ import annotations

import hashlib
import logging

logger = logging.getLogger(__name__)


def authenticate(username: str, password: str) -> bool:
    """Verify user credentials against stored hashes."""
    logger.info("Authenticating user: %s", username)
    if not username or not password:
        logger.warning("Empty credentials provided")
        return False
    pw_hash = hashlib.sha256(password.encode()).hexdigest()
    # Simulated credential store
    known: dict[str, str] = {
        "admin": "8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918",
    }
    valid = known.get(username) == pw_hash
    logger.info("Auth result for %s: %s", username, valid)
    return valid
''',
    ),
    (
        "billing",
        "process_payment",
        '''\
"""Payment processing module."""
from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)

SUPPORTED_CURRENCIES = {"USD", "EUR", "GBP", "JPY"}


def process_payment(amount: float, currency: str = "USD") -> dict[str, Any]:
    """Process a payment and return a transaction record."""
    logger.info("Processing payment: %.2f %s", amount, currency)
    if currency not in SUPPORTED_CURRENCIES:
        raise ValueError(f"Unsupported currency: {currency}")
    if amount <= 0:
        raise ValueError(f"Invalid amount: {amount}")
    fee = amount * 0.029 + 0.30
    net = amount - fee
    record: dict[str, Any] = {
        "amount": amount,
        "currency": currency,
        "fee": round(fee, 2),
        "net": round(net, 2),
        "status": "completed",
    }
    logger.info("Payment processed: %s", record)
    return record
''',
    ),
    (
        "cache",
        "invalidate_cache",
        '''\
"""Cache invalidation module."""
from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_store: dict[str, tuple[float, str]] = {}


def invalidate_cache(keys: list[str]) -> int:
    """Remove entries from the cache, returning the count of keys removed."""
    logger.info("Invalidating %d cache keys", len(keys))
    removed = 0
    for key in keys:
        if key in _store:
            del _store[key]
            removed += 1
            logger.debug("Removed key: %s", key)
    logger.info("Cache invalidation complete: %d/%d removed", removed, len(keys))
    return removed


def _set(key: str, value: str, ttl: float = 60.0) -> None:
    _store[key] = (time.monotonic() + ttl, value)
''',
    ),
    (
        "config",
        "load_config",
        '''\
"""Configuration loading module."""
from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

_DEFAULTS: dict[str, Any] = {
    "host": "localhost",
    "port": 8080,
    "debug": False,
    "log_level": "INFO",
}


def load_config(path: str | None = None) -> dict[str, Any]:
    """Load configuration from a JSON file, falling back to defaults."""
    logger.info("Loading config from: %s", path or "<defaults>")
    config = dict(_DEFAULTS)
    if path is not None:
        config_path = Path(path)
        if config_path.exists():
            with open(config_path) as f:
                overrides = json.load(f)
            config.update(overrides)
            logger.info("Applied %d overrides from %s", len(overrides), path)
    return config
''',
    ),
    (
        "database",
        "connect_db",
        '''\
"""Database connection module."""
from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

_connections: dict[str, bool] = {}


def connect_db(host: str, port: int = 5432) -> bool:
    """Establish a database connection and return success status."""
    dsn = f"{host}:{port}"
    logger.info("Connecting to database at %s", dsn)
    if dsn in _connections:
        logger.info("Reusing existing connection to %s", dsn)
        return True
    # Simulate connection attempt
    success = len(host) > 0 and 1 <= port <= 65535
    if success:
        _connections[dsn] = True
        logger.info("Connected to %s", dsn)
    else:
        logger.error("Failed to connect to %s", dsn)
    return success
''',
    ),
    (
        "email",
        "send_notification",
        '''\
"""Email notification module."""
from __future__ import annotations

import logging
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class EmailResult:
    recipient: str
    subject: str
    status: str
    message_id: str


def send_notification(recipient: str, subject: str, body: str) -> str:
    """Send an email notification and return the message ID."""
    logger.info("Sending email to %s: %s", recipient, subject)
    if "@" not in recipient:
        raise ValueError(f"Invalid email: {recipient}")
    msg_id = f"msg-{hash((recipient, subject)) & 0xFFFFFFFF:08x}"
    result = EmailResult(recipient=recipient, subject=subject, status="sent", message_id=msg_id)
    logger.info("Email sent: %s", result)
    return result.message_id
''',
    ),
    (
        "logging_setup",
        "setup_logger",
        '''\
"""Logging configuration module."""
from __future__ import annotations

import logging
import sys

_configured: set[str] = set()


def setup_logger(name: str, level: str = "INFO") -> bool:
    """Configure a named logger with the specified level."""
    if name in _configured:
        return True
    log = logging.getLogger(name)
    numeric_level = getattr(logging, level.upper(), logging.INFO)
    log.setLevel(numeric_level)
    handler = logging.StreamHandler(sys.stderr)
    handler.setFormatter(logging.Formatter("%(asctime)s [%(name)s] %(levelname)s: %(message)s"))
    log.addHandler(handler)
    _configured.add(name)
    log.info("Logger %s configured at level %s", name, level)
    return True
''',
    ),
    (
        "metrics",
        "track_event",
        '''\
"""Event tracking and metrics module."""
from __future__ import annotations

import json
import logging
import time
from typing import Any

logger = logging.getLogger(__name__)

_events: list[dict[str, Any]] = []


def track_event(event_name: str, properties: dict[str, Any] | None = None) -> str:
    """Record a tracking event and return its JSON representation."""
    logger.info("Tracking event: %s", event_name)
    event: dict[str, Any] = {
        "name": event_name,
        "timestamp": time.time(),
        "properties": properties or {},
    }
    _events.append(event)
    serialized = json.dumps(event)
    logger.debug("Event recorded: %s", serialized)
    return serialized
''',
    ),
    (
        "search",
        "process_data",
        '''\
"""Data processing and search module."""
from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)


def process_data(query: str, limit: int = 100) -> list[dict[str, Any]]:
    """Process a search query and return matching results."""
    logger.info("Processing query: %s (limit=%d)", query, limit)
    if not query:
        return []
    terms = query.lower().split()
    results: list[dict[str, Any]] = []
    for i, term in enumerate(terms):
        if i >= limit:
            break
        score = len(term) * 0.1 + (1.0 / (i + 1))
        results.append({"term": term, "score": round(score, 4), "rank": i + 1})
    results.sort(key=lambda r: r["score"], reverse=True)
    logger.info("Found %d results for query: %s", len(results), query)
    return results
''',
    ),
    (
        "validation",
        "validate_schema",
        '''\
"""Schema validation module."""
from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)

_REQUIRED_FIELDS = {"id", "type", "data"}


def validate_schema(data: dict[str, Any], schema: dict[str, Any] | None = None) -> bool:
    """Validate data against a schema, returning True if valid."""
    logger.info("Validating data with %d keys", len(data))
    required = set(schema.get("required", [])) if schema else _REQUIRED_FIELDS
    missing = required - set(data.keys())
    if missing:
        logger.warning("Missing required fields: %s", missing)
        return False
    for key, value in data.items():
        if value is None:
            logger.warning("Null value for field: %s", key)
            return False
    logger.info("Validation passed")
    return True
''',
    ),
    (
        "scheduler",
        "schedule_job",
        '''\
"""Job scheduling module."""
from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_queue: list[dict[str, object]] = []


def schedule_job(job_id: str, delay_seconds: int = 0) -> str:
    """Schedule a job for execution and return a confirmation token."""
    logger.info("Scheduling job %s with delay %ds", job_id, delay_seconds)
    run_at = time.time() + delay_seconds
    entry = {"job_id": job_id, "run_at": run_at, "status": "pending"}
    _queue.append(entry)
    token = f"tok-{hash(job_id) & 0xFFFFFFFF:08x}"
    logger.info("Job %s scheduled, token: %s", job_id, token)
    return token
''',
    ),
    (
        "storage",
        "upload_file",
        '''\
"""File storage module."""
from __future__ import annotations

import hashlib
import logging

logger = logging.getLogger(__name__)

_objects: dict[str, bytes] = {}


def upload_file(path: str, content: bytes) -> str:
    """Upload file content and return a storage key."""
    logger.info("Uploading file: %s (%d bytes)", path, len(content))
    digest = hashlib.sha256(content).hexdigest()[:16]
    key = f"obj/{digest}/{path.rsplit('/', 1)[-1]}"
    _objects[key] = content
    logger.info("Stored as: %s", key)
    return key
''',
    ),
    (
        "ratelimit",
        "check_limit",
        '''\
"""Rate limiting module."""
from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_windows: dict[str, list[float]] = {}
_MAX_REQUESTS = 100
_WINDOW_SECONDS = 60.0


def check_limit(client_id: str, action: str) -> bool:
    """Check if a client has exceeded the rate limit for an action."""
    key = f"{client_id}:{action}"
    logger.info("Checking rate limit for %s", key)
    now = time.time()
    window = _windows.setdefault(key, [])
    window[:] = [ts for ts in window if now - ts < _WINDOW_SECONDS]
    if len(window) >= _MAX_REQUESTS:
        logger.warning("Rate limit exceeded for %s (%d requests)", key, len(window))
        return False
    window.append(now)
    logger.debug("Rate limit ok for %s: %d/%d", key, len(window), _MAX_REQUESTS)
    return True
''',
    ),
    (
        "transform",
        "apply_transform",
        '''\
"""Data transformation module."""
from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)


def apply_transform(data: list[dict[str, Any]], mapping: dict[str, str] | None = None) -> list[dict[str, Any]]:
    """Apply field-name transformations to a list of records."""
    logger.info("Transforming %d records", len(data))
    if not mapping:
        return list(data)
    results: list[dict[str, Any]] = []
    for record in data:
        transformed: dict[str, Any] = {}
        for key, value in record.items():
            new_key = mapping.get(key, key)
            transformed[new_key] = value
        results.append(transformed)
    logger.info("Transformation complete: %d records", len(results))
    return results
''',
    ),
    (
        "export",
        "generate_report",
        '''\
"""Report generation module."""
from __future__ import annotations

import json
import logging
from typing import Any

logger = logging.getLogger(__name__)


def generate_report(data: dict[str, Any], fmt: str = "json") -> str:
    """Generate a formatted report from data."""
    logger.info("Generating %s report from %d fields", fmt, len(data))
    if fmt == "json":
        output = json.dumps(data, indent=2, default=str)
    elif fmt == "text":
        lines = [f"{k}: {v}" for k, v in sorted(data.items())]
        output = "\\n".join(lines)
    else:
        raise ValueError(f"Unsupported format: {fmt}")
    logger.info("Report generated: %d chars", len(output))
    return output
''',
    ),
    (
        "encryption",
        "encrypt_payload",
        '''\
"""Payload encryption module."""
from __future__ import annotations

import hashlib
import logging

logger = logging.getLogger(__name__)


def encrypt_payload(data: bytes, key: str) -> bytes:
    """Encrypt data using a simple XOR cipher with a derived key stream."""
    logger.info("Encrypting %d bytes", len(data))
    key_bytes = hashlib.sha256(key.encode()).digest()
    result = bytearray(len(data))
    for i, byte in enumerate(data):
        result[i] = byte ^ key_bytes[i % len(key_bytes)]
    logger.info("Encryption complete: %d bytes", len(result))
    return bytes(result)
''',
    ),
    (
        "healthcheck",
        "check_health",
        '''\
"""Service health check module."""
from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_service_registry: dict[str, float] = {}


def check_health(services: list[str] | None = None) -> dict[str, bool]:
    """Check health of registered services and return status map."""
    targets = services or list(_service_registry.keys())
    logger.info("Checking health of %d services", len(targets))
    results: dict[str, bool] = {}
    now = time.time()
    for svc in targets:
        last_seen = _service_registry.get(svc, 0.0)
        healthy = (now - last_seen) < 30.0 if last_seen > 0 else False
        results[svc] = healthy
        logger.debug("Service %s: %s", svc, "healthy" if healthy else "unhealthy")
    logger.info("Health check complete: %d/%d healthy", sum(results.values()), len(results))
    return results
''',
    ),
    (
        "migration",
        "run_migration",
        '''\
"""Database migration module."""
from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

_applied: set[str] = set()


def run_migration(version: str, dry_run: bool = False) -> bool:
    """Apply a database migration by version string."""
    logger.info("Running migration %s (dry_run=%s)", version, dry_run)
    if version in _applied:
        logger.info("Migration %s already applied", version)
        return True
    # Simulate migration steps
    steps = ["validate_schema", "alter_tables", "migrate_data", "update_version"]
    for step in steps:
        logger.debug("Migration %s: executing %s", version, step)
        if dry_run:
            logger.info("Dry run: would execute %s", step)
    if not dry_run:
        _applied.add(version)
    logger.info("Migration %s %s", version, "simulated" if dry_run else "applied")
    return True
''',
    ),
    (
        "webhook",
        "dispatch_webhook",
        '''\
"""Webhook dispatching module."""
from __future__ import annotations

import hashlib
import logging
from typing import Any

logger = logging.getLogger(__name__)

_delivery_log: list[dict[str, Any]] = []


def dispatch_webhook(url: str, payload: dict[str, Any]) -> int:
    """Dispatch a webhook payload and return an HTTP-like status code."""
    logger.info("Dispatching webhook to %s", url)
    if not url.startswith(("http://", "https://")):
        logger.error("Invalid webhook URL: %s", url)
        return 400
    sig = hashlib.sha256(str(payload).encode()).hexdigest()[:12]
    delivery = {"url": url, "signature": sig, "status": 200}
    _delivery_log.append(delivery)
    logger.info("Webhook dispatched: %s (sig=%s)", url, sig)
    return 200
''',
    ),
    (
        "permissions",
        "check_access",
        '''\
"""Access control module."""
from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

_policies: dict[str, set[str]] = {
    "admin": {"read", "write", "delete", "admin"},
    "editor": {"read", "write"},
    "viewer": {"read"},
}


def check_access(user_id: str, resource: str, action: str = "read") -> bool:
    """Check whether a user has permission to perform an action on a resource."""
    logger.info("Checking access: user=%s resource=%s action=%s", user_id, resource, action)
    # Simplified: derive role from user_id prefix
    role = "viewer"
    if user_id.startswith("admin-"):
        role = "admin"
    elif user_id.startswith("editor-"):
        role = "editor"
    allowed = _policies.get(role, set())
    has_access = action in allowed
    logger.info("Access %s for %s (%s) on %s", "granted" if has_access else "denied", user_id, role, resource)
    return has_access
''',
    ),
]


def generate_workspace(file_count: int, output_dir: Path, rng: random.Random) -> Path:
    """Generate a workspace with the given number of files.

    The target function `process_data` is always included. The remaining
    modules are selected randomly from the pool.
    """
    workspace = output_dir / f"{file_count}-files"
    src_dir = workspace / "src"
    src_dir.mkdir(parents=True, exist_ok=True)

    # Always include process_data (search module)
    target = next(m for m in MODULES if m[1] == "process_data")
    others = [m for m in MODULES if m[1] != "process_data"]

    needed = file_count - 1
    if needed > len(others):
        print(f"Warning: requested {file_count} files but only {len(others) + 1} available", file=sys.stderr)
        needed = len(others)
    selected = rng.sample(others, needed)

    all_modules = [target] + selected
    rng.shuffle(all_modules)

    for module_name, _, source in all_modules:
        (src_dir / f"{module_name}.py").write_text(source)

    # Generate prompt
    file_list = sorted(f.name for f in src_dir.iterdir() if f.suffix == ".py")
    prompt = (
        f"Read all {len(file_list)} Python files in the `src/` directory and identify "
        f"which one exports a function named `process_data`. "
        f"Report the filename and the function's signature.\n\n"
        f"Files in src/:\n"
    )
    for fname in file_list:
        prompt += f"- {fname}\n"
    (workspace / "prompt.md").write_text(prompt)

    # Generate README
    readme = (
        f"# Synthetic Workspace ({file_count} files)\n\n"
        f"This workspace contains {file_count} independent Python modules in `src/`.\n"
        f"Each module exports a single function. One of them exports `process_data`.\n"
    )
    (workspace / "README.md").write_text(readme)

    return workspace


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Generate synthetic workspaces for the parallel-ops benchmark",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=Path("fixtures"),
        help="Output directory (default: fixtures/)",
    )
    parser.add_argument(
        "--file-counts",
        type=str,
        default="5,10,15,20",
        help="Comma-separated file counts to generate (default: 5,10,15,20)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for reproducibility (default: 42)",
    )
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()

    output_dir: Path = args.output
    file_counts = [int(x.strip()) for x in args.file_counts.split(",")]
    rng = random.Random(args.seed)

    print(f"Generating workspaces: {file_counts}")
    print(f"Output directory: {output_dir}")
    print()

    for count in file_counts:
        workspace = generate_workspace(count, output_dir, rng)
        file_list = sorted(f.name for f in (workspace / "src").iterdir() if f.suffix == ".py")
        print(f"  {count}-files/: {len(file_list)} modules -> {workspace}")

    print()
    print("Done.")


if __name__ == "__main__":
    main()
