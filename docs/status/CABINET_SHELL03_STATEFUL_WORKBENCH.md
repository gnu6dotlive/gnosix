# CABINET-SHELL-03 — Stateful native workbench, checkpoint 01

Date: 2026-10-10. Branch: feat/cabinet-shelley-01-20261010.
Status: **LOCAL IMPLEMENTATION / SCOPED WAYLAND PASS**. Premium visual acceptance and public distribution remain OPEN.

## Implemented

- The persistent native Cabinet app now associates a CabinetTabState with each route/tab.
- Each tab state records bounded back/forward directory history, selected file path, inspector visibility and inspector width.
- Session schema version stays 1: legacy CORE-01 sessions without tab_states migrate to aligned default state, with unchanged existing routes and active tab. Mismatched state counts, bad paths, out-of-bounds inspector widths and excess history fail closed.
- Tab creation, close and switching retain independent per-tab state. Navigation is snapshotted before switching away and restored on return.
- Navigating between folders clears previous-file selection so stale selection does not leak into another folder.
- Native files surface now contains a responsive file-list / inspector workbench. The inspector can be resized with a GPUI pointer-driven splitter under explicit min/max constraints, or toggled with F9. Keyboard width changes use current window dimensions, not a hardcoded reference width.
- Inspector visibility and width are persisted to the app's typed, bounded state. No new network, filesystem mutation, package-manager or privilege authority was added.
- No changes to the XDG FileChooser backend, default GTK portal, or license configuration. The donor name is Shelly-GPUI; the separate MIT worker owns relicensing/provenance.

## Evidence

- Native Rust unit tests 52/52 PASS, including tests for legacy session migration, per-tab state isolation and invalid session fail-closed.
- Offline/locked native GPUI build and all-target strict Clippy PASS.
- Weston/Xvfb E2E routes + F9 visibility persistence PASS against exact built binary:
  /mnt/workbench/evidence/runs/cabinet-shell03-final-routes-20261010/
- Weston/Xvfb E2E real folder navigation and restart PASS:
  /mnt/workbench/evidence/runs/cabinet-shell03-final-files-20261010/
- Build and SHA256 receipt:
  /mnt/workbench/evidence/runs/cabinet-shell03-final-build-20261010/
- The canonical CORE-01 Wayland session oracle now includes F9 hide/show state assertions.
- Full repository hermetic CI: **48/48 PASS**, zero non-PASS gates.
  Evidence: /mnt/workbench/evidence/runs/cabinet-shell03-20261010-full-ci/

## Still open

- Independent E2E measurement of splitter pointer displacement on the standalone Cabinet app. The GPUI drag code compiles; this does not yet establish pixel-measured drag acceptance for the standalone surface.
- Per-tab scroll offset restoration, inspector metadata richness and human Hyprland acceptance.
- Session save failure currently reports to stderr instead of a native error banner.
- Component tokens, advanced motion, preview renderers, 100k filesystem paging and frame-time distributions.
- Store, News and Installer backends remain unavailable; their routes carry no privileged authority.
- Multi-window lifecycle, full screen reader accessibility, distribution rights/MIT gate and public release remain unqualified.
