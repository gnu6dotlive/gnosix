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
EVIDENCE = ROOT / "evidence/runs/gnosix-audio-01-phase2"
LOG = EVIDENCE / "observer.log"
RECEIPT = EVIDENCE / "receipt.json"
OUT = "@DEFAULT_AUDIO_SINK@"
IN = "@DEFAULT_AUDIO_SOURCE@"
PROBE = "gnos_audio_phase2_probe"


def run(*args: str) -> str:
    return subprocess.run(
        args, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()


def node_name(target: str) -> str:
    text = run("wpctl", "inspect", target)
    match = re.search(r'node\.name = "([^"]+)"', text)
    if not match:
        raise RuntimeError(f"node.name absent for {target}")
    return match.group(1)


def audio_state(target: str) -> tuple[float, bool]:
    text = run("wpctl", "get-volume", target)
    match = re.search(r"Volume:\s+([0-9.]+)", text)
    if not match:
        raise RuntimeError(f"volume absent for {target}: {text}")
    return float(match.group(1)), "[MUTED]" in text


def set_volume(target: str, value: float) -> None:
    run("wpctl", "set-volume", target, f"{value:.6f}")


def set_mute(target: str, muted: bool) -> None:
    run("wpctl", "set-mute", target, "1" if muted else "0")


def snapshots() -> list[dict]:
    result = []
    if not LOG.exists():
        return result
    for line in LOG.read_text().splitlines():
        if line.startswith("GNOS_AUDIO_SNAPSHOT "):
            result.append(json.loads(line.split(" ", 1)[1]))
    return result


def effective_endpoint(snapshot: dict, kind: str) -> dict | None:
    resolved = snapshot["defaults"][kind]["effective"]["resolved"]
    if resolved is None:
        return None
    for endpoint in snapshot["endpoints"]:
        if endpoint["id"] == resolved:
            return endpoint
    return None


def cubic_level(endpoint: dict | None) -> float | None:
    if endpoint is None:
        return None
    volume = endpoint["volume"]
    channels = volume.get("channels") or []
    linear = channels[0] if channels else volume.get("scalar")
    return None if linear is None else float(linear) ** (1.0 / 3.0)


def near(a: float | None, b: float, eps: float = 0.015) -> bool:
    return a is not None and abs(a - b) <= eps


def stop_observer(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    os.killpg(proc.pid, signal.SIGTERM)
    proc.wait(timeout=5)


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("")
    out_name_before = node_name(OUT)
    in_name_before = node_name(IN)
    out_before = audio_state(OUT)
    in_before = audio_state(IN)
    target_volume = max(0.0, out_before[0] - 0.03)
    module_id: str | None = None
    independent: list[dict] = []
    with LOG.open("w") as log:
        proc = subprocess.Popen(
            ["cargo", "run", "-q", "-p", "gnos-audio", "--bin", "gnos-audio-observe"],
            cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, text=True,
            start_new_session=True,
        )
        try:
            time.sleep(1.0)
            set_volume(OUT, target_volume)
            time.sleep(0.6)
            independent.append({"step": "output_volume_changed", "state": audio_state(OUT)})
            set_volume(OUT, out_before[0])
            time.sleep(0.6)
            independent.append({"step": "output_volume_restored", "state": audio_state(OUT)})

            set_mute(OUT, not out_before[1])
            time.sleep(0.6)
            independent.append({"step": "output_mute_changed", "state": audio_state(OUT)})
            set_mute(OUT, out_before[1])
            time.sleep(0.6)
            independent.append({"step": "output_mute_restored", "state": audio_state(OUT)})

            set_mute(IN, not in_before[1])
            time.sleep(0.6)
            independent.append({"step": "input_mute_changed", "state": audio_state(IN)})
            set_mute(IN, in_before[1])
            time.sleep(0.6)
            independent.append({"step": "input_mute_restored", "state": audio_state(IN)})

            module_id = run(
                "pactl", "load-module", "module-null-sink",
                f"sink_name={PROBE}", "sink_properties=device.description=GnosixPhase2Probe",
            )
            time.sleep(0.8)
            run("pactl", "unload-module", module_id)
            module_id = None
            time.sleep(0.8)
        finally:
            if module_id:
                subprocess.run(["pactl", "unload-module", module_id], check=False)
            subprocess.run(["wpctl", "set-volume", OUT, f"{out_before[0]:.6f}"], check=False)
            subprocess.run(["wpctl", "set-mute", OUT, "1" if out_before[1] else "0"], check=False)
            subprocess.run(["wpctl", "set-mute", IN, "1" if in_before[1] else "0"], check=False)
            time.sleep(0.4)
            stop_observer(proc)

    snaps = snapshots()
    out_name_after = node_name(OUT)
    in_name_after = node_name(IN)
    out_after = audio_state(OUT)
    in_after = audio_state(IN)
    states = []
    for snapshot in snaps:
        out = effective_endpoint(snapshot, "output")
        inp = effective_endpoint(snapshot, "input")
        states.append({
            "generation": snapshot["generation"],
            "output_level": cubic_level(out),
            "output_muted": None if out is None else out["muted"],
            "input_muted": None if inp is None else inp["muted"],
            "probe_present": any(PROBE in e["node_name"] for e in snapshot["endpoints"]),
        })

    def after_index(predicate, start: int = -1) -> int | None:
        for index, state in enumerate(states):
            if index > start and predicate(state):
                return index
        return None

    volume_changed = after_index(lambda s: near(s["output_level"], target_volume))
    volume_restored = after_index(lambda s: near(s["output_level"], out_before[0]), volume_changed or -1)
    output_mute_changed = after_index(lambda s: s["output_muted"] == (not out_before[1]))
    output_mute_restored = after_index(lambda s: s["output_muted"] == out_before[1], output_mute_changed or -1)
    input_mute_changed = after_index(lambda s: s["input_muted"] == (not in_before[1]))
    input_mute_restored = after_index(lambda s: s["input_muted"] == in_before[1], input_mute_changed or -1)
    probe_added = after_index(lambda s: s["probe_present"])
    probe_removed = after_index(lambda s: not s["probe_present"], probe_added or -1)

    final = snaps[-1] if snaps else None
    assertions = {
        "initial_snapshot_has_resolved_defaults": bool(final)
        and final["defaults"]["output"]["effective"]["resolved"] is not None
        and final["defaults"]["input"]["effective"]["resolved"] is not None,
        "output_volume_change_observed": volume_changed is not None,
        "output_volume_restore_observed": volume_restored is not None,
        "output_mute_change_observed": output_mute_changed is not None,
        "output_mute_restore_observed": output_mute_restored is not None,
        "input_mute_change_observed": input_mute_changed is not None,
        "input_mute_restore_observed": input_mute_restored is not None,
        "probe_endpoint_added": probe_added is not None,
        "probe_endpoint_removed": probe_removed is not None,
        "defaults_unchanged": out_name_before == out_name_after and in_name_before == in_name_after,
        "independent_state_restored": out_after == out_before and in_after == in_before,
        "exact_identity_shape_present": bool(final)
        and all(e["id"]["server_cookie"] > 0 and e["id"]["object_serial"] > 0 for e in final["endpoints"]),
    }
    receipt = {
        "gate": "AUDIO-01-PHASE2",
        "result": "PASS" if all(assertions.values()) else "FAIL",
        "assertions": assertions,
        "before": {"output": [out_name_before, *out_before], "input": [in_name_before, *in_before]},
        "after": {"output": [out_name_after, *out_after], "input": [in_name_after, *in_after]},
        "target_output_volume": target_volume,
        "independent_observations": independent,
        "state_sequence": states,
        "observer_log": str(LOG.relative_to(ROOT)),
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n")
    for name, passed in assertions.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    print(f"AUDIO_PHASE2_GATE={receipt['result']}")
    return 0 if receipt["result"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
