#!/usr/bin/env bash
# Protocol-only GFC-02 test; no GPUI window and no host portal activation.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
binary="$(printenv GFC_BINARY || true)"
if [[ -z "$binary" ]]; then binary=/mnt/sandbox/builds/cargo/debug/gnosix-portal-filechooser; fi
test -x "$binary" || { echo "backend not built: $binary" >&2; exit 2; }
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
printf 'fixture A\n' > "$work/alpha.txt"
printf 'fixture B\n' > "$work/beta.txt"
GFC02_FIXTURE="$work" GNOSIX_CABINET_RENDERER="$root/tools/gfc-02-mock-renderer.py" \
GFC02_BINARY="$binary" dbus-run-session -- bash -euo pipefail -c '
  "$GFC02_BINARY" > "$GFC02_FIXTURE/backend.log" 2>&1 &
  backend=$!
  trap "kill $backend 2>/dev/null || true; wait $backend 2>/dev/null || true" EXIT
  destination=org.freedesktop.impl.portal.desktop.gnosix
  base=/org/freedesktop/portal/desktop
  iface=org.freedesktop.impl.portal.FileChooser
  for i in $(seq 1 80); do
    if busctl --user introspect "$destination" "$base" "$iface" >/dev/null 2>&1; then break; fi
    sleep 0.05
  done
  call() {
    busctl --user call "$destination" "$base" "$iface" "$2" "osssa{sv}" \
      "$base/request/1_123/gfc02_$1" "org.gnosix.smoke" "" "$1" 0
  }
  call alpha OpenFile > "$GFC02_FIXTURE/alpha.out" &
  a=$!
  call slow OpenFile > "$GFC02_FIXTURE/slow.out" &
  s=$!
  call beta OpenFile > "$GFC02_FIXTURE/beta.out" &
  b=$!
  path="$base/request/1_123/gfc02_slow"
  found=false
  for i in $(seq 1 100); do
    if busctl --user introspect "$destination" "$path" "org.freedesktop.impl.portal.Request" >/dev/null 2>&1; then
      found=true
      break
    fi
    sleep 0.02
  done
  test "$found" = true
  busctl --user call "$destination" "$path" \
    org.freedesktop.impl.portal.Request Close
  wait "$a"; wait "$s"; wait "$b"
  call relative OpenFile > "$GFC02_FIXTURE/relative.out"
  call save SaveFile > "$GFC02_FIXTURE/save.out"
  grep -q "ua{sv} 0" "$GFC02_FIXTURE/alpha.out"
  grep -q "ua{sv} 0" "$GFC02_FIXTURE/beta.out"
  grep -q "ua{sv} 1 0" "$GFC02_FIXTURE/slow.out"
  grep -q "ua{sv} 2 0" "$GFC02_FIXTURE/relative.out"
  grep -q "ua{sv} 2 0" "$GFC02_FIXTURE/save.out"
  grep -q "alpha.txt" "$GFC02_FIXTURE/alpha.out"
  grep -q "beta.txt" "$GFC02_FIXTURE/beta.out"
  for case in alpha slow beta relative save; do
    echo "$case: $(cat "$GFC02_FIXTURE/$case.out")"
  done
  echo "GFC-02 private-bus concurrent mock smoke PASS"
'
