# Cabinet — GFC-03D/05: Real OpenFile keyboard and pointer qualification

Date: 2026-10-10
Scope: Native GPUI renderer, independent Wayland input seat. This is not a production portal release.

## Implemented correction

The native GPUI picker now dispatches Tab to Window::focus_next and Shift+Tab to Window::focus_prev, explicitly stopping key propagation after focus navigation. It uses GPUI's existing tab-stop/focus mechanism; Cabinet does not create a parallel focus authority.

No API, packaging, XDG service configuration, compositor configuration, filesystem permissions, or desktop defaults were changed.

## Independent end-to-end gates

All six tests were rerun from the **same consolidated oracle and independent compositor sessions**, with each run using a fresh evidence directory. They verify actual Wayland keyboard or pointer traffic and the final typed JSON response.

| Case | Gate | Required result |
| --- | --- | --- |
| Escape / Cancel | PASS | Exact cancelled outcome |
| SaveFile: Ctrl+A, ASCII filename, Return | PASS | Exact selected destination; picker does not create a file |
| OpenFile: Down, Enter, Enter | PASS | Exact selected fixture/alpha.txt |
| OpenFile: pointer select + Open | PASS | At least four actual wl_pointer.button press/release events and exact alpha.txt |
| OpenFile: Tab to file, Enter, Tab to Open, Enter | PASS | Exact selected alpha.txt |
| OpenFile: Shift+Tab reverse navigation, Enter, forward navigation, Enter | PASS | Exact selected alpha.txt |

The four OpenFile cases are different paths through the same real application. This is not evidence of complete application-level file chooser interoperability.

## Evidence receipts

The following folders contain the renderer request/response, Wayland protocol transcript, compositor/X11 logs, recorded source HEAD and SHA-256 of the built renderer and Xvfb executable.

- /mnt/workbench/evidence/runs/cabinet-gfc03d5-final-cancel-20261010/
- /mnt/workbench/evidence/runs/cabinet-gfc03d5-final-save-20261010/
- /mnt/workbench/evidence/runs/cabinet-gfc03d5-final-open-keyboard-20261010/
- /mnt/workbench/evidence/runs/cabinet-gfc03d5-final-open-pointer-20261010/
- /mnt/workbench/evidence/runs/cabinet-gfc03d5-final-open-tab-20261010/
- /mnt/workbench/evidence/runs/cabinet-gfc03d5-final-open-shift-tab-20261010/

For the pointer test, before-pointer.png and selected-pointer.png are additional visual evidence. The selected screenshot shows the highlighted file row and one selected file. Exact mouse coordinates are a fixed 980x720 test-fixture assumption, **not** a production accessibility or layout contract; moving the UI must trigger requalification.

The tests were initially performed against a working tree based on commit 160f8be. They include SHA-256 of the binaries; they do not claim that the baseline HEAD alone identifies all then-uncommitted source changes. A post-commit test against the final source HEAD should be recorded separately.

## Deterministic isolation and safety

- A private Xvfb display and Weston 15 X11 backend run through the approved gnu6-lab-run --exclusive workload boundary.
- unshare -Urn isolates network access. Tests never inject into the workstation's Hyprland seat.
- X11 authentication uses a temporary per-run xauth cookie. The harness owns the process groups, exits fail-closed and checks cleanup.
- The program waits for xdg_surface configure/ack and actual wl_keyboard.enter before sending keyboard events.
- Keyboard tests require wl_keyboard.key in the real Wayland log. The pointer test waits for wl_pointer.enter and for first-click button receipts before submitting Open, then requires four button events.
- Each proof uses an exact result oracle; a screenshot, success exit code or input tool exit status alone cannot yield PASS.
- No host packages, portal defaults, secrets or authorizations were changed.

## Compatibility and security boundaries

This isolated renderer test does not exercise the real xdg-desktop-portal frontend, application-specific toolkit routing, Wayland foreign-parent modality, Flatpak Documents Portal access grants or trust of app_id. The current host FileChooser stays GTK.

**Still OPEN:** Japanese/Chinese or other real IME composition, native clipboard/paste events, pointer caret placement and selection dragging in SaveFile, AT-SPI/screen reader output and focus announcement, file-dialog parenting, high-cardinality file lists, real Chromium/GTK/Qt/Flatpak callers and service installation.

Repository publication is a separate axis from qualified distribution. Gnosix and in.gnu6 were reported public by the operator, but this feature branch is local until an explicit integration/push action.

## Reproduce

From the existing isolated Gnosix worktree, with offline Cabinet debug renderer and a validated isolated Xvfb binary available, set a **new** evidence directory for each scenario and run:

    CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/cabinet-new-open-keyboard tools/cabinet-gfc03d-x11-input-oracle.sh open-keyboard
    CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/cabinet-new-open-pointer tools/cabinet-gfc03d-x11-input-oracle.sh open-pointer
    CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/cabinet-new-open-tab tools/cabinet-gfc03d-x11-input-oracle.sh open-tab
    CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/cabinet-new-open-shift-tab tools/cabinet-gfc03d-x11-input-oracle.sh open-shift-tab

The same oracle preserves earlier cancel and save modes. It invokes gnu6-lab-run --check and --exclusive itself; exit 75 must not be bypassed.

## Next gates

GFC-03D/06: independent Wayland clipboard and real IME text composition. Thereafter qualify pointer caret/selection and screen reader accessibility. GFC-03E parent-window modality and GFC-05A full XDG frontend/sandbox/toolkit interoperability remain separate projects.

## Final repository qualification before commit

- Complete repository-hermetic cargo xtask ci: **48/48 PASS**, with 0 non-PASS gates.
- Protected workload: gnu6-lab-run --exclusive, exit 0.
- Evidence: /mnt/workbench/evidence/runs/cabinet-gfc03d5-20261010-xtask-ci/
- Four historical isolated D-Bus contract suites and the independent FileChooser backend ABI oracle: **5/5 PASS**.
- Fail-closed script preflights: invalid mode rejected (64), evidence path outside workbench rejected (64), existing evidence refused (3).
- The six real Wayland input cases above were exercised in fresh sessions against the built GPUI renderer.
- Real clipboard/IME, screen reader, foreign-parent modality, toolkit and sandbox interoperability remain OPEN.
