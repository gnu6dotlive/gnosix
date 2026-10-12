# CAPSULE-DONOR-01 W01-E3 — Native GPUI scheduling experiment

**2026-10-11 · Bounded native application callback proof PASS; compositor presentation/FPS E3 gate OPEN.**

- Worktree: `/mnt/workbench/capsule-donor-01/w01-cabinet`, based on `c5779c5`.
- Opt-in only: `CABINET_DONOR01_NATIVE=1`, native `cabinet` GPUI window, internal three-second / 240-callback upper bound, no file mutation.
- A `Window::on_next_frame` callback uses a weak Cabinet entity, checks the source window handle, prevents a second pending callback and calls `cx.notify()` during the stimulus. This is a deliberately limited frame-loop probe, **not a generalized product motion subsystem**.
- Successful native Wayland/Hyprland trial: **240 sequential GPUI frame callbacks in 2,942,906 µs**, no app panic. Process was terminated by bounded `timeout` only after the finite frame stimulus concluded; exit code 124 therefore does not denote failure of the bounded stimulus.
- Input corpus was the checked-out Capsule source directory; **this trial does not meet a 100,001-file workload gate**.
- Independent oracle verifies exact 1–240 sequence, positive monotonic elapsed times and matching terminal count. Five total negative traces across both W01 and W02 reject missing events, mismatched completion and false retarget assertions.
- Weston headless control: launch intentionally failed because `gpui-pre-linux` currently unwraps `wl_seat` even when compositor is headless. This is a **backend compatibility limitation**, not proof that the donor algorithm fails. The private Weston compositor was terminated afterward.
- Standalone pure scheduler candidate was previously verified 7/7; Cabinet library baseline 88 PASS / 4 ignored.

**Open before product integration:** frame source→compositor Presented chain, true idle CPU/frame measurement, controlled 100k file navigation comparison, keyboard responsiveness, window recreation and output scale variants. Callback count is **not** physical refresh FPS.

No merge/push, persistent service change, compositor configuration mutation or production GPUI vendoring.
