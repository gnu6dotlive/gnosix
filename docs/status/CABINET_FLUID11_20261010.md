# CABINET-FLUID-01 / F11 — Source, queue and compositor attribution

**2026-10-10 · E3 partial PASS. F11 overall, async GPU timing and premium E4 OPEN.**

Product parent 51f3616; diagnostic parent 3d34781. All code instrumentation is in an isolated F11 worktree and optional environment-variable path. Product GPUI dependencies stay unchanged.

## Qualified instrumentation

The in-process source state marker (visible header serial) is written into a scoped GPUI render-thread-local when Cabinet constructs its element tree. GPUI clears stale tokens at the beginning of root drawing and logs the completed root serial. The Wayland backend attaches the same serial to a unique feedback request and exact wl_surface. The wgpu backend binds the request and serial to a unique SubmissionIndex in queue.submit; the frame.present CPU call logs them too. The compositor wp_presentation feedback and wgpu Queue::on_submitted_work_done callback are reconciled independently.

The callback elapsed time **includes work-done callback polling and delivery**, not just GPU hardware execution. The source marker is **not a pixel readback**. Compositor Presented is **not a photon timestamp**. Missing terminal GPU callbacks are kept OPEN, not counted as successful.

## Actual overlap correction

Historical F09 and F10 traffic was genuinely overlapping, despite the shell printing the F08_FINITE_STIMULUS_FINISHED message after its keyboard loop. The F09 Grid A raw timestamps show 260/260 keyboard events occurring during the visual pulse interval. However, startup/focus changes the actual overlap between runs. F11 therefore introduced an opt-in arm-file barrier: Cabinet waits for a trusted test-local signal created only after the mode, pointer and focus are qualified. An independent oracle checks the real monotonic intervals and refuses to label trials fully concurrent below 80% keyboard/pulse overlap.

## Native Wayland controlled trials, 100001 physical files

Four runs, order Grid → List → List → Grid, with 450 source-visible mutations and 260 compositor-routed keyboard events per run.

| Run | Keys during pulses | Feedbacks = GPU submissions = compositor presentations | Distinct nonzero source serials attached to successful presentations | GPU work-done callbacks |
|---|---:|---:|---:|---:|
| Grid 1 | 240/260 | 347 | 287/450 | 346 |
| List 1 | 241/260 | 339 | 288/450 | 338 |
| List 2 | 241/260 | 335 | 288/450 | 334 |
| Grid 2 | 240/260 | 342 | 285/450 | 341 |

The callback of exactly one terminal submission was not observed before shutdown in each run; no GPU failure or stall may be inferred. Work-done callback p95 ranged around 57–59 ms, which is NOT a measured GPU execution duration.

The number of distinct source serials is close for Grid and List under this controlled start. Differences in earlier experiments vary by protocol and trial. A source serial attached to a compositor presentation is logical source/frame attribution, not proof that this serial's pixels were on screen.

Earlier unsynchronized trials remain preserved but must be described using measured overlap. One short List run had only 33/80 keyboard events overlapping the pulse interval and is not qualified as a full concurrent workload.

## Independent controls

Five intentional trace corruptions must all fail closed: forged source serial at GPU submit; mismatched compositor wl_surface; removed queue submit; multiple missing queue completion callbacks; mismatched GPUI root serial immediately preceding the frame request. Every control must fail for its expected identity/completeness guard, not a missing path or unrelated parser failure.

## Boundaries and open gates

- E3 source → GPUI root → Wayland request → wgpu submission → CPU present → compositor Presented: PASS.
- API queue-completion event: OBSERVED; callback timing is not a GPU hardware timestamp.
- Concurrent stimulus: PASS for the four controlled trials, based on actual timestamps.
- Independently decoded per-frame pixel identity: OPEN.
- Hardware GPU timestamps, queue occupancy and pipeline timeline: OPEN.
- Reproduction and causal attribution of historical ~500ms Grid stall: OPEN.
- Sustained distinct 75 physical FPS, Spacedrive same-task visual comparison, and operator premium E4: OPEN.

The experimental branch is not merged or pushed. No permanent Hyprland or GPUI production configuration changes.
