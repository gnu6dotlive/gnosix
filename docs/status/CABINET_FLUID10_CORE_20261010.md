# CABINET-FLUID-01 / F10 — Deep GPUI/WGPU CPU attribution

**2026-10-10 · E3 scoped diagnostic, internal CPU phases qualified, GPU/UX acceptance OPEN.**

Baseline: diagnostic parent `6883045`, product checkpoint `1866732`.
Two explicitly optional local crate patches, `gpui-pre v0.3.5` and
`gpui-pre-wgpu v0.3.5`, are staged into this isolated diagnostic worktree.
Nothing is copied into the production application's dependency graph.
`tools/cabinet-fluid10-stage-core.sh` reconstructs these two exact crate
sources from hash-qualified crates.io registry snapshots and applies the
checked source patches. Cargo.toml/Cargo.lock point to these local crates
**only on the experimental branch**.

## Measurement contract

Native compositor: Hyprland 0.56.2, primary output HDMI-A-1 (nominal
74.964Hz), 1100x820 window. Each run traverses **100001 real generated
regular files** while a finite 450-pulse source-visible 75Hz stimulus and
80 authentic compositor-routed keyboard transitions execute. Local focus
is validated, and prior active-window identity restored.

GPUI core `draw_roots` samples:
- `prep_us`: setup + layout request preparation before root prepaint.
- `prepaint_us`: root element prepaint, deferred, inspector and layout work.
- `paint_us`: root paint + deferred paint and auxiliary content.
- `layout_compute_us`: CPU time only in non-nested `Window::compute_layout`
  calls during the current prepaint scope (explicit per-thread depth and
  phase gating). Includes the immediate wrapper/engine call, not all
  ancillary measurements in Taffy or paint; remains a subset of prepaint.
- `full_us`: the entire `draw_roots` method; all phases must sum within
  monotonic microsecond rounding tolerance.

wgpu CPU boundary, successful `WgpuRenderer::draw` calls only:
- `acquire_us`: `surface.get_current_texture()`.
- `prepare_record_us`: intermediate resources / uniform uploads / views.
- `record_cpu_us`: `record_frame(scene, view)` command recording.
- `present_cpu_us`: only the CPU call `frame.present()`.
- `total_cpu_us`: above stages combined.

**Important:** these clocks do not measure asynchronous GPU execution,
submission completion, scanned-out pixels or input-to-photon delay. GPUI
core and wgpu records lack a shared per-frame ID: the oracle reports
independent distributions, not an invented per-frame join.

## Oracle quality correction

A preliminary exploratory reader matched `paint_us` inside `prepaint_us`
using substring search; that paint subtotal was discarded, then recomputed
with exact-key extraction. A preliminary `layout_compute_us` accumulator
also double-counted nested/out-of-scope calls: its early samples are
REJECTED for layout attribution and preserved as negative evidence.
Final instrumentation scopes calls to root prepaint, accounts for nested
layout only at the outer depth, and enforces
`layout_compute_us <= prepaint_us + 2` microseconds. No coherence
violations were found in the accepted source-qualified pair.

## Qualified corrected layout pair (450 impulses each)

| Native measurement | List | Grid |
|---|---:|---:|
| Distinct compositor-present feedback rate correlated to pulses | 69.486/s | 63.071/s |
| draw_roots p95 CPU | 9.665 ms | 10.029 ms |
| compute_layout p95 CPU | 4.190 ms | 4.637 ms |
| prepaint p95 CPU | 5.499 ms | 5.780 ms |
| paint p95 CPU | 3.004 ms | 2.819 ms |
| wgpu draw CPU p95 | 1.049 ms | 1.089 ms |
| wgpu acquire CPU p95 | 0.129 ms | 0.135 ms |
| wgpu record_frame CPU p95 | 0.703 ms | 0.735 ms |
| Model-tree to compositor-feedback REQUEST p95 | see F09 oracle | see F09 oracle |

List and Grid have **comparable per-draw CPU costs** despite the observed
frame-coalescing deficit on Grid. The 0.447ms excess in Grid's p95
compute_layout duration cannot alone explain the 6.415/s disparity.
The more probable unmeasured interval is scheduling, tick availability,
redraw invalidation or deferred GPU/compositor synchronization. This is
an **investigation direction, NOT an established root cause**.

An earlier separate instrumented pair (`deep-{list,grid}-v1`) measured
prepaint/paint/wgpu CPU but did not capture accepted isolated Taffy timings;
it corroborates broadly similar CPU phases but must not be pooled into
the final isolated-layout comparison. Two early `layout-{list,grid}-v1`
trials are also excluded from isolated Taffy statistics.

## Evidence and status

- Accepted roots:
  `/mnt/workbench/evidence/runs/cabinet-fluid10-layout-list-100001-v2`
  and `/mnt/workbench/evidence/runs/cabinet-fluid10-layout-grid-100001-v2`.
- Parsed analysis:
  `/mnt/workbench/evidence/runs/cabinet-fluid10-layout-analysis-v2.json`.
- Independent parser: `tools/cabinet-fluid10-deep-oracle.py`.
- Negative controls: 3 intentional mutations rejected (layout
  overcount, missing wgpu record, wrong compositor clock).
- Source stage: `tools/cabinet-fluid10-stage-core.sh`; exact-byte
  patches in `tools/probes/cabinet-fluid10-{gpui-core,wgpu}.patch`.

**Acceptance:** F10 local E3 CPU separation partially PASS; GPU execution
attribution, historical 0.5s stall reproduction, per-pixel frame identity,
75 distinct delivered FPS and operator E4 premium UX remain OPEN.
No merge, push or product-dependency promotion.
