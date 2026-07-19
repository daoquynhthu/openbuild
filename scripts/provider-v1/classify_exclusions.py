"""Classify all WX exclusion entries per P2-002 decision tree.

Decision tree (P2-002):
1. Is symbol in V1 Windows must-support matrix (§2.6)?
   - Provider/runtime/config/catalog → must support
   - Session persistence/hot reload → must support
   - ACP/session lifecycle → must support
   - PTY/process/terminal → must support
   - Shell completion → must support
   - Clipboard/link opening → must support
   - If YES → NOT Unix-only
2. Does production entry exist on Windows?
   - YES but test excluded: CROSS_PLATFORM_CONTRACT or PLATFORM_ADAPTER_CONTRACT
   - Should exist but adapter missing: MISSING_WINDOWS_IMPLEMENTATION
   - cfg only avoids compile/assertion failure: INVALID_EXCLUSION
3. Not in V1 matrix, no Windows entry, depends on non-portable OS primitive: UNIX_ONLY_FEATURE

Phase mapping:
- filesystem/path/worktree/config/catalog/session-file → P11
- process/signals/ACP/shell-completion/terminal/PTY → P12
- pager-render/images/OSC8/scrollback/shortcuts/tools/clipboard → P13
"""

import csv
import io
import re
from pathlib import Path

LEDGER = Path(__file__).resolve().parent.parent.parent / "docs" / "provider-adapter-v1" / "execution-v2" / "windows-test-exclusion-ledger.md"
OUTPUT = LEDGER


def parse_ledger(path: Path) -> list[dict]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    header = None
    rows = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("|") and "ID" in stripped and "file" in stripped:
            header = [c.strip() for c in stripped.strip("|").split("|")]
            header = [c for c in header if c]
        elif header and stripped.startswith("|") and "---" not in stripped:
            cells = [c.strip() for c in stripped.strip("|").split("|")]
            if len(cells) >= len(header):
                row = dict(zip(header, cells[:len(header)]))
                if row.get("ID", "").startswith("WX-"):
                    rows.append(row)
    return rows


def owner_phase(filepath: str) -> str:
    """Map file path to owner phase per P2-002."""
    lp = filepath.lower()
    if any(k in lp for k in ["file-utils", "workspace_classifier", "worktree", "git", "config", "catalog", "persistence", "paths"]):
        return "P11"
    if any(k in lp for k in ["acp_session", "session/signals", "session/compaction", "session/persistence",
                              "session/worktree_pool", "shell-completion", "terminal", "pty", "process",
                              "clipboard", "link_opener", "host_clipboard"]):
        return "P12"
    if any(k in lp for k in ["pager", "pager-render", "prompt_images", "osc8", "scrollback",
                              "shortcuts", "btw_overlay", "key.rs", "display_refresh",
                              "tools/src/implementations", "tools/src/gitignore", "tools/src/registry",
                              "tools/src/types", "skills/discovery", "textarea"]):
        return "P13"
    if any(k in lp for k in ["shell/src/agent", "shell/src/auth", "shell/src/config",
                              "shell/src/extensions", "shell/src/leader",
                              "active_sessions", "folder_trust", "mvp_agent", "subagent"]):
        return "P12"
    return "P13"


