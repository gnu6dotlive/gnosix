# CAPSULE-DONOR-01 W02-E3 — Native Envelope motion and quiescence

**2026-10-11 · Bounded native GPUI motion and no-heartbeat behavior PASS; full compositor E3 OPEN.**

- Worktree `/mnt/workbench/capsule-donor-01/w02-envelope`, base `007045e`. The native qualification executable `gnos-ux-dock-envelope-qualify` has an opt-in `GNOS_ENVELOPE_DONOR01_NATIVE=1` driver, distinct from the active bar/dock product.
- Uses the **existing canonical `SharedMotion`** (analytic second-order critically damped motion) and `Window::on_next_frame`; no Capsule morph curves or dependency graft. The live driver maps `SharedMotion.vertical.value` into an existing DockQualification visual fraction and its input-region projection.
- First exploration: 110 native frame callbacks in ~4.03 seconds, but was **rejected as active-only scheduling evidence** because it queued frames while settled.
- Corrected Hyprland trial: **103 bounded frame callbacks**, **six settled idle intervals with no callback loop**, six explicit 240 ms delayed target re-arms, and 5,031,511 µs to the terminal trace. Native GPUI compiled successfully and produced no app panic.
- Native input regions were recomputed through the existing `resolve_dock_frame`, `project_dock_pointer_regions`, `window.set_input_region` and `window.set_exclusive_zone(0)` mechanisms during rendering. This is not an independent pointer click-through study or full ReactiveEnvelopeSolver Wayland qualification.
- Independent source oracle asserts sequential callback IDs, bounded motion values, exact idle/rearm count, alternating targets and terminal state. Negative controls reject forged event omissions.
- W02 analytic motion stress prior checkpoint: 3/3 new tests plus full GPUI 218/218 and surface 136/136.

**Important limitation:** the native visual demo is the pre-existing DockQualification surface driven by a `SharedMotion` channel. It does **not** establish complete Fused/Bridged/Detached composited topology, per-buffer rendered-pixel identity, input-to-photon latency, 75 Hz physical presentation rate, output DPI matrix or premium UX acceptance. The idle intervals are logged application callback quiescence, not measured system-wide CPU-idle power.

No product promotion, merge, push or persistent host configuration changes.
