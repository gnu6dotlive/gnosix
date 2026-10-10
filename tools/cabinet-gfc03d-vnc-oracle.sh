#!/usr/bin/env bash
# VNC preflight runs inside its own network namespace; never uses host display.
set -euo pipefail
root="$CABINET_VNC_EVIDENCE_DIR"
libdir="$CABINET_VNC_LIB_DIR"
test ! -e "$root" || { echo "Evidence path exists" >&2; exit 3; }
test -e "$libdir/libaml.so.1"
test -e "$libdir/libneatvnc.so.1"
mkdir -p "$root"
export CABINET_VNC_EVIDENCE_DIR="$root"
export LD_LIBRARY_PATH="$libdir"
unshare -Urn -- /usr/bin/bash -lc '
  set -euo pipefail
  ip link set lo up
  socket="cabinet-gfc03d-vnc-$$"
  weston --backend=vnc --no-config --idle-time=0 --debug \
    --width=960 --height=720 --port=5963 \
    --disable-transport-layer-security --socket="$socket" \
    --log="$CABINET_VNC_EVIDENCE_DIR/weston.log" \
    >"$CABINET_VNC_EVIDENCE_DIR/weston.stdout" 2>&1 &
  pid=$!
  trap "kill $pid 2>/dev/null || true; wait $pid 2>/dev/null || true" EXIT
  for i in $(seq 1 80); do
    test -S "/run/user/1000/$socket" && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if ! test -S "/run/user/1000/$socket"; then
    echo "GFC03D_VNC_COMPOSITOR_START_FAIL"
    tail -25 "$CABINET_VNC_EVIDENCE_DIR/weston.log"
    exit 20
  fi
  timeout --signal=TERM 12s vncdo -s 127.0.0.1::5963 -t 6 \
    capture "$CABINET_VNC_EVIDENCE_DIR/first-frame.png" \
    >"$CABINET_VNC_EVIDENCE_DIR/vnc-client.log" 2>&1
  test -s "$CABINET_VNC_EVIDENCE_DIR/first-frame.png" || {
    echo "GFC03D_VNC_CAPTURE_MISSING"
    exit 21
  }
  kill -0 "$pid" 2>/dev/null || {
    echo "GFC03D_VNC_COMPOSITOR_EXITED"
    exit 22
  }
  echo "GFC03D_VNC_TRANSPORT_AND_FRAME_PASS"
'
