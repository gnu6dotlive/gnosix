# CABINET-FLUID-01 / F12-C — Presentation timeline

**2026-10-11 · E3 scoped compositor-presentation timeline PASS; per-buffer identity OPEN.**

Starting point: F12-B product `3c5a401` and diagnostic `e07e84e`. This checkpoint analyzes the exact-source F12-B Grid/List experiments without altering the Cabinet runtime or Vulkan device.

The Wayland `wp_presentation` protocol exposes a compositor-present timestamp with clock ID 1 (`CLOCK_MONOTONIC`), feedback request sequence and `wl_surface` identity. This is bound to a source-visible GPUI serial and wgpu SubmissionIndex through F11 logs. It does **not** expose an independently verified `wl_buffer` identity. For F12-C, the independent oracle joins every request with every presentation and wgpu submission and joins sampled hardware encoder timestamps only on exact request IDs.

| Native exact F12-B workload | Grid | List |
|---|---:|---:|
| Requests / compositor-presented feedbacks | 464 / 464 | 460 / 460 |
| Matched hardware-GPU encoder samples | 116 | 115 |
| Request-to-presentation p50 | 16.349 ms | 16.431 ms |
| Request-to-presentation p95 | 16.828 ms | 17.186 ms |
| Request-to-presentation p99 | 25.499 ms | 25.380 ms |
| GPU encoder execution p95 | 0.234 ms | 0.199 ms |

The timestamp gap contains scheduling and potentially several other events. The GPU encoder span and request-to-presentation distributions cannot be subtracted to obtain an isolated queue delay or compositor composition duration. These are independent intervals with distinct boundaries. The former uses the GPU clock delta and the latter uses Wayland's reported monotonic presentation clock. This checkpoint does not attempt an unsupported GPU/CPU absolute timestamp calibration.

The fail-closed oracle checks: compositor presentation clock ID exactly 1, unique request and submission identities, matching `wl_surface`, positive monotonic presentation latency, bounded plausible duration, GPU serial agreement, nonzero positive GPU ticks and exact expected sampled GPU identities. Five intentional corruptions are rejected: wrong presentation clock, missing feedback, wrong surface, impossible duration and missing intermediate GPU sample.

Evidence: `/mnt/workbench/evidence/runs/cabinet-fluid12c-timeline-v1.json`; negatives under `/mnt/workbench/evidence/runs/cabinet-fluid12c-negative-v2/`. Source inputs are the F12-B exact-binary runs, without claiming a new independent native GPU trial.

**OPEN:** Wayland `wl_buffer` provenance, mapping one screenshot pixel marker to one exact buffer, driver queue residence, scanout/photon latency, 500 ms Grid stall attribution and operator E4 premium UX. No change or promotion of runtime, dependencies, shell settings or GTK portal. No merge/push.
