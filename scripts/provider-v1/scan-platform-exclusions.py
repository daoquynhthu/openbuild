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

    # Patterns for platform exclusions:
    # 1. #[cfg(not(target_os = "windows"))] — explicit Windows exclusion
    # 2. #[cfg(unix)] — positive unix cfg (equivalent on non-Windows)
    # 3. #[cfg(all(test, unix))] — unix + test combo
    # 4. #[cfg_attr(windows, ignore)] — test ignore on Windows
    # 5. #[cfg(any(unix, ...))] — unix in any-clause
    # 6. #[cfg(not(windows))] — short form (uncommon but possible)
    cfg_pattern = re.compile(
        r'#\[\s*cfg(?:\((?P<inner>[^)]+)\)|_attr\((?P<attr_inner>[^)]+)\))',
    )
    not_windows = re.compile(
        r'not\(\s*target_os\s*=\s*"windows"\s*\)|'
        r'(?<!\w)unix(?!\w)|'
        r'not\(\s*windows\s*\)'
    )
    windows_ignore = re.compile(r'target_os\s*=\s*"windows".*ignore|ignore.*target_os\s*=\s*"windows"')

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
            is_attr = m.group("attr_inner") is not None
            if not_windows.search(inner) or (is_attr and windows_ignore.search(inner)):
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
    parser.add_argument("--output", type=Path, help="Write exclusion ledger to this path")
    parser.add_argument("--check-ledger", action="store_true", help="Exit non-zero if any OPEN exclusions exist")
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

    if args.check_ledger:
        open_entries = [r for r in unique if r["status"] == "OPEN"]
        if open_entries:
            print(f"CHECK-LEDGER FAIL: {len(open_entries)} unresolved exclusion(s)")
            for r in open_entries:
                print(f"  {r['ID']}  {r['file']}:{r['line']}  {r['symbol/test']}")
            sys.exit(1)
        else:
            print(f"CHECK-LEDGER PASS: all {len(unique)} exclusions resolved")
        return

    if args.output:
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
    else:
        print("Specify --output to write the ledger or --check-ledger to verify", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
