"""Tests for scan-platform-exclusions.py.

R3-RED-13: Reproduce exclusion-ledger identity break.
RED tests assert correct behavior — they FAIL pre-fix, PASS post-fix.
"""
import subprocess
import sys
import tempfile
from pathlib import Path
import importlib.util

SCRIPT = Path(__file__).resolve().parent.parent / "scan-platform-exclusions.py"


def _load_module():
    spec = importlib.util.spec_from_file_location(
        "scan_platform_exclusions",
        str(SCRIPT),
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _run_scan(src_text: str) -> list[dict]:
    """Write src to a temp .rs file inside a fake crates/ tree and run scan_file."""
    mod = _load_module()
    with tempfile.TemporaryDirectory() as tmp:
        crates = Path(tmp) / "crates"
        (crates / "foo").mkdir(parents=True)
        src = crates / "foo" / "test.rs"
        src.write_text(src_text, encoding="utf-8")

        orig = mod.CRATES
        mod.CRATES = crates
        try:
            raw = mod.scan_file(src)
        finally:
            mod.CRATES = orig
    return raw


def test_scan_detects_bare_unix():
    """R3-RED-13: bare cfg(unix) IS detected."""
    entries = _run_scan(r"""#[cfg(unix)]
fn posix_only() {}""")
    assert len(entries) >= 1, "R3-RED-13: cfg(unix) not detected"


def test_scan_detects_all_test_unix():
    """R3-RED-13: all(test, unix) IS detected."""
    entries = _run_scan(r"""#[cfg(all(test, unix))]
mod test_mod { #[test] fn integration_test() {} }""")
    assert len(entries) >= 1, "R3-RED-13: all(test, unix) not detected"


def test_scan_detects_not_target_os_windows():
    """R3-RED-13: not(target_os = windows) must be detected.

    RED: fails pre-fix because [^)]+ in cfg_pattern truncates the nested
         not(...) closing paren, so not_windows regex never matches.
    """
    entries = _run_scan(r"""#[cfg(not(target_os = "windows"))]
fn unix_only() {}""")
    cfg_exprs = {e["cfg expression"] for e in entries}
    assert any("not" in e and "target_os" in e for e in cfg_exprs), (
        f"R3-RED-13: not(target_os = windows) not detected (nested paren bug): {cfg_exprs}"
    )


def test_scan_detects_short_form_not_windows():
    """R3-RED-13: short form not(windows) must be detected.

    RED: fails pre-fix — same nested-paren truncation as target_os form.
    """
    entries = _run_scan(r"""#[cfg(not(windows))]
fn short_not_windows() {}""")
    cfg_exprs = {e["cfg expression"] for e in entries}
    assert any("not(windows)" in e for e in cfg_exprs), (
        f"R3-RED-13: not(windows) not detected (nested paren bug): {cfg_exprs}"
    )


def test_scan_detects_cfg_attr_windows_ignore():
    """R3-RED-13: cfg_attr(windows, ignore) must be detected.

    RED: fails pre-fix because windows_ignore regex expects
         target_os = "windows" rather than bare windows token.
    """
    entries = _run_scan(r"""#[cfg_attr(windows, ignore)]
#[test]
fn skipped_on_windows() {}""")
    assert len(entries) >= 1, (
        f"R3-RED-13: cfg_attr(windows, ignore) not detected: entries={len(entries)}"
    )


def test_scan_output_format():
    """Ledger output follows the expected markdown table format."""
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "ledger.md"
        out.write_text(
            "| ID | file | line | symbol/test | cfg expression | classification | owner phase | repair task | rationale evidence | status |\n"
            "|---|---|---|---|---|---|---|---|---|---|\n"
            "| WX-abc123 | src/foo.rs | 10 | test_bar | cfg(not(windows)) | | | | | OPEN |\n"
        )
        content = out.read_text(encoding="utf-8")
        assert "WX-abc123" in content
        assert "OPEN" in content


def test_wx_id_stability():
    """WX ID is stable across same file/symbol/cfg."""
    mod = _load_module()
    id1 = mod.wx_id("src/foo.rs", "mod::test_fn", "cfg(not(windows))")
    id2 = mod.wx_id("src/foo.rs", "mod::test_fn", "cfg(not(windows))")
    assert id1 == id2
    id3 = mod.wx_id("src/bar.rs", "mod::test_fn", "cfg(not(windows))")
    assert id1 != id3


def test_wx_id_stable_across_nonstructural_insertions():
    """R3-RED-13: WX ID unchanged when unrelated lines are inserted.

    Uses cfg(unix) which IS detected pre-fix.
    Inserting blank lines/comments before the fn must not change the WX ID.
    """
    base_src = r"""#[cfg(unix)]
fn target_fn() {}"""
    modified_src = r"""// unrelated comment

#[cfg(unix)]
fn target_fn() {}"""
    base_entries = _run_scan(base_src)
    modified_entries = _run_scan(modified_src)
    assert len(base_entries) == 1, "R3-RED-13: base scan should have 1 entry"
    assert len(modified_entries) == 1, "R3-RED-13: modified scan should have 1 entry"
    base_id = base_entries[0]["ID"]
    modified_id = modified_entries[0]["ID"]
    assert base_id == modified_id, (
        f"R3-RED-13: WX ID changed after non-structural insertion: "
        f"{base_id} != {modified_id}"
    )


def test_legacy_ledger_migration():
    """R3-RED-13: old ledger can be parsed and re-emitted."""
    with tempfile.TemporaryDirectory() as tmp:
        ledger = Path(tmp) / "ledger.md"
        ledger.write_text(
            "| ID | file | line | symbol/test | cfg expression | classification | owner phase | repair task | rationale evidence | status |\n"
            "|---|---|---|---|---|---|---|---|---|---|\n"
            "| WX-000001 | src/foo.rs | 10 | test_bar | cfg(not(windows)) | LEGACY | phase-0 | | | OPEN |\n"
            "| WX-000002 | src/bar.rs | 20 | integration | cfg(unix) | LEGACY | phase-0 | | | OPEN |\n",
        )
        content = ledger.read_text(encoding="utf-8")
        assert "WX-000001" in content
        assert "WX-000002" in content
        assert "LEGACY" in content
        subprocess.run(
            [sys.executable, str(SCRIPT), "--output", str(ledger)],
            capture_output=True, text=True,
        )
        content2 = ledger.read_text(encoding="utf-8")
        assert "| ID | file | line | symbol/test" in content2


if __name__ == "__main__":
    for name, fn in sorted({k: v for k, v in globals().items() if k.startswith("test_")}.items()):
        print(f"  {name}...", end=" ")
        try:
            fn()
            print("OK")
        except Exception as e:
            print(f"FAIL: {e}")
            raise
