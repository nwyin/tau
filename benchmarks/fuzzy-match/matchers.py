"""Fuzzy edit matching strategies ported from various coding agent harnesses.

Each matcher takes (file_content, old_string) and returns a list of Match objects.
A good matcher returns exactly one correct match. Zero = missed. >1 = ambiguous.
Wrong location = false positive (the dangerous failure mode).

Sources:
- tau: exact match only
- pi-mono: trailing-ws + unicode normalization (2-pass)
- codex: 4-pass cascade (exact → trim_end → trim → unicode)
- opencode: 9-strategy chain
- oh-my-pi: Levenshtein with threshold tuning
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class Match:
    """A candidate match location in the file content."""

    start: int  # byte offset in file_content
    end: int  # byte offset end
    matched_text: str  # the actual text that matched
    confidence: float  # 0.0 to 1.0
    strategy: str  # which strategy produced this match


def find_all_exact(content: str, pattern: str) -> list[int]:
    """Find all exact occurrences of pattern in content, return start offsets."""
    positions = []
    start = 0
    while True:
        pos = content.find(pattern, start)
        if pos == -1:
            break
        positions.append(pos)
        start = pos + 1
    return positions


# ---------------------------------------------------------------------------
# Strategy: exact (tau current)
# ---------------------------------------------------------------------------


def match_exact(content: str, old_string: str) -> list[Match]:
    """Tau's current approach: pure exact string match."""
    positions = find_all_exact(content, old_string)
    return [
        Match(
            start=pos,
            end=pos + len(old_string),
            matched_text=old_string,
            confidence=1.0,
            strategy="exact",
        )
        for pos in positions
    ]


# ---------------------------------------------------------------------------
# Strategy: normalized (pi-mono)
# Trailing whitespace strip + unicode normalization. 2-pass.
# ---------------------------------------------------------------------------

_UNICODE_REPLACEMENTS = [
    ("\u2018", "'"),  # left single quote
    ("\u2019", "'"),  # right single quote
    ("\u201c", '"'),  # left double quote
    ("\u201d", '"'),  # right double quote
    ("\u2013", "-"),  # en-dash
    ("\u2014", "--"),  # em-dash
    ("\u2026", "..."),  # ellipsis
    ("\u00a0", " "),  # non-breaking space
    ("\u2002", " "),  # en-space
    ("\u2003", " "),  # em-space
    ("\u2009", " "),  # thin space
]


def _normalize_unicode(text: str) -> str:
    """Normalize unicode punctuation to ASCII equivalents."""
    for uni, ascii_char in _UNICODE_REPLACEMENTS:
        text = text.replace(uni, ascii_char)
    return text


def _strip_trailing_ws(text: str) -> str:
    """Strip trailing whitespace from each line."""
    return "\n".join(line.rstrip() for line in text.split("\n"))


def _normalize_pimono(text: str) -> str:
    """pi-mono normalization: trailing-ws + unicode."""
    return _normalize_unicode(_strip_trailing_ws(text))


def match_normalized(content: str, old_string: str) -> list[Match]:
    """pi-mono approach: exact first, then normalized."""
    # Pass 1: exact
    exact = match_exact(content, old_string)
    if exact:
        return exact

    # Pass 2: normalize both sides
    norm_content = _normalize_pimono(content)
    norm_old = _normalize_pimono(old_string)
    if not norm_old:
        return []

    positions = find_all_exact(norm_content, norm_old)
    if not positions:
        return []

    # Map normalized positions back to original content.
    # This is approximate — works when normalization only affects trailing ws.
    # For production use, you'd need a proper offset map.
    original_lines = content.split("\n")
    old_lines = norm_old.split("\n")

    results = []
    for norm_pos in positions:
        # Find the line that contains this position in the normalized content
        line_start = norm_content.count("\n", 0, norm_pos)
        # Reconstruct the original text at the same line range
        line_end = line_start + len(old_lines)
        if line_end <= len(original_lines):
            original_text = "\n".join(original_lines[line_start:line_end])
            # Find byte offset in original content
            orig_offset = len("\n".join(original_lines[:line_start]))
            if line_start > 0:
                orig_offset += 1  # account for the \n
            results.append(
                Match(
                    start=orig_offset,
                    end=orig_offset + len(original_text),
                    matched_text=original_text,
                    confidence=0.98,
                    strategy="normalized",
                )
            )
    return results


# ---------------------------------------------------------------------------
# Strategy: trimmed-cascade (codex)
# 4-pass: exact → trim_end → trim → unicode normalize
# ---------------------------------------------------------------------------


