#!/usr/bin/env python3
"""Assert Provider V1 invariants: scan production code for forbidden patterns.

Each check is independently reported. The script exits non-zero when any
production occurrence is found.
"""

import argparse
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent.parent
CRATES_DIR = REPO_ROOT / "crates"
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"

# Check definitions: (name, file_glob, regex, exclude_patterns)
CHECKS = [
    (
        "Handle::block_on in provider request preparation",
        "*.rs",
        r"Handle(::current\(\))?\.block_on\(",
        ["xai-grok-shell", "xai-grok-provider", "xai-grok-sampler", "xai-grok-pager"],
    ),
    (
        "Runtime::block_on in provider request preparation",
        "*.rs",
        r"(?:tokio::runtime::)?(?:Runtime(::new\(\))?(\.unwrap\(\))?|rt|runtime)\.block_on\(",
        ["xai-grok-shell", "xai-grok-provider", "xai-grok-sampler", "xai-grok-pager"],
    ),
    (
        "unimplemented!/todo! in xai-grok-provider production code",
        "*.rs",
        r"\bunimplemented!|\btodo!",
        ["xai-grok-provider"],
    ),
    (
        "ProviderRuntime::new() in Pager production paths",
        "*.rs",
        r"ProviderRuntime::new\(\)",
        ["xai-grok-pager"],
    ),
    (
        "route-compiler error followed by legacy sampling_config_for_model fallback",
        "*.rs",
        r"Err\(_\)\s*=>\s*crate::agent::config::sampling_config_for_model",
        ["xai-grok-shell"],
    ),
    (
        "unknown protocol silently defaulted to ChatCompletions",
        "*.rs",
        r"_\s*=>\s*(?:.*\s*::\s*)?ApiBackend::ChatCompletions",
        ["xai-grok-provider"],
    ),
    (
        "manual ModelEntry construction in provider crate (bypasses config resolution)",
        "*.rs",
        r"ModelEntry",
        ["xai-grok-provider"],
    ),
    (
        "continue-on-error in release-gate workflows",
        "*.yml",
        r"continue-on-error:\s*true",
        [],
    ),
]

# Crate-specific filter: only scan files under these crate directories
CRATE_FILTERS: dict[str, set[str]] = {
    "xai-grok-provider": {"xai-grok-provider"},
    "xai-grok-pager": {"xai-grok-pager"},
}


def _find_test_ranges(lines: list[str]) -> list[tuple[int, int]]:
    """Find line ranges (start, end) of test modules.

    Returns list of (start_line_0index, end_line_0index) for each
    #[cfg(test)] mod tests { ... } block.
    """
    ranges: list[tuple[int, int]] = []
    stack: list[tuple[int, int]] = []  # (start_line, start_brace_depth)

    brace_depth = 0
    for i, line in enumerate(lines):
        s = line.strip()

        if s.startswith("#[cfg(test)]"):
            rest = s[len("#[cfg(test)]"):].strip()
            # mod tests/mod test_ on same line or next line
            has_mod = bool(re.match(r"mod\s+(tests|test_\w*)\s*\{", rest))
            if not has_mod and i + 1 < len(lines):
                has_mod = bool(re.match(r"\s*mod\s+(tests|test_\w*)\s*\{", lines[i + 1]))
            if has_mod:
                stack.append((i, brace_depth))

        opener_count = s.count("{")
        closer_count = s.count("}")
        brace_depth += opener_count - closer_count

        if opener_count != closer_count:
            while stack:
                start, start_depth = stack[-1]
                if brace_depth <= start_depth:
                    ranges.append((start, i))
                    stack.pop()
                else:
                    break

    for start, _ in stack:
        ranges.append((start, len(lines) - 1))

    return ranges


def _is_test_module(lines: list[str], line_idx: int) -> bool:
    """Check if line_idx is inside a #[cfg(test)] or mod tests block."""
    ranges = _find_test_ranges(lines)
    for start, end in ranges:
        if start <= line_idx <= end or end == -1 and start <= line_idx:
            return True
    return False


def _is_test_file(path: Path) -> bool:
    """Check if a file is a test file by path convention."""
    rel = path.as_posix()
    return (
        "/tests/" in rel
        or "/benches/" in rel
        or rel.endswith("_tests.rs")
        or rel.endswith("/tests.rs")
        or "_test.rs" in rel
    )


