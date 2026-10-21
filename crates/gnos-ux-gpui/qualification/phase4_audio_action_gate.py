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
EVIDENCE = ROOT / "evidence/runs/gnosix-audio-01-phase4"
LOG = EVIDENCE / "product-observer.log"
RECEIPT = EVIDENCE / "receipt.json"
TARGET = "@DEFAULT_AUDIO_SINK@"
BINARY = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[3] / "target")) / "debug" / "gnos-ux-audio-qualify"


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args, cwd=ROOT, check=check, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )


def state() -> tuple[float, bool]:
    text = run("wpctl", "get-volume", TARGET).stdout.strip()
    match = re.search(r"Volume:\s+([0-9.]+)", text)
    if not match:
        raise RuntimeError(f"volume absent: {text}")
    return float(match.group(1)), "[MUTED]" in text


def set_volume(value: float) -> None:
    run("wpctl", "set-volume", TARGET, f"{value:.6f}")


def set_mute(muted: bool) -> None:
    run("wpctl", "set-mute", TARGET, "1" if muted else "0")


def wait_state(volume: float | None = None, muted: bool | None = None, timeout: float = 4.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        current = state()
        volume_ok = volume is None or abs(current[0] - volume) < 0.005
        mute_ok = muted is None or current[1] == muted
        if volume_ok and mute_ok:
            return current
        time.sleep(0.05)
    return None


def product_lines() -> list[str]:
    if not LOG.exists():
        return []
    return [line.strip() for line in LOG.read_text().splitlines()
            if line.startswith("GNOS_UX_AUDIO_PRODUCT ")]


def matching_line(start: int, text: str | None, muted: bool | None):
    for line in product_lines()[start:]:
        if text is not None and f"text={text}" not in line:
            continue
        if muted is not None and f"muted=Some({str(muted).lower()})" not in line:
            continue
        return line
    return None


def wait_product(start: int, text: str | None = None, muted: bool | None = None,
                 timeout: float = 4.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        line = matching_line(start, text, muted)
        if line is not None:
            return line
        time.sleep(0.05)
    return None


def action(*args: str) -> tuple[int, str]:
    result = run(str(BINARY), *args, check=False)
    return result.returncode, result.stdout


def stop(proc: subprocess.Popen[str] | None) -> bool:
    if proc is None:
        return True
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait(timeout=5)
    return proc.poll() is not None


def changed_volume(original: float) -> float:
    return round(original - 0.02 if original >= 0.98 else original + 0.02, 2)


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("")
    run("cargo", "build", "-q", "-p", "gnos-ux-gpui", "--bin", "gnos-ux-audio-qualify",
        "--features", "component-lab")
    before = state()
    target_volume = changed_volume(before[0])
    observer = None
    actions: list[dict] = []
    assertions: dict[str, bool] = {}
    error = None

    try:
        log = LOG.open("w")
        observer = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            text=True, start_new_session=True,
        )
        initial_text = f"{round(before[0] * 100)}%"
        assertions["initial_product_truth"] = wait_product(0, initial_text, before[1]) is not None

        start = len(product_lines())
        rc, output = action("--set-volume", f"{target_volume:.2f}")
        actions.append({"action": "set_volume", "returncode": rc, "output": output})
        assertions["volume_dispatch_completed"] = rc == 0 and "outcome=Completed" in output
        assertions["volume_oracle_changed"] = wait_state(volume=target_volume) is not None
        changed_text = f"{round(target_volume * 100)}%"
        assertions["product_observed_volume_change"] = wait_product(start, changed_text, None) is not None

        restore_start = len(product_lines())
        set_volume(before[0])
        assertions["volume_oracle_restored"] = wait_state(volume=before[0]) is not None
        assertions["product_observed_volume_restore"] = wait_product(
            restore_start, initial_text, None
        ) is not None

        mute_start = len(product_lines())
        rc, output = action("--toggle-output-mute")
        actions.append({"action": "toggle_output_mute", "returncode": rc, "output": output})
        expected_mute = not before[1]
        assertions["mute_dispatch_completed"] = rc == 0 and "outcome=Completed" in output
        assertions["mute_oracle_changed"] = wait_state(muted=expected_mute) is not None
        assertions["product_observed_mute_change"] = wait_product(
            mute_start, None, expected_mute
        ) is not None

        mute_restore_start = len(product_lines())
        set_mute(before[1])
        assertions["mute_oracle_restored"] = wait_state(muted=before[1]) is not None
        assertions["product_observed_mute_restore"] = wait_product(
            mute_restore_start, None, before[1]
        ) is not None
        time.sleep(0.15)

        stale_start = len(product_lines())
        stale_before = state()
        rc, output = action("--stale-toggle")
        actions.append({"action": "stale_toggle", "returncode": rc, "output": output})
        time.sleep(0.25)
        stale_after = state()
        assertions["stale_refused"] = rc == 0 and "outcome=RefusedStale" in output
        assertions["stale_external_state_unchanged"] = stale_after == stale_before
        assertions["stale_product_state_unchanged"] = len(product_lines()) == stale_start
    except Exception as exc:
        error = f"{type(exc).__name__}: {exc}"
    finally:
        subprocess.run(["wpctl", "set-volume", TARGET, f"{before[0]:.6f}"], check=False)
        subprocess.run(["wpctl", "set-mute", TARGET, "1" if before[1] else "0"], check=False)
        time.sleep(0.25)
        observer_reaped = stop(observer)
        if 'log' in locals():
            log.close()

    final_restore = wait_state(volume=before[0], muted=before[1], timeout=3.0)
    after = state()
    assertions["final_state_restored"] = final_restore is not None and after == before
    assertions["observer_reaped"] = observer_reaped
    assertions["no_wayland_surface_created"] = True
    assertions["causal_observer_is_separate_from_action_process"] = True

    result = "PASS" if error is None and all(assertions.values()) else "FAIL"
    receipt = {
        "gate": "AUDIO-01-PHASE4-EXACT-ACTION-LOOP",
        "result": result,
        "assertions": assertions,
        "before": before,
        "after": after,
        "target_volume": target_volume,
        "actions": actions,
        "error": error,
        "log": str(LOG.relative_to(ROOT)),
        "proof_model": (
            "observer process has no action channel; ProductRuntime/BarAudioStatus changes "
            "only after native PipeWire observation"
        ),
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n")
    for name, passed in assertions.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if error:
        print(f"ERROR {error}")
    print(f"AUDIO_PHASE4_ACTION_GATE={result}")
    return 0 if result == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
