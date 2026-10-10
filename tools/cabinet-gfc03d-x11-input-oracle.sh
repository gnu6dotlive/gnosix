#!/usr/bin/env bash
# GFC-03D independent real-key test against an isolated Weston/X11 seat.
# This never modifies the user's portal, compositor config or X11 session.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
mode="${1:-}"
case "$mode" in
  cancel|save|open-keyboard|open-pointer|open-tab|open-shift-tab|ux-history|ux-search|ux-hidden) ;;
  *) echo "Usage: CABINET_EVIDENCE_DIR=/mnt/workbench/evidence/runs/unique $0 {cancel|save|open-keyboard|open-pointer|open-tab|open-shift-tab|ux-history|ux-search|ux-hidden}" >&2; exit 64;;
esac
ev="${CABINET_EVIDENCE_DIR:?Set a new unique evidence directory}"
case "$ev" in /mnt/workbench/evidence/runs/*) ;; *) echo "Evidence must reside under workbench evidence/runs" >&2; exit 64;; esac
test ! -e "$ev" || { echo "Refusing to overwrite existing evidence: $ev" >&2; exit 3; }
binary="${CABINET_BINARY:-/mnt/sandbox/builds/cargo/debug/gnosix-cabinet}"
xvfb="${CABINET_XVFB_BIN:-/mnt/workbench/evidence/runs/cabinet-gfc03d-xvfb-dependency-20261010/root/usr/bin/Xvfb}"
test -x "$binary" || { echo "Cabinet renderer binary missing: $binary" >&2; exit 66; }
test -x "$xvfb" || { echo "Xvfb binary missing: $xvfb" >&2; exit 66; }
for tool in gnu6-lab-run unshare weston xauth xprop xdotool import; do
  command -v "$tool" >/dev/null || { echo "Missing required tool: $tool" >&2; exit 66; }
done
export CABINET_INPUT_MODE="$mode" CABINET_EVIDENCE_DIR="$ev" CABINET_BINARY="$binary" CABINET_XVFB_BIN="$xvfb"
cd "$root"
gnu6-lab-run --check
gnu6-lab-run --exclusive -- unshare -Urn -- bash "$root/tools/cabinet-gfc03d-x11-input-session.sh"
