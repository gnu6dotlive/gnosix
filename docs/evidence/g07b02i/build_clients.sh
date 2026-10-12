#!/bin/sh
set -eu
SELF=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${1:?Provide output directory}
mkdir -p "$OUT"
W=$OUT/xdg-shell
C=$OUT/pointer-constraints
R=$OUT/relative-pointer
wayland-scanner client-header /usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml "$W-client-protocol.h"
wayland-scanner private-code /usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml "$W-protocol.c"
cc -std=gnu11 -Wall -Wextra -I"$OUT" "$SELF/dnd_native.c" "$W-protocol.c" -lwayland-client -o "$OUT/dnd-native"
wayland-scanner client-header /usr/share/wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml "$C-client-protocol.h"
wayland-scanner private-code /usr/share/wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml "$C-protocol.c"
wayland-scanner client-header /usr/share/wayland-protocols/unstable/relative-pointer/relative-pointer-unstable-v1.xml "$R-client-protocol.h"
wayland-scanner private-code /usr/share/wayland-protocols/unstable/relative-pointer/relative-pointer-unstable-v1.xml "$R-protocol.c"
cc -std=gnu11 -Wall -Wextra -I"$OUT" "$SELF/relative_lock_probe.c" "$W-protocol.c" "$C-protocol.c" "$R-protocol.c" -lwayland-client -o "$OUT/relative-lock-probe"
sha256sum "$OUT/dnd-native" "$OUT/relative-lock-probe"
