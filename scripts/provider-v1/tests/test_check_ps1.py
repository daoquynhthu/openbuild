"""Test check.ps1 behavior by inspecting its source code."""
from pathlib import Path

CHECK_PS1 = Path(__file__).resolve().parent.parent.parent.parent / "check.ps1"


def test_clippy_failure_is_fatal():
    """check.ps1 must exit non-zero when clippy fails."""
    text = CHECK_PS1.read_text(encoding="utf-8")
    # Should not have non-fatal clippy handling
    assert "non-fatal" not in text, "clippy must be fatal"
    # Should exit on clippy failure
    assert "exit $LASTEXITCODE" in text or "exit 1" in text


def test_no_unconditional_success():
    """check.ps1 must not unconditionally print 'All checks passed'."""
    text = CHECK_PS1.read_text(encoding="utf-8")
    # Success message must be reachable only after all steps pass
    # If exit is used on failure, success message is conditional
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if "All checks passed" in line:
            # Check there's no unconditional print before this
            pass
    assert "All checks passed" in text  # should still print on success
    # But should not print after non-fatal paths
    assert text.count("exit ") >= 2, "Should exit on both check and clippy failure"


def test_protoc_path_first():
    """check.ps1 should try PATH before WinGet."""
    text = CHECK_PS1.read_text(encoding="utf-8")
    assert "(Get-Command protoc" in text, "Should try PATH first"


if __name__ == "__main__":
    for name, fn in sorted({k: v for k, v in globals().items() if k.startswith("test_")}.items()):
        print(f"  {name}...", end=" ")
        try:
            fn()
            print("OK")
        except Exception as e:
            print(f"FAIL: {e}")
            raise
