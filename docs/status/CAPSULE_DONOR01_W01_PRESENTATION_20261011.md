# CAPSULE-DONOR-01 — W01 Cabinet native compositor correspondence

**11 October 2026 · E3 scoped Wayland request → compositor Presented PASS; not an equal-workload fluidity benchmark.**

Source worktree `w01-cabinet`, based on Capsule donor branch `28fcca4`. Product stays unchanged. The vendor GPUI diagnostic instrumentation is separately archived as `tools/probes/capsule_donor01_wayland.patch`, and reverted from the worktree after measurement.

An opt-in `CAPSULE_DONOR01_WP_TRACE=1` instrument records a unique Wayland `wl_surface` and request ID immediately before invoking renderer.draw, and receives per-request compositor `wp_presentation.Presented` feedback in the same app process. A separately gated `CABINET_DONOR01_NATIVE=1` drives 240 GPUI `on_next_frame` callbacks over ~2.94 seconds. Both trials use the same instrumented binary and 100001 real, locally generated empty regular files. Each trial runs up to eight seconds, with isolated state files and a native compositor PNG.

| Mode | Compositor Presented | Application animation callbacks | Request→Present p50 | p95 | p99 |
|---|---:|---:|---:|---:|---:|
| Static/control | 33 | 0 | 10.391 ms | 13.062 ms | 21.776 ms |
| On-next-frame candidate | 240 | 240 | 6.117 ms | 11.078 ms | 12.901 ms |

The control receives redraws from asynchronous indexing/startup operations; it is **not** an equal-animation-workload comparison. The candidate updates an internal animation serial only, not a visible transition or filename selection model. 240 request/Presented events are evidence of compositor delivery, not 240 independently decoded distinct images or sustained 75Hz output.

The compositor `wp_presentation` reports **clock_id=1 (monotonic)** and **refresh_ns=6063656**, consistent with the machine's ~164.9 Hz display, **not** the 74.964 Hz HDMI output that FLUID-01 originally targeted. The test must not be used to qualify 75Hz physical display behavior.

The independent protocol oracle verifies exact request-to-feedback joins, surface identity, positive time intervals and monotonic presented timestamps. Six corrupted feedback traces fail closed. This trace does not directly join an individual GPU command submission index or per-buffer pixel identity.

**Boundaries:** same executable, unlike workloads; 100001 files, no input/navigation stress; no independent wl_buffer ID, GPU queue occupancy, click-through test, 1.25x/2x scale, optical input-to-photon, or operator E4. No merge/push. Generated 100001 empty fixture files released using an owner/filename/content-verified manifest; screenshots and logs preserved.
