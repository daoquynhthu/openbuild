"""Find the new WX entries not in the ledger."""
import sys
sys.path.insert(0, r"D:\grok_build\scripts\provider-v1")
from pathlib import Path
from wx_scanner import scan_all

CRATES = Path(__file__).resolve().parent.parent.parent.parent / "crates"
entries = scan_all(CRATES)

fresh = {r["ID"]: r for r in entries}

text = Path(r"D:\grok_build\docs\provider-adapter-v1\execution-v2\windows-test-exclusion-ledger.md").read_text(encoding="utf-8")
import re
ledger_ids = set(re.findall(r"WX-[a-f0-9]{12}", text))

for wid in sorted(fresh.keys() - ledger_ids)[:5]:
    r = fresh[wid]
    print(f"{wid}: {r['file']}:{r['line']} {r['symbol/test']} cfg={r['cfg expression']}")
