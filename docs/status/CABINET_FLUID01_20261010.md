# CABINET-FLUID-01 — Native fluidity and frame provenance

**ACTIVE / proof acquisition.** Base 032cc21; isolated branch
feat/cabinet-fluid-01-20261010. This contract is not a PASS receipt.

## Goal

Explain and reproduce Cabinet latency, jitter and peak resident footprint
under genuine native GPUI List and Grid workloads. Keep input-to-state,
application tree construction, Wayland callback pacing and compositor
present timestamps as four distinct measurement domains.

## Bounds and prior evidence

P05 passes 88 Rust tests and 48/48 hermetic CI, indexed scans >300k, E3
1699-selection persistence and deletion reconciliation. The previous
260-transition keyboard input-to-state result is not GPU presentation.

- CABINET_VIEW_BUILD_LATENCY is CPU view-tree time.
- CABINET_INPUT_STATE_LATENCY starts inside the GPUI keyboard handler,
  not at a physical HID interrupt, and stops at application model mutation.
- wl_surface.frame and wl_callback.done indicate compositor frame callbacks,
  not the instant a surface becomes visible on screen.
- Actual compositor presentation requires wp_presentation feedback for
  the real Cabinet wl_surface, its clock and Presented/Discarded events,
  or another independent instrument. This remains NOT INSTRUMENTED.
- An internal renderer state named Presented is not external compositor proof.

## Isolated reproducibility protocol

Run real Cabinet native on 100003 entries (100001 files, subdir, example)
under a dedicated Weston/Xvfb in a scoped GNU6 lab, separate root/repeat.

For both modes: controlled keyboard navigation, same workload count/pacing,
raw input and view logs, Wayland callback events, process RSS and screenshot.
Capture exact source HEAD, binary SHA-256, GPU, Rust, compositor and
protocol globals. The oracle fails on missing samples or mismatched scope.
Use p50/p95/p99; multi-run aggregates are descriptive rather than an SLA.

No host package/configuration changes, privileged profiling or GTK changes.
The live Hyprland desktop must not be disrupted by isolated trials.

## Gates

F01: List/Grid input-to-model distributions (at least 250 events/run).
F02: CPU view-tree p50/p95/p99 with count and provenance.
F03: frame callbacks measured separately, filtering initialization callbacks,
     protocol wp_presentation availability reported explicitly.
F04: peak resident-memory samples and VmHWM, and allocation-leak caveat.
F05: actual compositor presentation feedback for Cabinet surface, else OPEN.
F06: live Hyprland, viewport/scale and operator E4 versus Spacedrive, else OPEN.
F07: exact-HEAD CI/tests, reproducible evidence, local commit/clean worktree.

Never close all FLUID-01 gates because F01-F04 passed.

## Vendor provenance

Preserve upstream GPUI vendor source bytes; do not silently patch renderer
or Wayland library to manufacture presentation observations. If the extension
is available, a separately reviewed/isolated probe overlay may be proposed.

## First E3 observations — isolated native GPUI

Native Cabinet, 100003 entries, 1100x820 X11-backed Weston/pixman,
one independent trial per view and 260 real xdotool keyboard transitions
in each, on the same build; per-run input, CPU-tree, RSS and Wayland traces.

| Metric | List | Grid |
|---|---:|---:|
| Handler-to-state p95 (us) | 149 | 112 |
| Handler-to-state p99 (us) | 182 | 203 |
| GPUI view construction last rolling p95 (us) | 221 | 172 |
| GPUI view construction last rolling p99 (us) | 337 | 330 |
| wl_surface.frame callback interval p95 (ms) | 18.806 | 18.801 |
| wl_surface.frame callback interval p99 (ms) | 20.657 | 21.513 |
| Sampled peak RSS (KiB) | 333020 | 333288 |

The view metrics reflect a rolling window of 256 observations rather than
all frames, and the callback intervals cover application callbacks across
the entire journey, including idle and startup; neither directly proves
application visual response timing or actual compositor present events.
The RSS samples do not establish zero leak or steady state.

Source oracle: tools/cabinet-fluid01-oracle.py.
Evidence: /mnt/workbench/evidence/runs/cabinet-files05-fluidlist-20261010-v1
and /mnt/workbench/evidence/runs/cabinet-files05-fluidgrid-20261010-v1,
machine-readable analysis in
/mnt/workbench/evidence/runs/cabinet-files05-fluid-analysis-20261010-v1.json.

F01-F04 have bounded E3 evidence. F05 remains NOT_OBSERVED; F06 E4
operator acceptance OPEN. No 60/75 FPS product claim.
