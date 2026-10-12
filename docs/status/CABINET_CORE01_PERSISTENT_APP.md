# CABINET-CORE-01 — Persistent native Cabinet application

Date: 2026-10-10. Branch: feat/cabinet-shelley-01-20261010.
**Gate: G01/G02 local qualification, product acceptance remains OPEN.**
Not a public release, deployed app, default portal, app store, RSS backend or installer.

## Independent native application

- New opt-in executable **cabinet** (Cargo feature native-gpui).
- Existing **gnosix-cabinet** remains the JSON-request/response XDG FileChooser renderer.
- Standalone Cabinet does not read an XDG request on stdin or return portal selection JSON.
- Independent GPUI application ID: in.gnu.os.cabinet.app. Single native window on launch.
- Four typed surfaces: Files, Store, News, Installer. Only Files is operational (read-only browsing).
- Store/News/Installer are accessible routes with explicit unavailable / not-configured states.
- F1–F4 select surfaces; Ctrl+T opens a Files tab; Ctrl+W closes a tab (minimum one).
- Files displays real bounded asynchronous directory scans and virtualized rows; folder clicks navigate.
- Files does not support copy/delete/rename, filesystem watchers or directories larger than 4096 entries.

## Typed session and persistence

- Rust CabinetSession: schema version 1, maximum 12 tabs, active-tab index, hidden-file preference.
- Tagged CabinetRoute variants: Files with absolute folder, Store, News, Installer.
- The model contains no command strings, execution grants, package mutation or network authority.
- A dedicated session-writer thread serializes snapshot updates; UI events enqueue rather than write.
- Save uses a private session directory, mode 0600 file, atomic same-directory rename and sync.
- Existing permissive session directories, symlink state files and invalid/oversized schemas fail closed.
- Recovery substitutes the explicitly supplied fallback for directories no longer present.
- Writes are asynchronous: a failed save is reported to stderr, not yet as a UI error badge.
- Default state path: XDG_STATE_HOME/gnosix/cabinet-session.json; fallback to HOME/.local/state.
- CLI: cabinet [--start-folder ABSOLUTE_DIRECTORY] [--state-file ABSOLUTE_PATH].
- Testing uses explicit state-file paths under the private evidence root, never the daily host state.
## Isolated runtime evidence

Source baseline used during first application bring-up: 8fa492c (later modified in local branch).
- Route/session oracle: /mnt/workbench/evidence/runs/cabinet-core01-routes-e2e-20261010-v3/
  Real keyboard events F1–F4 and Ctrl+T/Ctrl+W, four surface screenshots,
  five persisted tabs, selected News route, explicit process termination and independent
  relaunch. Reopened News with the same typed saved session.
  Visual alignment check tolerates Weston top-level translation (+3px in that run)
  but checks bounded RGB difference after alignment; route screenshots differ.
- Files/folder oracle: /mnt/workbench/evidence/runs/cabinet-core01-files-e2e-20261010-v1/
  Real Weston X11 -> Wayland pointer click into a real subdirectory, async
  listing, persisted absolute folder, process termination and reopened same folder.
- Each oracle launches private GPG-verified Xvfb, Weston X11 backend, per-run
  X11 cookie, isolated network namespace and Wayland socket under gnu6-lab-run
  exclusive. It does not touch the operator's Hyprland seat or portal configuration.
- Both tests assert the new PID differs after restart and the relaunched
  process remains alive without any incoming XDG request.
- Source-level session model: 49 Rust tests PASS, including malformed state,
  stale folder recovery, symlink refusal and directory-permission non-mutation.
- The first app binary passed strict Clippy and native build; the hardened
  final-source binary also passed 49 Rust tests, strict Clippy and native build
  (receipt: /mnt/workbench/evidence/runs/cabinet-core01-final-source-build-20261010/).
- **FINAL-BINARY E2E PASS:** initial post-hardening E2E attempt was refused
  by GNU6's memory guard (available approximately 7 GiB). After natural
  headroom recovery, both independent Wayland journeys passed on the hardened
  binary. No lock/memory guard was bypassed or unrelated process terminated.
  Receipts:
  /mnt/workbench/evidence/runs/cabinet-core01-hardened-routes-20261010-v2/
  /mnt/workbench/evidence/runs/cabinet-core01-hardened-files-20261010-v2/
- Full repository hermetic CI: **48/48 PASS**, no non-PASS gate.
  Receipt: /mnt/workbench/evidence/runs/cabinet-core01-20261010-full-ci/.

## Reproduce

First build native Cabinet using the repository-pinned toolchain and qualified offline lock:

    cargo test --offline --locked -p in-gnu-os-filechooser-portal --features native-gpui
    cargo clippy --offline --locked -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    cargo build --offline --locked -p in-gnu-os-filechooser-portal --features native-gpui --bin cabinet

With a private evidence directory that does not already exist:

    CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/UNIQUE_NAME tools/cabinet-core01-app-oracle.sh routes
    CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/UNIQUE_NAME_2 tools/cabinet-core01-app-oracle.sh files

The CLI option --state-file allows explicit isolated-session testing; never
select an uncontrolled location or alter the host XDG portal for this test.
## Still OPEN

- Clean independent installation/build, desktop entry and startup integration.
- Production-grade errors for failed session writes and migration/version upgrades.
- Per-tab history preservation, multi-window lifecycle, close/reopen policies and keyboard accessibility review.
- Runtime benchmarks, actual frame-time p95/p99 and long-lived filesystem watcher / indexing.
- Store catalog, RSS reader and installer are not implemented; routes are explicitly non-operational.
- Operator visual approval on actual Hyprland and independent multi-compositor/a11y qualification.
- MIT/provenance rights are delegated to a separate worker. GPL-derived code is not publicly promoted.
- Whole-product R19 and default FileChooser switch are not authorized by this gate.

**Authority invariant:** a standalone application process does not inherit any
privileged execution permission from its sibling FileChooser crate, installed
services or route names. Store and RSS do not grant install authority.
