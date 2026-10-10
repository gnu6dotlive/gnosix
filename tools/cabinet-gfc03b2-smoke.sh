#!/usr/bin/env bash
# Private GFC-03B/02 choices D-Bus ABI qualification.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
backend=/mnt/sandbox/builds/cargo/debug/gnosix-portal-filechooser
test -x "$backend"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
printf 'fixture\n' > "$tmp/alpha.txt"
GFC02_FIXTURE="$tmp" GNOSIX_CABINET_RENDERER="$root/tools/gfc-02-mock-renderer.py" \
GFC_CHOICES_BACKEND="$backend" dbus-run-session -- bash -euo pipefail -c '
  "$GFC_CHOICES_BACKEND" > "$GFC02_FIXTURE/backend.log" 2>&1 &
  backend_pid=$!
  trap "kill $backend_pid 2>/dev/null || true; wait $backend_pid 2>/dev/null || true" EXIT
  for i in $(seq 1 80); do
    if busctl --user introspect org.freedesktop.impl.portal.desktop.gnosix \
       /org/freedesktop/portal/desktop org.freedesktop.impl.portal.FileChooser >/dev/null 2>&1; then break; fi
    sleep .05
  done
  python3 - <<"PY"
from concurrent.futures import ThreadPoolExecutor
import subprocess
base="/org/freedesktop/portal/desktop"
dest="org.freedesktop.impl.portal.desktop.gnosix"
interface="org.freedesktop.impl.portal.FileChooser"
def call(title, wrong_start=False):
    # Busctl serializes choices as a(ssa(ss)s).
    options = ["choices", "a(ssa(ss)s)", "2",
        "encoding","Encoding","2",
        "utf8","Unicode UTF8","latin15","Western",("absent" if wrong_start else "latin15"),
        "reencode","Reencode","0","false"]
    args=["busctl","--user","call",dest,base,interface,"OpenFile",
        "osssa{sv}",base+"/request/1_123/gfc03b_choices_"+title,
        "in.gnu.os.cabinet.qualify","",title,"1",*options]
    res=subprocess.run(args,capture_output=True,text=True,timeout=9)
    if res.returncode:
        raise RuntimeError(f"{title}: {res.stderr}; args={args}")
    return res.stdout.strip()
with ThreadPoolExecutor(max_workers=2) as pool:
    keep=pool.submit(call,"choice_keep")
    switch=pool.submit(call,"choice_switch")
    a,b=keep.result(),switch.result()
assert a.startswith("ua{sv} 0") and "choices" in a and "encoding" in a and "latin15" in a and "reencode" in a and "false" in a,a
assert b.startswith("ua{sv} 0") and "utf8" in b and "true" in b,b
bad_value=call("choice_invalid_value")
bad_id=call("choice_wrong_id")
bad_start=call("choice_bad_start",wrong_start=True)
for value in [bad_value,bad_id,bad_start]:
    assert value.startswith("ua{sv} 2 0"),value
for name,value in [("DEFAULT",a),("SWITCH",b),("INVALID_VALUE",bad_value),("FORGED_ID",bad_id),("INVALID_START",bad_start)]:
    print(name,value)
print("CABINET_GFC03B02_CHOICES_DBUS_PASS")
PY
'
