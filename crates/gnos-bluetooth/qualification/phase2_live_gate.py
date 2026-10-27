#!/usr/bin/env python3
"""BLUETOOTH-01 Phase 2 read-only live qualification."""

from __future__ import annotations

import datetime as dt
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / "evidence/runs/gnosix-bluetooth-01-phase2"
RECEIPT = EVIDENCE / "receipt.json"
LOG = EVIDENCE / "observer.log"
MAC = re.compile(r"(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}")
ANSI = re.compile(r"\x1b\[[0-9;]*m")

def run(*args: str, check: bool = True, timeout: int = 30) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        check=check, timeout=timeout,
    )

def source_head() -> str:
    return run("git", "rev-parse", "HEAD").stdout.strip()

def tree_state() -> tuple[bool, str]:
    status = run("git", "status", "--porcelain").stdout
    return bool(status), status.rstrip("\n")
def clean_lines(output: str) -> list[str]:
    return [ANSI.sub("", line).strip() for line in output.splitlines() if line.strip()]

def device_addresses() -> list[str]:
    addresses = []
    for line in clean_lines(run("bluetoothctl", "devices").stdout):
        parts = line.split()
        if len(parts) >= 2 and parts[0] == "Device" and MAC.fullmatch(parts[1]):
            addresses.append(parts[1])
    return addresses

def count_devices(filter_name: str | None = None) -> int:
    args = ["bluetoothctl", "devices"]
    if filter_name:
        args.append(filter_name)
    return sum(1 for line in clean_lines(run(*args).stdout) if line.startswith("Device "))

def adapter_oracle() -> tuple[int, int]:
    controllers = []
    for line in clean_lines(run("bluetoothctl", "list").stdout):
        parts = line.split()
        if len(parts) >= 2 and parts[0] == "Controller" and MAC.fullmatch(parts[1]):
            controllers.append(parts[1])
    powered = 0
    for address in controllers:
        output = clean_lines(run("bluetoothctl", "show", address).stdout)
        powered += any(line == "Powered: yes" for line in output)
    return len(controllers), powered

def rssi_oracle() -> int:
    present = 0
    for address in device_addresses():
        output = clean_lines(run("bluetoothctl", "info", address).stdout)
        present += any(line.startswith("RSSI:") for line in output)
    return present
def parse_line(line: str) -> dict[str, str]:
    fields = {}
    for token in line.split():
        if "=" in token:
            key, value = token.split("=", 1)
            fields[key] = value
    return fields

def observer_binary() -> Path:
    metadata = json.loads(run("cargo", "metadata", "--format-version", "1", "--no-deps").stdout)
    return Path(metadata["target_directory"]) / "debug" / "gnos-bluetooth-observe"

def run_observer_window(seconds: int = 8) -> tuple[list[str], bool]:
    run("cargo", "build", "-q", "-p", "gnos-bluetooth", "--bin", "gnos-bluetooth-observe", timeout=120)
    proc = subprocess.Popen(
        [str(observer_binary())],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    time.sleep(seconds)
    proc.terminate()
    try:
        output, _ = proc.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, 9)
        output, _ = proc.communicate(timeout=5)
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    return lines, proc.poll() is not None

def provider_owner() -> str:
    for line in run("busctl", "--system", "list").stdout.splitlines():
        if line.startswith("org.bluez "):
            parts = line.split()
            if len(parts) > 4:
                return parts[4]
    return "UNKNOWN"

def source_is_read_only() -> bool:
    source = (ROOT / "crates/gnos-bluetooth/src/bluez_observer.rs").read_text()
    runner = (ROOT / "crates/gnos-bluetooth/src/runner.rs").read_text()
    forbidden = ["StartDiscovery", "StopDiscovery", "RemoveDevice", ".Set(", "Pair()", "Connect()", "Disconnect()"]
    return not any(token in source or token in runner for token in forbidden)
