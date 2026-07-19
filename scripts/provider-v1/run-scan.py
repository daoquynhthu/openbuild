"""Entry point for scan-platform-exclusions."""
import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "scan_excl",
    Path(__file__).resolve().parent / "scan-platform-exclusions.py",
)
mod = importlib.util.module_from_spec(spec)
sys.modules["scan_excl"] = mod
spec.loader.exec_module(mod)

out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("windows-test-exclusion-ledger.md")
mod.args_output = out  # inject
# Re-assign sys.argv for argparse inside the module
sys.argv = ["scan", "--output", str(out)]
mod.main()
