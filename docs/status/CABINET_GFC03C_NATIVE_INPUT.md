# Cabinet GFC-03C — Native GPUI Unicode Filename Input

Date: 2026-10-10. Local prototype, not a production portal activation.

## Implemented

- SaveFile now uses a native GPUI entity with EntityInputHandler and
  ElementInputHandler attached during canvas paint via Window::handle_input.
- Replaces old key_char accumulation. GPUI platform transports text,
  selection, marked composition ranges, UTF-16 editable ranges and
  caret geometry to the filename editor.
- FilenameBuffer independently handles UTF-16/UTF-8 offsets, surrogate
  pairs, composed accents, grapheme-safe Backspace/Delete and marked
  text replacement. Native keyboard shortcuts route selection, cursor,
  Ctrl/Cmd+A,C,X,V, Home/End, Backspace and Delete.
- Cursor, selection and composition mark are rendered using GPUI.
  The SaveFile editor takes focus at window creation; mouse click
  returns focus to its input handler.
- Child-editor changes notify parent view through an entity observer.
- Maximum filename length is 240 UTF-8 bytes; slash and control chars,
  long initial names and path traversal are rejected at ingress.
- SaveFile does not write files; existing destinations are rejected.
- No GTK/Qt/Electron, host-portal installation or public IPC authority.

## Qualification

- 29 Rust crate unit tests PASS, including six Unicode text-model tests:
  surrogate/UTF-16, selection, marked IME replacement, grapheme
  boundaries, clipboard-like replacement and filename byte limits.
- Native GPUI check/build and Clippy strict -D warnings PASS.
- Four private D-Bus regression suites PASS (Open/Close, Save,
  filters and choices).
- Isolated Weston/fake-seat compositor creates native Cabinet
  xdg_toplevel and configure/ack with no renderer errors. A screenshot
  shows the Unicode filename Été 🦀 – rapport.md, cursor and Save.
  Evidence: /mnt/workbench/evidence/runs/cabinet-gfc03c-native-ime-20261010-wayland/
- Native OpenFile compositor startup regression also PASS.
  Evidence: /mnt/workbench/evidence/runs/cabinet-gfc03c-open-regression-20261010/
- Raw PNG is preserved; inspection image corrects Weston GL's inverted
  capture Y-axis. Screenshots prove rendering, not real text input.
- Full repository hermetic xtask CI: 48/48 PASS; evidence: /mnt/workbench/evidence/runs/cabinet-gfc03c-20261010-full-xtask/

## Explicitly still OPEN

- Real keyboard events, external IME composition and clipboard provider
  not simulated; native input hooks are implemented but not E2E qualified.
- Pointer caret placement, drag-select, accessibility, horizontal scroll
  for long filenames and screen reader oracles are not qualified.
- Parent-window Wayland modality, foreign toplevel ownership,
  writable/current_file, overwrite confirmation, real Chromium/Flatpak
  frontend and Documents Portal grants remain unfinished.
- Current host FileChooser stays configured to GTK; no service restarts,
  portal override, deployment or push.

## Reproduce

Run gnu6-lab-run --check before work and use --exclusive for build,
tests, CI and Wayland probe. Exit 75 means another heavy job is active.

    cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    cargo build --offline -p in-gnu-os-filechooser-portal --features native-gpui
    tools/gfc-02-smoke.sh
    tools/cabinet-gfc03-smoke.sh
    tools/cabinet-gfc03b-smoke.sh
    tools/cabinet-gfc03b2-smoke.sh

For compositor smoke, set CABINET_SMOKE_MODE=save and a fresh
CABINET_EVIDENCE_DIR, then run tools/cabinet-headless-smoke.sh under
gnu6-lab-run --exclusive.

## Next GFC-03D

Qualify real GPUI text events and composition/selection against a
controlled Wayland seat. Then implement parent-window modality and
independent Chromium/Flatpak XDG frontend interoperability.
