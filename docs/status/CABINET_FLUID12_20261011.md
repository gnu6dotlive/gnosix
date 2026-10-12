# CABINET-FLUID-01 / F12 — Independent compositor pixel marker

**2026-10-11 · F12-A BOUNDED E3 PIXEL SAMPLE PASS; F12 full, hardware GPU timeline and premium E4 OPEN.**

Product baseline `eebadb1`; diagnostic parent `4c2ab29`. The experimental worktree stages both previous GPUI diagnostic crates from hash-verified F10 sources and F11 patches. No GPUI production modifications.

## Visual oracle

A strictly opt-in Cabinet header strip adds two fully saturated green synchronization tiles followed by ten pure-black/pure-white binary tiles encoding the integer source serial 0–900. The marker is part of the actual GPUI-rendered UI, not overlaid onto screenshots by the oracle. The existing F11 chain independently binds the source serial to the GPU command submission and Wayland compositor presentation feedback.

Under native Hyprland on the NVIDIA RTX 3070, with 100001 genuine temporary files and the source-visible 75Hz stimulus, the harness captures 30 screenshots directly via `grim` during animation. An independent Pillow decoder discovers the green synchronization tiles in a bounded header ROI, then reads exact pixels of the binary tiles, rejects any nonbinary color or malformed sentinel, and checks every decoded number against the submission log. This deliberately uses no OCR or app source-model values as decoding input.

| Evidence | Grid | List |
|---|---:|---:|
| Compositor PNGs decoded | 30/30 | 30/30 |
| Unique pixel-decoded source states | 30 | 30 |
| First decoded serial | 5 | 4 |
| Last decoded serial | 330 | 327 |
| Every decoded state in submitted wgpu source serial set | PASS | PASS |
| Monotonic sampled source states | PASS | PASS |
| 100001 real files, finite 450 updates, 260 keyboard events | YES | YES |

Four deliberate corruptions of the Grid sample set (missing synchronization sentinel, a nonbinary tile, a valid-looking but out-of-range/unsubmitted serial, and a missing PNG) all fail closed and preserve original evidence.

## Claims boundary

This is an **independent pixel observation of composited screenshots**, not an independently decoded serial from EVERY submitted/presented GPU buffer. A screenshot may reflect a compositor output state at a sampling time; it does not identify every Wayland presentation or the GPU buffer underlying it. The source-serial lookup into wgpu logs checks equality of values, not strict one-to-one wl_buffer identity.

This does not measure photons, motion-to-photon latency, sustained distinct 75 FPS, GPU command duration or driver queue residence. GPU `on_submitted_work_done` callbacks from F11 are not hardware timestamps and may include polling/delivery overhead. Do not claim GPU timeline completion merely from callback presence.

## Disposition

- **F12-A opt-in rendered pixel marker and independent screenshot decoder:** E3 PASS for 60 samples under two native modes.
- **F12-A negative controls:** 4/4 PASS.
- **F12-B hardware GPU timestamp query with explicit adapter feature negotiation, calibrated timestamp period, and nonblocking readback:** OPEN; no completed GPU timestamp experiment in this checkpoint.
- **F12-C per-buffer end-to-end pixel serial / scanout time:** OPEN.
- **Historical Grid 0.5s stall reproduction:** OPEN.
- **E4 operator Spacedrive-like fluidity / premium visual acceptance:** OPEN.

No merge, push, GTK portal change, display configuration change or production GPU dependency replacement.
