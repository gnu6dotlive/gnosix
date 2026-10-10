#!/usr/bin/env bash
# Test only on a private D-Bus session; never install or restart an XDG portal.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
backend="$(printenv GFC_BINARY || true)"
if test -z "$backend"; then
  backend="$(cargo metadata --offline --locked --no-deps --format-version 1 |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"] + "/debug/gnosix-portal-filechooser")')"
fi
test -x "$backend"
GFC03D_ROOT="$root" GFC03D_BINARY="$backend" dbus-run-session -- bash -euo pipefail -c '
  "$GFC03D_BINARY" >/tmp/cabinet-gfc03d-abi-backend.log 2>&1 &
  child=$!
  trap "kill $child 2>/dev/null || true; wait $child 2>/dev/null || true" EXIT
  for attempt in $(seq 1 100); do
    if busctl --user introspect org.freedesktop.impl.portal.desktop.gnosix \
      /org/freedesktop/portal/desktop org.freedesktop.impl.portal.FileChooser >/dev/null 2>&1; then
      break
    fi
    sleep .05
  done
  python3 "$GFC03D_ROOT/tools/cabinet-gfc03d-abi-oracle.py"
'
