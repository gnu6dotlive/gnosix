# CAPSULE-DONOR-01 / G07-B — Dynamic click-through and operator visual review

**12 October 2026 · Dynamic click-through: REJECTED · E4: NOT RATIFIED · No product promotion**

## Baseline and hypothesis

G07-A (commit `24fad7e`) independently qualified **36/36 first-attempt static click-through deliveries** across `HDMI-A-1` and `DP-2`, including 12 across different native GPUI processes. G07-B deliberately probes a *different* property: whether a changing input region and its independently observed pointer routing remain coherent **during active physical animation**.

The diagnostic application keeps the original `SharedMotion`, `resolve_dock_frame`, `project_dock_pointer_regions`, `Window::set_input_region`, and LayerShell authority. A reversible opt-in switch `GNOS_ENVELOPE_G07B_DYNAMIC` drives 20 rapid alternating retargets spaced by 420 ms, under either timer 13 ms or GPUI `on_next_frame`. Input regions and clicks are independently tagged with epoch timestamps. The underlying witness executes in a **separate native GPUI process** with its own `wl_surface`, namespace, LayerShell layer and pid. Only the owned 900×560 test pair receives synthetic clicks.

Physical outputs were independently verified with `hyprctl layers -j`:
- LG QHD: `HDMI-A-1`, 2560×1440, 74.964 Hz, scale 1.
- Acer: `DP-2`, 1920×1080, 164.917 Hz, portrait transform 1, scale 1.

All four dynamic runs use the **identical** diagnostic binary SHA256 `104f0d97b78b554f267e10a7185077977ea01d00067f70226c2fcae42d9cfea0`, archived in `g07b-archive/g07b-dynamic-diagnostic.bin.zst`, compressed SHA256 `57446af401f53019364aa6ecb5cca5964975e9ff71cae8772095cf588b2610d5`.

## Native dynamic result — FAIL

Each run injects four geometrically bounded clicks: main region at (450,460) while the published region includes the point; same point while excluded; invariant reveal at (450,499); invariant hole at (450,270). Published regions are checked before and after; a requested click is never sent outside the **independent witness surface**.

| Output | Wakeup policy | Expected | Wrong receiver | No receiver |
|---|---|---:|---:|---:|
| HDMI-A-1 | on_next_frame | 3 | 1 | 0 |
| HDMI-A-1 | timer 13 ms | 3 | 1 | 0 |
| DP-2 | on_next_frame | 2 | 1 | 1 |
| DP-2 | timer 13 ms | 3 | 1 | 0 |
| **Total** | | **11/16** | **4/16** | **1/16** |

- Every invariant reveal-region click reached the overlay; every invariant always-transparent hole click reached the witness.
- Three of four main-area clicks reached the witness while the latest GPUI region, both before and after, included the clicked coordinate. The fourth main click had no observed recipient.
- On `DP-2` in the on_next_frame run, the *excluded-area* test reached the overlay despite both published regions excluding the coordinate.
- Independent logs show that Gnosix's region projection is updated during real motion. However, source-side `G07B_REGION` messages are **not an atomic proof of the compositor's applied input-region state at the button event**. The physical mismatch is established, but not its kernel/Wayland/compositor/GPUI root cause.

**The gate is REJECTED.** Do not reinterpret discrepancies as a positive success, a 75 Hz tolerance, or a frame-rate issue.

## Causal control: static partial-open surface

The same executable was tested at exactly **70% visible fraction**, not animated, on both outputs. The main coordinate (450,460) reaches the overlay, the uncovered coordinate reaches the independent witness, and the reveal strip reaches the overlay. All **6 final recipients** are correct. On `DP-2`, the first synthetic main click was again not observed; the second attempt was received. This is independent corroboration that synthetic input/focus remains a possible issue. It is not proof of a compositor bug or a fixed first-click defect.

The source for `gpui-pre-linux 0.3.5` currently makes `wl_surface.set_input_region` followed by a `wl_surface.commit()` immediately in `WaylandWindow::set_input_region`. Consequently the discrepancy **cannot be explained by asserting that GPUI intentionally defers every input-region commit until the next frame**. A separate causal trace is required to determine whether the compositor had processed/applied the commit and which layer received pointer focus.

## E4 visual evidence — independent scope

The diagnostic capture pack is under:

`/mnt/workbench/capsule-donor-01/G07B-E4-VISUAL-REVIEW/index.html`

Four comparisons contain 36 verified, native `grim -g` compositor ROI captures each (900×560; total 144 sampled frames), timing manifest, and timestamp-proportional VFR video. Sampling medians are approximately **42–67 ms**, **not** the panels' 74.964/164.917 Hz refresh rates. The videos are suitable for gross transition/geometry inspection but not for declaring physical 75/165 FPS, optical photon timing, or perceptual premium E4.

An exploratory GPU recorder run returned black frames with `kms server error: drmfb handle is NULL`; it is **REJECTED and omitted from the operator comparison index**. That failure is not a failure of Envelope's own GPU renderer.

**No operator E4 signoff has been recorded.** The Fused/Bridged/Detached live shell topologies, scaling matrix 1.25×/2×, compositor pixel identity, and full input-to-photon study remain OPEN.

## Reproduction and follow-up contract

- Independent result oracle: `/mnt/workbench/capsule-donor-01/g07b_oracle.py`, terminal `G07B-DYNAMIC-ORACLE.json`, expected status `DYNAMIC_CLICKTHROUGH_REJECTED`.
- Eight fail-closed mutation controls: `g07b_negative.py` and `g07b-negatives-v1/*.reject.txt`.
- Input source runner: `g07b_dynamic_clicks.py`; static partial probe `g07b_static_partial.py`.
- Compositor visual capture runner: `g07b_roi_record.py`; videos/manifest files in review directory.

**Proposed next engineering gate: G07B-01 Input-Region Commit/Pointer Focus Causality.** Instrument at the Wayland boundary, for both independent processes, exact request ordering for `wl_surface.set_input_region`, `commit`, and seat pointer enter/leave/button. Correlate display-sync and actual event timing under the moving trajectory. Separate pointer-device event absence from layer hit-testing and protocol flush/commit timing. Only then test a minimal repair without changing the Gnosix physics or granting broader compositor authority.

**Decision:** E3 static input qualified (G07-A); E3 moving-input REJECTED (G07-B). Envelope remains **HOLD for production promotion**, pending causal diagnosis and new repeatable dynamic PASS. Cabinet remains independently HOLD on scheduling. No product merge, push, monitor changes or runtime deployment.
