# CAPSULE-DONOR-01 / W02 — Envelope animation and topology

**2026-10-11 · existing Gnosix physics E2 PASS; donor scheduling adoption still OPEN.**

Reference Capsule `v2.0.0` at `6bac0208`, `new_capsule/animator.rs` (fixed duration, `apple_island_morph`, interrupted retarget starts from current dimensions) and `new_capsule/module/mod.rs` (the active-only `on_next_frame` callback chain).

**Target baseline:** Gnosix `007045e` in `gnos-ux-surface` and `gnos-ux-gpui`. The existing `ReactiveEnvelopeSolver` and `SharedMotion::advance_observed` already preserve current phase and velocity across zero-time retargets, respect elapsed wall-clock time, converge to exact endpoints, and expose `needs_frame`. Existing topology distinguishes Fused/Bridged/Detached; safe paint/input intersections and material ownership are part of Gnosix semantics.

## Execution

- Baseline `cargo test --offline -p gnos-ux-surface --lib`: **136/136 PASS**.
- Added W02-only tests in `crates/gnos-ux-gpui/src/product_shared_envelope.rs`: **3/3 PASS**, then complete GPUI crate suite **218/218 PASS**.
- Retarget stress: 3,600 frame steps and 500+ inversions with an artificial 2-hour idle before each new target; zero-time retarget preserves phase and velocity. Every sampled input region corner remained inside the painted envelope.
- Settlement: no heartbeat for 1,000 repeated checks at each endpoint; reduced-motion transitions terminate immediately.
- Invalid elapsed (NaN, infinities, negative) fails closed with no mutation of motion state.

Evidence: `/mnt/workbench/capsule-donor-01/w02-surface-baseline.log`, `w02-envelope-targeted.log`, `w02-gpui-full.log`.

## Decision boundary

**RETAIN:** native analytic second-order physics, topology transitions, material ownership, input-region clipping and semantics.
**ADAPT:** Capsule's transition lifecycle and active-only frame callback **only if native Wayland measurements prove a real gain**.
**REJECT:** replacement of the analytic solver or current input topology by Capsule's width/height morph curves.

This is E2 code/test qualification on Gnosix, not an E3 comparison of compositor frames or E4 signoff. W02 remains a diagnostic worktree; production `gnosix` is untouched, including its preexisting untracked `docs/diagrams/`.
