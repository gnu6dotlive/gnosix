#!/usr/bin/env python3
"""B1 click probe — which signal, if any, reaches Gnosix when the operator
clicks somewhere?

Read-only. Listens at the same time to:
  - Hyprland's event socket (.socket2.sock);
  - the gnos-ux-scene journal (journalctl --user -f), noisy receipts dropped.
Before each click the operator presses Enter; the probe writes a numbered
marker with millisecond wall-clock time, then gives a fixed window for the
click. Every line carries the same clock, so click N maps to its events
without guessing.

Window titles are personal data: they are never written. Only the window
class is kept; title events are reduced to their name.

Usage: python3 -I b1_click_probe.py [--out FILE] [--window SECONDS]
"""

import argparse
import datetime
import os
import socket
import subprocess
import sys
import threading
import time

STEPS = [
    "clic dans la fenêtre DÉJÀ focalisée (écran LG)",
    "clic dans une AUTRE fenêtre du même écran (LG)",
    "clic sur le fond vide, sans fenêtre (LG)",
    "clic dans une fenêtre de l'écran Acer (DP-2)",
    "clic sur le fond vide (Acer)",
    "clic sur une zone VIDE du Bar (pas un bouton)",
    "clic sur une zone VIDE du Deck (pas une carte)",
    "contrôle : NE PAS cliquer, ne rien toucher",
]

NOISY = (
    "DASHBOARD_MEDIA",
    "MPRIS_OBSERVED",
    "NETWORK_RECEIPT",
    "STATUS_RECEIPT",
    "GNOS_UX_DECK ",
)

lock = threading.Lock()


def now() -> str:
    return datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]


def emit(out, source: str, text: str) -> None:
    line = f"{now()} {source:7} {text}"
    with lock:
        out.write(line + "\n")
        out.flush()


def redact_hypr(event: str) -> str:
    name, _, payload = event.partition(">>")
    if name == "activewindow":
        return f"{name}>>{payload.split(',', 1)[0]},<titre masqué>"
    if name.startswith("windowtitle") or name == "openwindow":
        return f"{name}>><masqué>"
    return event


def socket2_path() -> str:
    sig = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")
    run = os.environ.get("XDG_RUNTIME_DIR")
    if not sig or not run:
        sys.exit("HYPRLAND_INSTANCE_SIGNATURE ou XDG_RUNTIME_DIR absent")
    return os.path.join(run, "hypr", sig, ".socket2.sock")


def listen_hypr(path: str, out, stop: threading.Event) -> None:
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(path)
    sock.settimeout(0.2)
    buf = b""
    while not stop.is_set():
        try:
            chunk = sock.recv(4096)
        except socket.timeout:
            continue
        if not chunk:
            emit(out, "hypr", "!! socket fermé")
            return
        buf += chunk
        while b"\n" in buf:
            raw, buf = buf.split(b"\n", 1)
            emit(out, "hypr", redact_hypr(raw.decode(errors="replace")))


def listen_journal(cmd, out, stop: threading.Event) -> None:
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, text=True)
    try:
        for line in proc.stdout:
            if stop.is_set():
                break
            if "GNOS_UX" in line and not any(n in line for n in NOISY):
                emit(out, "scene", line.rstrip()[-400:])
    finally:
        proc.terminate()


def main() -> None:
    ap = argparse.ArgumentParser()
    state = os.path.expanduser("~/.local/state/gnosix-sync")
    ap.add_argument("--out", default=os.path.join(state, "b1-probe.log"))
    ap.add_argument("--window", type=float, default=4.0)
    ap.add_argument("--socket", default=None, help="test only")
    ap.add_argument("--journal-cmd", default=None, help="test only")
    args = ap.parse_args()

    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    stop = threading.Event()
    with open(args.out, "w", encoding="utf-8") as out:
        emit(out, "probe", f"début ; fenêtre par clic {args.window:.1f} s")
        journal = (
            args.journal_cmd.split()
            if args.journal_cmd
            else ["journalctl", "--user", "-u", "gnos-ux-scene", "-f", "-n", "0",
                  "-o", "cat", "--no-pager"]
        )
        threads = [
            threading.Thread(target=listen_hypr,
                             args=(args.socket or socket2_path(), out, stop),
                             daemon=True),
            threading.Thread(target=listen_journal, args=(journal, out, stop),
                             daemon=True),
        ]
        for t in threads:
            t.start()
        time.sleep(1.0)
        for n, step in enumerate(STEPS, 1):
            print(f"\n[{n}/{len(STEPS)}] {step}")
            input("    Place la souris, puis appuie sur Entrée et fais le geste "
                  f"dans les {args.window:.0f} s. Ne bouge plus ensuite. ")
            emit(out, "MARK", f"### {n} DÉBUT — {step}")
            time.sleep(args.window)
            emit(out, "MARK", f"### {n} FIN")
        emit(out, "probe", "fin")
        stop.set()
    print(f"\njournal : {args.out}")


if __name__ == "__main__":
    main()
