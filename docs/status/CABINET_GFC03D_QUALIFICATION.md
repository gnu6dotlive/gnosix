# Cabinet GFC-03D — Independent qualification, focus and Unicode

Date: 2026-10-10. Baseline: d2e532d. LOCAL qualification only; not a portal activation.

## GFC-03D/01 — freedesktop protocol oracle

**METHOD ABI PASS.** This check is registered as a HostLive xtask gate,
not a repository-hermetic CI gate. tools/cabinet-gfc03d-abi-oracle.py independently introspects
the backend on a private D-Bus session and checks against canonical interface
signatures. It imports no Cabinet Rust implementation.

OpenFile, SaveFile, SaveFiles:
  inputs: o, s, s, s, a{sv}
  outputs: u, a{sv}

Reference: https://github.com/flatpak/xdg-desktop-portal/blob/main/data/org.freedesktop.impl.portal.FileChooser.xml

**OPTION SEMANTICS PARTIAL.** Existing tests cover Open/Save, filters, choices and
Request.Close. Nonempty parent_window, current_file, writable, parented modality,
overwrites, document permissions and full sandbox interoperability remain OPEN.
The app_id string is not an authenticated principal.

## GFC-03D/02 — native filename input

**UNICODE MODEL PASS.** FilenameBuffer now tracks selection anchor and moving
caret independently of normalized byte range. Shift+Left/Right correctly
retracts selections across emojis and combining graphemes. UTF-16 reverse
selections survive until replacement, and EntityInputHandler reports reversal.
Two targeted regression tests were added.

**LIVE IME OPEN.** Unit tests, GPUI hooks and a screenshot do not prove native
IME composition, clipboard delivery, selection dragging or Undo/Redo.

## GFC-03D/03 — navigation/accessibility

**STRUCTURAL GPUI CHECK PASS.** Up, filter, choices, individual file rows,
Cancel and Confirm are Tab-focusable with Button roles, accessible labels,
Enter/Space handlers and propagation control. The filename field has a
TextInput role/label; file results have a Group role/label.

**LIVE INPUT AND A11Y OPEN.** Keyboard-only navigation, real pointer events,
focus outlines, screen reader announcements, actual IME and native clipboard
behavior have not been independently tested.

## Independent compositor evidence

- Native SaveFile GPUI window starts in private Weston headless/fake-seat:
  xdg_toplevel and configure/ack observed, no renderer errors.
  /mnt/workbench/evidence/runs/cabinet-gfc03d-headless-a11y-20261010/
- To obtain a seat with input injection, we isolated Weston VNC inside
  unshare -Urn with loopback-only networking and no host listener.
- libaml.so.1/libneatvnc.so.1 were missing. Official CachyOS packages
  were signature-verified using the existing system GPG keyring, SHA-256
  recorded, and extracted ONLY to this private test fixture:
  /mnt/workbench/evidence/runs/cabinet-gfc03d-vnc-dependencies-20261010/
  No pacman installs or host library changes were made.
- Weston VNC subsequently initialized but aborted before a valid frame.
  A VNC client exit code without a saved PNG is NOT a PASS.
  /mnt/workbench/evidence/runs/cabinet-gfc03d-vnc-socket-20261010/
  Repeatable fail-closed preflight: tools/cabinet-gfc03d-vnc-oracle.sh.
- Do not inject events into the host Hyprland seat or patch vendored GPUI
  or Weston solely to evade this blocker.

## Final local checkpoint

- Cabinet crate: 31/31 Rust unit tests PASS.
- xtask: 92/92 tests PASS.
- Four private D-Bus Open/Save/Filters/Choices regression suites PASS.
- New freedesktop method ABI HostLive gate PASS, evidence:
  /mnt/workbench/evidence/runs/cabinet-gfc03d-20261010-abi-host/
- Full repository-hermetic xtask CI: **48/48 PASS**, evidence:
  /mnt/workbench/evidence/runs/cabinet-gfc03d-20261010-xtask-ci-recheck/
- The first CI run returned 46/48 because Clippy rejected a literal
  reversed range in the directional-selection test. The test now uses
  the actual UTF-16 string length; both strict Clippy variants PASS.
  Original red evidence remains at:
  /mnt/workbench/evidence/runs/cabinet-gfc03d-20261010-xtask-ci/
- No live user input, screen-reader or sandbox grant was qualified here.

## Promotion and safety

- All heavy work under gnu6-lab-run --exclusive; exit 75 is a stop condition.
- GTK remains the active XDG FileChooser in both user portal configs.
- No system portal switch, service restart, merge, push, or distribution.
- An exact METHOD ABI PASS is narrower than full XDG compatibility.

## Next gate

1. Diagnose Weston VNC abort with its signed, isolated library fixture.
2. Run real input and accessibility oracles in a controlled compositor.
3. Keep parent-window modality, genuine xdg-desktop-portal frontend/Flatpak
   Documents Portal, Chromium and external application interop as separate gates.