def main() -> int:
    head = source_head()
    dirty, status = tree_state()
    before_adapters, before_powered = adapter_oracle()
    before = {
        "adapters": before_adapters,
        "powered": before_powered,
        "devices": count_devices(),
        "connected": count_devices("Connected"),
        "paired": count_devices("Paired"),
        "bonded": count_devices("Bonded"),
        "trusted": count_devices("Trusted"),
        "rssi_available": rssi_oracle(),
    }

    lines, reaped = run_observer_window()
    baseline = parse_line(lines[0]) if lines else {}
    after_adapters, after_powered = adapter_oracle()
    after = {
        "adapters": after_adapters,
        "powered": after_powered,
        "devices": count_devices(),
        "connected": count_devices("Connected"),
        "paired": count_devices("Paired"),
        "bonded": count_devices("Bonded"),
        "trusted": count_devices("Trusted"),
        "rssi_available": rssi_oracle(),
    }

    def integer(name: str) -> int | None:
        try:
            return int(baseline[name])
        except (KeyError, ValueError):
            return None

    changed_lines = [line for line in lines[1:] if parse_line(line).get("changed") == "true"]
    assertions = {
        "source_tree_clean_before_gate": not dirty,
        "baseline_observed": bool(baseline),
        "baseline_provider_available": baseline.get("provider") == "Available",
        "baseline_owner_matches_live_provider": baseline.get("owner") == provider_owner(),
        "baseline_is_not_a_change": baseline.get("changed") == "false",
        "baseline_is_marked_baselined": baseline.get("baselined") == "true",
        "adapter_count_matches_oracle": integer("adapters") == before["adapters"],
        "powered_adapter_count_matches_oracle": integer("powered") == before["powered"],
        "device_count_matches_oracle": integer("devices") == before["devices"],
        "connected_count_matches_oracle": integer("connected") == before["connected"],
        "paired_count_matches_oracle": integer("paired") == before["paired"],
        "bonded_count_matches_oracle": integer("bonded") == before["bonded"],
        "trusted_count_matches_oracle": integer("trusted") == before["trusted"],
        "missing_rssi_is_not_fabricated": integer("rssi_available") == before["rssi_available"],
        "idle_has_no_semantic_transition": not changed_lines,
        "observer_output_contains_no_hardware_address": not any(MAC.search(line) for line in lines),
        "observer_source_is_read_only": source_is_read_only(),
        "host_state_unchanged": before == after,
        "observer_reaped": reaped,
    }
    result = "PASS" if all(assertions.values()) else "FAIL"
    signal_status = "OBSERVED" if len(lines) > 1 else "NOT_OBSERVED_WITHIN_WINDOW"
    now = dt.datetime.now(dt.timezone.utc)
    receipt = {
        "ticket": "BLUETOOTH-01-PHASE2-LIVE",
        "observed_at_utc": now.isoformat(),
        "source_head": head,
        "tree_dirty_before_gate": dirty,
        "tree_status_before_gate": status,
        "provider_package": run("pacman", "-Q", "bluez").stdout.strip(),
        "provider_owner": provider_owner(),
        "window_seconds": 8,
        "result": result,
        "passed": sum(assertions.values()),
        "total": len(assertions),
        "assertions": assertions,
        "signal_delivery": signal_status,
        "observer_line_count": len(lines),
        "oracle_before": before,
        "oracle_after": after,
        "mutations_performed": [],
        "privacy": "Receipt and observer log contain no Bluetooth hardware addresses or device names.",
    }

    EVIDENCE.mkdir(parents=True, exist_ok=True)
    sanitized = [line for line in lines if not MAC.search(line)]
    LOG.write_text("\n".join(sanitized) + ("\n" if sanitized else ""))
    payload = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    RECEIPT.write_text(payload)
    archive = EVIDENCE / f"receipt-{now.strftime('%Y%m%dT%H%M%SZ')}.json"
    archive.write_text(payload)

    for name, ok in assertions.items():
        print(("PASS " if ok else "FAIL ") + name)
    print(f"SIGNAL_DELIVERY={signal_status}")
    print(f"BLUETOOTH_PHASE2_LIVE_GATE={result}")
    return 0 if result == "PASS" else 1

if __name__ == "__main__":
    raise SystemExit(main())
