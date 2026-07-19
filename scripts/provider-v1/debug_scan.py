"""Debug the scan script on a specific file."""
import importlib.util
import re
import sys
from pathlib import Path

# Import the module (filename has hyphens)
spec = importlib.util.spec_from_file_location(
    "scan_platform_exclusions",
    Path(__file__).resolve().parent / "scan-platform-exclusions.py",
)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
scan_file = mod.scan_file

CRATES = Path(__file__).resolve().parent.parent.parent / "crates"
test_file = CRATES / "codegen" / "xai-file-utils" / "src" / "workspace_classifier.rs"

print(f"Scanning {test_file}")
print(f"File exists: {test_file.exists()}")

results = scan_file(test_file)
print(f"Found {len(results)} exclusions:")
for r in results:
    print(f"  {r['ID']}:{r['line']} {r['symbol/test']} cfg={r['cfg expression']}")

if not results:
    print("\nTrying raw regex...")
    text = test_file.read_text(encoding="utf-8", errors="replace")
    cfg_pat = re.compile(
        r'#\[\s*cfg(?:\((?P<inner>[^)]+)\)|_attr\((?P<attr_inner>[^)]+)\))'
    )
    nw_pat = re.compile(r'not\(\s*target_os\s*=\s*"windows"\s*\)')
    count = 0
    for i, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        for m in cfg_pat.finditer(stripped):
            inner = m.group("inner") or m.group("attr_inner") or ""
            if nw_pat.search(inner):
                print(f"  Line {i}: MATCHED: {stripped[:80]}")
                count += 1
    print(f"Total raw matches: {count}")
