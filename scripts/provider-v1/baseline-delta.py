#!/usr/bin/env python3
"""Compare current failure/ignore/allow/skip counts against baseline.

Usage:
    python baseline-delta.py --baseline <baseline-file> --current <current-results-dir>

Returns non-zero if any metric has increased.
"""

import argparse
import re
import sys
from pathlib import Path


def count_patterns(directory: Path, patterns: list[str]) -> dict[str, int]:
    """Count occurrences of regex patterns in all files under directory."""
    counts = {}
    for p in patterns:
        total = 0
        pat = re.compile(p)
        for f in sorted(directory.rglob("*.txt")):
            text = f.read_text(encoding="utf-8", errors="replace")
            total += len(pat.findall(text))
        counts[p] = total
    return counts


def main():
    parser = argparse.ArgumentParser(description="Baseline delta checker")
    parser.add_argument("--baseline", type=Path, required=True, help="Static fingerprint file (from P0-006)")
    parser.add_argument("--current", type=Path, required=True, help="Directory with current scans")
    args = parser.parse_args()

    baseline_counts = {}
    if args.baseline.exists():
        text = args.baseline.read_text(encoding="utf-8")
        for line in text.splitlines():
            m = re.match(r"^\| (\w[\w\s-]+) \| (\d+)", line)
            if m:
                baseline_counts[m.group(1).strip()] = int(m.group(2))

    changed = False
    for label, pattern in [
        ("windows exclusions", r'not\(target_os = "windows"\)'),
        ("ignored tests", r'#\[ignore\]'),
        ("allow attributes", r'#\[allow\('),
    ]:
        current_count = count_patterns(args.current, [pattern]).get(pattern, 0)
        baseline_count = baseline_counts.get(label, -1)
        direction = ""
        if baseline_count >= 0:
            if current_count > baseline_count:
                print(f"REGRESSION: {label}: {current_count} > baseline {baseline_count}")
                changed = True
                direction = " ↑"
            elif current_count < baseline_count:
                direction = f" ↓ ({baseline_count} → {current_count})"
            else:
                direction = " (unchanged)"
        print(f"  {label}: {current_count}{direction}")

    if changed:
        print("DELTA FAILED: regressions detected")
        sys.exit(1)
    print("DELTA PASSED: no regressions")


if __name__ == "__main__":
    main()
