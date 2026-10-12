#!/usr/bin/env python3
"""Archive integrity regression only; does not rerun runtime scenarios."""
from pathlib import Path
import hashlib,json,subprocess
D=Path(__file__).resolve().parent
BASE=D.parent
def h(p):return hashlib.sha256(p.read_bytes()).hexdigest()
checks={}
for name in ("g07b02g2","g07b02h","g07b02i"):
 root=BASE/name
 receipt=json.loads((root/"receipt.json").read_text())
 key="trace_archives_sha256" if name=="g07b02g2" else (
     "evidence_sha256" if name=="g07b02h" else "archive_sha256")
 recorded=receipt[key]
 paths=[]
 for file,expected in recorded.items():
    path=root/"traces"/file if name!="g07b02h" else root/file
    assert path.is_file() and h(path)==expected,(name,file)
    if path.suffix==".zst":subprocess.run(["zstd","-qt",str(path)],check=True)
    paths.append(str(path.relative_to(BASE)))
 checks[name]={"archive_bytes_verified":len(paths),"files":paths,
              "receipt_sha256":h(root/"receipt.json")}
g2=json.loads((BASE/"g07b02g2/receipt.json").read_text())
assert g2["experiments"]["historical"]["gpui_callbacks"]==135
assert g2["experiments"]["replay"]["gpui_callbacks"]==136
h2=json.loads((BASE/"g07b02h/receipt.json").read_text())
assert h2["controls"]["bounded_horizontal_confine_two_outputs"]=="2/2_PASS"
i2=json.loads((BASE/"g07b02i/audit.json").read_text())
assert i2["same_output_successes"]==2 and i2["cross_output_successes"]==0
report={"schema":"g07b02j-prior-evidence-integrity-v1",
 "status":"ARCHIVE_INTEGRITY_PASS_ONLY_NOT_LIVE_E2E",
 "checks":checks,
 "historical_scopes":{"g07b02g2":"271/271 callbacks archived",
   "g07b02h":"2/2 horizontal confinements archived",
   "g07b02i":"2/2 same-output DnD, 0/2 cross-output archived"}}
(D/"offline_regressions.json").write_text(json.dumps(report,indent=2)+"\n")
print("OFFLINE_REGRESSION_INTEGRITY_PASS",
      {k:v["archive_bytes_verified"] for k,v in checks.items()})
