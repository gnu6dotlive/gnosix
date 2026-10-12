# CABINET-UX-FOUNDATION-01 — Native Files-first workbench qualification

Date: 2026-10-10 · **operator-ratified** · isolated branch `feat/cabinet-ux-foundation-01-20261010`
Baseline: `be1e480d7cedb720d80bb9fe274da308c8d11c65`.
Disposition: **LOCAL TECHNICAL SLICE QUALIFIED; WHOLE-PRODUCT PARITY OPEN**.

## Authority and source boundary

- Historical donor is private/incomplete **Shelly-GPUI**, `6fce3747`, read-only throughout this port. No donor files or Blade renderer imports were copied.
- Source-of-truth contracts: `docs/status/CABINET_UX_DONOR_AUDIT_20261010.md` (20 source decisions) and `docs/status/CABINET_FILES_VISUAL_PARITY_20261010.md` (V01–V16).
- This implementation works on its own branch/worktree; no merge, push, deployment, host GTK portal change, GPU render experiment, Store backend, News network call or Installer execution was authorized/performed.
- MIT and GPL provenance remain exclusively owned by the separate licence worker; no relicensing is claimed.

## Phase 1 — architecture and primitive contracts

- `cabinet_ux.rs` is a standalone pure Rust role/token and workspace-geometry layer; renders no package manager authority.
- `Workspace::Files` and `Workspace::Software` own the top-level information architecture. The CORE-01 `CabinetRoute::{Files,Store,News,Installer}` on-disk schema remains unchanged for backward-compatible saved sessions.
- Software has subordinate `Discover / News / Transactions` routes. Transactions visibly states no executor/privilege; Software never appears as a file tab.
- Files owns a separate `Locations` rail, real Home/Projects navigation and **only Files-specific tabs**.
- Semantic `GnosixTokens` own the initial dark baseline roles (canvas, panels, rail, border, text, muted, accent, selected, hover, active, warning) rather than view-level duplicated hexadecimal styles.
- `WorkspaceGeometry::for_viewport` performs finite/bounded responsive rail/canvas accounting with explicit small vs regular thresholds; compact hides the secondary rail.
- Existing location directories are resolved at construction rather than stat-ed inside the GPUI render callback.
- Limit: no light theme switch or full Gnosix DS v3.4.1 token audit; current colors are a native, testable **dark** baseline, not final design-system parity.

## Phase 2 — keyboard and native interaction

- Virtual file rows have `tab_stop(false)`; one focusable ListBox owns Up/Down/Home/End/Enter navigation.
- Pointer selection forwards focus to the list owner. Keyboard moves a bounded cursor, projects selection/scroll and gives visible row emphasis.
- No synthetic tab-stops for every row. This fixes the structural O(number of rows) keyboard Tab problem; large-list AT-SPI/screen-reader and exact focus-ring contrast still require independent qualification.
- The underlying Files read-only scan, per-tab session, inspector splitter and F9 reveal remain unchanged in authority; all existing typed persistence is preserved.

## Actual evidence and gates

| Gate | Exact evidence | Qualification |
| --- | --- | --- |
| Route/domain separation (V01, V14) | Native Wayland keyboard E2E F1–F4, tab creation/closure, restart; screenshots `files-start.png`, `store.png`, `news.png`, `installer.png` | **E3 scoped PASS**, human/product E4 OPEN |
| Adaptive Files canvas (V02) | Pure Rust layout tests at 513,720,849,850,1100,1440,1920 widths and native 1100×820 screenshot | **E2 geometry; E3 one viewport; E4 matrix OPEN** |
| UI role contrast (V13) | Rust WCAG contrast tests for primary/secondary text across dark canvas/panel/rail/active backgrounds | **E2 dark baseline; light/theme parity OPEN** |
| Keyboard focus (V08) | Real Wayland click-to-focus, Home→Down→Up→Enter, folder transition and restart, independent session receipt | **E3 two-item fixture PASS; 4k/100k + AT-SPI OPEN** |
| Files browsing continuity | Existing real pointer folder navigation with restart, route switching and F9 inspector state | **E3 regression PASS** |
| Full repository | Hermetic xtask receipt at `/mnt/workbench/evidence/runs/cabinet-ux-foundation01-full-ci-20261010/` | **48/48 PASS, zero non-PASS** |
| Spacedrive visual parity | No matching dataset/reference-renderer capture or operator acceptance | **OPEN — no PASS claimed** |

Evidence roots:
- `/mnt/workbench/evidence/runs/cabinet-ux-foundation01-build-20261010/` — exact instrumented native binary, test/build/Clippy logs and SHA-256.
- `/mnt/workbench/evidence/runs/cabinet-ux-foundation01-routes-20261010/` — four software/domain screenshots, typed saved session and keyboard events.
- `/mnt/workbench/evidence/runs/cabinet-ux-foundation01-files-20261010/` — actual directory navigation/restart.
- `/mnt/workbench/evidence/runs/cabinet-ux-foundation01-focus-20261010/` — real list-owner keyboard focus/selection/restart.
- `/mnt/workbench/evidence/runs/cabinet-ux-foundation01-full-ci-20261010/` — repository CI receipt, if produced.

The `tools/cabinet_ux_foundation_receipt.py` source/binary/screenshot
inventory records SHA-256 and image dimensions for the three native journeys.
It establishes provenance and file presence, NOT semantic or pixel-level
Spacedrive parity. Run it against a new private output file after the final
commit, so `source_dirty=false` and the recorded HEAD identifies the
qualified revision.

## Visual review — truthful limitations

The real 1100×820 captures demonstrate a distinct Files locations rail and a distinct Software Discover/News/Transactions subrail. The old four equally weighted global routes are no longer duplicated above the file canvas. File list remains dominant over optional inspector.

However, visual review still identifies large gaps versus premium file-manager references: generic file glyphs, a raw truncated path instead of real breadcrumbs/path editor, no scope-aware search bar, no Grid/Columns/Media mode, sparse two-entry data, plain inspector metadata, and a header text that should receive further compact-layout polish. These images **cannot** establish Spacedrive v2 parity: dataset, viewport, design system and feature scope differ.

## Remaining acceptance gates

1. **V02/V13:** controlled normalized screenshots at 513×933, 980×720, 1100×820, 1440×900, 1920×1080 and scale factors, then actual Hyprland human inspection; confirm clipping/contrast and focus visibility.
2. **V03/V04:** genuine breadcrumb+path edit and folder-scope search behavior; do not display inert text fields.
3. **V05/V07/V08:** real List/Grid behavior and MIME-aware identity; richer inspector; 4k/100k keyboard and screen-reader oracle with bounded Tab count.
4. **V12/V15:** real splitter drag pixels, frame-time p50/p95/p99 and asynchronous traversal on reference RTX 3070; existing 4096 scan cap is still a hard limitation.
5. **E5/E6:** independent accessibility, clean installation, licensing/provenance and release review; no advanced Software/RSS/Installer backend until Files-first P0 closes.

**Promotion invariant:** passing the source tests and isolated Wayland journeys is insufficient for a claim of premium UX or visual parity. Operator visual acceptance remains independent.
