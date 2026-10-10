# GFC-02A — Native GPUI FileChooser bridge (2026-10-10)

**State: bounded prototype; GPU UI compiles, isolated D-Bus mock integration passes. NOT production-qualified.**
No host portal installation, config change, restart, or desktop compositor mutation.

## Current implementation

- Rust D-Bus backend preserves org.freedesktop.impl.portal.FileChooser and
  org.freedesktop.impl.portal.desktop.gnosix, without attempting frontend ownership.
- A separate native Rust/GPUI renderer, gnosix-filechooser-gpui, is enabled with
  the native-gpui Cargo feature. It uses pre-existing gpui-pre 0.3.5 /
  gpui-pre-platform 0.3.5 Wayland APIs. No GTK/Qt/WebView/Electron/TypeScript.
- The backend launches a renderer per in-flight request over private stdin/stdout,
  using typed JSON. The only activation is an explicit absolute executable in
  GNOSIX_GFC02_RENDERER. Without it, requests fail closed (response=2).
- Bounded to 64 in-flight handles, 8 simultaneous renderer processes, 16 KiB
  wire input/output and a 600-second process timeout.
- Request.Close cancels the specific request and child; stale renderer replies
  cannot complete a new request that reused the old handle.
- GPUI can browse local directories, select existing files (including multi-file),
  select a directory, cancel and confirm. No file mutations are performed.

## Strictly not yet supported

- SaveFile, SaveFiles, filters, current_filter, choices, unknown option keys,
  file writable option, nonempty parent_window, and multiple directory choice.
  These fail closed, not silently ignored.
- Real Wayland parent/focus/modal semantics, end-to-end accessibility,
  keyboard-driven path entry, IME, async enumeration, previews, search,
  sandbox Document Portal grants, caller authentication, external providers.
- The native window has been compiled, not visually qualified under the
  operator's Hyprland session.
- app_id is caller-supplied metadata; do NOT interpret it as proof of origin.
- The backend mediates user selections; it does not create OS privileges.

## Verification

Commands (from isolated Gnosix worktree):

    cargo test --offline -p in-gnu-os-filechooser-portal --features native-gpui
    cargo clippy --offline -p in-gnu-os-filechooser-portal --all-targets --features native-gpui -- -D warnings
    cargo build --offline -p in-gnu-os-filechooser-portal --features native-gpui --bin gnosix-filechooser-gpui
    tools/gfc-01-smoke.sh
    tools/gfc-02-smoke.sh

Current facts:
- 8 Rust tests PASS.
- GFC-02 isolated D-Bus smoke PASS, with two simultaneous successful file URI
  responses, a third cancelled via Request.Close, a rejected relative path,
  and rejected unsupported SaveFile.
- Smoke uses a test-only Python renderer in dbus-run-session, NOT live GPUI.
- 8 dedicated xtask checks PASS; log bundle:
  /mnt/workbench/evidence/runs/gfc-02-20261010-targeted/
- Full repository-hermetic xtask CI: **48/48 PASS**; evidence:
  /mnt/workbench/evidence/runs/gfc-02-20261010-full/
- Production-default portal remains GTK, no installation.

## Gate ledger

| Gate | Scope | State |
|---|---|---|
| G01 Protocol | OpenFile selected + Save* rejected | PARTIAL |
| G02 Concurrency | 3 concurrent real D-Bus calls, exact Close | ISOLATED PASS |
| G03 Authority | identity validation, grants | OPEN |
| G04 Wayland UI | GPUI compile, but no live interaction | BUILD PASS ONLY |
| G05 Real interop | Chromium, native callers, Flatpak, Qt-portal | OPEN |

## Next

- Qualify GPUI window under a controlled Wayland compositor (not the
  operator's primary Hyprland session).
- Add proper parent-window ownership, filters, choices, editable SaveFile,
  SaveFiles and full selection semantics.
- Validate actual XDG frontend -> backend -> renderer -> Documents Portal
  behavior with real callers before any reversible host-default switch.
- GPUIX/TypeScript components are a future design choice, not currently used.

References:
https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.FileChooser.html
https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Request.html
