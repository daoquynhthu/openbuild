"""Tests for baseline-delta.py."""
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "baseline-delta.py"


def _run(*args, expect_fail=False):
    result = subprocess.run(
        [sys.executable, str(SCRIPT)] + list(args),
        capture_output=True, text=True,
    )
    if expect_fail:
        assert result.returncode != 0, f"Expected failure, got:\n{result.stdout}"
    else:
        assert result.returncode == 0, f"Script failed:\n{result.stderr}"
    return result


def test_pass_when_unchanged():
    """No regressions should pass."""
    with tempfile.TemporaryDirectory() as tmp:
        b = Path(tmp) / "baseline.md"
        b.write_text("| windows exclusions | 5 |\n| ignored tests | 10 |\n")
        cur = Path(tmp) / "current"
        cur.mkdir()
        s = cur / "scan.txt"
        s.write_text("#[cfg(not(target_os = \"windows\"))]\n")
        _run("--baseline", str(b), "--current", str(cur))


def test_fail_when_increased():
    """More exclusions than baseline should fail."""
    with tempfile.TemporaryDirectory() as tmp:
        b = Path(tmp) / "baseline.md"
        b.write_text("| windows exclusions | 1 |\n| ignored tests | 10 |\n")
        cur = Path(tmp) / "current"
        cur.mkdir()
        s = cur / "scan.txt"
        s.write_text(
            "#[cfg(not(target_os = \"windows\"))]\n"
            "#[cfg(all(test, not(target_os = \"windows\")))]\n"
        )
        _run("--baseline", str(b), "--current", str(cur), expect_fail=True)


if __name__ == "__main__":
    for name, fn in sorted({k: v for k, v in globals().items() if k.startswith("test_")}.items()):
        print(f"  {name}...", end=" ")
        try:
            fn()
            print("OK")
        except Exception as e:
            print(f"FAIL: {e}")
            raise
