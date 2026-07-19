"""Debug the scan script on a specific file."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from scan_platform_exclusions import scan_file

CRATES = Path(__file__).resolve().parent.parent.parent.parent / "crates"
test_file = CRATES / "codegen" / "xai-file-utils" / "src" / "workspace_classifier.rs"

print(f"Scanning {test_file}")
print(f"File exists: {test_file.exists()}")

results = scan_file(test_file)
print(f"Found {len(results)} exclusions:")
for r in results:
    print(f"  {r['ID']}:{r['line']} {r['symbol/test']} cfg={r['cfg expression']}")

if not results:
    # Try reading the file directly
    text = test_file.read_text(encoding="utf-8", errors="replace")
    import re
    cfg_pat = re.compile(
        r'#\[\s*cfg(?:\((?P<inner>[^)]+)\)|_attr\((?P<attr_inner>[^)]+)\))'
    )
    nw_pat = re.compile(r'not\(\s*target_os\s*=\s*"windows"\s*\)')
    for i, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        for m in cfg_pat.finditer(stripped):
            inner = m.group("inner") or m.group("attr_inner") or ""
            if nw_pat.search(inner):
                print(f"  Line {i}: MATCHED: {stripped[:80]}")
