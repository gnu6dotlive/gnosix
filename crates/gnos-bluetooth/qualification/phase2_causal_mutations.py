#!/usr/bin/env python3
"""Causal negative gates for BLUETOOTH-01 Phase 2."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / "evidence/runs/gnosix-bluetooth-01-phase2"
LOG = EVIDENCE / "causal-mutations.log"
RECEIPT = EVIDENCE / "causal-mutations.json"

def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        check=False, timeout=120,
    )

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def replace_once(path: Path, old: str, new: str) -> bytes:
    original = path.read_bytes()
    text = original.decode()
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor count for {path}: {text.count(old)}")
    path.write_text(text.replace(old, new, 1))
    return original
CASES = [
    {
        "name": "B3_BLOCKED_IS_NOT_OFF",
        "file": "crates/gnos-bluetooth/src/model.rs",
        "old": '"off-blocked" => Self::Blocked,',
        "new": '"off-blocked" => Self::Off,',
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "model_contract", "power_state_decoding_is_verified_only"],
    },
    {
        "name": "B1_PROVIDER_LOSS_IS_NOT_AVAILABLE",
        "file": "crates/gnos-bluetooth/src/reducer.rs",
        "old": "self.provider = BluetoothProviderState::Unavailable;",
        "new": "self.provider = BluetoothProviderState::Available;",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "model_contract", "provider_loss_is_not_adapter_off"],
    },
    {
        "name": "B5_OWNER_REPLACEMENT_DROPS_RUNTIME_OBJECTS",
        "file": "crates/gnos-bluetooth/src/reducer.rs",
        "old": "self.owner = owner.map(str::to_owned);\n        self.adapters.clear();\n        self.devices.clear();\n        self.baselined = false;",
        "new": "self.owner = owner.map(str::to_owned);\n        self.baselined = false;",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "model_contract", "provider_replacement_invalidates_runtime_objects"],
    },
    {
        "name": "B4_RUNTIME_IDENTITY_INCLUDES_PROVIDER_OWNER",
        "file": "crates/gnos-bluetooth/src/model.rs",
        "old": "provider_owner: provider_owner.into(),",
        "new": "provider_owner: String::from(\":fixed\"),",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "model_contract", "runtime_identity_is_provider_scoped"],
    },
    {
        "name": "B8_PASSIVE_OBSERVER_HAS_NO_DISCOVERY",
        "file": "crates/gnos-bluetooth/src/bluez_observer.rs",
        "old": "//! Native, read-only BlueZ observer.",
        "new": "//! Native, read-only BlueZ observer. StartDiscovery",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "runner_contract", "observer_and_runner_are_read_only_and_event_driven_by_source"],
    },
    {
        "name": "B11_EMPTY_OBJECT_STREAM_CANNOT_SPIN",
        "file": "crates/gnos-bluetooth/src/runner.rs",
        "old": "Some(_) = object_events.next(), if !object_events.is_empty() => {",
        "new": "Some(_) = object_events.next() => {",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "runner_contract", "observer_and_runner_are_read_only_and_event_driven_by_source"],
    },
    {
        "name": "B12_PROTOCOL_VOCABULARY_STAYS_AT_BOUNDARY",
        "file": "crates/gnos-bluetooth/src/model.rs",
        "old": "//! Typed BlueZ read model for Gnosix.",
        "new": "//! Typed BlueZ read model for Gnosix. org.bluez",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "observer_contract", "product_model_does_not_import_the_bluez_protocol"],
    },
    {
        "name": "B13_DEVICE_REQUIRES_CURRENT_ADAPTER",
        "file": "crates/gnos-bluetooth/src/reducer.rs",
        "old": "                    && adapters.contains_key(&device.adapter)",
        "new": "                    && true",
        "command": ["cargo", "test", "-q", "-p", "gnos-bluetooth", "--test", "model_contract", "device_without_observed_adapter_is_rejected"],
    },
]
def main() -> int:
    head = run("git", "rev-parse", "HEAD").stdout.strip()
    status_before = run("git", "status", "--porcelain").stdout
    tree_clean_before = not status_before
    logs = []
    results = {}

    for case in CASES:
        path = ROOT / case["file"]
        before_hash = sha(path)
        original = replace_once(path, case["old"], case["new"])
        try:
            result = run(*case["command"])
            red = result.returncode != 0
            logs.append(
                f"=== {case['name']} rc={result.returncode} expected=RED ===\n"
                + result.stdout[-5000:]
            )
        finally:
            path.write_bytes(original)
        restored = sha(path) == before_hash
        results[case["name"]] = {"red": red, "restored": restored}
        if not restored:
            raise RuntimeError(f"failed to restore {case['file']}")

    status_after = run("git", "status", "--porcelain").stdout
    tree_same_after = status_after == status_before
    passed = sum(v["red"] and v["restored"] for v in results.values())
    result_name = "PASS" if tree_clean_before and tree_same_after and passed == len(CASES) else "FAIL"
    now = dt.datetime.now(dt.timezone.utc)
    receipt = {
        "ticket": "BLUETOOTH-01-PHASE2-CAUSAL-MUTATIONS",
        "observed_at_utc": now.isoformat(),
        "source_head": head,
        "tree_clean_before": tree_clean_before,
        "tree_same_after": tree_same_after,
        "result": result_name,
        "passed": passed,
        "total": len(CASES),
        "cases": results,
    }

    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("\n".join(logs) + "\n")
    RECEIPT.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")

    for name, value in results.items():
        print(f"{'PASS' if value['red'] and value['restored'] else 'FAIL'} {name}")
    print(f"BLUETOOTH_PHASE2_CAUSAL_MUTATIONS={result_name}")
    return 0 if result_name == "PASS" else 1

if __name__ == "__main__":
    raise SystemExit(main())
