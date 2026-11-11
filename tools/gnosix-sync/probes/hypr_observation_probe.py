#!/usr/bin/env python3
"""Hyprland observation probe — does Hyprland still answer the exact batch
Gnosix reads?

Read-only. Sends to $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock
the same bytes as gnosix-hyprland's ReadQuery::SemanticSnapshot (and the
single queries), then reports what came back the way the Rust parser sees it:
a stream of concatenated JSON values, of which the semantic snapshot needs
exactly 12. Anything between values that is not whitespace is reported, since
it is what makes the parser refuse the reply.

A masked copy of each reply is written as a fixture: window titles are
replaced (title, initialTitle); everything else is structure.

Usage: python3 -I hypr_observation_probe.py [--out DIR] [--socket PATH]
"""

import argparse
import json
import os
import re
import socket
import sys

SEMANTIC = ("[[BATCH]]j/monitors ; j/workspaces ; j/clients ; j/activeworkspace ; "
            "j/activewindow ; j/layers ; j/monitors ; j/workspaces ; j/clients ; "
            "j/activeworkspace ; j/activewindow ; j/layers")
SINGLES = ["j/monitors", "j/workspaces", "j/clients", "j/activeworkspace",
           "j/activewindow", "j/layers", "j/devices", "j/version"]
MASKED_KEYS = {"title", "initialTitle", "lastwindowtitle"}
EXPECTED = 12


def ask(path: str, wire: str, timeout: float = 2.0) -> bytes:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(timeout)
    s.connect(path)
    s.sendall(wire.encode())
    out = b""
    while True:
        chunk = s.recv(65536)
        if not chunk:
            return out
        out += chunk


def split_values(text: str):
    """Concatenated JSON values, as serde_json's StreamDeserializer reads them."""
    dec = json.JSONDecoder()
    values, junk, i = [], [], 0
    while True:
        j = i
        while j < len(text) and text[j].isspace():
            j += 1
        if j >= len(text):
            return values, junk, None
        try:
            value, end = dec.raw_decode(text, j)
        except json.JSONDecodeError as err:
            return values, junk, (j, text[j:j + 160], str(err))
        values.append(value)
        i = end


TITLE_RE = re.compile(r'"(title|initialTitle|lastwindowtitle)"\s*:\s*"(?:[^"\\]|\\.)*"')


def scrub(text: str) -> str:
    """Raw reply text with every title value masked, for excerpts."""
    return TITLE_RE.sub(lambda m: f'"{m.group(1)}": "<masqué>"', text)


def mask(value):
    if isinstance(value, dict):
        return {k: ("<masqué>" if k in MASKED_KEYS and isinstance(v, str) and v else mask(v))
                for k, v in value.items()}
    if isinstance(value, list):
        return [mask(v) for v in value]
    return value


def shape(value) -> str:
    if isinstance(value, list):
        keys = sorted(value[0].keys()) if value and isinstance(value[0], dict) else []
        return f"liste[{len(value)}] clés={keys}"
    if isinstance(value, dict):
        return f"objet clés={sorted(value.keys())}"
    return type(value).__name__


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.expanduser(
        "~/.local/state/gnosix-sync/hypr-observation"))
    ap.add_argument("--socket", default=None)
    args = ap.parse_args()
    path = args.socket
    if not path:
        sig = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")
        run = os.environ.get("XDG_RUNTIME_DIR")
        if not sig or not run:
            sys.exit("HYPRLAND_INSTANCE_SIGNATURE ou XDG_RUNTIME_DIR absent")
        path = os.path.join(run, "hypr", sig, ".socket.sock")
    os.makedirs(args.out, exist_ok=True)

    raw = ask(path, SEMANTIC)
    text = raw.decode(errors="replace")
    values, _, error = split_values(text)
    print(f"LOT SÉMANTIQUE : {len(raw)} octets, {len(values)} valeurs JSON "
          f"(attendu {EXPECTED}) → {'CONFORME' if len(values) == EXPECTED and not error else 'REFUSÉ PAR LE PARSEUR'}")
    if error:
        pos, excerpt, msg = error
        print(f"  arrêt à l'octet {pos} : {msg}")
        print(f"  texte à cet endroit (titres masqués) : {scrub(text[pos:pos + 400])[:160]!r}")
    for n, v in enumerate(values, 1):
        print(f"  valeur {n:2} : {shape(v)}")
    with open(os.path.join(args.out, "semantic-batch.masked.json"), "w") as f:
        json.dump([mask(v) for v in values], f, indent=1)
    with open(os.path.join(args.out, "semantic-batch.raw-head.txt"), "w") as f:
        f.write("\n".join(f"{i}: {t[:200]}" for i, t in
                          enumerate(scrub(text).splitlines()[:5])) + "\n")

    for q in SINGLES:
        r = ask(path, q).decode(errors="replace")
        vals, _, err = split_values(r)
        status = "ok" if len(vals) == 1 and not err else f"valeurs={len(vals)} erreur={bool(err)}"
        print(f"{q:20} {len(r):7} octets  {status}  {shape(vals[0]) if vals else repr(scrub(r)[:120])}")
        if vals:
            with open(os.path.join(args.out, q.replace('/', '_') + ".masked.json"), "w") as f:
                json.dump(mask(vals[0]), f, indent=1)
    print(f"jeux d'essai masqués : {args.out}")


if __name__ == "__main__":
    main()
