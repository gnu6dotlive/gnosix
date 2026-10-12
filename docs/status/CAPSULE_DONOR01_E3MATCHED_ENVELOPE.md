# CAPSULE-DONOR-01 / E3-MATCHED — Envelope

**2026-10-12 · physically attributed E3-matched Wayland PASS; complete topology and E4 OPEN.**

## Hardware identification is explicit

The operator has **two different physical monitors**, not a panel with inconsistent modes:
- LG QHD `HDMI-A-1` = 2560×1440, 74.964 Hz, 1× scale.
- Acer `DP-2` = 1920×1080, 164.917 Hz, 1× scale.

Do not infer an output from a Wayland reported refresh_ns alone. The earlier Envelope `v1/v2` comparisons had an unverified physical-output assignment and retain only their narrow compositor-feedback status.

The cause of the monitor ambiguity was initialization order, not panel Hz tolerance. When an Envelope layer-shell window is opened immediately, `cx.displays()` can be empty and `WindowOptions.display_id` remains None; the compositor is then allowed to pick the output. A bounded deferred window opening of 350 ms in the **isolated diagnostic executable**, followed by output selection from a nonempty `cx.displays()`, resulted in `hdmi_2560_1440_found=true`.

A fully independent compositor query, `hyprctl layers -j`, captured during **each of four native runs** physically locates the exact `gnos.ux:dock:atlas-qualification` layer on `HDMI-A-1`, with a stable monitor topology before, during and after. The `wp_presentation` feedback additionally reports clock 1 and refresh period 13,339,736 ns. Unlike the initial record, the physical output attribution is established from layer metadata independently of refresh.

## Equal trajectory, two inverted pairs

All four runs use identical binary SHA256 `6b7d5ad39354c65631bf4c8ea268098e54eb6f59980de9ecc8ebed15c4e54ed8`, native GPUI, the *existing* Gnosix `SharedMotion` analytic physical solver, 7 open/close target events separated by 650 ms, identical geometry and a 900×560 layer-shell surface. Only the frame-wakeup policy changes: 13ms timer versus `Window::on_next_frame`. No monitor settings, Polkit or desktop configuration were altered.

| Ordered trial | Policy | App motion callbacks | Wayland Presented | request→present p95 |
|---|---|---:|---:|---:|
| v4 A | 13ms timer | 246 | 252 | 26.840 ms |
| v4 B | on_next_frame | 98 | 98 | 18.967 ms |
| v5 A | on_next_frame | 98 | 98 | 19.680 ms |
| v5 B | 13ms timer | 253 | 259 | 26.184 ms |

The candidate consumes fewer animation callbacks and delivers a lower source request→compositor feedback p95 on both comparisons, but settles slightly later (~526–527ms p95 target→idle versus ~511–513ms timer). This small, nonrandomized n=2 pair is **descriptive, not a causal guarantee or E4 visual acceptance**. The percentage of physical photons or distinct pixels was not measured.

**Oracles:** `matched_envelope_oracle.py` validates the seven target/idle events and request/Presented ordering. `matched_physical_oracle.py` independently validates `hyprctl` monitor inventory, layer namespace, physical output, binary identity, trace digest and refresh clock across four trials. `matched_physical_negative.py` rejects six tampered physical-output cases.

**Evidence:** `/mnt/workbench/capsule-donor-01/E3MATCHED-PHYSICAL-OUTPUT.json`, traces under `matched-envelope-output-qualified-v4` and `v5`. Source-only diagnostic `wp_presentation` probe is retained as a patch, not promoted into the GPUI vendor code.

**Open:** independently proved GPU submission/buffer serial, buffer-level pixel capture, click-through and pointer hit-testing on Envelope, DPI 1.25×/2×, all Fused/Bridged/Detached compositor topologies, stable CPU/power benchmark, 500ms historical Cabinet stall and user E4 signoff.

**Port decision:** ADAPT the state-driven active-only scheduling pattern, preserve Gnosix physical solver and typed compositor authority, no direct GPUI fork migration, no unrestricted shell reload, no production merge/push.
