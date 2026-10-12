# CABINET-FLUID-01 / F09 — Scheduling and GPU covariates under paired native load

**2026-10-10 — SCOPED E3, causal root remains OPEN.**

Base product 4789d79; F08 diagnostic ancestor 5fb4640. F09 uses a
separate worktree. No GPUI production code or compositor configuration
changes. The GPU data is DEVICE-WIDE and not Cabinet-specific.

## Protocol

Real native Cabinet on Hyprland 0.56.2, HDMI-A-1, 1100x820 pixels, 100001
regular temporary files. 900 finite source-visible graphic mutations
targeting 75Hz plus 260 compositor-injected Down keystrokes per run.
Application focus verified throughout; a first List attempt LOST FOCUS
and was REJECTED, without completing keyboard input.

External 40ms samples of process leader thread schedstat capture cumulative
CPU runtime and runnable wait; external 250ms NVML reads device-wide GPU
utilization, clocks and power. Both clocks are monotonic for correlation
to F08 model/view/request/Presented timestamps. NVML utilization updates
at its own cadence; rapid polling cannot enhance its actual resolution.

Accepted ordering: Grid A, List B retry, List C, Grid D. Two observations
per mode, non-randomized. Grid E enables verbose F07 frame-loop tracing.

## Measured List/Grid pairs

| Trial | Correlated Presents/s | Tree to request p95 | p99 | Main thread CPU share | Main runqueue wait in ~12s | Device GPU utilization p50 |
|---|---:|---:|---:|---:|---:|---:|
| Grid A | 56.813 | 48.935ms | 64.800ms | 84.13% | 14.178ms | 36% |
| List B retry | 62.347 | 37.767ms | 55.950ms | 83.25% | 10.485ms | 37% |
| List C | 62.845 | 38.070ms | 46.807ms | 82.77% | 2.907ms | 28% |
| Grid D | 59.865 | 38.121ms | 48.094ms | 85.23% | 10.394ms | 30% |

All four contain 900 visible mutations and 260 actual keyboard model
transitions and source-to-compositor feedback chains. Mean List 62.596/s,
mean Grid 58.339/s, difference -4.257/s for Grid. This is descriptive,
not population statistical significance or a promised sustained FPS.

Grid E: 59.044 correlated Presents/s; 84.91% main-thread occupancy;
9.137ms runnable wait, 6659 frame-loop observations (4962 labelled
AwaitingCallback). No >75ms keyboard-model-to-next-request interval
was observed. AwaitingCallback is a normal compositor pacing state.

## Interpretation

The leader thread is highly utilized in all four accepted pairs; cumulative
Linux runqueue delay is only 2.9–14.2ms over their ~12s pulse windows.
Prolonged runnable-thread starvation is not evidenced in these runs.
This DOES NOT prove CPU-bound drawing, exclude blocking synchronization,
driver stalls or GPU contention, or identify the historic F06 freeze.

The 40ms scheduler sampler may miss short events and NVML GPU metrics
are device-wide; no GPU-specific completion timeline is measured.
Frame attribution ends at Wayland compositor Present feedback, not pixels
or input-to-photon. The historic ~0.5s Grid freeze was NOT reproduced.

## Gate disposition

- F09-A Focus fail-closed/native E3: PASS (interrupted trial excluded)
- F09-B CPU schedstat plus device NVML capture: PASS, covariates only
- F09-C Four paired List/Grid observations: E3 PASS, descriptive only
- F09-D Same-run historic freeze causation: OPEN
- F09-E CPU stack/flamegraph and GPU queue ownership: OPEN
- F09-F Premium E4, 75 distinct pixel-verified FPS and Spacedrive: OPEN

No production vendor promotion, merge or push. No benchmark CI Actions.

## CPU profiler follow-up (diagnostic, not matched pair)

The same experimental Cabinet binary was profiled in separate Grid and
List trials with a bounded perf record attached to its user-owned PID:
cpu-clock:u, 49 Hz, DWARF user-space stacks, 16s, no elevated privileges.

- Grid: 465 user-space CPU samples, 0 reported lost.
- List: 495 user-space CPU samples, 0 reported lost.
- Raw perf.data, perf.report, and native application evidence are retained.
- The top symbolized samples show GPUI geometry/layout, Taffy layout,
  text and scene construction, libc, and diffuse generic operations.
  No individual hot symbol accounts for the majority of samples.
- perf's 0.5% display threshold covers only ~23-24% of samples in
  listed rows; this is NOT a defensible total breakdown by subsystem.
- The List profiling run ended with a mismatch in restored active-window
  identity after the long profiler wait. Its user-space CPU samples remain
  usable as a qualitative profile; it is NOT another accepted paired
  focus-restoration qualification.

Profiling supports further investigation of CPU-side layout and scene
work; it does not identify an exclusive bottleneck, per-frame GPU wait
or the historic intermittent ~500ms freeze. A longer, profiled and
per-buffer-identity-controlled experiment is needed for causal closure.

## Independent oracle negative controls

First control run caught an oracle weakness: a forged feedback surface
outside the selected pulse chains was not rejected. No result from that
weak validator was promoted. The strengthened F09 oracle now validates
EVERY compositor request and Presented feedback for exact surface
identity, unique IDs, completeness and monotonic order. Replayed negative
controls reject forged clock IDs, forged surface IDs and non-monotonic
leader-thread schedstat. The failed initial control remains archived as
audit evidence, not hidden.
