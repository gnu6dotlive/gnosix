#!/usr/bin/env bash
# Isolated Weston smoke test; only creates a private compositor socket.
set -euo pipefail
out="${CABINET_EVIDENCE_DIR:-/mnt/workbench/evidence/runs/cabinet-gfc02b-headless-20261010}"
binary="${CABINET_BINARY:-/mnt/sandbox/builds/cargo/debug/gnosix-cabinet}"
test -x "$binary" || { echo "missing Cabinet executable" >&2; exit 2; }
if test -e "$out"; then echo "evidence destination exists: $out" >&2; exit 3; fi
mkdir -p "$out"
socket="cabinet-smoke-${BASHPID}-${RANDOM}"
export XDG_RUNTIME_DIR=/run/user/1000
wpid=
cpid=
cleanup() {
  if test -n "$cpid"; then kill "$cpid" 2>/dev/null || true; wait "$cpid" 2>/dev/null || true; fi
  if test -n "$wpid"; then kill "$wpid" 2>/dev/null || true; wait "$wpid" 2>/dev/null || true; fi
}
trap cleanup EXIT
weston --debug --backend=headless --fake-seat --renderer=gl --no-config --idle-time=0 --socket="$socket" --log="$out/weston.log" >"$out/weston.stdout" 2>&1 &
wpid=$!
for i in $(seq 1 80); do test -S "$XDG_RUNTIME_DIR/$socket" && break; sleep .1; done
if ! test -S "$XDG_RUNTIME_DIR/$socket"; then echo "WESTON_SOCKET_FAILURE"; tail -30 "$out/weston.log"; exit 12; fi
mkdir -p "$out/fixture/folder"
printf 'demo\n' > "$out/fixture/readme.txt"
printf 'image\n' > "$out/fixture/graphic.png"
python3 - "$out/request.json" "$out/fixture" <<'PY'
import json,sys
import os
mode = os.environ.get("CABINET_SMOKE_MODE", "open")
if mode not in ("open", "save"):
    raise ValueError("CABINET_SMOKE_MODE must be open or save")
req = {
  "title": "Cabinet Filtered Wayland Qualifier",
  "app_id": "in.gnu.os.cabinet.qualify",
  "initial_folder": sys.argv[2],
  "multiple": False,
  "directory": False,
  "kind": "open_file",
  "filename": "",
  "requested_files": [],
  "accept_label": "",
  "filters": [
     {"name": "Text", "rules": [[0, "*.txt"]]},
     {"name": "Images", "rules": [[1, "image/png"]]}
  ],
  "selected_filter": 0,
  "choices": [
    {"id":"encoding", "label":"Encoding",
     "options":[["utf8","Unicode"], ["latin15","Western"]],
     "selected":"latin15"},
    {"id":"reencode","label":"Reencode","options":[],"selected":"false"}
  ]
}
if mode == "save":
  req.update({
     "title": "Cabinet Native Unicode / IME",
     "kind": "save_file",
     "directory": False,
     "filename": "Été 🦀 – rapport.md",
  })
with open(sys.argv[1],"w") as f: json.dump(req,f)
PY
test -s "$out/request.json"
WAYLAND_DISPLAY="$socket" WAYLAND_DEBUG=client timeout -s TERM 12s "$binary" <"$out/request.json" >"$out/renderer.stdout" 2>"$out/renderer.stderr" &
cpid=$!
sleep 3
if ! kill -0 "$cpid" 2>/dev/null; then
  echo "CABINET_PREMATURE_EXIT"
  tail -50 "$out/renderer.stderr" || true
  exit 20
fi
echo "CABINET_ALIVE_AT_3S"
(cd "$out" && WAYLAND_DISPLAY="$socket" timeout 6s weston-screenshooter >screenshooter.stdout 2>screenshooter.stderr) || true
printf 'TOPLEVEL_PROTOCOL_EVENTS\n'
grep -m 9 -E 'xdg_toplevel|xdg_surface|wl_surface@[0-9]+\.commit' "$out/renderer.stderr" || true
printf 'SCREENSHOT_FILES\n'
find "$out" -maxdepth 1 -iname '*.png' -printf '%f %s bytes\n'
printf 'RENDERER_ERRORS\n'
grep -Ei 'panic|fatal|error|failed|cannot' "$out/renderer.stderr" | tail -10 || true
echo CABINET_HEADLESS_LIFECYCLE_PROBE_COMPLETE
