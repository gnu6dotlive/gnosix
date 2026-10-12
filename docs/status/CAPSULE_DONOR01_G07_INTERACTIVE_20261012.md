# CAPSULE-DONOR-01 / G07 — Interactive Envelope qualification and promotion decision

**12 October 2026 · E3 scoped positive input / rapid interruption PASS · full click-through, E4, and production promotion OPEN**

## Authority and exact material

- Donor Capsule `v2.0.0` fixed at `6bac0208fb389e6ebba882dd3812230e64b99c5b`.
- Diagnostic Gnosix Envelope branch `probe/capsule-donor01-g07-interactive-20261012`, based on `ccc82d9`; only its opt-in qualification surface changes.
- Active Gnosix checkout `/home/tension_atoi/Projects/gnosix` remains unmodified. Existing untracked `docs/diagrams/` remains untouched.
- No `gpui-ce` wholesale migration, no Capsule theme reloader or unscoped shell execution.
- E3 measurements from `/mnt/workbench/capsule-donor-01/G07-INTERACTIVE-ORACLE.json`; independent negative tests at `g07-negative-v1/`.

## Verified physical hardware

| Hyprland output | Physical screen | Native mode | Transform | Captured screenshot |
|---|---|---|---:|---|
| `HDMI-A-1` | LG QHD | 2560×1440 at 74.964 Hz | 0 | 2560×1440 PNG |
| `DP-2` | Acer | 1920×1080 at 164.917 Hz | 1 (portrait transformation) | 1080×1920 PNG |

These are two distinct screens. `wp_presentation.refresh_ns` must not be used to guess which screen owns a surface; the test obtains exact output attribution from `hyprctl layers -j` using the unique `gnos.ux:dock:atlas-qualification` namespace and process ID. Monitor settings were not changed.

## G07-INPUT — Positive native hit-testing

The qualifying GPUI application explicitly calls `Window::set_input_region(Some(rectangles))`. Its runtime rectangles have been observed and validated:

- Open: main area `[30,392,870,496]` plus the bottom reveal strip `[0,496,900,501.3]` inside a 900×560 LayerShell surface.
- Hidden: reveal strip only, same coordinates.
- The compositor physically placed the LayerShell surface on the requested screen, including `DP-2`, and a bounded virtual `uinput` pointer moved only after checking the live window geometry and exact point.
- Across both monitors and both states: **six GPUI mouse-down events at accepted, mathematically inside coordinates**: open main and reveal, hidden reveal on each screen.
- All logged GPUI mouse-move events for those runs were within a currently valid input rectangle. Separately verified outside-region moves sent **no click**, to avoid interacting with unrelated user windows.
- Four full-screen native PNG captures preserved with correct output-dependent dimensions. This proves the screenshot files and native surface, not a user's subjective E4 acceptance or independent Wayland buffer identity.

**Important counterfinding (DP-2 synthetic first press):** two consecutive `ydotool` left-click commands were executed at the same verified main-rectangle point; **only one raw `wl_pointer::Event::Button` press/release pair** was received at the instrumented Wayland client (the third click on the reveal strip produced another raw pair). The GPUI handler received both raw presses it actually got. Initial single-click trials also failed at the first cross-monitor main press. The source of the missing synthetic event is therefore **upstream of the instrumented GPUI Wayland-button dispatch**, not demonstrated to be a geometry-solver bug. Compositor focus handoff versus virtual `uinput` device behavior remains unisolated. This does not establish a physical-mouse bug; full click-through requires an independent recipient behind the surface.

The temporary `ydotoold` process was terminated, its owned socket removed, and the virtual cursor restored during each run. No destructive outside click was executed.

## G07-INTERRUPT — Real native rapid reversals

Four native trials, `HDMI-A-1` and `DP-2` × `timer 13ms` and `on_next_frame`, used the **same experimental binary** SHA-256 `591a6546ca943aa198a0434885a5d751e89f0282c30d3e6301395e97f81f847b`, the same original Gnosix `SharedMotion`, and a deterministic 13-state/12-retarget pattern with target intervals **160.1–161.5ms**. This forces repeated reversal before settling.

| Output | Scheduler | Native motion callbacks | Rapid retargets |
|---|---|---:|---:|
| HDMI-A-1 | timer | 182 | 12 |
| HDMI-A-1 | on_next_frame | 62 | 12 |
| DP-2 | timer | 163 | 12 |
| DP-2 | on_next_frame | 68 | 12 |

Across 48 reversals: position and velocity *immediately before and after the zero-time retarget* are identical at nine-decimal telemetry precision. No NaNs, out-of-bounds `SharedMotion` values, or input rectangles outside the 900×560 envelope were accepted. Every run included an actual native compositor layer and image capture. Fewer callbacks do **not** by themselves imply smoother physical presentation or lower CPU/power consumption.

The exact measured binary is archived under `g07-archive/g07-interrupt-native.bin.zst` with SHA-256 `1e1ba98f986fadde48ac7580bd886530638839a98c789e865dcb2e61703b5112`. The raw Wayland button trace is a separate reversible GPUI vendor patch `tools/probes/capsule_donor01_g07_raw_button.patch`; this vendor modification is **not** incorporated into the branch.

## Independent gates and limits

- **E2 native source and tests:** Gnosix `gnos-ux-gpui` 218/218 PASS, `gnos-ux-surface` 136/136 PASS; strict Clippy and cargo fmt PASS.
- **E3 scoped input+motion:** 6 validated GPUI clicks, 4 native interruption runs, 48 continuous retargets, 8/8 negative mutation controls fail closed.
- **OPEN:** demonstrated click-through into an independent underlying application, complete Fused/Bridged/Detached topology in live native compositor, 1.25×/2× DPI matrix, frame serial→GPU submission→Wayland buffer/pixel identity and photon timing, independent CPU/power benchmark, user E4 perceptual acceptance.
- **Known additional issue:** a missing *first synthetic* press on DP-2, not yet attributed at compositor/virtual input boundary. Must not claim solved.

## G07 integration decision

**ADAPT Envelope in experimental staging only.** The active-only `on_next_frame` technique fits the existing `SharedMotion` while preserving Gnosix's solver, clipping and authority, but the current tests do not authorize a production promotion or E4 acceptance. An appropriately narrow opt-in integration can be prepared after first-click/underlying-window tests and real operator review.

**Cabinet HOLD.** Previous matched 75Hz tests showed no meaningful p95 advantage. Do not transplant this scheduling policy there without an independent need.

**No merge, no push, no change to live shell, monitor configuration or Gnosix production code.**
