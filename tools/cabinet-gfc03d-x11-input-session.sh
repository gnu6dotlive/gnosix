#!/usr/bin/env bash
set -euo pipefail
export PATH=/usr/bin:/bin
umask 077
mode="$CABINET_INPUT_MODE"
case "$mode" in save|cancel) ;; *) echo "unsupported input oracle mode" >&2; exit 64;; esac
ev="$CABINET_EVIDENCE_DIR"
binary="$CABINET_BINARY"
test -x "$binary"
test -x "$CABINET_XVFB_BIN"
test ! -e "$ev"
mkdir -p "$ev"
ip link set lo up
display_number=$((90 + BASHPID % 90))
export DISPLAY=":$display_number" XAUTHORITY="$ev/xauth" WAYLAND_DISPLAY="cabinet-x11-$BASHPID"
test ! -S "/tmp/.X11-unix/X$display_number" || { echo "XVFB_DISPLAY_ALREADY_BOUND"; exit 20; }
xpid=0; wpid=0; cpid=0
stop_one() {
  local p="$1" pg
  if (( p>0 )) && kill -0 "$p" 2>/dev/null; then
    pg=$(ps -o pgid= -p "$p" | tr -d " ")
    if [ "$pg" = "$p" ]; then kill -TERM -- "-$p" 2>/dev/null || true; else kill -TERM "$p" 2>/dev/null || true; fi
  fi
}
cleanup() {
  local rc=$?
  trap - EXIT
  stop_one "$cpid"
  stop_one "$wpid"
  stop_one "$xpid"
  sleep .2
  stop_one "$cpid"
  stop_one "$wpid"
  stop_one "$xpid"
  wait "$cpid" 2>/dev/null || true
  wait "$wpid" 2>/dev/null || true
  wait "$xpid" 2>/dev/null || true
  echo "X11_ORACLE_CLEANUP_EXIT_$rc"
  exit "$rc"
}
trap cleanup EXIT
cookie=$(head -c 16 /dev/urandom | /usr/bin/od -An -tx1 | tr -d " \n")
touch "$XAUTHORITY"
xauth -f "$XAUTHORITY" add "$DISPLAY" . "$cookie"
setsid "$CABINET_XVFB_BIN" "$DISPLAY" -screen 0 1280x960x24 -nolisten tcp -auth "$XAUTHORITY" >"$ev/xvfb.log" 2>&1 &
xpid=$!
for i in $(seq 1 80); do
  xprop -root >/dev/null 2>&1 && break
  kill -0 "$xpid" 2>/dev/null || { echo "XSERVER_DIED"; tail -20 "$ev/xvfb.log"; exit 21; }
  sleep .08
done
xprop -root >/dev/null
echo "XVFB_PRIVATE_SCREEN_PASS"
setsid weston --backend=x11 --no-config --idle-time=0 --renderer=pixman --width=980 --height=720 --socket="$WAYLAND_DISPLAY" --log="$ev/weston.log" >"$ev/weston.stdout" 2>&1 &
wpid=$!
for i in $(seq 1 80); do
  test -S "/run/user/1000/$WAYLAND_DISPLAY" && break
  kill -0 "$wpid" 2>/dev/null || { echo WESTON_DIED; tail -30 "$ev/weston.log"; exit 22; }
  sleep .08
done
test -S "/run/user/1000/$WAYLAND_DISPLAY"
echo "WESTON_X11_PRIVATE_SOCKET_PASS"
window_id=$(grep -oE 'window id [0-9]+' "$ev/weston.log" | tail -1 | awk '{print $3}')
test -n "$window_id"
for i in $(seq 1 80); do
  xdotool getwindowgeometry --shell "$window_id" >"$ev/x11-window-geometry.txt" 2>/dev/null && break
  kill -0 "$wpid" 2>/dev/null || { echo WESTON_DIED_AFTER_SOCKET; exit 24; }
  sleep .08
done
test -s "$ev/x11-window-geometry.txt"
printf '%s\n' "$window_id" >"$ev/x11-window-id.txt"
echo WESTON_X11_WINDOW_ID="$window_id"
cat "$ev/x11-window-geometry.txt"
xdotool getwindowname "$window_id"
echo "WESTON_X11_INPUT_SEAT_PREFLIGHT_PASS"

