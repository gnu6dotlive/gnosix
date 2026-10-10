# Cabinet UX-01 — Native, stateful, animated shell

Date: 2026-10-10. Branch: feat/gfc-01-filechooser-portal-20261010.
Status: local engineering qualification; not deployed, installed, or set as host FileChooser.

## What was implemented

- Real Rust CabinetNavigation state owns current directory plus bounded back/forward history. New navigation invalidates forward branches. Up and crumbs navigate real existing absolute directories.
- Quick places Home, Documents, Downloads, Pictures, Projects are only shown when the corresponding directory exists. No invented locations or provider authority.
- The top bar exposes Back, Forward, Up, clickable path crumbs, a native GPUI search field with Ctrl+F, and hidden-file visibility toggle via UI or F7.
- FilenameInput's real GPUI EntityInputHandler supports a configurable accessible placeholder; search uses the same native text pathway as SaveFile, not keystroke string accumulation.
- Search is a live, case-folded, per-directory filter over already loaded metadata. It is NOT indexed global search, a filesystem watcher, or a content search engine.
- Async background folder scans retain epoch fencing. Directory changes retain a single source of truth and do not block the GPUI UI thread.
- File rows render hover/press feedback and native GPUI SpringAnimation on the active indicator (230, 22, 1). Navigation/content mounts have a 175ms GPUI Animation eased with ease_out_quint. GPUI honors reduced-motion preferences.
- Layout is responsive: the places sidebar is removed when the window is less than 710 physical GPUI pixels wide; the file region expands. Default window request is 1020x740. This is not a guarantee that a tiling compositor will grant the requested bounds.
- The shell explicitly sets font family Montreal, available on the reference workstation, while retaining the signal-orange accent.
- Keyboard accessibility: F3 focuses the file-list region; after F3, Tab traverses native controls. Ctrl+F focuses search, Down exits search to the list. Alt+Left/Right navigates history. Key focus is visible without geometry-changing borders.
- An existing defect was fixed: Enter on a selected file row now activates the same selection/confirmation logic as the list, rather than re-selecting forever.

## Evidence and qualification

- 34 portal crate Rust unit tests PASS, including three navigation/search tests.
- Offline GPUI build, cargo check and Clippy -D warnings PASS.
- Nine real independent Weston/Xvfb Wayland E2E paths PASS from the consolidated oracle:
  cancel; save; open-keyboard; open-tab; open-shift-tab; open-pointer; ux-hidden; ux-history; ux-search.
- The independent oracle injects real XTest events into a private Xvfb display hosting Weston X11, observes Wayland keyboard/pointer events, and checks exact typed renderer JSON outputs. It never uses the host Hyprland input seat.
- E2E evidence receipts: /mnt/workbench/evidence/runs/cabinet-ux01-final-{mode}-20261010/ (replace mode with one of the nine listed above).
- First native screenshot with the new shell: /mnt/workbench/evidence/runs/cabinet-ux01-headless-preview-20261010/cabinet-ux01-upright.png.
- Search evidence shows beta.txt as the sole visible result. Original oracle failures v1-v3 were investigated: initial clicks were outside the row or Enter on a selected file row only re-selected. Corrected v4 and final requalification are PASS.
- The GPUI animation APIs are verified compiled, but no real frame-time benchmark or high-refresh FPS claim is made.
- Full repository hermetic xtask CI: **48/48 PASS**, evidence /mnt/workbench/evidence/runs/cabinet-ux01-20261010-full-ci/.

## Product and architecture boundaries

- Cabinet is currently a chooser renderer with typed XDG response and bounded filesystem authority. It does not rename, delete, copy, move, index, mount or manage files.
- Spacedrive-class explorer capabilities need a separate long-lived application surface and a coherent filesystem/index/watch/operation backend. Do not confuse them with an XDG FileChooser modal.
- The current implementation enumerates up to 4096 entries and renders a bounded list. It does not yet virtualize, paginate, thumbnail, maintain a background index or keep listings fresh using inotify.
- This native shell is interactive and animated, not a static HTML prototype. However, parity with Spacedrive's motion, file management, and throughput is not claimed.
- Real mixed-toolkit XDG frontend/Flatpak grant, native IME composition, screen readers, foreign Wayland parent modality, mobile/wide resizing, and load testing remain OPEN.
- No portal config modification; GTK remains the host FileChooser. No push or release approval inferred from repositories being public.

## Next: CABINET-UX-02 — measurable fluidity

1. Introduce a virtualized native GPUI list and viewport-driven row recycling while preserving selection/focus semantics.
2. Qualify input-to-render and p95 frame time on short and high-cardinality directories; establish a reference target, without inventing FPS measurements.
3. Add filesystem event watchers with bounded invalidation, cancellation and no stale-selection acceptance.
4. Build real file metadata display and type-specific preview providers behind non-blocking, cancellable jobs.
5. Derive tokens from the ratified Gnosix design system and qualify a floating, resizable window visually on actual Hyprland.
6. Keep CABINET portal FileChooser and future Cabinet Explorer as separate callers of shared typed navigation/read models and explicit grants.

All heavy tests must run via gnu6-lab-run --exclusive and respect exit 75.
