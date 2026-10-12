# CABINET-FILES-PARITY-02 — Native file interaction checkpoint

2026-10-10 — Isolated branch feat/cabinet-files-parity-02-20261010.
Base cce5532. Local E2/E3 acceptance; Spacedrive and operator E4 parity OPEN.

## Real components

- V03: Navigable breadcrumbs, absolute path editor with native GPUI IME and Ctrl+L, Back/Forward/Parent/Refresh.
- V04: Folder-local filename search with Unicode/IME, Ctrl+F, hidden-file policy, 240-byte query limit, explicit zero-result state.
- V05: Real virtualized List and two-column Grid render trees; file-kind glyphs rather than fabricated thumbnails.
- V07: Inspector reads metadata cached during background scan: estimated kind, size, modification, absolute path; no synchronous paint I/O.
- Independent per-tab view mode and query persisted with backwards-compatible schema 1 defaults.
- Path field uses the shared native GPUI editor, with a distinct 4096-byte full-path ingress contract. No mutation authority was added.

## Executed evidence

- 62 Rust tests PASS, native compilation and all-target strict Clippy PASS.
- Native build SHA-256 878e42d183c6d3ba8ae68ec26f68a4a7d01246a5b7db6c09aeb0d155cbeca97a.
- Real isolated Wayland routes/restart: /mnt/workbench/evidence/runs/cabinet-files02-routes-20261010-v1/ — PASS.
- Real isolated Wayland search input, click Grid, Ctrl+L navigate to subdirectory and query/view session restart: /mnt/workbench/evidence/runs/cabinet-files02-view-20261010-v2/ — PASS.
- Repository hermetic CI: /mnt/workbench/evidence/runs/cabinet-files-parity02-full-ci-20261010/ — 48/48 PASS, zero non-PASS gates.

## Honest open gates

- No claim of Spacedrive visual parity. E4 matching reference fixtures, operator acceptance, varied DPI and viewport matrix OPEN.
- No media thumbnails, Columns/Media mode, indexed cross-folder search, screenreader acceptance, rich selection or drag-and-drop.
- Filesystem enumeration remains bounded to 4096 entries, with no 100k/watch overflow qualification.
- Store/News/Installer remain non-operational and privilege-separated; no portal default switch, merge, push or license changes.

Next: E4 screenshot normalization, large-fixture performance and accessibility, real thumbnail provenance and input semantics.
