"""Debug the parse_table function using importlib."""
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "mwp",
    str(Path(__file__).resolve().parent.parent / "materialize-work-packets.py"),
)
mwp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mwp)
parse_table = mwp.parse_table

path = Path(__file__).resolve().parent.parent.parent.parent / "docs" / "provider-adapter-v1" / "execution-v2" / "windows-test-exclusion-ledger.md"
lines = path.read_text(encoding="utf-8").splitlines()

print(f"Total lines: {len(lines)}")
results = parse_table(lines, "ID | file | line")
print(f"Parse results: {len(results)} rows")

if len(results) == 0:
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("|") and "ID" in stripped and "file" in stripped:
            print(f"  Header at line {i}: {stripped[:80]}...")
            cols = [c.strip() for c in stripped.strip("|").split("|")]
            cols = [c for c in cols if c]
            print(f"  Col count: {len(cols)}")
            for j in range(i+1, min(i+5, len(lines))):
                s2 = lines[j].strip()
                if s2.startswith("|") and "---" not in s2:
                    cells = [c.strip() for c in s2.strip("|").split("|")]
                    cells = [c for c in cells if c]
                    print(f"  Data at {j}: cells={len(cells)}, cells[:4]={cells[:4]}")