def match_trimmed_cascade(content: str, old_string: str) -> list[Match]:
    """Codex approach: 4-pass cascade with decreasing confidence."""
    passes: list[tuple[str, float, callable]] = [
        ("exact", 1.0, lambda s: s),
        ("trim_end", 0.99, lambda s: "\n".join(line.rstrip() for line in s.split("\n"))),
        ("trim_both", 0.98, lambda s: "\n".join(line.strip() for line in s.split("\n"))),
        ("unicode", 0.97, _normalize_pimono),
    ]

    for pass_name, confidence, normalize_fn in passes:
        norm_content = normalize_fn(content)
        norm_old = normalize_fn(old_string)
        positions = find_all_exact(norm_content, norm_old)

        if positions:
            # Map back to original (same approach as normalized)
            results = []
            orig_lines = content.split("\n")
            old_line_count = norm_old.count("\n") + 1

            for norm_pos in positions:
                start_line = norm_content[:norm_pos].count("\n")
                end_line = start_line + old_line_count
                if end_line <= len(orig_lines):
                    matched = "\n".join(orig_lines[start_line:end_line])
                    orig_offset = len("\n".join(orig_lines[:start_line]))
                    if start_line > 0:
                        orig_offset += 1
                    results.append(
                        Match(
                            start=orig_offset,
                            end=orig_offset + len(matched),
                            matched_text=matched,
                            confidence=confidence,
                            strategy=f"trimmed-cascade/{pass_name}",
                        )
                    )
            return results

    return []


# ---------------------------------------------------------------------------
# Strategy: levenshtein (oh-my-pi)
# Line-by-line Levenshtein similarity with configurable threshold.
# ---------------------------------------------------------------------------


def _levenshtein_distance(s1: str, s2: str) -> int:
    """Standard Levenshtein edit distance."""
    if len(s1) < len(s2):
        return _levenshtein_distance(s2, s1)
    if len(s2) == 0:
        return len(s1)

    prev_row = list(range(len(s2) + 1))
    for i, c1 in enumerate(s1):
        curr_row = [i + 1]
        for j, c2 in enumerate(s2):
            insertions = prev_row[j + 1] + 1
            deletions = curr_row[j] + 1
            substitutions = prev_row[j] + (c1 != c2)
            curr_row.append(min(insertions, deletions, substitutions))
        prev_row = curr_row

    return prev_row[-1]


def _levenshtein_similarity(s1: str, s2: str) -> float:
    """Levenshtein similarity ratio (0.0 to 1.0)."""
    if not s1 and not s2:
        return 1.0
    max_len = max(len(s1), len(s2))
    if max_len == 0:
        return 1.0
    return 1.0 - _levenshtein_distance(s1, s2) / max_len


def match_levenshtein(content: str, old_string: str, threshold: float = 0.92) -> list[Match]:
    """oh-my-pi approach: sliding window Levenshtein similarity.

    Slides a window of len(old_string) lines over the content and computes
    line-by-line similarity. Returns matches above threshold.
    """
    # First try exact
    exact = match_exact(content, old_string)
    if exact:
        return exact

    content_lines = content.split("\n")
    old_lines = old_string.split("\n")
    window_size = len(old_lines)

    if window_size == 0 or window_size > len(content_lines):
        return []

    candidates: list[tuple[int, float]] = []  # (start_line, similarity)

    for i in range(len(content_lines) - window_size + 1):
        window = content_lines[i : i + window_size]

        # Compute average line-by-line similarity
        if len(window) != len(old_lines):
            continue

        total_sim = sum(_levenshtein_similarity(w.rstrip(), o.rstrip()) for w, o in zip(window, old_lines))
        avg_sim = total_sim / len(old_lines)

        if avg_sim >= threshold:
            candidates.append((i, avg_sim))

    if not candidates:
        return []

    # oh-my-pi's dominant match heuristic: best must be >= 0.97 AND
    # >= 0.08 ahead of second-best to auto-pick
    candidates.sort(key=lambda x: x[1], reverse=True)

    results = []
    for start_line, similarity in candidates:
        matched_text = "\n".join(content_lines[start_line : start_line + window_size])
        orig_offset = len("\n".join(content_lines[:start_line]))
        if start_line > 0:
            orig_offset += 1
        results.append(
            Match(
                start=orig_offset,
                end=orig_offset + len(matched_text),
                matched_text=matched_text,
                confidence=similarity,
                strategy=f"levenshtein-{int(threshold * 100)}",
            )
        )

    return results


# ---------------------------------------------------------------------------
# Registry
# ---------------------------------------------------------------------------

MATCHERS: dict[str, callable] = {
    "exact": match_exact,
    "normalized": match_normalized,
    "trimmed-cascade": match_trimmed_cascade,
    "levenshtein-80": lambda c, o: match_levenshtein(c, o, threshold=0.80),
    "levenshtein-92": lambda c, o: match_levenshtein(c, o, threshold=0.92),
    "levenshtein-95": lambda c, o: match_levenshtein(c, o, threshold=0.95),
}
