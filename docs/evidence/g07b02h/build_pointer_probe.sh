#!/bin/sh
# Build the exact Wayland protocol client used in the isolated G07B-02G2 test.
set -eu
SELF=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${1:-/tmp/g07b02g2-pointer-constraints-probe}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
XDG=/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml
PC=/usr/share/wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml
test -r "$XDG"
test -r "$PC"
wayland-scanner client-header "$XDG" "$TMP/xdg-shell-client-protocol.h"
wayland-scanner private-code "$XDG" "$TMP/xdg-shell-protocol.c"
wayland-scanner client-header "$PC" "$TMP/pointer-constraints-client-protocol.h"
wayland-scanner private-code "$PC" "$TMP/pointer-constraints-protocol.c"
cc -std=gnu11 -Wall -Wextra -I"$TMP" \
  "$SELF/pointer_constraints_probe.c" "$TMP/xdg-shell-protocol.c" \
  "$TMP/pointer-constraints-protocol.c" -o "$OUT" -lwayland-client -lm
echo "BUILT $OUT"
