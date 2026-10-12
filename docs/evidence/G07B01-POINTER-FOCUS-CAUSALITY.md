# G07B-01 — Input region commit / pointer focus causality

Status: **FOCUS DESYNCHRONIZATION REPRODUCED; ROOT COMPOSITOR MECHANISM NOT YET ATTRIBUTED; PROMOTION BLOCKED**.
Baseline: `9434994f518994ddf8be5f93b50b03a9457958a8`.
Private diagnostic binary SHA-256: `104f0d97b78b554f267e10a7185077977ea01d00067f70226c2fcae42d9cfea0`.

## Observations (LG QHD, controlled real Wayland session)

The original moving-input G07-B result was 11/16 expected recipients; G07-A static remains 36/36 PASS. Those statuses are unchanged.

The vendored GPUI Wayland backend's `set_input_region` invokes `wl_surface.commit()` immediately. Client-side `WAYLAND_DEBUG=client` independently records `wl_surface.set_input_region` followed by `wl_surface.commit`. This refutes a missing **client-side** commit, but is not an internal compositor-state trace.

Full-open static probe: spawn underlying witness first and position the pointer at local `(450,460)`, then map an Envelope overlay whose input region includes that point, wait **at least 1.2 seconds** after the qualifying region observation, and click without pointer motion. The witness still receives the click, despite stable `visible=1.0`. This excludes a transient GPUI animation-phase mismatch as a sufficient explanation.

Repeat with the same binary, same input rectangle, same click location, same wait, but add relative pointer motions `+4,-4` pixels before clicking: Envelope receives the click. Client protocol traces show `wl_pointer.leave` from the witness and `wl_pointer.enter` into Envelope before its `wl_pointer.button`.

Reverse-direction probe: prime pointer focus on Envelope using motion, then let the input region contract so that `(450,460)` is excluded. With no further pointer motion, the next click still reaches Envelope (wrong receiver). Both input-region snapshots around this second click exclude that point; no cursor repositioning is reported.

## Bounded causal assessment

* **Missing client-side wl_surface.commit:** refuted.
* **Only a few milliseconds of application-region delay:** insufficient explanation; the fully settled case still fails.
* **Stale pointer focus without motion across region/surface changes:** directly supported by bidirectional differential tests and actual `wl_pointer` events.
* **Compositor's applied input-region state at the precise button dispatch:** not independently instrumented. Do not claim an exact internal Hyprland code defect or Wayland protocol violation yet.
* **Scheduler-choice root cause:** not supported. No scheduler promotion.

This is not corrected by synthesizing cursor movement in production: the nudge is an **experimental intervention**, not an acceptable end-user input-routing policy. A durable fix must ensure stable correct click routing even if the pointer remains stationary while an interactive region expands or contracts.

## Next bounded gate

Qualify the compositor's pointer-focus invalidation/recomputation on **region-changing surface commits** in an isolated compositor harness; record surface/region application, pointer focus owner, event recipient and an explicit post-commit synchronization barrier. Propose a minimal compositor-side fix or an architecture-preserving Gnosix workaround only once the mechanism is observed. Repeat on both physical outputs, both schedulers, opening and closing, with unassisted stationary-pointer clicks, including negative controls and restoration. E4 operator perceptual review remains separate and pending.

## Evidence and authority

Machine-readable receipt: `docs/evidence/G07B01-INPUT-FOCUS-RECEIPT.json`.
Full raw protocol and control witnesses retained privately under `/mnt/workbench/capsule-donor-01/g07b01-*`; exact hashes are in the receipt. Experimental scripts are in that capsule, not production. Gnosix standard executable and Cabinet were not replaced. No merge, push, persistent session change, or production runtime correction is claimed.
