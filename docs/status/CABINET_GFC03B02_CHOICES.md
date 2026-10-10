# Cabinet — GFC-03B/02: typed XDG choices

Date: 2026-10-10. Local prototype, NOT deployed.

## Implemented

- Parse XDG choices with exact D-Bus signature a(ssa(ss)s).
- Preserve ID, user-visible label, ordered options and selected value.
  Empty option list means boolean true/false checkbox.
- Missing initial selection defaults to first offered combo choice or
  false for boolean. Validate maximum 16 choices and 32 options per choice,
  duplicate IDs, invalid selections and control characters.
- GPUI Cabinet displays a native clickable row for each offered choice.
  Clicking cycles through only the offered values; the renderer replies
  with explicit selected values.
- Renderer and backend use a typed SelectedWithMetadata private pipe
  response. The backend checks exact choice IDs, order and values;
  forged IDs or values fail closed with response 2.
- Successful backend results contain a typed a(ss) choices field alongside
  file:// URIs, optionally with current_filter metadata.
- Existing OpenFile, SaveFile and SaveFiles protocol paths remain intact.

## Reproducible proofs

- 23 Rust unit tests PASS; strict Clippy and native GPUI build PASS.
- Four isolated D-Bus smoke suites PASS:
  tools/gfc-02-smoke.sh
  tools/cabinet-gfc03-smoke.sh
  tools/cabinet-gfc03b-smoke.sh
  tools/cabinet-gfc03b2-smoke.sh
- Last suite tests overlapping default/changed selections, exact
  a(ss) results, rejection of forged choice ID and option value and
  rejection of invalid starting selection.
- Weston headless with fake seat and private compositor socket PASS
  initial xdg_toplevel configure/ack and app ID in.gnu.os.cabinet.
  Visual capture shows Encoding/Western, Reencode/Off, the filter bar,
  an unoccluded file list and action buttons.
- Evidence:
  /mnt/workbench/evidence/runs/cabinet-gfc03b2-20261010-wayland/
- Corrected image is for GL readback inspection; raw PNG retained.
- Full-repository hermetic xtask CI: **48/48 PASS**, evidence: /mnt/workbench/evidence/runs/cabinet-gfc03b2-20261010-xtask/

## Not yet qualified

- No synthesized real mouse/keyboard interaction with the GUI in the
  Weston smoke; renderer click handlers are wired but unqualified.
- No genuine GPUI IME editor, clipboard, text selection or composition
  for SaveFile yet. GPUI exposes EntityInputHandler and ElementInputHandler
  as a possible path requiring separate implementation and tests.
- No parent_window foreign toplevel binding, modal focus enforcement,
  accessibility, XDG current_file or writable support, overwrite
  confirmation, real Chromium/Flatpak frontend or Documents Portal grant.
- MIME matches extension guesses only, not file-content classification.
- No system default switch, GTK replacement, host-service restart,
  production installation, merge or push.

## Reproduction

Use gnu6-lab-run --check before work.
Use gnu6-lab-run --exclusive for cargo builds, full CI and Weston probing.
Exit code 75 means another heavy job is active; do not bypass.

    cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    cargo build --offline -p in-gnu-os-filechooser-portal --features native-gpui
    tools/gfc-02-smoke.sh
    tools/cabinet-gfc03-smoke.sh
    tools/cabinet-gfc03b-smoke.sh
    tools/cabinet-gfc03b2-smoke.sh

Protocol reference:
https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.FileChooser.html

## Next GFC-03C

Implement GPUI native filename text input using EntityInputHandler and
ElementInputHandler; prove selection, UTF-16 ranges, clipboard and IME
composition in controlled Wayland. Then qualify parent-window modality
and XDG frontend interoperability before any host portal switch.
