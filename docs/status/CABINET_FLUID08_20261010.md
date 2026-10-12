# CABINET-FLUID-01 / F08 — Source-visible 75Hz pacing probe

**E3 instrumented observations; release F08 and premium E4 remain OPEN.**

Parent checkpoint 927cdf8 / GPUI frame-loop probe. No permanent product
code or live compositor configuration changes.

## Objective and method

Use an opt-in finite app-owned stimulus rather than IPC-generated keypresses.
For CABINET_F08_PULSE_COUNT=450, Cabinet awaits a 13,333,333ns deadline
(75Hz nominal), updates one visible header serial and notifies GPUI.
The generator waits for the complete indexed Files view plus 1300ms startup
settle. The generator never runs without opt-in and rejects counts over 900.

Native host: Hyprland 0.56.2, HDMI-A-1 74.964Hz, 1100x820 window, 100001
actual empty temporary files. Other monitor DP-2 can report 165Hz: DO NOT
use unordered startup feedback intervals to infer Cabinet output refresh.
Only feedback matched to app pulse chains counts toward delivered rate.

Correlate identical CLOCK_MONOTONIC timestamps in order:
- stimulus model mutation / serial increment
- actual GPUI tree construction containing the visible serial
- later feedback request on the same Cabinet wl_surface
- compositor wp_presentation.Presented with matching request ID.

The chain is source-to-compositor, not a renderer pixel readback or physical
input-to-photon measurement. Each compositor Present may cover multiple
source serial updates. Such merges are explicitly counted. Real frame
identity/pixel serial readback remains OPEN.

## Comparable runs (each 450 pulses in ~6 seconds)

| Run | Loop trace | Pulse Hz | Correlated distinct buffer Presents /450 | Mean correlated Presents/s | Pulse-to-present p95 |
|---|---|---:|---:|---:|---:|
| Grid A | on | 74.945 | 419 | 69.771 | 39.906ms |
| List A | on | 74.998 | 443 | 73.829 | 29.236ms |
| Grid B | on | 75.006 | 424 | 70.663 | 36.607ms |
| List B | on | 74.824 | 420 | 69.824 | 40.062ms |
| Grid C | off | 74.946 | 408 | 67.935 | 41.392ms |
| List C | off | 75.037 | 440 | 73.366 | 30.018ms |

These are descriptive one-run values; mode and logging effects are NOT
causally isolated because run-to-run timing varies. GPUI F07 tracing is not
sufficient to explain losses; disabling it did not remove them.

## Stronger source serial attribution (no F07 verbose trace)

| Result | List D | Grid D |
|---|---:|---:|
| Model updates | 450 | 450 |
| Unique serials observed in actual GPUI view trees | 449 | 437 |
| Distinct correlated compositor Present feedbacks | 449 | 430 |
| Mean correlated Present feedback rate | 74.871/s | 71.659/s |
| Pulse-to-tree p95 | 3.924ms | 3.913ms |
| Tree-to-feedback-request p95 | 9.622ms | 10.020ms |
| Tree-to-feedback-request p99 | 11.028ms | 27.750ms |
| Request-to-Present p95 | 16.982ms | 16.951ms |
| Pulse-to-Present p95 | 29.399ms | 29.434ms |
| Pulse-to-Present p99 | 29.509ms | 39.483ms |

Actual timer wakeup lateness, event-loop scheduling, and compositor cadence
vary between runs. Source-tree building and compositor response after the
feedback request have comparable p95; Grid's p99 tree-to-request increases
and its visible states are merged more often. The data identifies the
interval to investigate, not a definitive GPU/GPUI driver root cause.

## Admissible conclusions

- Genuine native GPUI tree mutations sustain approximately 75 pulses/s.
- All eight runs received source-to-present feedback chains, but several
  pulses coalesced; 75 distinct display frames/s is NOT established.
- List D is near 75 distinct compositor feedbacks/s for six seconds,
  not a distribution-free FPS release claim.
- The earlier F06 Grid 374.851ms p95 incident was not reproduced.
  Its causal root remains OPEN and may involve the scheduler, compositor,
  resource contention or an interaction not present in F08.
- No source vendor patch from diagnostics is promoted to production.

Evidence lives under /mnt/workbench/evidence/runs/cabinet-fluid08-*.
Machine-readable oracles: tools/cabinet-fluid08-pacing-oracle.py
and tools/cabinet-fluid08-serial-oracle.py. Negative/host control proofs
are in the checkpoint evidence root.

## Remaining acceptance

1. Reproduce the intermittent ~0.5s freeze with frame-loop and GPU driver
   evidence captured in the SAME failing run.
2. Profile GPUI view-to-request jitter and late timer wakeups with repeated
   paired trials, counterbalanced ordering and GPU utilization telemetry.
3. Attribute actual pulse serial to physical presented buffer content using
   readback/video/approved independent instrumentation.
4. Qualify 2x scale interactions and Spacedrive-matched perceptual E4 review.

F08 is not closed; no merge, push, GTK portal or system compositor mutation.
