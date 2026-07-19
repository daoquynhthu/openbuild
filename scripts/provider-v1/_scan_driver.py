"""Simple driver that calls scan logic directly."""
import sys
import os
from pathlib import Path

# Add parent to path
sys.path.insert(0, str(Path(__file__).resolve().parent))

# Import with correct name
import importlib.util
spec = importlib.util.spec_from_file_location(
    "excl_scan",
    os.path.join(os.path.dirname(__file__), "scan-platform-exclusions.py"),
)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

# Now run
out_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("wx-ledger.md")
mod.main_custom(out_path)
