#!/usr/bin/env python3
"""Generate G07B-02J bounded, non-promotional, source-backed receipt."""
from pathlib import Path
import json,hashlib
D=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
aud=json.loads((D/"audit.json").read_text())
reg=json.loads((D/"offline_regressions.json").read_text())
assert len(aud["lock_recycle"]["new_matched_controls"])==4
assert all(e["observed_stable"] for e in aud["lock_recycle"]["new_matched_controls"])
assert aud["lock_recycle"]["historic_WAYLAND_1_failure"]["inner_before"]!=aud["lock_recycle"]["historic_WAYLAND_1_failure"]["inner_after"]
assert aud["cross_dnd"]["movement_events_during_grab"]==38
assert aud["cross_dnd"]["max_source_surface_local_x"]>2400
assert aud["cross_dnd"]["parent_enter_leave_during_held_button"]==0
assert reg["status"]=="ARCHIVE_INTEGRITY_PASS_ONLY_NOT_LIVE_E2E"
files={p.name:sha(p) for p in sorted(D.iterdir()) if p.is_file()
       and p.name not in ("receipt.json","make_receipt.py")}
receipt={"schema":"g07b02j-causal-backend-lock-qualification-v1",
 "date":"2026-10-12","base_main":"1481dd7ef44b5107bcce6ffb98f3e8232c7d726e",
 "hyprland_source":"efb50993780079460b0cbed1363e2166a2de1d9f",
 "aquamarine_source":"f31c47a1b9d300847d8fc3108ad959448103dfc1",
 "hyprland_experimental_patch":"4feb578bcf5bf731c3cb250091f1e087c9cc8633",
 "experimental_compositor_binary_sha256":"3b2c93852f2941d1ac609eca64c1b84a06fcccf09edf5a9635eb4cdb1679f637",
 "primary_compositor_binary_sha256":"0b98aa2e2de1080ca2e5534717c6002e9924e359b22cba42a77714c9dd343070",
 "input_source":"ydotool isolated socket, separate native Wayland clients, parent WAYLAND_DEBUG=client",
 "primary_compositor_pid":3123,
 "evidence_sha256":files,"archive_sha256":aud["archive_sha256"],
 "gates":{
  "J1_cross_output_dnd_root_cause":"PASS_NESTED_PARENT_IMPLICIT_GRAB",
  "J2_weston_alternate_compositor_dual_output":"PASS_INFRA_ONLY",
  "J2_weston_independent_dnd_E2E":"NOT_RUN",
  "J2_direct_DRM_two_head":"NOT_RUN",
  "J3_lock_short_and_long_AB":"4/4_PASS_BOUNDED",
  "J3_historical_lock_failure_explained":"OPEN_NONDETERMINISTIC",
  "J4_native_relative_motion":"NOT_QUALIFIED",
  "J5_prior_archive_integrity":"19_FILES_PASS_NOT_LIVE_E2E",
  "J6_human_perceptual_E4":"PENDING",
  "G07B_production_promotion":"BLOCKED"},
 "matched_lock_control_results":aud["lock_recycle"]["new_matched_controls"],
 "historic_failure":aud["lock_recycle"]["historic_WAYLAND_1_failure"],
 "nested_cross_drag":aud["cross_dnd"],
 "weston_two_head":aud["weston_independent_backend"],
 "offline_legacy_integrity":{k:v["archive_bytes_verified"] for k,v in reg["checks"].items()},
 "production_runtime_changed":False,"main_product_source_changed":False,
 "published":False,"pushed":False,"merge_of_external_hyprland_patch":False}
(D/"receipt.json").write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+"\n")
print("G07B02J_RECEIPT_SHA256",sha(D/"receipt.json"))
print("GATES",receipt["gates"])
print("PRESERVED_ARCHIVES",len(receipt["archive_sha256"]))
