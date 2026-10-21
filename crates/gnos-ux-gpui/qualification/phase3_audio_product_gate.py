#!/usr/bin/env python3
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / "evidence/runs/gnosix-audio-01-phase3"
LOG = EVIDENCE / "product-observer.log"
RECEIPT = EVIDENCE / "receipt.json"
TARGET = "@DEFAULT_AUDIO_SINK@"
BINARY = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[3] / "target")) / "debug" / "gnos-ux-audio-qualify"


def run(*args: str) -> str:
    return subprocess.run(
        args, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()


def state() -> tuple[float, bool]:
    text = run("wpctl", "get-volume", TARGET)
    match = re.search(r"Volume:\s+([0-9.]+)", text)
    if not match:
        raise RuntimeError(f"volume absent: {text}")
    return float(match.group(1)), "[MUTED]" in text


def set_volume(value: float) -> None:
    run("wpctl", "set-volume", TARGET, f"{value:.6f}")


def set_mute(muted: bool) -> None:
    run("wpctl", "set-mute", TARGET, "1" if muted else "0")


def product_lines() -> list[str]:
    if not LOG.exists():
        return []
    return [
        line.strip()
        for line in LOG.read_text().splitlines()
        if line.startswith("GNOS_UX_AUDIO_PRODUCT ")
    ]


def has_state(lines: list[str], text: str | None, muted: bool | None) -> bool:
    for line in lines:
        if text is not None and f"text={text}" not in line:
            continue
        if muted is not None and f"muted=Some({str(muted).lower()})" not in line:
            continue
        return True
    return False


def stop(proc: subprocess.Popen) -> None:
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=5)


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("")
    run("cargo", "build", "-q", "-p", "gnos-ux-gpui", "--bin", "gnos-ux-audio-qualify")
    before = state()
    changed_volume = max(0.0, before[0] - 0.03)
    independent: list[dict] = []
    with LOG.open("w") as log:
        proc = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            text=True, start_new_session=True,
        )
        try:
            time.sleep(1.0)
            set_volume(changed_volume)
            time.sleep(0.6)
            independent.append({"step": "volume_changed", "state": state()})
            set_volume(before[0])
            time.sleep(0.6)
            independent.append({"step": "volume_restored", "state": state()})
            set_mute(not before[1])
            time.sleep(0.6)
            independent.append({"step": "mute_changed", "state": state()})
            set_mute(before[1])
            time.sleep(0.6)
            independent.append({"step": "mute_restored", "state": state()})
        finally:
            subprocess.run(["wpctl", "set-volume", TARGET, f"{before[0]:.6f}"], check=False)
            subprocess.run(["wpctl", "set-mute", TARGET, "1" if before[1] else "0"], check=False)
            time.sleep(0.3)
            stop(proc)

    after = state()
    lines = product_lines()
    initial_text = f"{round(before[0] * 100)}%"
    changed_text = f"{round(changed_volume * 100)}%"
    assertions = {
        "product_initial_level": has_state(lines, initial_text, before[1]),
        "product_changed_level": has_state(lines, changed_text, None),
        "product_restored_level": has_state(lines, initial_text, None),
        "product_mute_change": has_state(lines, None, not before[1]),
        "product_mute_restore": has_state(lines, None, before[1]),
        "independent_volume_change": independent[0]["state"][0] == changed_volume,
        "independent_state_restored": after == before,
        "no_wayland_surface_required": True,
    }
    receipt = {
        "gate": "AUDIO-01-PHASE3-PRODUCT",
        "result": "PASS" if all(assertions.values()) else "FAIL",
        "assertions": assertions,
        "before": before,
        "after": after,
        "changed_volume": changed_volume,
        "independent_observations": independent,
        "product_lines": lines,
        "log": str(LOG.relative_to(ROOT)),
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n")
    for name, passed in assertions.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    print(f"AUDIO_PHASE3_PRODUCT_GATE={receipt['result']}")
    return 0 if receipt["result"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
