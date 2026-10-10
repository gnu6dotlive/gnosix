# GFC-01 — Gnosix FileChooser Plane (2026-10-10)

**State:** prototype only. No graphical renderer, no installed portal, no claim of a working file picker.

## Identity and protocol

- Product: Gnosix FileChooser Plane, under in.gnu.os service authority.
- Application-facing frontend (not replaced): org.freedesktop.portal.FileChooser
- Implemented backend interface: org.freedesktop.impl.portal.FileChooser
- Staged bus name: org.freedesktop.impl.portal.desktop.gnosix
- Staged object path: /org/freedesktop/portal/desktop
- Future extension namespace: in.gnu.os.impl.portal.FileChooser1 (not implemented).
- The legacy service org.freedesktop.impl.portal.desktop.gnosis is untouched.

## Implemented

- Workspace crate: crates/in-gnu-os-filechooser-portal.
- D-Bus OpenFile, SaveFile, SaveFiles, and dynamically scoped org.freedesktop.impl.portal.Request.Close.
- Per-handle registry, generation-aware cancellation, 64 concurrent in-flight requests maximum.
- Exact-once response, cancellation (response 1), unavailable renderer (response 2).
- Local absolute paths converted to file:// URIs, invalid selections rejected.
- Caller options preserved for the future renderer; selected boolean options typed.
- Unit tests for out-of-order completion, stale cancellation, duplicated IDs, bounds, invalid paths, Unicode.
- No active GUI receiver: backend always returns status 2 rather than fabricated selection.

## Unqualified or not implemented

- GPUI/GPUIX UI, keyboard navigation, preview, parent-window ownership, focus and modal semantics.
- Full filters, choices, current_filter, overwrite, SaveFiles requested filename matching, directories, actual file operations.
- Caller identity and authorization, renderer credential binding, sandbox Documents Portal integration.
- Wayland live tests, Flatpak/Chromium interoperability, recovery, accessibility, packaging, deployment.
- Applications that bypass XDG Portal require separate integration.

A backend file chooser is not a privileged sandbox policy engine. Do not trust app_id as authenticated identity. The XDG frontend remains responsible for Documents Portal grants. GFC-01 cannot be advertised as G01-G05 PASS.

## Reproducible checks (worktree)

    cargo check --offline -p in-gnu-os-filechooser-portal
    cargo test --offline -p in-gnu-os-filechooser-portal
    cargo build --offline -p in-gnu-os-filechooser-portal
    tools/gfc-01-smoke.sh

The smoke script creates a separate D-Bus session via dbus-run-session and probes all three methods. It does not change the real user bus or desktop configuration.

The portal and D-Bus service descriptors checked into the repo are examples ONLY, not installed. Do NOT install or select them as the active chooser without working GPUI UI, an app matrix and an operator-approved rollback. Never restart Hyprland, xdg-desktop-portal, or the legacy in.gnu.os process for this stage.

## Qualification ledger

| Gate | State | Evidence or blocker |
|---|---|---|
| G01 D-Bus contract | PARTIAL | Build/isolated protocol; no successful UI path |
| G02 Parallelism | UNIT PASS | 5 unit scenarios, not integrated callers |
| G03 Authority | OPEN | No real caller authentication or scoped grant |
| G04 Wayland | OPEN | No GUI, modality or compositor proof |
| G05 Compatibility | OPEN | No host app tests or portal selection |

## Next slice

1. Add native GPUI renderer behind a private, authenticated one-way request/result bridge.
2. Add strict full option parsing, SaveFiles cardinality, path/file filters, typed result roundtrip.
3. Use an isolated mock renderer to exercise D-Bus concurrency, cancellation and success results.
4. Test in a controlled Wayland environment, then obtain explicit approval for a reversible FileChooser-only xdg-desktop-portal configuration.

Source: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.FileChooser.html


## Verification receipts (local machine)

- Full repository-hermetic xtask baseline: 43/43 PASS before the GFC-specific catalogue additions. Evidence: /mnt/workbench/evidence/runs/gfc-01-20261010-xtask-ci/
- Canonical GFC test, Clippy, build, xtask regression, fmt and diff gates: 6/6 PASS after catalogue integration. Evidence: /mnt/workbench/evidence/runs/gfc-01-20261010-new-gates-fixed/
- Isolated D-Bus smoke: three standard methods introspected, 3/3 returned response=2 without UI. Log: /tmp/gnosix-gfc-01-smoke-final.log
- No system portal configuration, installed user service, or Hyprland session was mutated.