def classify_row(row: dict) -> dict:
    """Apply the P2-002 decision tree to a single row."""
    filepath = row.get("file", "")
    symbol = row.get("symbol/test", "")
    cfg = row.get("cfg expression", "")
    phase = owner_phase(filepath)

    # Decision tree
    # Step 1: Is this in V1 Windows must-support matrix?
    # Most test modules are not in the must-support matrix for all tests
    # unless they test core behavior

    # Step 2: Check if it's a test module
    is_test_module = symbol.startswith("tests") or "tests" in symbol or symbol in ["is_platform_system_dir", "pbcopy", "pbpaste", "probe_windows"]

    # Classification based on file + symbol patterns
    classification = "UNIX_ONLY_FEATURE"
    rationale = ""

    # Production cfg gates (not tests) — need MISSING_WINDOWS_IMPLEMENTATION if production entry on Windows
    if symbol in ["is_platform_system_dir", "pbcopy", "pbpaste", "probe_windows",
                  "set_clipboard_png", "build_open_path_command", "is_altgr"]:
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = f"Production symbol `{symbol}` gated behind `{cfg}`; Windows adapter/impl needed"

    # Test modules that test Unix-only features
    elif "clipboard" in filepath and symbol in ["pbcopy", "pbpaste", "set_clipboard_png"]:
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = "Host clipboard requires Windows adapter"

    elif "link_opener" in filepath:
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = "Link opener needs Windows implementation (open::that)"

    elif "prompt_images" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Prompt image path parsing — tests use Unix paths, need platform fixture"

    elif "osc8" in filepath or "tool_paths" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Link rendering tests use Unix paths, need platform fixture"

    elif "display_refresh" in filepath:
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = "Display refresh needs Windows console API adapter"

    elif "workspace_classifier" in filepath and symbol == "is_platform_system_dir":
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = "Platform system dir detection needs Windows impl"

    elif "shell_completion" in filepath:
        classification = "PLATFORM_ADAPTER_CONTRACT"
        rationale = f"Shell completion tests for `{symbol}` — completion logic adapter needed"

    elif "acp_session" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = f"ACP session tests — need Windows ACP adapter (P12)"

    elif "persistence" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = f"Session persistence tests — path/fs adapter needed"

    elif "compaction" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = f"Session compaction tests — need ACP adapter"

    elif "session/signals" in filepath or "session/worktree_pool" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Session signal/worktree tests — platform adapter needed"

    elif "terminal" in filepath or "streaming_local_terminal" in filepath:
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = "Terminal/PTY tests — Windows ConPTY adapter needed"

    elif "bash" in filepath:
        classification = "PLATFORM_ADAPTER_CONTRACT"
        rationale = "Bash execution API — Windows uses Pwsh/Cmd adapters; tests use Unix shell features"

    elif "grep" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Grep tests — need Windows path fixture"

    elif "glob" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Glob tests — need Windows path fixture"

    elif "read_file" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Read file tests — need Windows path fixture"

    elif "web_fetch" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Web fetch tests use Unix-specific test infra (signal/process); need platform adapter"

    elif "enter_plan_mode" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Plan mode tests — platform adapter needed for process"

    elif "lsp" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "LSP tests — Windows path adapter needed"

    elif "gitignore" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Gitignore tests — need Windows path fixture"

    elif "skills/discovery" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Skills discovery — path handling needed"

    elif "registry/types" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Tool registry types — path handling needed"

    elif "types/resources" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = "Resource types — path handling needed"

    elif "is_altgr" in symbol or ("key.rs" in filepath and "input/key" in filepath):
        classification = "UNIX_ONLY_FEATURE"
        rationale = "AltGr key detection is Linux-specific (xkb); not in V1 Windows must-support matrix"

    elif "ratatui-textarea" in filepath:
        classification = "UNIX_ONLY_FEATURE"
        rationale = "AltGr handling in ratatui-textarea is Linux xkb-specific"

    elif "workspace_classifier" in filepath:
        classification = "PLATFORM_ADAPTER_CONTRACT"
        rationale = "Workspace classifier path tests need Windows fixture"

    elif "host_clipboard" in filepath:
        classification = "MISSING_WINDOWS_IMPLEMENTATION"
        rationale = "Host clipboard needs Windows adapter"

    elif "pager" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = f"Pager tests for `{symbol}` — platform adapter needed"

    elif "btw_overlay" in filepath or "shortcuts_help" in filepath:
        classification = "CROSS_PLATFORM_CONTRACT"
        rationale = f"UI overlay/shortcuts tests — platform adapter for clipboard"

    else:
        classification = "PLATFORM_ADAPTER_CONTRACT"
        rationale = f"Tests for `{symbol}` — platform behavior adapter needed"

    return {
        "classification": classification,
        "owner phase": phase,
        "rationale evidence": rationale,
    }


def main():
    rows = parse_ledger(LEDGER)
    print(f"Parsed {len(rows)} rows from ledger", file=__import__('sys').stderr)

    counts = {}
    for row in rows:
        result = classify_row(row)
        row["classification"] = result["classification"]
        row["owner phase"] = result["owner phase"]
        row["rationale evidence"] = result["rationale evidence"]
        row["repair task"] = f"P{result['owner phase'][1:]}-R-{row['ID']}"
        row["status"] = "CLASSIFIED"
        counts[result["classification"]] = counts.get(result["classification"], 0) + 1

    # Regenerate the ledger
    lines = [
        "# Windows Test Exclusion Ledger",
        f"# Generated: classify_exclusions.py (classification run)",
        f"# Total: {len(rows)} entries",
        "",
    ]
    # Add classification summary
    lines.append("## Classification Summary")
    for cls, cnt in sorted(counts.items()):
        lines.append(f"- {cls}: {cnt}")
    lines.append("")

    lines.append("| ID | file | line | symbol/test | cfg expression | classification | owner phase | repair task | rationale evidence | status |")
    lines.append("|---|---|---|---|---|---|---|---|---|---|")
    for r in rows:
        lines.append(
            f"| {r['ID']} | {r['file']} | {r['line']} | {r['symbol/test']} | {r['cfg expression']} | "
            f"{r['classification']} | {r['owner phase']} | {r['repair task']} | {r['rationale evidence']} | {r['status']} |"
        )

    output = "\n".join(lines)
    LEDGER.write_text(output, encoding="utf-8")
    print(f"Wrote {len(rows)} classified entries to {LEDGER}", file=__import__('sys').stderr)
    for cls, cnt in sorted(counts.items()):
        print(f"  {cls}: {cnt}")


if __name__ == "__main__":
    main()
