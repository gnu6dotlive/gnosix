#!/usr/bin/env python3
"""G07B-02G2 independent event-transport auditor. Input: archived G2/G1 logs."""
from pathlib import Path
from collections import Counter
import re, json, hashlib, statistics, os
ROOT=Path(os.getenv("G07B02G2_TRACES","/mnt/workbench/capsule-donor-01/g07b02g2-private"))
OUT=Path(__file__).resolve().parent
NAMES=("g07b02b-g2-output1-frame","g07b02b-g2-output1-timer",
       "g07b02b-g2-output2-frame","g07b02b-g2-output2-timer")
COMPOSITOR=Path("/mnt/workbench/capsule-donor-01/g07b02g2-private/replay-compositor.log").read_text(errors="replace")
B_RE=re.compile(r"G07B02E_BUTTON epoch_ns=(\d+) state=([01]) pointer_focus=(\d+) client_available=(true|false) dnd=(true|false)")
BUTTONS=[{"epoch":int(m[1]),"state":int(m[2]),"focus":m[3],
          "resource":m[4]=="true","dnd":m[5]=="true"} for m in B_RE.finditer(COMPOSITOR)]
W_RE=re.compile(r"wl_pointer#\d+\.button\((\d+), (\d+), 272, ([01])\)")
A_RE=re.compile(r"G07B_(OVERLAY|WITNESS)_CLICK epoch_ns=(\d+)")
# Calibration anchor: known injection #4, which arrived at the overlay after the old 12ms window.
ANCHOR_EPOCH=1791794236385019994
ANCHOR_WL_MS=146289562
OFFSET_MS=ANCHOR_EPOCH/1e6-ANCHOR_WL_MS
MAX_TIMESTAMP_ERROR_MS=2.5
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def client_logs(path,kind):
    proto=(path/(kind+".stderr.log")).read_text(errors="replace")
    app=(path/(kind+".stdout.log")).read_text(errors="replace")
    wire=[{"serial":int(m[1]),"ms":int(m[2]),"state":int(m[3]),"receiver":kind}
          for m in W_RE.finditer(proto)]
    callbacks=[int(m[2]) for m in A_RE.finditer(app) if m[1].lower()==kind]
    wire_presses=[e for e in wire if e["state"]==1]
    wire_releases=[e for e in wire if e["state"]==0]
    assert len(wire_presses)==len(wire_releases)==len(callbacks),(
        kind,len(wire_presses),len(wire_releases),len(callbacks))
    for i,event in enumerate(wire_presses):
        event["callback_epoch"]=callbacks[i]
        assert event["callback_epoch"]>=0
    return wire,wire_presses
