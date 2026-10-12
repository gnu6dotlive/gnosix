# G07B-02D — Active GPUI Motion & Scheduler Causality
12 October 2026 · native nested Wayland · **bounded 2×2 differential PASS** · production **BLOCKED**

## Contract, provenance and isolation
Revisit the G07-B pointer-routing failure during *actual* GPUI `SharedMotion` animation, not manually scripted discrete regions. Compare the two existing scheduling policies — `on_next_frame` and the 13 ms timer — using separately built stock and candidate-patched **Hyprland v0.56.2** from upstream commit `efb50993780079460b0cbed1363e2166a2de1d9f`. Baseline: G07B-02C evidence commit `823bd62`. The existing experimental GPUI fixture is at commit `bb63742`; G07B-02D required **no new production code changes**.

Both compositor binaries have pinned SHA-256 in `receipt.json`, the same compiler/build provenance, and were executed only on the disposable nested socket `wayland-2`. The **parent Hyprland configuration was not changed**; the temporary nested window was floated and sized to **1250 × 800** through ephemeral window dispatches. Two independent GPUI layer-shell processes overlap at 900 × 560. At the point of initial setup, the physical pointer was parked over the witness before animation began. No physical motion was injected during subsequent animation/click observation.

## Actual animation evidence
The fixture uses the existing `matched_begin → matched_schedule → matched_tick` path. It retargets **21 times** (20 alternating targets after initial retarget) every 420 ms; `SharedMotion` computes changing geometry, GPUI calls `set_input_region`, and the compositor receives actual `wl_surface.commit` requests. The frame policy schedules through `window.on_next_frame`; timer policy uses the existing 13 ms async wakeup. All four runs logged `MATCHED_COMPLETE`.

| Compositor | Policy | Motion callbacks | Click injections | Strictly classifiable | Correct | Wrong | Intermediate fraction qualified / wrong |
|---|---|---:|---:|---:|---:|---:|---:|
| Patched | on_next_frame | 250 | 34 | 21 | **21** | 0 | 10 / 0 |
| Patched | timer | 585 | 34 | 23 | **23** | 0 | 13 / 0 |
| Stock | on_next_frame | 250 | 34 | 22 | 10 | **12** | 11 / 1 |
| Stock | timer | 584 | 34 | 21 | 8 | **13** | 15 / 10 |

**Combined:** patched 44/44 correct vs stock 18/43 correct and 25 wrong. Among strictly qualified events at intermediate animation fractions, patched 23/23 correct. Callback totals are observations only; do not extrapolate a performance benefit or suggest timer is inherently faster.

## Two-pass causality oracle
First pass: compare each separate `G07B_OVERLAY_CLICK` / `G07B_WITNESS_CLICK` counter with the expected region at start and end of injection. This produced 24/24 and 28/28 on patch; 10/25 and 10/30 on stock. **These are preliminary counts, not the final gate.**

Independent pass (`g07b02d_audit.py`): parse **all region events** in the interval around each injected click, check their timestamp order, require unchanged membership at the tested point, confirm exactly one timestamped native recipient, and reject events lacking a sufficiently close preceding region sample. This conservatively excludes **49 of 136 injected clicks** (24 on the patch, 25 on stock). The stricter numbers above are the ratified scoped gate.

Raw `WAYLAND_DEBUG=client` captures include real `wl_surface.set_input_region`, `commit`, `wl_pointer.enter`, `leave` and `button` events. They are preserved in `traces/*.tar.zst`, with hashes recorded in the machine receipt. There is **one patched overlay callback carrying `inside=false`** in a *non-qualified* transient sample. This is neither silently credited as PASS nor proof of an applied compositor-region violation; investigate the callback/region epoch separately.

## Evidence and constraints
- `receipt.json`: source/binary hashes, actual callback/commit counts, 2×2 observations, strict verdicts, limitations.
- `g07b02d_motion_probe.py`: nested native GPUI motion harness and event/click collector.
- `g07b02d_audit.py`: independent region/click classification.
- `traces/{patched-frame,patched-timer,stock-frame,stock-timer}.tar.zst`: original logs, independent per-run observations and audit.
- Every run restored its initial physical cursor location; all experimental compositor and GPUI processes were stopped. Only the primary Hyprland was left running.

## Verdict / precise gates
- **PASS:** real GPUI motion and two scheduling policies; isolated Wayland output; 2×2 stock/patch differential; correct routing of 44 qualified patched clicks including 23 intermediate-fraction samples.
- **BLOCKED:** G07-B runtime promotion. **NOT_RUN:** original 16-case dynamic matrix on both physical outputs, pointer grabs, drag/resize/double-click, focus under competing windows, timing/perf SLAs, compositor internal applied-region epoch trace and upstream acceptance. No source merge, push, system binary replacement or runtime rollout.

**Next: G07B-02E — Boundary Epoch & Pointer Grab Stress.** Establish compositor-side focus/region epoch logging, investigate the unqualified `inside=false` callback, and test moving boundaries at the input press/release interval, grab ownership, drag, and double-click. Then independently rerun the original two-output matrix. A patched compositor should not be promoted until these additional gates are met.
