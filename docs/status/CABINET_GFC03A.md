# Cabinet — GFC-03A Save contract qualification

Date: 2026-10-10. Status: local implementation checkpoint; NOT deployed, not production qualified.

## New typed capabilities

- FileChooser backend still exposes OpenFile, SaveFile, SaveFiles with the freedesktop ABI.
- Native Cabinet renderer has a minimal SaveFile name editor using GPUI keyboard events.
- SaveFile accepts one newly named destination in an existing folder, with no write operation.
- SaveFiles selects one existing destination folder and projects each requested filename into a URI, maintaining original argument order.
- Filenames are typed and bounded, require one normal filesystem component and exclude directory traversal.
- SaveFiles rejects empty arrays, duplicate names, more than 128 names, bad NUL termination, path separators and collisions.
- SaveFile/SaveFiles reject an already existing destination, including a dangling symlink.
- accept_label is forwarded into the renderer, so supported button labels are not silently ignored.
- All selected resources remain normalized local file:// paths, not new access permissions.
- Concurrent D-Bus calls still retain independent handles, generation fencing, cancellation and process isolation.

## Explicitly unfinished

- SaveFile keyboard entry uses GPUI keystrokes, not a qualified IME/text-input system. Paste, editing selection and accessibility are not ready.
- No overwrite confirmation or conflict resolver; existing destinations fail closed.
- No filter MIME / glob UI, current_filter roundtrip, choices, writable semantics or current_file.
- Non-empty parent_window is rejected; modality and focus ownership against another application are not qualified.
- Applications relying on the document portal, Chromium, Flatpak and toolkit chooser bridges are not yet interoperable.
- Fail closed on unknown or unsupported XDG option fields rather than pretending they were honored.
- The active system FileChooser remains gtk.
- A subsequent isolated Weston visual check was blocked by gnu6-lab-run exit 75 while a different heavy job was active.
- No host XDG service changes, compositor restart, portal switch, merge, or push.

## Verification evidence

- 13 crate Rust tests PASS, including SaveFile name and collision guards, SaveFiles ordering and traversal guards.
- Strict Clippy -D warnings PASS and native GPUI/backend builds PASS after accept_label integration.
- GFC-03 actual private bus against test-only renderer:
    /tmp/cabinet-gfc03-smoke.log
  SaveFile returns exactly one file URI; SaveFiles returns two ordered URIs; collision and traversal yield response=2.
- Existing GFC-02 private bus regression PASS (two opens + Close + invalid paths and unsupported SaveFile case):
    /tmp/cabinet-gfc03-regression-gfc02.log
- Full repository-hermetic CI: 48/48 PASS, evidence:
    /mnt/workbench/evidence/runs/cabinet-gfc03a-20261010-xtask-ci/
- GFC-02B prior Weston proof remains separate:
    /mnt/workbench/evidence/runs/cabinet-gfc02b-layout-fixed-20261010/
- Interface specification:
    https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.FileChooser.html

## Reproduce safely

From the isolated Gnosix worktree:

    gnu6-lab-run --check
    gnu6-lab-run --exclusive -- cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    gnu6-lab-run --exclusive -- cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    gnu6-lab-run --exclusive -- cargo build --offline -p in-gnu-os-filechooser-portal --features native-gpui
    tools/gfc-02-smoke.sh
    tools/cabinet-gfc03-smoke.sh

If gnu6-lab-run returns 75, wait for the other exclusive job; never bypass the launcher.

## Next GFC-03B

1. Native text-input + IME, filename validation, folder UI and overwrite confirmation.
2. Typed filters a(sa(us)), current_filter roundtrip, MIME/glob matches, choices and writable behavior.
3. Correct parent_window foreign toplevel semantics, activation, focus and cancellation in nested Wayland.
4. Real XDG frontend and sandboxed caller matrix before even proposing a reversible host-default switch.
