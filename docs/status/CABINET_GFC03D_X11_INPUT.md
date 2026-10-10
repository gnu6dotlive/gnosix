# Cabinet — GFC-03D/04, Real Keyboard Input Qualification

Date: 2026-10-10
Baseline before oracle addition: `43aa0d06d0147d603168e557d71f1b35fd3a5a24`.
Status: **PASS for the two explicitly exercised keyboard paths**, NOT a general IME/accessibility or production PASS.

## Scope and trust boundary

The test uses the independent, unmodified upstream Weston 15.0.1 X11 backend, running as a client of an isolated Xvfb display. XTest key events are injected only into that virtual X11 display, translated by Weston to a real Wayland `wl_keyboard.key` event, handled by the native GPUI Cabinet renderer, and checked against the renderer's exact typed JSON response.

The test is **not** using a mocked GPUI callback or the D-Bus mock renderer. It is also **not** exercising the public `xdg-desktop-portal` frontend or a Flatpak application. Those remain independent, open gates.

## PASS oracles

**GFC03D-KEY-01 — Escape / cancellation: PASS.**

- Wait for the renderer to acknowledge its xdg_surface configure.
- Independently wait for a genuine `wl_keyboard.enter`, then focus the Weston X11 toplevel.
- Inject Escape via XTest, require `wl_keyboard.key` in the renderer's real Wayland protocol trace.
- Require the exact response: `{"outcome":"cancelled"}`.
- Require the compositor/render process tree to exit without leaking the exclusive lab lock.

Evidence: `/mnt/workbench/evidence/runs/cabinet-gfc03d-x11-oracle-cancel-v1-20261010/`.

**GFC03D-KEY-02 — SaveFile Unicode model with real ASCII input: PASS.**

- Same isolated seat, configure, keyboard focus and Wayland key evidence.
- XTest sends Ctrl+A, then the ASCII filename `cabinet-e2e-save.txt`, then Return.
- Require the renderer's typed JSON to select exactly `fixture/cabinet-e2e-save.txt`.
- Assert no destination file was created by the chooser.
- Require successful process-group cleanup.

Evidence: `/mnt/workbench/evidence/runs/cabinet-gfc03d-x11-oracle-save-v1-20261010/`.

Both proofs record `renderer.stderr` (Wayland protocol), `response.json`, exact `source-head.txt`, binary hashes, Weston logs, and virtual X11 window geometry.

## Reproduction

Prerequisites: the offline GNOSIX debug Cabinet renderer, Weston with the X11 backend, XTest/xdotool/xauth/xprop, and a local Xvfb executable. The qualification used an official CachyOS Xvfb package downloaded and GPG-verified independently, then extracted into a private workbench directory without installing it system-wide:

`/mnt/workbench/evidence/runs/cabinet-gfc03d-xvfb-dependency-20261010/`

No network listener is opened. The runner uses `unshare -Urn` to create a separate network namespace, X11 cookie authentication and a separate `DISPLAY`, a unique `WAYLAND_DISPLAY`, and `gnu6-lab-run --exclusive`. It refuses pre-existing evidence directories and terminates its own renderer, compositor and virtual X server via process-group cleanup.

From the repository root, supply a **fresh** evidence path per run:

```sh
CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/cabinet-x11-cancel-next \
  tools/cabinet-gfc03d-x11-input-oracle.sh cancel
CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/cabinet-x11-save-next \
  tools/cabinet-gfc03d-x11-input-oracle.sh save
```

Source files:

- `tools/cabinet-gfc03d-x11-input-oracle.sh` — bounded lab launcher and host safeguards.
- `tools/cabinet-gfc03d-x11-input-session.sh` — isolated compositor and event/result oracle.

## Historical failures and causal findings

The first real keyboard test failed because it injected Escape after the first `ack_configure` but **before** `wl_keyboard.enter`. The correct synchronization primitive is the actual keyboard-enter event; the corrected test returned Cancelled and the renderer trace contained `wl_keyboard.key`.

The first Weston/VNC attempt aborted in NeatVNC authentication (a GMP divide-by-zero). Weston 15.0.1's VNC backend requires PAM authentication; no user credentials were requested or bypassed. RDP/TLS reached the graphical-session negotiation but had client interoperability problems. Neither VNC nor RDP is part of the qualified input test runtime.

The initial X11 helper used `xdotool search --pid` as its window oracle. Weston does not consistently advertise its process PID in X11 metadata; the qualified test reads the real X11 output window identifier from the compositor log and validates its dimensions. The helper `od` was also shadowed on the host PATH by an unrelated stale launcher; the isolated session now constrains `PATH=/usr/bin:/bin`.

## Explicit remaining gates

**OPEN:** IME composition (Japanese/Chinese or other real input-method engines), non-ASCII keyboard layouts, X11/Wayland clipboard and real paste, pointer placement/selection, Tab/Shift+Tab traversal through every native control, screen-reader and AT-SPI integration, long filename horizontal editing, external application focus/modality, Chromium/GTK/Qt caller interoperability, Flatpak Documents Portal authorization.

**UNCHANGED:** the active XDG FileChooser remains GTK; no host default switch, no production service change, no compositor restart, no merge or remote push was required.

Next qualification: implement independent pointer and keyboard-only OpenFile oracles using the same dedicated X11/Weston seat, then qualify live IME composition separately before promoting GFC-03D.