def _passes_crate_filter(path: Path, crate_filter: list[str]) -> bool:
    """Return True if path matches one of the allowed crate names."""
    if not crate_filter:
        return True
    rel = path.as_posix()
    for name in crate_filter:
        if f"/{name}/" in rel:
            return True
    return False


def scan_file(path: Path) -> list[dict]:
    """Scan a single file for all invariants. Returns list of findings."""
    findings = []
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()

    is_test = _is_test_file(path)

    for name, glob, pattern, crate_filter in CHECKS:
        if glob != "*.rs":
            continue

        if not _passes_crate_filter(path, crate_filter):
            continue

        for m in re.finditer(pattern, text):
            line_idx = text[: m.start()].count("\n")
            line_num = line_idx + 1

            if is_test:
                continue

            if path.suffix == ".rs" and _is_test_module(lines, line_idx):
                continue

            findings.append({
                "check": name,
                "path": str(path),
                "line": line_num,
                "matched": m.group(),
            })

    return findings


def scan_unknown_fields() -> list[dict]:
    """Check that provider config types use deny_unknown_fields."""
    findings = []
    config_paths = [
        CRATES_DIR / "codegen" / "xai-grok-provider" / "src" / "config.rs",
    ]
    for path in config_paths:
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        if "#[serde(deny_unknown_fields)]" not in text:
            findings.append({
                "check": "unknown-field-tolerant provider schema",
                "path": str(path.relative_to(REPO_ROOT)),
                "line": 1,
                "matched": "missing #[serde(deny_unknown_fields)] on provider config type(s)",
            })
    return findings


def scan_workflows() -> list[dict]:
    """Scan workflow YAML files for continue-on-error in release-gate jobs."""
    findings = []

    # Baseline diagnostic workflow is allowed to use continue-on-error
    baseline_name = "provider-v1-baseline.yml"

    for wf in WORKFLOWS_DIR.glob("*.yml"):
        if wf.name == baseline_name:
            continue

        text = wf.read_text(encoding="utf-8", errors="replace")
        # Only flag workflows that are assert/release/CI gates
        if not re.search(r"release|publish|deploy|package|ci|\bgate\b", text, re.IGNORECASE):
            continue
        for m in re.finditer(r"continue-on-error:\s*true", text):
            line_idx = text[: m.start()].count("\n")
            findings.append({
                "check": "continue-on-error in release-gate workflows",
                "path": str(wf.relative_to(REPO_ROOT)),
                "line": line_idx + 1,
                "matched": m.group(),
            })

    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description="Assert Provider V1 invariants")
    parser.add_argument(
        "--crates-dir",
        type=Path,
        default=CRATES_DIR,
        help="Path to crates directory",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Print all findings",
    )
    args = parser.parse_args()

    crates_dir = args.crates_dir.resolve()

    all_findings = []

    # Scan Rust source files
    rust_files = sorted(crates_dir.rglob("*.rs"))
    for f in rust_files:
        try:
            findings = scan_file(f)
            all_findings.extend(findings)
        except Exception as e:
            print(f"Error scanning {f}: {e}", file=sys.stderr)

    # Check for unknown-field-tolerant provider schema
    all_findings.extend(scan_unknown_fields())

    # Scan workflow files
    all_findings.extend(scan_workflows())

    # Group by check for reporting
    from collections import defaultdict
    by_check: dict[str, list[dict]] = defaultdict(list)
    for finding in all_findings:
        by_check[finding["check"]].append(finding)

    # Report
    has_violations = False
    for check_name, items in sorted(by_check.items()):
        print(f"\n{'=' * 60}")
        print(f"CHECK: {check_name}")
        print(f"{'=' * 60}")
        prod_items = items
        if prod_items:
            print(f"  VIOLATIONS ({len(prod_items)}):")
            for item in sorted(prod_items, key=lambda x: (x["path"], x["line"])):
                print(f"    {item['path']}:{item['line']}  ({item['matched']})")
            has_violations = True
        else:
            print(f"  OK — no violations found")

    print(f"\n{'=' * 60}")
    print(f"SUMMARY")
    print(f"{'=' * 60}")
    total = len(all_findings)
    print(f"  Total production violations: {total}")
    print(f"  Checks failing: {len(by_check)}")

    if has_violations:
        print(f"\nFAIL: INVARIANT VIOLATIONS DETECTED — fix or document as allowed")
        return 1
    else:
        print(f"\nPASS: ALL INVARIANTS PASS — no production violations")
        return 0


if __name__ == "__main__":
    sys.exit(main())