mkdir -p "$ev/fixture"
printf 'sample\n' > "$ev/fixture/alpha.txt"
python3 - "$ev/request.json" "$ev/fixture" <<'PY'
import json,sys
request={
  "title":"Cabinet GFC-03D Real Keyboard Oracle",
  "app_id":"in.gnu.os.cabinet.qualify",
  "initial_folder":sys.argv[2],
  "multiple":False, "directory":False,"kind":"save_file",
  "filename":"initial.txt", "requested_files":[],
  "accept_label":"Save", "filters":[],
  "selected_filter":None, "choices":[]
}
with open(sys.argv[1],"w") as f:json.dump(request,f)
PY
setsid env -u DISPLAY WAYLAND_DISPLAY="$WAYLAND_DISPLAY" WAYLAND_DEBUG=client "$binary" <"$ev/request.json" >"$ev/response.json" 2>"$ev/renderer.stderr" &
cpid=$!
for i in $(seq 1 110); do
  grep -q 'xdg_surface.*ack_configure' "$ev/renderer.stderr" 2>/dev/null && break
  kill -0 "$cpid" 2>/dev/null || { echo "RENDERER_PREMATURE_EXIT"; tail -25 "$ev/renderer.stderr"; exit 30; }
  sleep .08
done
grep -q 'xdg_surface.*ack_configure' "$ev/renderer.stderr" || { echo "NO_TOPLEVEL_ACK"; exit 31; }
for i in $(seq 1 110); do
  grep -qE 'wl_keyboard#[0-9]+\.enter' "$ev/renderer.stderr" 2>/dev/null && break
  kill -0 "$cpid" 2>/dev/null || { echo "RENDERER_LOST_BEFORE_KEYBOARD_ENTER"; exit 32; }
  sleep .08
done
grep -qE 'wl_keyboard#[0-9]+\.enter' "$ev/renderer.stderr" || { echo "KEYBOARD_ENTER_MISSING"; exit 32; }
echo "WAYLAND_KEYBOARD_ENTER_PASS"
xdotool windowfocus --sync "$window_id"
test "$(xdotool getwindowfocus)" = "$window_id" || { echo "FOCUS_NOT_IN_ISOLATED_WESTON"; exit 32; }
sleep .25
if [ "$mode" = cancel ]; then
  xdotool key --clearmodifiers --delay 100 Escape
else
  xdotool key --clearmodifiers --delay 80 ctrl+a
  xdotool type --clearmodifiers --delay 50 "cabinet-e2e-save.txt"
  xdotool key --clearmodifiers --delay 80 Return
fi
for i in $(seq 1 35); do
  grep -qE 'wl_keyboard#[0-9]+\.key\(' "$ev/renderer.stderr" 2>/dev/null && break
  sleep .06
done
if ! grep -qE 'wl_keyboard#[0-9]+\.key\(' "$ev/renderer.stderr"; then echo "NO_WAYLAND_KEY_FROM_XTEST"; tail -19 "$ev/renderer.stderr"; exit 34; fi
echo "WAYLAND_XTEST_KEY_EVENT_PASS"
for i in $(seq 1 100); do
  test -s "$ev/response.json" && break
  kill -0 "$cpid" 2>/dev/null || break
  sleep .08
done
test -s "$ev/response.json" || { echo "NO_RENDERER_RESPONSE_AFTER_KEY_INPUT"; tail -30 "$ev/renderer.stderr"; exit 33; }
python3 - "$ev/response.json" "$ev/fixture" "$mode" <<'PY'
import json,sys
from pathlib import Path
response=json.load(open(sys.argv[1]))
mode=sys.argv[3]
if mode=="cancel":
    assert response=={"outcome":"cancelled"},response
    print("GFC03D_REAL_WAYLAND_ESCAPE_CANCEL_PASS")
else:
    destination=Path(sys.argv[2])/"cabinet-e2e-save.txt"
    expected={"outcome":"selected","paths":[str(destination)]}
    assert response==expected,(response,expected)
    assert not destination.exists(),"Cabinet may select, but must not create the file"
    print("GFC03D_REAL_WAYLAND_SAVEFILE_TEXT_PASS")
PY
git rev-parse HEAD >"$ev/source-head.txt"
sha256sum "$binary" "$CABINET_XVFB_BIN" >"$ev/binary-sha256.txt"