def audit(name):
    path=ROOT/name
    observed=json.loads((path/"observations.json").read_text())
    clients={kind:client_logs(path,kind) for kind in ("overlay","witness")}
    press_wire=[e for pair in clients.values() for e in pair[1]]
    rows=[]
    used=set()
    for attempt in observed["attempts"]:
        lo,hi=attempt["inject_epoch_ns"]
        presses=[b for b in BUTTONS if b["state"]==1 and lo-4_000_000<=b["epoch"]<=hi+4_000_000]
        releases=[b for b in BUTTONS if b["state"]==0 and lo-4_000_000<=b["epoch"]<=hi+8_000_000]
        assert len(presses)==len(releases)==1,(name,attempt["index"],presses,releases)
        press=presses[0]
        assert press["resource"] and not press["dnd"],(name,attempt["index"],press)
        candidates=sorted(press_wire,key=lambda w:abs(w["ms"]+OFFSET_MS-press["epoch"]/1e6))
        match=candidates[0]
        error_ms=match["ms"]+OFFSET_MS-press["epoch"]/1e6
        assert abs(error_ms)<=MAX_TIMESTAMP_ERROR_MS,(name,attempt["index"],error_ms)
        token=(match["receiver"],match["serial"])
        assert token not in used,(name,token)
        used.add(token)
        same_wire=clients[match["receiver"]][0]
        own_presses=[w for w in same_wire if w["state"]==1]
        own_releases=[w for w in same_wire if w["state"]==0]
        idx=next(i for i,w in enumerate(own_presses) if w["serial"]==match["serial"])
        assert all(w["state"]==(1 if k%2==0 else 0) for k,w in enumerate(same_wire)),(name,match["receiver"],"nonalternating")
        release_event=own_releases[idx]
        assert release_event["serial"]>match["serial"] and release_event["ms"]>=match["ms"]
        assert idx==len([w for w in own_releases if w["serial"]<release_event["serial"]])
        release_error_ms=release_event["ms"]+OFFSET_MS-releases[0]["epoch"]/1e6
        release_attributed=abs(release_error_ms)<=MAX_TIMESTAMP_ERROR_MS
        delay_ms=(match["callback_epoch"]-press["epoch"])/1e6
        assert 0<=delay_ms<1500,(name,attempt["index"],delay_ms)
        rows.append({"index":attempt["index"],"press_ns":press["epoch"],
            "release_ns":releases[0]["epoch"],"focused_surface":press["focus"],
            "wl_client":match["receiver"],"wl_press_serial":match["serial"],
            "wl_press_ms":match["ms"],"wl_release_ms":release_event["ms"],
            "wl_epoch_difference_ms":round(error_ms,4),
            "release_attributed":True,"release_epoch_lag_ms":round(-release_error_ms,3),
            "app_callback_ns":match["callback_epoch"],
            "callback_delay_ms":round(delay_ms,4),
            "old_12ms_window_would_exclude":match["callback_epoch"]>hi+12_000_000})
    assert len(rows)==observed["clicks_injected"],(name,len(rows))
    assert len(used)==len(press_wire)==observed["clicks_injected"],(name,len(used),len(press_wire))
    return {"name":name,"injections":len(rows),
        "wire_presses":len(press_wire),
        "callback_count":sum(len(x[1]) for x in clients.values()),
        "max_callback_delay_ms":max(x["callback_delay_ms"] for x in rows),
        "median_callback_delay_ms":round(statistics.median(x["callback_delay_ms"] for x in rows),3),
        "late_callbacks_old_oracle":sum(x["old_12ms_window_would_exclude"] for x in rows),
        "traces_sha256":{kind+"."+ext+".log":digest(path/(kind+"."+ext+".log"))
                          for kind in ("overlay","witness") for ext in ("stdout","stderr")},
        "rows":rows}
def main():
    results=[audit(name) for name in NAMES]
    total=sum(x["injections"] for x in results)
    assert total>=120 and all(x["injections"]==x["wire_presses"]==x["callback_count"] for x in results)
    hit=next(x for x in results[0]["rows"] if x["index"]==4)
    document={"schema":"g07b02g2-transport-v1",
        "premise":"Wayland button timestamps identify a press independent of delayed client dispatch",
        "wayland_offset_ms":OFFSET_MS,"tolerance_ms":MAX_TIMESTAMP_ERROR_MS,
        "compositor_sha256":digest(Path("/mnt/workbench/capsule-donor-01/g07b02g2-private/replay-compositor.log")),"total_injections":total,
        "total_protocol_press":sum(x["wire_presses"] for x in results),
        "total_app_callbacks":sum(x["callback_count"] for x in results),
        "unattributed":0,"index4":hit,"results":results,
        "limits":["Clock offset calibrated from a single identified Wayland event",
          "Client callback timestamps record handling, not server transmission",
          "No latency percentile guarantees from four trials",
          "A transport match does not by itself certify pointer constraints or DnD"]}
    path=OUT/"replay_transport_audit.json"
    path.write_text(json.dumps(document,indent=2,ensure_ascii=False)+"\n")
    print("TRANSPORT_GATE_PASS",total,"of",total,"callbacks",document["total_app_callbacks"])
    print("INDEX4",hit)
    for r in results:
        print("RUN",r["name"],r["injections"],"late",r["late_callbacks_old_oracle"],
              "max_delay_ms",r["max_callback_delay_ms"])
    print("RECEIPT_SHA256",digest(path))
if __name__=="__main__": main()
