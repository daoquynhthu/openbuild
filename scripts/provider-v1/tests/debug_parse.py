"""Debug the parse_table function."""
import sys
sys.path.insert(0, r"D:\grok_build\scripts\provider-v1")
from materialize_work_packets import parse_table

path = r"D:\grok_build\docs\provider-adapter-v1\execution-v2\windows-test-exclusion-ledger.md"
lines = open(path, encoding="utf-8").read().splitlines()

print(f"Total lines: {len(lines)}")
print("Looking for 'ID | file | line'...")

results = parse_table(lines, "ID | file | line")
print(f"Parse results: {len(results)} rows")

if len(results) == 0:
    # Try to debug
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("|") and "ID" in stripped and "file" in stripped:
            print(f"  Found potential header at line {i}: {stripped[:80]}...")
            # Try manual parse
            cols = [c.strip() for c in stripped.strip("|").split("|")]
            cols = [c for c in cols if c]
            print(f"  Columns ({len(cols)}): {cols[:6]}...")
            
            # Check next non-empty, non-separator line
            for j in range(i+1, min(i+5, len(lines))):
                s2 = lines[j].strip()
                print(f"  Next line {j}: {s2[:60]}...")
                if s2.startswith("|") and "---" not in s2:
                    cells = [c.strip() for c in s2.strip("|").split("|")]
                    cells = [c for c in cells if c]
                    print(f"  Cells count: {len(cells)} (need {len(cols)})")
                    print(f"  Cells: {cells[:4]}...")
