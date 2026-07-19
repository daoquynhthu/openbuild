"""Scan crates/ for Windows platform exclusions and generate WX ledger."""

import hashlib
import os
import re
import sys
from pathlib import Path


def wx_id(relative_path, symbol_path, cfg_expr):
    raw = f"{relative_path}||{symbol_path}||{cfg_expr}"
    return "WX-" + hashlib.sha256(raw.encode()).hexdigest()[:12]


def scan_file(path, crates_root):
    """Scan a single Rust file for Windows cfg exclusions."""
    results = []
    rel = str(path.relative_to(crates_root).as_posix())
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return results
    lines = text.splitlines()

    nw_pat = re.compile(r'not\(\s*target_os\s*=\s*"windows"\s*\)')
    current_mods = []

    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        for m in re.finditer(r'^\s*(?:pub\s+)?mod\s+(\w+)', stripped):
            current_mods.append(m.group(1))
        for m in re.finditer(r'^\s*\}', stripped):
            if current_mods:
                current_mods.pop()

        if not re.search(r'#\[\s*cfg', stripped):
            continue

        m = nw_pat.search(stripped)
        if not m:
            continue

        symbol = "unknown"
        for j in range(i, min(i + 5, len(lines))):
            fn_m = re.match(
                r'^\s*(?:pub\s+)?(?:fn|mod|struct|enum|trait|type|const|static)\s+(\w+)',
                lines[j],
            )
            if fn_m:
                symbol = fn_m.group(1)
                break
            if re.match(r'^\s*#\[\s*test\s*\]', lines[j]):
                for k in range(j + 1, min(j + 3, len(lines))):
                    fn_m2 = re.match(r'^\s*(?:pub\s+)?fn\s+(\w+)', lines[k])
                    if fn_m2:
                        symbol = fn_m2.group(1)
                        break
                break

        cfg_line = m.group(0)
        symbol_path = "::".join(current_mods + [symbol]) if current_mods else symbol
        wid = wx_id(rel, symbol_path, cfg_line)
        results.append({
            "ID": wid,
            "file": rel,
            "line": str(i),
            "symbol/test": symbol_path,
            "cfg expression": cfg_line,
            "classification": "",
            "owner phase": "",
            "repair task": "",
            "rationale evidence": "",
            "status": "OPEN",
        })
    return results


def scan_all(crates_root):
    all_results = []
    for root, dirs, files in os.walk(crates_root):
        for f in files:
            if f.endswith(".rs"):
                p = Path(root) / f
                try:
                    results = scan_file(p, crates_root)
                    all_results.extend(results)
                except Exception:
                    pass

    seen = set()
    unique = []
    for r in all_results:
        if r["ID"] not in seen:
            seen.add(r["ID"])
            unique.append(r)

    unique.sort(key=lambda r: (r["file"], int(r["line"]), r["symbol/test"]))
    return unique


def write_ledger(entries, output_path):
    lines = [
        "# Windows Test Exclusion Ledger",
        f"# Generated: wx_scanner.py",
        f"# Total: {len(entries)} entries",
        "",
        "| ID | file | line | symbol/test | cfg expression | classification | owner phase | repair task | rationale evidence | status |",
        "|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in entries:
        lines.append(
            f"| {r['ID']} | {r['file']} | {r['line']} | {r['symbol/test']} | {r['cfg expression']} | "
            f"{r['classification']} | {r['owner phase']} | {r['repair task']} | {r['rationale evidence']} | {r['status']} |"
        )
    output_path.write_text("\n".join(lines), encoding="utf-8")
    return len(entries)


def validate_ledger(ledger_path: Path, crates_root: Path) -> int:
    """Validate that ledger scan results match current source."""
    fresh = scan_all(crates_root)
    fresh_ids = {r["ID"] for r in fresh}

    text = ledger_path.read_text(encoding="utf-8")
    import re
    ledger_ids = set(re.findall(r"WX-[a-f0-9]{12}", text))

    only_fresh = fresh_ids - ledger_ids
    only_ledger = ledger_ids - fresh_ids

    if only_fresh:
        print(f"ERROR: {len(only_fresh)} exclusion(s) in source but not in ledger:", file=sys.stderr)
        for wid in sorted(only_fresh)[:10]:
            print(f"  {wid}", file=sys.stderr)
    if only_ledger:
        print(f"ERROR: {len(only_ledger)} exclusion(s) in ledger but not in source:", file=sys.stderr)
        for wid in sorted(only_ledger)[:10]:
            print(f"  {wid}", file=sys.stderr)

    return len(only_fresh) + len(only_ledger)


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    parser.add_argument("--validate-ledger", type=Path, help="Validate an existing ledger against current source")
    args = parser.parse_args()

    crates_root = Path(__file__).resolve().parent.parent.parent / "crates"

    if args.validate_ledger:
        errors = validate_ledger(args.validate_ledger, crates_root)
        if errors:
            print(f"VALIDATION FAILED: {errors} discrepancies", file=sys.stderr)
            exit(1)
        print("VALIDATION PASSED: ledger matches source", file=sys.stderr)
        exit(0)

    entries = scan_all(crates_root)
    n = write_ledger(entries, args.output)
    print(f"Wrote {n} entries to {args.output}", file=sys.stderr)
