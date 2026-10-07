#!/usr/bin/env python3
"""Catch feature-local raw visual values; native control sizes remain allowed."""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parent.parent
rules = {
    "raw color": r"Color\(\s*(?:red:|white:|hue:|hex:)",
    "raw font size": r"\.font\(\s*\.system\(\s*size:",
    "raw corner radius": r"cornerRadius:\s*\d",
    "raw padding": r"\.padding\(\s*(?:\.\w+\s*,\s*)?\d",
    "raw frame dimension": r"\.frame\(\s*(?:width|height|minWidth|minHeight|idealWidth|maxWidth):\s*\d",
}
failures = []
for path in (root / "Sources/CleanYourMacUI").rglob("*.swift"):
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        for label, pattern in rules.items():
            if re.search(pattern, line):
                failures.append(f"{path.relative_to(root)}:{line_number}: {label}")
if failures:
    print("\n".join(failures))
    sys.exit(1)
print("Design-system token checks passed.")
