#!/usr/bin/env python3
"""Independent evidence validation for bounded G07B-02J compositor experiments."""
from pathlib import Path
import hashlib, io, json, re, subprocess, tarfile
D=Path(__file__).resolve().parent
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def bundle(name):
    raw=subprocess.run(["zstd","-dc",str(D/"traces"/name)],capture_output=True,check=True).stdout
    with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
        return {m.name:tar.extractfile(m).read().decode(errors="replace")
                for m in tar if m.isfile()}
t=bundle("parent-wayland-dnd-cross.tar.zst")
root=t["nested-parent-wayland.log"]
pattern=re.compile(r"^\[([^\]]+)\] \{[^}]*\} wl_pointer#11\.(enter|leave|motion|button)\((.*)\)$",re.M)
events=[{"time":m[1],"kind":m[2],"data":m[3]} for m in pattern.finditer(root)]
assert len(events)>40,("pointer trace too short",len(events))
press=[(i,x) for i,x in enumerate(events) if x["kind"]=="button" and re.search(r"^98465, \d+, 272, 1$",x["data"])]
release=[(i,x) for i,x in enumerate(events) if x["kind"]=="button" and re.search(r"^98466, \d+, 272, 0$",x["data"])]
assert len(press)==len(release)==1,(press,release)
start,pressed=press[0]; stop,released=release[0]
assert start<stop
before=[x for x in events[:start] if x["kind"]=="enter"]
during=events[start+1:stop]
after=events[stop+1:]
assert before and "wl_surface#10" in before[-1]["data"]
assert not any(x["kind"] in ("enter","leave") for x in during)
motion=[x for x in during if x["kind"]=="motion"]
values=[float(x["data"].split(", ")[1]) for x in motion]
assert len(values)>=10 and max(values)>2000,(len(values),max(values))
after_leave=next(x for x in after if x["kind"]=="leave")
after_enter=next(x for x in after if x["kind"]=="enter")
assert "wl_surface#10" in after_leave["data"]
assert "wl_surface#16" in after_enter["data"]
client_source=t["dnd-cross-source.log"];client_target=t["dnd-cross-target.log"]
assert "DND_SOURCE_DRAG_STARTED" in client_source and "DND_TARGET_DROP" in client_source
assert "DND_DATA_OFFER" not in client_target and "DND_TARGET_DROP" not in client_target
receipt=json.loads(t["dnd-cross-receipt.json"])
assert receipt["mode"]=="cross" and not receipt["received"] and not receipt["source_finished"]
logs=t["nested-stdout.log"]
assert "G07B02G1_WARP output=WAYLAND-1 normalized_x=1" in logs
locked=bundle("lock-matched-4cases.tar.zst")
matrix=[]
for size in ("short","long"):
    for recycle in (False,True):
        suffix=f"one-{size}-{'recycle' if recycle else 'no-recycle'}"
        r=json.loads(locked[f"receipt-{suffix}.json"])
        c=locked[f"client-{suffix}.log"]
        assert r["active"] and r["inner_before"]==r["inner_after"]
        assert r["outer_after"][0]-r["outer_before"][0]>(50 if size=="short" else 500)
        assert c.count("G2_CONSTRAINT_ACTIVE locked")==(2 if recycle else 1)
        if recycle: assert "I_LOCK_DESTROY_RECREATE_REQUESTED" in c
        matrix.append({"size":size,"recycle":recycle,
                       "locked_activation_events":c.count("G2_CONSTRAINT_ACTIVE locked"),
                       "inner_before":r["inner_before"],"inner_after":r["inner_after"],
                       "outer_x_delta":r["outer_after"][0]-r["outer_before"][0],
                       "observed_stable":True})
historic=bundle("relative-lock.tar.zst") if (D/"traces/relative-lock.tar.zst").exists() else None
original_dir=D.parent/"g07b02i"/"traces"/"relative-lock.tar.zst"
raw=subprocess.run(["zstd","-dc",str(original_dir)],capture_output=True,check=True).stdout
with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
    historic={m.name:tar.extractfile(m).read().decode() for m in tar if m.isfile()}
old=json.loads(historic["relative-WAYLAND-1-receipt.json"])
assert old["inner_before"]!=old["inner_after"]
weston=subprocess.run(["zstd","-dc",str(D/"traces/weston-15-wayland-dual-output.log.zst")],
                       capture_output=True,check=True,text=True).stdout
assert "Creating 1250x800 wayland output at (0, 0)" in weston
assert "Creating 1250x800 wayland output at (1250, 0)" in weston
assert "Output 'wayland0' enabled" in weston and "Output 'wayland1' enabled" in weston
out={
 "schema":"g07b02j-independent-causal-audit-v1",
 "cross_dnd":{
  "press":pressed,"release":released,
  "original_parent_surface":before[-1]["data"],
  "movement_events_during_grab":len(motion),
  "max_source_surface_local_x":max(values),
  "parent_enter_leave_during_held_button":0,
  "post_release_leave":after_leave,"post_release_enter":after_enter,
  "aquamarine_warp_clamped_to_origin_output":True,
  "client_target_offer_received":False,
  "transport_gate":"FAIL_EXPLAINED_PARENT_IMPLICIT_GRAB"},
 "lock_recycle":{"new_matched_controls":matrix,
  "historic_WAYLAND_1_failure":{"inner_before":old["inner_before"],
                                 "inner_after":old["inner_after"]},
  "conclusion":"INTERMITTENT_OR_SEAT_STATE_DEPENDENT_NOT_REPRODUCED_IN_J"},
 "weston_independent_backend":{"version":"15.0.1","two_wayland_outputs":True,
                                "drag_injection_and_transfer":"NOT_RUN"},
 "archive_sha256":{p.name:sha(p) for p in sorted((D/"traces").glob("*.zst"))},
 "G07B_production_promotion":"BLOCKED",
 "E4_human_review":"PENDING"}
(D/"audit.json").write_text(json.dumps(out,indent=2,ensure_ascii=False)+"\n")
print("J_INDEPENDENT_AUDIT_PASS",len(motion),round(max(values),2),
      "parent_focus_switches_during_drag",0,"lock_stable",len(matrix),"/4")
print("AUDIT_SHA",sha(D/"audit.json"))
