#!/usr/bin/env python3
"""Scan crates/ for Windows platform exclusions and generate WX ledger."""

import hashlib
import re
import sys
from pathlib import Path

CRATES = Path(__file__).resolve().parent.parent.parent / "crates"


def wx_id(relative_path: str, symbol_path: str, cfg_expr: str) -> str:
    """Generate stable WX-<12 hex> ID."""
    raw = f"{relative_path}||{symbol_path}||{cfg_expr}"
    return "WX-" + hashlib.sha256(raw.encode()).hexdigest()[:12]


def scan_file(path: Path) -> list[dict]:
    """Scan a single Rust file for Windows cfg exclusions."""
    results = []
    rel = path.relative_to(CRATES).as_posix()
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()

    # Pattern: #[cfg(...not(target_os = "windows")...)]
    # or #[cfg_attr(...not(target_os = "windows")...)]
    cfg_pattern = re.compile(
        r'#\[\s*cfg(?:\((?P<inner>[^)]+)\)|_attr\((?P<attr_inner>[^)]+)\))',
    )
    not_windows = re.compile(r'not\(\s*target_os\s*=\s*"windows"\s*\)')

    # Track whether we're inside a test module to build symbol path
    current_mods = []

    for i, line in enumerate(lines, 1):
        stripped = line.strip()

        # Track module nesting (simplified)
        for m in re.finditer(r'^\s*(?:pub\s+)?mod\s+(\w+)', stripped):
            current_mods.append(m.group(1))
        for m in re.finditer(r'^\s*\}', stripped):
            if current_mods:
                current_mods.pop()

        for m in cfg_pattern.finditer(stripped):
            inner = m.group("inner") or m.group("attr_inner") or ""
            if not_windows.search(inner):
                # Determine symbol context
                # Look for the next fn/mod/struct definition
                symbol = "unknown"
                for j in range(i, min(i + 5, len(lines))):
                    fn_m = re.match(
                        r'^\s*(?:pub\s+)?(?:fn|mod|struct|enum|trait|type|const|static)\s+(\w+)',
                        lines[j],
                    )
                    if fn_m:
                        symbol = fn_m.group(1)
                        break
                    test_fn = re.match(
                        r'^\s*#\[\s*test\s*\]',
                        lines[j],
                    )
                    if test_fn:
                        # Next line with fn
                        for k in range(j + 1, min(j + 3, len(lines))):
                            fn_m2 = re.match(
                                r'^\s*(?:pub\s+)?fn\s+(\w+)',
                                lines[k],
                            )
                            if fn_m2:
                                symbol = fn_m2.group(1)
                                break
                        break

                symbol_path = "::".join(current_mods + [symbol]) if current_mods else symbol
                cfg_expr = inner.strip()
                wid = wx_id(rel, symbol_path, cfg_expr)

                results.append({
                    "ID": wid,
                    "file": rel,
                    "line": str(i),
                    "symbol/test": symbol_path,
                    "cfg expression": cfg_expr,
                    "classification": "",
                    "owner phase": "",
                    "repair task": "",
                    "rationale evidence": "",
                    "status": "OPEN",
                })

    return results


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Scan for Windows platform exclusions")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    all_results = []
    for path in sorted(CRATES.rglob("*.rs")):
        try:
            results = scan_file(path)
            all_results.extend(results)
        except Exception as e:
            print(f"Error scanning {path}: {e}", file=sys.stderr)

    # Deduplicate by WX ID
    seen = set()
    unique = []
    for r in all_results:
        if r["ID"] not in seen:
            seen.add(r["ID"])
            unique.append(r)

    unique.sort(key=lambda r: (r["file"], int(r["line"]), r["symbol/test"]))

    lines = [
        "# Windows Test Exclusion Ledger",
        f"# Generated: scan-platform-exclusions.py",
        f"# Total: {len(unique)} entries",
        "",
        "| ID | file | line | symbol/test | cfg expression | classification | owner phase | repair task | rationale evidence | status |",
        "|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in unique:
        lines.append(
            f"| {r['ID']} | {r['file']} | {r['line']} | {r['symbol/test']} | {r['cfg expression']} | "
            f"{r['classification']} | {r['owner phase']} | {r['repair task']} | {r['rationale evidence']} | {r['status']} |"
        )

    output = "\n".join(lines)
    args.output.write_text(output, encoding="utf-8")
    print(f"Wrote {len(unique)} entries to {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
