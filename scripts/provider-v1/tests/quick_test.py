"""Quick test to debug scan issue."""
import re
from pathlib import Path

crates = Path(__file__).resolve().parent.parent.parent.parent / "crates"

test_file = crates / "codegen" / "xai-grok-pager-render" / "src" / "host" / "display_refresh.rs"
print(f"File: {test_file}")
print(f"Exists: {test_file.exists()}")

text = test_file.read_text(encoding="utf-8", errors="replace")

# Look for all cfg lines
cfg_lines = []
for i, line in enumerate(text.splitlines(), 1):
    if "cfg" in line.lower() and "windows" in line.lower():
        cfg_lines.append((i, line.strip()))

print(f"Lines with cfg + windows: {len(cfg_lines)}")
for ln, l in cfg_lines[:10]:
    print(f"  {ln}: {l[:120]}")

# Now test our regex
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
            print(f"  MATCH line {i}: {stripped[:100]}")
            count += 1

print(f"Total regex matches: {count}")
