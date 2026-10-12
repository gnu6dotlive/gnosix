# CABINET-FLUID-01 / F07 — Frame-loop stall attribution and 2x scale

**E3 SCOPED / MAIN UX ACCEPTANCE OPEN · 2026-10-10**

Source parent: e25bddd (F06 optional diagnostic). This is a separate
diagnostic worktree, not a production GPUI dependency promotion.

## Target

1. Determine whether the single prior Grid run with p95 374.851ms
   model-to-compositor-present was caused by a Wayland frame callback,
   GPUI scheduling state, application tree building, or the graphics driver.
2. Qualify actual Wayland integer output scale=2 with native GPUI, route,
   state restore and visual image parity, without modifying real Hyprland.

## Scoped GPUI frame-loop instrumentation

Optional CABINET_F07_LOOP_TRACE=1 emits monotonic timestamps, state,
pending wl_surface.frame callback flag, redraw latch and presentation state
at schedule_frame, frame, complete_frame, callback, scheduler and retry
ticks, redraw request and renderer draw entry. Core frame transitions remain
unchanged. The F06 model/view/request/present timestamps are retained and
analyzed independently.

New oracle: tools/cabinet-fluid07-loop-oracle.py.

### Native Hyprland stress runs (100001 physical empty files)

Three additional real Grid runs on HDMI-A-1 / scale 1, in a temporary
Cabinet window under Hyprland 0.56.2. Native keyboard loop focus is asserted,
and original active-window identity is restored on completion.

| Run | Event count | Model-to-next-feedback-REQUEST p95 | p99 | Max | Loop trace count |
|---|---:|---:|---:|---:|---:|
| Grid F07-1, nominal 20ms step | 260 | 11.085ms | 12.404ms | 12.817ms | 4271 |
| Grid F07-2, nominal 12ms step | 260 | 12.650ms | 17.272ms | 20.225ms | 3328 |
| Grid F07-3, nominal 8ms step | 400 | 21.036ms | 21.695ms | 29.174ms | 4234 |

All 920 input mutations had bounded next feedback requests and no
observed model-to-request stalls exceeding 75ms. Most loop samples in
the runs were in AwaitingCallback (normal compositor pacing). This does
not establish that AwaitingCallback was the cause of the earlier
intermittent stall. It remains an OPEN regression, because it was not
reproduced while instrumented.

The original uninstrumented Grid F06 first run remains a counterexample:
250/260 model updates correlated, 122 coalesced inputs, p95 374.851ms
and p99 450.236ms. Its long delay lay between view build and subsequent
frame request; root cause cannot be identified retroactively without
equivalent loop-state traces.

No frame pacing claim at 75Hz or input-to-photon guarantee.

## Scoped Wayland 2x

An independent Xvfb + Weston/X11 Pixman compositor was configured with
output X1 at mode=1100x820, scale=2, producing a 2200x1640 host window.
Native Cabinet logs contained wl_output.scale(2); the routes, tab
persistence and app restart passed. No host monitor DPI/scale or
Hyprland configuration was changed.

- The first 2x trial with too-large host Weston output was visually
  cropped; it is rejected for visual qualification.
- At 2200x1640 a strict full-compositor screenshot oracle rejected raw
  post-restart equality because Weston moved app content 6 physical
  pixels vertically. We did not overwrite the negative result.
- Scoped interior-only comparison, with shift disclosed and excluding
  compositor chrome, showed 99.98107% exact RGB pixel equality;
  sidebar 100%; main area 99.97379%; central area 99.94769%.
- E3 app-interior aligned parity only, not full-frame pixel parity or
  E4 operator approval. Fractional scale and 2x input performance OPEN.

Evidence roots:
- /mnt/workbench/evidence/runs/cabinet-fluid07-loop-grid-100001-v1
- /mnt/workbench/evidence/runs/cabinet-fluid07-loop-grid-100001-v2
- /mnt/workbench/evidence/runs/cabinet-fluid07-loop-grid-100001-v3
- /mnt/workbench/evidence/runs/cabinet-fluid07-loop-analysis-v1.json
- /mnt/workbench/evidence/runs/cabinet-fluid07-scale2-weston-x11-v1 (cropped)
- /mnt/workbench/evidence/runs/cabinet-fluid07-scale2-weston-x11-v2 (2x)
- /mnt/workbench/evidence/runs/cabinet-fluid07-scale2-weston-x11-v2/aligned-parity.json

## Gates still open

- Trigger and attribute intermittent frame loop stall with equivalent
  F07 logs (GPUI callback, rendering submission, compositor or scheduler).
- Continuous redraw/present at a controlled 75Hz stimulus, not
  merely 20ms/12ms/8ms dispatch sleeps.
- Confirm 2x accessibility, content reachability, inspector drag, keyboard
  focus and fractional scaling.
- Independent Spacedrive same-workload comparison and operator E4 acceptance.

F07 instrumentation is controlled evidence tooling only. No production
vendor patch, no merge, no push.
