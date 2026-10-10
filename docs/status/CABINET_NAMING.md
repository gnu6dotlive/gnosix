# Cabinet — Gnosix native file picker

**Naming slice: 2026-10-10.** Product and UI identity is **Cabinet**, with the visible label CABINET / GNOSIX.

## Canonical boundaries

- User-facing name: Cabinet.
- GPUI window application ID: in.gnu.os.cabinet.
- Native executable: gnosix-cabinet (native-gpui Cargo feature).
- Opt-in launcher setting: GNOSIX_CABINET_RENDERER (absolute path).
- Historical GNOSIX_GFC02_RENDERER is accepted as a fallback to avoid breaking the existing GFC-02 prototype.
- Internal Cargo package remains in-gnu-os-filechooser-portal, backend binary remains gnosix-portal-filechooser.
- Standard XDG interfaces must remain org.freedesktop.portal.FileChooser and org.freedesktop.impl.portal.FileChooser; backend bus name remains org.freedesktop.impl.portal.desktop.gnosix.
- No changes to on-disk user configuration, D-Bus service registrations or default portals.

GFC-01 and GFC-02 historical evidence retains the earlier technical spelling and binary identity as provenance. This document supersedes their user-facing nomenclature; do not retroactively modify evidence receipts.

## Verification

    cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    cargo build --offline -p in-gnu-os-filechooser-portal --features native-gpui --bin gnosix-cabinet
    cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    tools/gfc-02-smoke.sh

Scope is exclusively naming and compatibility. No live Wayland interaction, filter handling, SaveFile, accessibility or external app interoperability is claimed.

## Qualification receipts

- Rust unit tests: 8/8 PASS; Cabinet native GPUI build PASS; Clippy strict PASS.
- Concurrent mock D-Bus smoke: PASS, including two selections, one cancellation and rejection cases.
- Full repository-hermetic cargo xtask ci: 48/48 PASS. Evidence: /mnt/workbench/evidence/runs/cabinet-naming-20261010/
- Current desktop FileChooser default remains GTK; no user-session configuration was changed.
