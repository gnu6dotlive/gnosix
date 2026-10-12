# CAPSULE-DONOR-01 — W02 Envelope native compositor correspondence

**11 October 2026 · E3 scoped application motion → Wayland compositor feedback PASS; no full topology/pixel qualification.**

Consumer worktree `w02-envelope`, branch `probe/capsule-donor01-envelope-20261011`, source baseline `19417a9`. The full native shell/dock runtime is unchanged. The optional diagnostic Wayland patch is byte-identical to W01 and archived separately, then removed from the GPUI vendor tree.

Two native LayerShell qualifications under Hyprland, with optional `CAPSULE_DONOR01_WP_TRACE=1`, use the same executable but different animation strategies:

| Mode | Model | Source animation updates | Compositor Presented | Request→Presented p50 | p95 | p99 |
|---|---|---:|---:|---:|---:|---:|
| Control | Existing DockQualification timer-based visibility fraction | 22 morph steps | 24 | 5.730 ms | 11.385 ms | 11.647 ms |
| Candidate | `SharedMotion` vertical analytic channel and `on_next_frame` with quiescence | 101 callbacks; six settled idle stages | 101 | 7.750 ms | 18.857 ms | 19.771 ms |

Both paths call `resolve_dock_frame`, project pointer regions and use `Window::set_input_region` and `set_exclusive_zone(0)` to maintain bounded compositor geometry. **The test does not inject or independently verify pointer events against those regions.** The candidates have different state trajectories and refresh scheduling; these numbers **are not evidence that one strategy is faster or visually better**.

Wayland presentation clock ID 1, exact request → feedback identities and same `wl_surface` pass the independent oracle. The compositor reports refresh period 6,063,656 ns (~164.9 Hz), not the 74.964 Hz FLUID-01 monitor; 75Hz qualification stays OPEN. The `wp_presentation` feedback is not a physical scanout/photon timestamp, and no independent source-serial per-buffer pixel readback or GPU submission-index correlation was acquired.

Previous W02 tests remain valid: 136/136 `gnos-ux-surface`; 218/218 `gnos-ux-gpui`; new retarget/stability E2 tests 3/3. No original `ReactiveEnvelopeSolver` topology replacement, module/bus change, production shell mutation, merge or push.

**Next E3 gate:** exact same scripted `SharedMotion` trajectory under two schedulers, 75Hz and scale/output matrix, source→GPU→Present identity, actual input-region click-through/occlusion validation and 500ms stall reproduction; E4 requires operator visual approval.
