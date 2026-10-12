# CABINET-FLUID-01 / F06 — Hyprland correlation diagnostic

**Experiment only.** Parent: 56f52d2 (optional F05 presentation feedback).
Do not merge this instrumented GPUI tree into the product by implication.

## Objective

Correlate real GPUI keyboard model changes with the next Cabinet UI-tree
construction, the next Wayland surface feedback request, and the compositor
Presented timestamp on CLOCK_MONOTONIC (clock ID 1). Source:
tools/cabinet-fluid06-hyprlab.sh and tools/cabinet-fluid06-correlate.py.

Only temporary Cabinet windows, generated 100001-item fixture roots and a
temporary user-owned ydotoold socket were used. Native compositor: Hyprland
0.56.2. Real compositor input injection is mediated by Lua dispatchers
and temporary pointer focus; no persistent Hyprland configuration changes.

## Critical semantics

- Handler-to-model traces exclude HID-to-handler delay.
- Monotonic clock is shared by model commit, view built, feedback requested
  and compositor present (this is checked at runtime). No comparisons across
  mismatched clock IDs.
- The oracle requires matching wl_surface IDs, unique request IDs, valid
  request/present order and a plausible <=500ms causal window.
- The resulting statistic is **model-commit-to-next-compositor-present**,
  conditional on the chain; it is not direct input-to-photon.
- Multiple input commits may resolve to one frame; coalescing is accounted
  for, not silently counted as distinct GPU frames.
- Gaps across idle intervals do not establish FPS during continuous load.
- Failure to restore an exact previous focus (in one experimental v3)
  is retained as an environmental qualification issue.

## 100001 actual-file corpus / 1100 x 820

Each run injected 260 DOWN events using Hyprland 0.56 Lua dispatch with
nominal 20ms step (actual producer rate is slower due IPC overhead).
Source and observations are independently attributable in analysis JSON.

| Run | Model updates | Correlated | p50 ms | p95 ms | p99 ms | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|---:|
| List v1 | 260 | 260 | 20.106 | 26.319 | 28.903 | 203.4 |
| List v2 | 260 | 260 | 20.603 | 28.299 | 34.231 | 204.3 |
| Grid v3 | 260 | 250 | 36.683 | 374.851 | 450.236 | 200.4 |
| Grid v4 | 260 | 260 | 20.813 | 29.211 | 34.944 | 202.1 |
| Grid v5 | 260 | 260 | 19.818 | 27.074 | 29.451 | 203.3 |

The only severe outlier is Grid v3. It is not repeatable in the next
two Grid runs. During that run, GPUI view construction continued in sub-ms
times while frame-feedback *requests* experienced long gaps, and many inputs
coalesced into one presented frame. This suggests a frame-scheduling or
submission interruption, but does not identify the causal layer. The
regression remains **OPEN / intermittent**, not silently averaged away.

## Viewport E3 visual observations

- 860 x 700: inspector stacks below the list; toolbar wraps, path controls
  remain in bounds. Bottom of inspector requires user inspection for
  accessibility and reachability.
- 1100 x 820: List and Grid both show selection and inspector; matching
  100001 source count. One earlier Grid screenshot instead captured
  another window after losing focus: **REJECTED** as evidence.
- 1400 x 900: inspector remains side-by-side; breadcrumbs truncate and
  controls remain in bounds.
- No controlled Spacedrive session or DPI 2x trial; **E4 remains OPEN**.

## Qualification targets still open

- Reproduce/root-cause the intermittent Grid scheduling stall under an
  isolated repeat and CPU/GPU trace, not by assuming a known fault.
- 75Hz sustained render-target/input-to-present with direct stimulus
  timestamps and proof of frame identity.
- Real 2x scale on a bounded environment, focus/scroll accessibility.
- Operator perceptual acceptance with a provenance-matched Spacedrive
  comparison.

The measured one-off success cases are not product promises or a 75 FPS
claim. Keep F06 ACTIVE.
