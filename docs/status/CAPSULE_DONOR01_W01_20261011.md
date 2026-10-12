# CAPSULE-DONOR-01 / W01 — Cabinet scheduling qualification

**2026-10-11 · E2 candidate mechanism PASS; native integration E3 OPEN.**

## Source attribution

Capsule `v2.0.0` at `6bac0208`, `crates/app/src/new_capsule/module/mod.rs:495–529`.
The donor queues an `on_next_frame` callback only when `animation_frame_pending` is false and animation is active. The callback checks whether the current window matches the source window, advances the animator on observed `Instant::now()`, updates window geometry only when needed, and schedules another callback while activity remains.

Cabinet baseline: `c5779c5`, read-only file manager with existing F01–F12 input→model→GPU→Wayland evidence. No `on_next_frame` integration currently exists in its `cabinet.rs`; direct `cx.notify()` is used for model changes.

## Qualified candidate

`tools/probes/capsule_donor01_frame_gate.rs` is a newly authored **pure Rust scheduler gate prototype**, deliberately not incorporated into any Cabinet product module. It models the bounded scheduling part of the donor without copying the Capsule GPUI runtime or its visual curve. It prevents duplicate frame requests, declines idle requests, preserves the latest retarget behind one queued callback, and ignores a callback belonging to a replaced window.

Seven deterministic native `rustc --test` checks passed:
- no frame demand after 100,000 idle schedule attempts;
- 10,001 retargets coalesce into one outstanding callback;
- exactly one pending callback throughout 1,000 active frames;
- no heartbeat after settling;
- stale callback rejected on recreated window;
- stopping with a pending callback does not restart animation;
- fresh animation rearms once after previous settlement.

Evidence: `/mnt/workbench/capsule-donor-01/w01-gate-tests.log` (7/7 PASS, SHA-256 `c0cb4b3c4f128b7b872d25f459b094cc8ae6a3bc66e4e2eb83113def5250b372`).
Unmodified baseline Cabinet library: 88 tests PASS, 4 ignored (`w01-cabinet-baseline.log`).

## Boundaries and recommendation

**ADAPT**, do not cherry-pick directly. The test validates a stand-alone bounded algorithm, **not the real GPUI event loop, Wayland frame pacing, a runtime window recreation, or UI motion**. Integrating it into Cabinet would require a proper product-owned state machine, real `on_next_frame` wiring and a full measured native workload with idle, interruption, 100k+ files, scaled displays and controlled input. The source `gpui-pre 0.3.5` already has `Window::on_next_frame`; replacing the GPUI fork is not a prerequisite.

No merge, push, runtime rewrite or mutation of the GTK FileChooser.
