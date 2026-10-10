#!/usr/bin/env python3
"""Independent D-Bus ABI oracle, compared against freedesktop's backend XML."""
import json
import subprocess
import sys
import xml.etree.ElementTree as ET

DEST = "org.freedesktop.impl.portal.desktop.gnosix"
PATH = "/org/freedesktop/portal/desktop"
INTERFACE = "org.freedesktop.impl.portal.FileChooser"
METHODS = {
    method: {"in": ["o", "s", "s", "s", "a{sv}"], "out": ["u", "a{sv}"]}
    for method in ("OpenFile", "SaveFile", "SaveFiles")
}

def main() -> int:
    result = subprocess.run([
        "busctl", "--user", "introspect", "--xml-interface",
        DEST, PATH, INTERFACE,
    ], text=True, capture_output=True, check=True, timeout=10)
    document = ET.fromstring(result.stdout)
    interface = document.find(f"./interface[@name='{INTERFACE}']")
    if interface is None:
        raise AssertionError(f"missing {INTERFACE}")
    actual = {}
    for method in interface.findall("method"):
        args = method.findall("arg")
        actual[method.get("name")] = {
            direction: [arg.get("type") for arg in args
                        if arg.get("direction") == direction]
            for direction in ("in", "out")
        }
    if actual != METHODS:
        raise AssertionError(f"ABI drift: expected={METHODS!r}, actual={actual!r}")
    report = {
        "oracle": "freedesktop FileChooser backend method signatures",
        "source": "https://github.com/flatpak/xdg-desktop-portal/blob/main/data/org.freedesktop.impl.portal.FileChooser.xml",
        "interface": INTERFACE,
        "result": "ABI_METHOD_SIGNATURES_PASS",
        "verified": actual,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0

if __name__ == "__main__":
    sys.exit(main())
