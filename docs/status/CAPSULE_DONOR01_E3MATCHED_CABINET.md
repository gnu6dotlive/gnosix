# CAPSULE-DONOR-01 / E3-MATCHED — Cabinet

**2026-10-12 · E3 comparative source→Wayland compositor feedback scoped PASS; E4 OPEN**

## Machine and output

Two physical outputs are confirmed through Hyprland monitor metadata:
- `HDMI-A-1`: LG QHD, 2560×1440 at 74.964 Hz, scale 1, monitor ID 0.
- `DP-2`: Acer, 1920×1080 at 164.917 Hz, scale 1, monitor ID 1.

These are distinct devices. Do not infer an output solely from a `wp_presentation` refresh period; numerical correspondence is not an output locator. Differences within 74–75 Hz have no separate UX gate.

## Native trials

The experiment uses exactly one native GPUI Cabinet diagnostic binary (SHA256 `4f598b84f335d450d0957559c803ef55d48b77549aff70bde3b72761b973c060`), a single real 100001-empty-file fixture, the same visible animated scalar with targets toggling every 650 ms, and a native Hyprland window independently located at monitor 0 (HDMI-A-1) in both policies.

The two order-reversed pairs return the following verified results:

| Pair | Policy | GPUI animation callbacks | Wayland Presented | request→present p95 |
|---|---|---:|---:|---:|
| A (timer→frame) | timer 13 ms | 143 | 159 | 27.062 ms |
| A | on_next_frame | 172 | 172 | 26.991 ms |
| B (frame→timer) | on_next_frame | 167 | 168 | 27.217 ms |
| B | timer 13 ms | 146 | 162 | 27.112 ms |

All seven target transitions and exact serial continuity passed. Pair B had ~8 ms target phase drift and **failed the original ±4 ms comparison requirement**; a later explicitly relaxed one-display-period bound accepted it. Preserve both facts.

**Decision:** no demonstrated general Cabinet presentation latency advantage to transplant. The candidate has *more*, not fewer, callbacks in these trials. No keyboard-navigation equivalence, page transitions, CPU/GPU utilization, input-to-photon measurement, per-buffer pixel identification, 1.25×/2× output scale or operator E4 qualification has been established. The historical ~500 ms Grid delay is not reproduced or fixed by this comparison.

Provenance: `/mnt/workbench/capsule-donor-01/matched-cabinet-evidence-v1` and `matched-cabinet-evidence-v2` with strict and one-period oracles. The source-only Wayland presentation patch is preserved as `tools/probes/capsule_donor01_wayland.patch`; no permanent vendor change. The 100001 generated fixture files were released with manifest `E3MATCHED-FIXTURE-RELEASE.json`.

No production mutation, merge, push or E4 promotion.
