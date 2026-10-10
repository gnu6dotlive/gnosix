#!/usr/bin/env bash
# GFC-01 controlled verification on an isolated D-Bus session.
set -euo pipefail
binary="$(printenv GFC_BINARY || true)"
if [[ -z "$binary" ]]; then
  binary="/mnt/sandbox/builds/cargo/debug/gnosix-portal-filechooser"
fi
test -x "$binary" || { echo "missing binary: $binary" >&2; exit 2; }
dbus-run-session -- bash -euo pipefail -c '
  "$1" > /tmp/gnosix-gfc-01-isolated.log 2>&1 &
  backend=$!
  trap "kill $backend 2>/dev/null || true; wait $backend 2>/dev/null || true" EXIT
  destination=org.freedesktop.impl.portal.desktop.gnosix
  object=/org/freedesktop/portal/desktop
  interface=org.freedesktop.impl.portal.FileChooser
  for i in $(seq 1 60); do
    if busctl --user introspect "$destination" "$object" "$interface" >/tmp/gnosix-gfc-01-introspection.log 2>/dev/null; then break; fi
    sleep 0.1
  done
  grep -E "OpenFile|SaveFile|SaveFiles" /tmp/gnosix-gfc-01-introspection.log
  for method in OpenFile SaveFile SaveFiles; do
    reply=$(busctl --user call "$destination" "$object" "$interface" "$method" "osssa{sv}" \
      "/org/freedesktop/portal/desktop/request/1_123/gfc01_$method" "org.gnosix.smoke" "" "GFC-01 test" 0)
    echo "$method -> $reply"
    [[ "$reply" == *" 2 0" ]] || {
      echo "expected fail-closed response=2, got: $reply" >&2
      exit 1
    }
  done
  echo "GFC-01 isolated bus smoke PASS"
' -- "$binary"
