#!/usr/bin/env python3
"""Audit of independent native Wayland DnD and pointer-lock observations."""
from pathlib import Path
import subprocess,tarfile,io,json,hashlib
D=Path(__file__).resolve().parent
payload="G07B02I-WAYLAND-DND-20261012-8D8F63A1"
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def files(label):
    raw=subprocess.run(["zstd","-dc",str(D/"traces"/label)],capture_output=True,check=True).stdout
    with tarfile.open(fileobj=io.BytesIO(raw),mode="r:") as t:
        return {m.name:t.extractfile(m).read().decode() for m in t if m.isfile()}
records=[]
for name in ("same","same2","cross","reverse"):
    a=files("dnd-"+name+".tar.zst")
    src=a["dnd-"+name+"-source.log"]
    dst=a["dnd-"+name+"-target.log"]
    result=json.loads(a["dnd-"+name+"-receipt.json"])
    should=name in ("same","same2")
    delivered=("DND_TARGET_RECEIVED bytes=37 valid=1 content="+payload in dst)
    finished=("DND_SOURCE_FINISHED" in src)
    assert "DND_SOURCE_DRAG_STARTED" in src
    assert "DND_READY target" in dst and "DND_MAPPED" in dst
    assert delivered==finished==should,(name,delivered,finished)
    assert result["received"]==result["source_finished"]==should
    if should:
        assert "DND_TARGET_DROP" in dst and "DND_SOURCE_SEND bytes=37 valid=1" in src
    else:
        assert "DND_TARGET_DROP" in src and "DND_DATA_OFFER" not in dst
    records.append({"case":name,"expected_local":should,"delivered":delivered,
        "target_offer":"DND_DATA_OFFER" in dst,
        "source_cursor":result["source_cursor"],"target_cursor":result["target_cursor"]})
a=files("relative-lock.tar.zst")
locks=[]
for label in ("WAYLAND-1","WAYLAND-2"):
    client=a["relative-"+label+"-client.log"]
    rec=json.loads(a["relative-"+label+"-receipt.json"])
    assert "I_RELATIVE_GLOBAL available=1 bound=1" in client
    assert client.count("G2_CONSTRAINT_ACTIVE locked")==2
    assert "I_LOCK_DESTROY_RECREATE_REQUESTED" in client
    assert "I_RELATIVE_MOTION" not in client
    before=rec["inner_before"];after=rec["inner_after"]
    stationary=before==after
    if label=="WAYLAND-1":
        assert not stationary and after[0]>=1250
        assert "(1250.0," in a["recycle-lock-one-repeat.log"]
    else:assert stationary
    locks.append({"output":label,"activated_twice":True,"stationary":stationary,
        "relative_events":0,"inner_before":before,"inner_after":after})
hashes={p.name:sha(p) for p in sorted((D/"traces").glob("*.zst"))}
assert len(hashes)==6
out={"schema":"g07b02i-native-wayland-audit-v1",
    "dnd":records,"same_output_successes":2,"same_output_attempts":2,
    "cross_output_successes":0,"cross_output_attempts":2,
    "lock_recycle":locks,
    "relative_protocol":"available_and_bound_but_zero_events_observed",
    "relative_motion_status":"NOT_QUALIFIED",
    "lock_continuity":"FAIL_BOUNDED_ON_WAYLAND-1",
    "E4":"PENDING_HUMAN","production":"BLOCKED",
    "archives_sha256":hashes,
    "limits":["Nested backend failure is not proof of physical DRM backend failure",
      "Synthetic host movement does not prove native relative-motion delivery",
      "No external perceptual E4 validation or production promotion"]}
(D/"audit.json").write_text(json.dumps(out,indent=2,ensure_ascii=False)+"\n")
print("INDEPENDENT_AUDIT_PASS",[(x["case"],x["delivered"]) for x in records],locks)
print("AUDIT_SHA256",sha(D/"audit.json"))
