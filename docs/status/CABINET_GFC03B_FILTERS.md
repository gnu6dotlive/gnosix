# Cabinet — GFC-03B/01: typed XDG filters

Date: 2026-10-10. Local checkpoint, NOT the active system picker.

## Implemented

- XDG filters signature a(sa(us)) and current_filter signature (sa(us)).
- Up to 16 filters with up to 24 rules each; glob type 0 and MIME type 1.
- Case-sensitive basename glob matching; MIME extension guesses with subtype wildcards, such as image/*.
- Initial current_filter respected; first filter selected by default. A lone current_filter without filters also works.
- Native GPUI UI shows current filter; its bar and F6 action are wired to cycle filters.
- Asynchronous directory scanning hides nonmatching files but preserves directories and generation fencing.
- Renderer reports chosen filter index on the private stdio bridge. Backend rejects invalid indices.
- Successful backend results include the selected typed current_filter with file:// URI results.
- Filters are *selection hints*, not authorization or sandbox policy.

## Evidence

- 19 Rust unit tests PASS; strict Clippy and native GPUI build PASS.
- All three private D-Bus suites PASS: gfc-02-smoke, cabinet-gfc03-smoke and cabinet-gfc03b-smoke.
- GFC-03B D-Bus: simultaneous Text/Images calls return separate file URIs and distinct correct typed current_filter results; malformed filter and fake index return response 2.
- Real GPUI renderer launches under a private Weston headless compositor socket with fake seat; screenshot shows folder and readme.txt under Text filter while graphic.png is excluded.
- Wayland evidence: /mnt/workbench/evidence/runs/cabinet-gfc03b-20261010-wayland/
- The screenshot converted for inspection is vertically flipped to correct Weston/GL readback. The original raw PNG is retained.
- Full repository CI currently PENDING: two attempts with gnu6-lab-run --exclusive both refused with exit 75 (another heavy job active). No global PASS is claimed for this checkpoint.

## Boundaries

- MIME matches only filename extensions, NOT file content or magic bytes. Some valid complex glob/MIME rules are conservatively rejected.
- No real clicks or F6 key events were exercised in the compositor smoke test; UI wiring is not an end-user interaction PASS.
- File list is capped at 4096 qualifying entries; no pagination or search in this tranche.
- XDG choices, writable, current_file, nonempty parent_window, parented modality, IME text composition, accessibility, overwrite, Documents Portal sandbox grant, real Chromium/Flatpak callers remain OPEN.
- No installed portal, no user-session configuration changes, no replacement of GTK chooser, no push.

## Reproduction

Use gnu6-lab-run --check first. Heavy builds and Wayland runs must be under gnu6-lab-run --exclusive. Exit 75 means stop and wait, not bypass.

    cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    cargo build --offline -p in-gnu-os-filechooser-portal --features native-gpui
    tools/gfc-02-smoke.sh
    tools/cabinet-gfc03-smoke.sh
    tools/cabinet-gfc03b-smoke.sh

Specification:
https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.FileChooser.html

## Next slice

GFC-03B/02: real GPUI text input and IME, exact XDG choices and result values, writable and current_file. Then foreign-parent Wayland modality, accessibility and end-to-end frontend/Flatpak interoperability before a reversible host-default switch.
