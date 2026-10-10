#!/usr/bin/env bash
# Cabinet GFC-03: a private session bus; never mutates the real user's portal.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
backend=/mnt/sandbox/builds/cargo/debug/gnosix-portal-filechooser
test -x "$backend"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
printf 'exists\n' > "$tmp/alpha.txt"
GFC02_FIXTURE="$tmp" GNOSIX_CABINET_RENDERER="$root/tools/gfc-02-mock-renderer.py" GFC03_BACKEND="$backend" dbus-run-session -- bash -euo pipefail -c '
 "$GFC03_BACKEND" > "$GFC02_FIXTURE/backend.log" 2>&1 &
 server=$!
 trap "kill $server 2>/dev/null || true; wait $server 2>/dev/null || true" EXIT
 for i in $(seq 1 100); do
   if busctl --user introspect org.freedesktop.impl.portal.desktop.gnosix /org/freedesktop/portal/desktop org.freedesktop.impl.portal.FileChooser >/dev/null 2>&1; then break; fi
   sleep .05
 done
 python3 - <<"PY"
from concurrent.futures import ThreadPoolExecutor
import subprocess
base="/org/freedesktop/portal/desktop"
dest="org.freedesktop.impl.portal.desktop.gnosix"
iface="org.freedesktop.impl.portal.FileChooser"
def call(method, title, options):
    args=["busctl","--user","call",dest,base,iface,method,"osssa{sv}",
          base+"/request/1_123/gfc03_"+title,"in.gnu.os.cabinet.smoke","",title,str(len(options))]
    for key,ty,val in options:
        args.extend([key,ty])
        if ty=="aay":
            args.append(str(len(val)))
            for name in val:
                raw=name.encode("utf8")+bytes([0])
                args.append(str(len(raw)))
                args.extend(map(str,raw))
        else:
            args.append(str(val))
    run=subprocess.run(args,text=True,capture_output=True,timeout=9)
    if run.returncode: raise RuntimeError(f"{method} {title} {run.stderr}; {args}")
    return run.stdout.strip()
with ThreadPoolExecutor(max_workers=2) as pool:
    file=pool.submit(call,"SaveFile","save_one",[("current_name","s","new-cabinet-file.txt")])
    files=pool.submit(call,"SaveFiles","save_many",[("files","aay",["z-last.txt","a-first.txt"])])
    a=file.result()
    b=files.result()
assert a.startswith("ua{sv} 0") and "new-cabinet-file.txt" in a,a
assert b.startswith("ua{sv} 0") and "z-last.txt" in b and "a-first.txt" in b,b
assert b.index("z-last.txt") < b.index("a-first.txt"),b
c=call("SaveFiles","save_collision",[("files","aay",["alpha.txt"])])
assert c.startswith("ua{sv} 2 0"),c
d=call("SaveFiles","save_traversal",[("files","aay",["../bad.txt"])])
assert d.startswith("ua{sv} 2 0"),d
for label,value in [("SAVEFILE",a),("SAVEFILES",b),("COLLISION",c),("TRAVERSAL",d)]:
    print(label,value)
print("CABINET_GFC03A_ISOLATED_DBUS_PASS")
PY
'
