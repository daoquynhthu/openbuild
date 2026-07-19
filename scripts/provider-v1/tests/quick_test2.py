"""Debug regex matching."""
import re
from pathlib import Path

crates = Path(__file__).resolve().parent.parent.parent.parent / "crates"
test_file = crates / "codegen" / "xai-grok-pager-render" / "src" / "host" / "display_refresh.rs"

text = test_file.read_text(encoding="utf-8", errors="replace")
lines = text.splitlines()

# Test each regex step separately
cfg_pat = re.compile(r'#\[\s*cfg\(')
nw_pat = re.compile(r'not\(\s*target_os\s*=\s*"windows"\s*\)')

for i, line in enumerate(lines, 1):
    stripped = line.strip()
    if "cfg" in stripped and "windows" in stripped:
        print(f"  {i}: {repr(stripped)}")
        # Match whole thing
        full_pat = re.compile(r'#\[\s*cfg\(([^)]+)\)')
        m = full_pat.search(stripped)
        if m:
            print(f"    full match: {m.group(0)}")
            print(f"    inner: {m.group(1)}")
            if nw_pat.search(m.group(1)):
                print("    NW MATCH!")
            else:
                print("    No NW match")
        else:
            print("    No full match")
            
            # Try simpler
            simple = re.compile(r'not\(target_os')
            if simple.search(stripped):
                print("    but simple not(target_os) matches!")
