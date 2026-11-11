"""Literal detection: find exact strings by grammar, before any model runs.

A literal is a typed exact string with a byte span into its evidence. The value
is always a byte-for-byte copy of that span and is never generated
(INV-3, docs/architecture/06-memory-formation.md section 5).

This first version covers three kinds. More grammars are added the same way.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import List

_PATTERNS = [
    # Order matters: earlier kinds win where spans overlap.
    ("command", re.compile(r"`([^`\n]+)`")),
    ("url", re.compile(r"https?://[^\s<>\"'`)\]]*[^\s<>\"'`)\].,;:!?]")),  # no trailing punctuation
    ("version", re.compile(r"(?<![\w.])v?\d+\.\d+(?:\.\d+)?(?:-[0-9A-Za-z.]+)?(?![\w.]*\d)")),
]


@dataclass(frozen=True)
class Literal:
    kind: str
    exact: str
    byte_start: int
    byte_end: int


def detect_literals(text: str) -> List[Literal]:
    """Return the literals in `text`, in order, with UTF-8 byte offsets."""
    found: List[Literal] = []
    taken: List[range] = []

    for kind, pattern in _PATTERNS:
        for match in pattern.finditer(text):
            group = 1 if match.lastindex else 0
            start, end = match.span(group)
            if any(start < r.stop and r.start < end for r in taken):
                continue
            taken.append(range(start, end))
            byte_start = len(text[:start].encode("utf-8"))
            exact = text[start:end]
            found.append(Literal(kind, exact, byte_start, byte_start + len(exact.encode("utf-8"))))

    return sorted(found, key=lambda lit: lit.byte_start)
