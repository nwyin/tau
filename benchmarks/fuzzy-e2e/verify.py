"""Output verification for fuzzy-e2e benchmark.

Normalizes actual output and compares against expected, producing a
VerifyResult with success status, diff, and normalized texts.

Normalization pipeline (ported from edit-bench):
1. CRLF -> LF
2. Strip trailing whitespace per line
3. Collapse blank line runs (3+ -> 2)
4. Optional: format with language formatter (auto-detected from extension)
5. Exact text comparison
"""

from __future__ import annotations

import difflib
import os
import re
import subprocess
from dataclasses import dataclass


@dataclass
class VerifyResult:
    success: bool
    diff: str | None = None
    actual_normalized: str = ""
    expected_normalized: str = ""
    formatter_used: str | None = None


FORMATTERS_BY_EXT: dict[str, list[str]] = {
    ".py": ["uvx", "ruff", "format", "--stdin-filename", "{filename}", "-"],
    ".rs": ["rustfmt", "--edition", "2021"],
    ".go": ["gofmt"],
    ".js": ["npx", "prettier", "--stdin-filepath", "{filename}"],
    ".jsx": ["npx", "prettier", "--stdin-filepath", "{filename}"],
    ".ts": ["npx", "prettier", "--stdin-filepath", "{filename}"],
    ".tsx": ["npx", "prettier", "--stdin-filepath", "{filename}"],
}

_warned_formatters: set[str] = set()


def normalize_line_endings(text: str) -> str:
    """CRLF/CR -> LF, strip trailing whitespace per line, remove trailing blank lines."""
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[-1]:
        lines.pop()
    if not lines:
        return "\n"
    return "\n".join(lines) + "\n"


def collapse_blank_lines(text: str) -> str:
    """Collapse runs of 3+ consecutive newlines into 2 (max one blank line between content)."""
    return re.sub(r"\n{3,}", "\n\n", text)


def restore_whitespace_only_diffs(expected: str, actual: str) -> str:
    """If a line differs only in whitespace, use the expected version.

    Ported from react-edit-benchmark's restoreWhitespaceOnlyLineDiffs.
    """
    exp_lines = expected.split("\n")
    act_lines = actual.split("\n")
    out: list[str] = []
    for i in range(max(len(exp_lines), len(act_lines))):
        exp = exp_lines[i] if i < len(exp_lines) else None
        act = act_lines[i] if i < len(act_lines) else None
        if exp is None or act is None:
            out.append(act if act is not None else "")
        elif exp != act and re.sub(r"\s+", "", exp) == re.sub(r"\s+", "", act):
            out.append(exp)
        else:
            out.append(act)
    return "\n".join(out)


def format_code(text: str, filename: str) -> tuple[str, str | None]:
    """Format code through the appropriate language formatter.

    Returns (formatted_text, formatter_name). Falls back to input on failure.
    """
    ext = os.path.splitext(filename)[1]
    cmd_template = FORMATTERS_BY_EXT.get(ext)
    if not cmd_template:
        return text, None
    cmd = [part.replace("{filename}", filename) for part in cmd_template]
    try:
        result = subprocess.run(cmd, input=text, capture_output=True, text=True, timeout=10)
        if result.returncode == 0 and result.stdout:
            return result.stdout, cmd[0]
    except FileNotFoundError:
        if cmd[0] not in _warned_formatters:
            _warned_formatters.add(cmd[0])
            print(f"Warning: formatter '{cmd[0]}' not found, skipping format normalization for {ext}")
    except subprocess.TimeoutExpired:
        pass
    return text, None


def verify(actual_text: str, expected_text: str, filename: str, use_formatter: bool = True) -> VerifyResult:
    """Full normalization pipeline: normalize, restore whitespace diffs, collapse blanks, format.

    Returns VerifyResult with success status and diff.
    """
    actual_norm = normalize_line_endings(actual_text)
    expected_norm = normalize_line_endings(expected_text)

    actual_norm = restore_whitespace_only_diffs(expected_norm, actual_norm)

    actual_norm = collapse_blank_lines(actual_norm)
    expected_norm = collapse_blank_lines(expected_norm)

    formatter_used = None
    if use_formatter:
        actual_norm, f1 = format_code(actual_norm, filename)
        expected_norm, f2 = format_code(expected_norm, filename)
        formatter_used = f1 or f2

    if actual_norm == expected_norm:
        return VerifyResult(
            success=True,
            actual_normalized=actual_norm,
            expected_normalized=expected_norm,
            formatter_used=formatter_used,
        )

    diff_lines = list(
        difflib.unified_diff(
            expected_norm.splitlines(keepends=True),
            actual_norm.splitlines(keepends=True),
            fromfile=f"expected/{filename}",
            tofile=f"actual/{filename}",
        )
    )
    diff_text = "".join(diff_lines)[:2000]
    return VerifyResult(
        success=False,
        diff=diff_text,
        actual_normalized=actual_norm,
        expected_normalized=expected_norm,
        formatter_used=formatter_used,
    )


def verify_file(actual_path: str, expected_path: str, use_formatter: bool = True) -> VerifyResult:
    """Verify a file on disk against expected output."""
    from pathlib import Path

    actual_p = Path(actual_path)
    expected_p = Path(expected_path)

    if not actual_p.exists():
        return VerifyResult(success=False, diff="actual file does not exist")
    if not expected_p.exists():
        return VerifyResult(success=False, diff="expected file does not exist")

    actual_text = actual_p.read_text()
    expected_text = expected_p.read_text()
    filename = actual_p.name

    return verify(actual_text, expected_text, filename, use_formatter=use_formatter)
