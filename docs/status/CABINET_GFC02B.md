# Cabinet GFC-02B — Async browsing and isolated Wayland qualification

Date: 2026-10-10. Scope: new functionality in the Cabinet native UI; NOT deployed.

## Implemented
- Filesystem enumeration moved from GPUI UI thread to GPUI background executor.
- Generation fencing prevents stale directory scans overwriting newer navigation.
- Explicit bounded directory listing (4096 entries), folder-first stable sort, paths preserved.
- Keyboard: arrow up/down, Enter, Backspace (parent), Escape (cancel); initial focus.
- Scrolling container bounds corrected to prevent overlap with status/actions.
- Unit tests: directory ordering, missing-folder fail-closed, 4097-entry truncation.
- Existing GFC-02 per-request backend, private stdio renderer contract, and host portal settings unchanged.

## Qualification
- 11 Rust tests PASS; strict Clippy (-D warnings) PASS; GPUI debug build PASS.
- Weston 15 headless with --fake-seat, private Wayland socket, opt-in GL backend:
  Cabinet survives initial startup, creates xdg_surface/xdg_toplevel,
  uses app_id in.gnu.os.cabinet, and acknowledges configure.
- Weston screenshot authorized only after --debug. Native compositor screenshot
  exhibits vertical GL readback inversion; a separately produced FLIP_TOP_BOTTOM
  image is for visual inspection only, not the raw screenshot. The corrected
  image demonstrates visible contents, scrolling region bounds and actions.
- Raw/corrected screenshot and Wayland debug logs:
  /mnt/workbench/evidence/runs/cabinet-gfc02b-layout-fixed-20261010/
- First Weston run without --fake-seat panicked in vendored GPUI seat.unwrap().
  Second with --fake-seat succeeded; don't patch vendored GPUI to mask missing seat.
- The first screenshot with --debug absent was black and reported unauthorized,
  so it is not a visual proof. Later --debug run produced a nonempty image.
- No host services restarted, no changes to GTK FileChooser default, no push.

## Remaining gates
- Real click/keyboard interaction under Wayland NOT tested; no result-end-to-end with native GPUI.
- Parent-window modality, complex filters, writable/choices, SaveFile/SaveFiles,
  Flatpak document grants, actual callers and accessibility still OPEN.
- At 4096 entries browsing is limited rather than searchable; pagination/search
  required for directories with many files.
- The compositor screenshot is not a substitute for real UX qualification.

## Reproduce
From the Gnosix worktree:

    gnu6-lab-run --check
    gnu6-lab-run --exclusive -- cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    gnu6-lab-run --exclusive -- cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    gnu6-lab-run --exclusive -- bash tools/cabinet-headless-smoke.sh

The smoke script uses a private socket and should not access the operator's active compositor.
