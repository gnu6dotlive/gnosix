# CABINET-SHELLEY-02 — Virtualized GPUI rows and interruptible motion primitive

Local qualification: 2026-10-10.
Source baseline: feat/cabinet-shelley-01-20261010 @ b63785b.
Donor design reference: private shelly-gpui @ 6fce3747, state/motion.rs and views/package_workstation.rs.
Status: **PARTIAL TECHNICAL PASS**; motion behavior in live window and performance/FPS are NOT qualified.

## What changed

- Replaced eager per-frame construction of all visible-file rows in the Cabinet chooser with the native gpui-pre 0.3.5 uniform_list API.
- A stable UniformListScrollHandle is retained by the view; cursor movement requests nearest-item scrolling. Directory scans reset scroll to the first item.
- Render requests materialize only the range of rows requested by GPUI. Selection, keyboard handlers, hover feedback and native spring indicators are preserved within materialized rows.
- Kept the 4096-entry directory scan cap and explicit truncation flag. Virtualization of row rendering is **not** a filesystem paging or indexing implementation.
- Added cabinet_motion.rs, a pure scalar controller with sample-before-retarget reversal, finite-target validation, terminal snap, and reduced-motion snap. This is an independently adapted behavior model, not a copied Shelley GPUI view.
- Motion controller is **not yet connected to a live panel animation or GPUI frame task**; the native existing GPUI springs remain the live animation path.

## Gates

- 43 Rust tests PASS (includes 3 pure motion tests).
- All-target Clippy -D warnings PASS.
- Offline/locked native GPUI build PASS.
- Existing real Wayland E2E interaction oracles on virtualized build: Cancel, SaveFile, OpenFile keyboard, Tab, and search PASS. Binary evidence: /mnt/workbench/evidence/runs/cabinet-shelley02-vlist-binary-20261010/.
- Full repository hermetic CI: NOT YET RUN against this change.
- 4096-entry performance, frame-time distribution, memory allocation and scroll to near-last item: OPEN.
- Accessibility for virtual rows, focus restoration on recycling and high-rate motion interruption under real pointer input: OPEN.
- Real Shelley-02 visual review on Hyprland: OPEN.

## Source and publication constraints

This branch is a local engineering worktree. Neither the private Shelley donor nor the active XDG GTK chooser was changed. No additional GPUI/Blade or other donor crates were introduced. Licence transition is handled by a separate worker; do not publish, merge or deploy the ported branch before provenance/licence approval.

## Next gates

1. Real GPUI scrolling through a fixture with at least 4096 entries and viewport-bound row materialization instrumentation.
2. Frame-time and allocation tracing at reference desktop scale, including inspector resizing while scrolling.
3. Integrate interruptible motion with frame request semantics and respect reduced-motion from GPUI settings, then verify pixel-level reversals.
4. Full xtask hermetic CI; final-source commit and post-commit validation; visual acceptance on Hyprland.
