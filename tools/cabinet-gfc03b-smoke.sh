#!/usr/bin/env bash
# GFC-03B: isolated D-Bus session; no compositor or live portal mutation.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
backend="${GFC_BINARY:-/mnt/sandbox/builds/cargo/debug/gnosix-portal-filechooser}"
test -x "$backend"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
printf 'text\n' > "$tmp/alpha.txt"
printf 'png\n' > "$tmp/pixel.png"
GFC02_FIXTURE="$tmp" GNOSIX_CABINET_RENDERER="$root/tools/gfc-02-mock-renderer.py" \
GFC03B_BACKEND="$backend" dbus-run-session -- bash -euo pipefail -c '
  "$GFC03B_BACKEND" > "$GFC02_FIXTURE/backend.log" 2>&1 &
  server=$!
  trap "kill $server 2>/dev/null || true; wait $server 2>/dev/null || true" EXIT
  for i in $(seq 1 80); do
    if busctl --user introspect org.freedesktop.impl.portal.desktop.gnosix \
      /org/freedesktop/portal/desktop org.freedesktop.impl.portal.FileChooser >/dev/null 2>&1; then break; fi
    sleep 0.05
  done
  python3 - <<"PY"
from concurrent.futures import ThreadPoolExecutor
import subprocess
base="/org/freedesktop/portal/desktop"
destination="org.freedesktop.impl.portal.desktop.gnosix"
interface="org.freedesktop.impl.portal.FileChooser"
filters=[("Text",[(0,"*.txt")]),("Images",[(1,"image/png")])]
def field(name,rules):
    values=[name,str(len(rules))]
    for kind,pattern in rules: values += [str(kind),pattern]
    return values
def call(title, current=None, bad_glob=False):
    candidates=[("Bad",[(0,"[")])] if bad_glob else filters
    options=["filters","a(sa(us))",str(len(candidates))]
    for name,rules in candidates: options+=field(name,rules)
    count=1
    if current is not None:
        count+=1
        options+=["current_filter","(sa(us))"]+field(*candidates[current])
    args=["busctl","--user","call",destination,base,interface,"OpenFile",
          "osssa{sv}",base+"/request/1_123/gfc03b_"+title,
          "in.gnu.os.cabinet.qualify","",title,str(count),*options]
    result=subprocess.run(args,capture_output=True,text=True,timeout=9)
    if result.returncode:
        raise RuntimeError(f"{title}: {result.stderr}; args={args}")
    return result.stdout.strip()
with ThreadPoolExecutor(max_workers=2) as pool:
    text=pool.submit(call,"filter_text",0)
    png=pool.submit(call,"filter_image",1)
    a=text.result()
    b=png.result()
assert a.startswith("ua{sv} 0") and "alpha.txt" in a,a
assert "current_filter" in a and "Text" in a and "*.txt" in a,a
assert b.startswith("ua{sv} 0") and "pixel.png" in b,b
assert "current_filter" in b and "Images" in b and "image/png" in b,b
invalid=call("filter_invalid",bad_glob=True)
assert invalid.startswith("ua{sv} 2 0"),invalid
stale=call("filter_bad_index",1)
assert stale.startswith("ua{sv} 2 0"),stale
for key,value in [("TEXT",a),("IMAGE",b),("MALFORMED",invalid),("BAD_INDEX",stale)]:
    print(key,value)
print("CABINET_GFC03B_FILTERS_DBUS_PASS")
PY
'
