# CABINET-FLUID-01 / F10 — Renderer boundary investigation

**2026-10-10 · E3 partial diagnostic, not F10 closure.**

Product HEAD at investigation start: c18fbbe. Diagnostic parent: d2c965f.
Scope: dedicated diagnostic worktree only; native Wayland, Hyprland,
100001 actual temporary files, 1100x820, 450 source-visible pulses,
80 real keyboard events per run, original focus restored.

The Wayland GPUI adapter was instrumented around
`WgpuRenderer::draw(scene)` only, emitting CPU wall time and counts
of GPU scene primitives. This encompasses surface acquisition,
CPU encoding and presentation call overhead, but not asynchronously
executed GPU work or photons. No vendor changes applied to product.

| Measurement | List | Grid |
|---|---:|---:|
| Correlated compositor presents/s | 67.983 | 64.440 |
| WgpuRenderer.draw CPU p50 | 0.938ms | 0.930ms |
| CPU p95 | 1.066ms | 1.098ms |
| CPU p99 | 1.720ms | 1.837ms |
| CPU max | 4.583ms | 4.551ms |
| Model tree to feedback request p95 | 28.395ms | 38.272ms |
| Renderer.draw failures | 0 | 0 |

The current evidence does not support a substantially greater CPU cost
of the renderer call as the cause of the Grid presentation deficit.
However the earlier gap involves several scheduling stages, and the
GPUI internal prepaint/layout/paint and asynchronous GPU operations
remain uninstrumented. Data is descriptive over one trial per mode.
No claim of exclusive attribution or 75 distinct FPS.

Next gates: opt-in instrumentation in an isolated copy of gpui-pre
core for layout/prepaint/paint and a separate GPUI wgpu resource trace;
independent oracle with negative controls; repeated paired runs;
source-frame identity; E4 signoff. No promotion, merge, or push.
