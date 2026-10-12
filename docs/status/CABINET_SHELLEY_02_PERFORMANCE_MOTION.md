# CABINET-SHELLEY-02 — Native 4096-item virtualization and live motion

Date: 2026-10-10. Branch: feat/cabinet-shelley-01-20261010.
Status: isolated engineering qualification; no public publication, merge or deployment.

## Implemented

- The GPUI-pre 0.3.5 uniform_list callback renders only requested index ranges.
- Persistent UniformListScrollHandle handles pointer scroll and keyboard cursor movement.
- End navigates to the final visible item, scrolling it into the viewport.
- F8 and the inspector's keyboard-accessible button control an animated metadata reveal.
- The animation controller samples before retargeting, so reversal has no position jump.
- GPUI request_animation_frame is issued only while the animation remains active.
- GPUI reduce_motion is passed to the controller; immediate snap has a unit-test proof.
- CABINET_VLIST_METRICS=1 enables bounded, atomic laboratory telemetry. The source
  does no synchronous logging or filesystem IO from the row rendering callback.
- No new crate or renderer dependencies; Cargo.lock is unchanged.

## Real 4096-item Wayland gate

Evidence: /mnt/workbench/evidence/runs/cabinet-shelley02-4096-last-20261010-v1/
- 4096 fixture files; last sorted file item-4094.dat selected and returned exactly.
- Max uniform_list range materialization: 10 rows.
- Range callback count: 48; total rows materialized across callbacks: 192.
- Highest reached range end: 4096.
- Longest row callback: 155 microseconds, one laboratory execution.
- These are CPU row-construction metrics, NOT full GUI frame times or GPU timings.
- Explicit 4096 read_dir scan limit stays intact. No filesystem pagination or indexing.
- The isolated test injected actual keyboard events and verified typed JSON output.

## Real motion gate

Evidence: /mnt/workbench/evidence/runs/cabinet-shelley02-inspector-motion-20261010-v1/
- Real Weston/Xvfb session, selected file and native F8 key events.
- Inspector screenshots: before, intermediate, expanded, collapsed and interrupted.
- Pixel difference in inspector ROI: opening 2,922,993; closing 2,922,993.
- Intermediate frame differs from the initial frame by 2,718,464 (RGB-distance units).
- Return after rapid F8 reversal: zero pixel difference in tested ROI.
- Explicit outcome Cancel, exact comparison, and cleanup PASS.
- This establishes visual change, not a guaranteed frame rate or perceptual quality.

## Regression and quality boundaries

- Rust crate: 43 tests PASS. Strict all-target Clippy and offline native GPUI build PASS.
- OpenFile Tab/Shift+Tab focus order changed because the new metadata button is
  an actual keyboard stop. The initial legacy Tab oracle returned Cancel (FAIL).
  Requalified sequence passes through the metadata button before Open.
- Full repository hermetic cargo xtask ci: **48/48 PASS**, 0 non-PASS.
  Evidence: /mnt/workbench/evidence/runs/cabinet-shelley02-20261010-final-ci/.
- FPS, frame-time p95, allocation trends, 4K+ filesystem paging, AT-SPI and
  real user acceptance on the primary Hyprland session remain OPEN.
- This technical worktree remains local. Shelley private source was read-only.
  The separate MIT/relicensing worker exclusively owns distribution decisions.
## Reproduce (fresh evidence paths required)

- Build through gnu6-lab-run --exclusive under the qualified Cargo.lock.
- Run tools/cabinet-gfc03d-x11-input-oracle.sh ux-large-last against the exact binary.
- Run tools/cabinet-gfc03d-x11-input-oracle.sh ux-motion with a fresh evidence directory.
- Preserve source HEAD and SHA-256 of the binaries in each evidence receipt.
- Do not change the active GTK FileChooser or the operator's Hyprland configuration.
- Do not promote this tranche as complete Shelley, production XDG, or 60/120 FPS.
