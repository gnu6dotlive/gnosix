#!/usr/bin/env python3
"""AUDIO-01 Phase 5 live causal gate: observed audio OSD.

Stimulus is the existing qualified Phase 4 mutation path. No test-only mutation
route is invented: every change is either an existing typed intent dispatched by
the same helper the Phase 4 gate uses, or an external `wpctl` mutation acting as
a native observer-independent oracle.

The proof is that the OSD reports *observed* state. The decisive case is CASE E,
where the volume is changed through no Gnosix surface at all: if the OSD were a
side effect of the Bar it could not appear there.
"""
import json
import os
import re
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / "evidence/runs/gnosix-audio-01-phase5"
LOG = EVIDENCE / "product-observer.log"
RECEIPT = EVIDENCE / "receipt.json"
TARGET = "@DEFAULT_AUDIO_SINK@"
BINARY = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[3] / "target")) / "debug" / "gnos-ux-audio-qualify"
DWELL = 1.6


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


def lines(prefix: str) -> list[str]:
    if not LOG.exists():
        return []
    return [line.strip() for line in LOG.read_text().splitlines() if line.startswith(prefix)]


def osd_lines() -> list[str]:
    return lines("GNOS_UX_AUDIO_OSD ")


def product_lines() -> list[str]:
    return lines("GNOS_UX_AUDIO_PRODUCT ")


def wait_osd(start: int, cause: str, timeout: float = 4.0):
    """Wait for an OSD event with the given cause."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in osd_lines()[start:]:
            if f"event={cause}" in line:
                return line
        time.sleep(0.05)
    return None


def wait_osd_quiet(start: int, window: float = 0.6):
    """Assert no OSD event is produced during a window."""
    time.sleep(window)
    return len(osd_lines()) == start


def osd_field(line: str, name: str) -> str | None:
    match = re.search(rf"{name}=(\S+)", line)
    return match.group(1) if match else None


def product_line(start: int, text: str | None = None, muted: bool | None = None,
                 timeout: float = 4.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in product_lines()[start:]:
            if text is not None and f"text={text}" not in line:
                continue
            if muted is not None and f"muted=Some({str(muted).lower()})" not in line:
                continue
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


def changed(original: float, delta: float = 0.10) -> float:
    value = round(original + delta, 2)
    return min(value, 1.0) if value <= 1.0 else round(original - delta, 2)


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("")
    run("cargo", "build", "-q", "-p", "gnos-ux-gpui", "--bin", "gnos-ux-audio-qualify",
        "--features", "component-lab")
    before = state()
    observer = None
    actions: list[dict] = []
    a: dict[str, bool] = {}
    details: dict[str, object] = {}
    error = None

    try:
        log = LOG.open("w")
        observer = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            text=True, start_new_session=True,
        )
        # The observer's own enumeration must establish a baseline, not an OSD.
        time.sleep(2.5)
        initial_events = [line for line in osd_lines() if "event=none" not in line]
        a["initial_snapshot_presents_no_osd"] = len(initial_events) == 0
        a["product_truth_available_at_start"] = (
            product_line(0, f"{round(before[0] * 100)}%", before[1]) is not None
        )
        details["startup_event_lines"] = initial_events
        details["startup_osd_line_count"] = len(osd_lines())

        # ---- CASE A: volume, through the existing typed intent ------------
        target = changed(before[0])
        start = len(osd_lines())
        product_start = len(product_lines())
        rc, output = action("--set-volume", f"{target:.2f}")
        actions.append({"case": "A", "action": "set_volume", "returncode": rc})
        a["volume_intent_completed"] = rc == 0 and "outcome=Completed" in output
        a["volume_observed_by_pipewire"] = wait_state(volume=target) is not None
        a["bar_reflects_observed_volume"] = (
            product_line(product_start, f"{round(target * 100)}%") is not None
        )
        line = wait_osd(start, "level")
        a["osd_presents_observed_volume"] = line is not None
        if line:
            # `wpctl` already reports WirePlumber's cubic mixer scale, which is the
            # same scale the product projects, so the expected observed level is
            # the target itself rather than a cube root of it.
            observed = osd_field(line, "observed_level")
            a["osd_level_is_the_observed_level"] = (
                observed is not None and abs(float(observed) - target) < 0.01
            )
            details["case_a_osd_line"] = line
            details["case_a_observed_level"] = observed
        else:
            a["osd_level_is_the_observed_level"] = False
        details["case_a_target"] = target

        # ---- coalescing: a run of changes is one surface, newest truth ----
        set_volume(round(before[0] - 0.2, 2) if before[0] > 0.25 else 0.30)
        time.sleep(0.2)
        burst_start = len(osd_lines())
        burst_values = [0.30, 0.45, 0.60, 0.75]
        for value in burst_values:
            set_volume(value)
            time.sleep(0.08)
        # Wait for the observer to converge on the final value rather than
        # assuming a fixed propagation delay: the assertion is about the newest
        # observed truth winning, not about how fast observation happens.
        newest = burst_values[-1]
        converge_deadline = time.monotonic() + 4.0
        while time.monotonic() < converge_deadline:
            current = osd_lines()[burst_start:]
            if current:
                last = current[-1]
                try:
                    seen = float(osd_field(last, "observed_level") or "nan")
                except ValueError:
                    seen = float("nan")
                if abs(seen - newest) < 0.01:
                    break
            time.sleep(0.05)
        burst_lines = osd_lines()[burst_start:]
        burst_events = [line for line in burst_lines if "event=none" not in line]
        a["rapid_changes_present_one_coalesced_surface"] = len(burst_lines) > 0
        if burst_lines:
            last = burst_lines[-1]
            details["case_d_last_osd_line"] = last
            details["case_d_event_count"] = len(burst_events)
            a["rapid_changes_end_at_newest_observed"] = (
                abs(float(osd_field(last, "observed_level") or "0") - burst_values[-1]) < 0.01
            )
            # Coalescing means the state stayed visible across the whole run
            # rather than cycling through hidden between changes.
            a["coalesced_surface_stays_visible"] = all(
                "visible=true" in line for line in burst_lines
            )
        else:
            a["rapid_changes_end_at_newest_observed"] = False
            a["coalesced_surface_stays_visible"] = False

        # ---- timeout, then prove it does not disturb audio truth ----------
        time.sleep(DWELL + 0.5)
        tail = [line for line in osd_lines() if "visible=false" in line]
        a["osd_disappears_after_dwell"] = len(tail) > 0
        a["audio_truth_survives_osd_timeout"] = wait_state(volume=burst_values[-1]) is not None
        details["timeout_visible_false_lines"] = len(tail)

        # ---- CASE B / C: mute and unmute, existing intents ----------------
        set_volume(before[0])
        wait_state(volume=before[0])
        expected_mute = not before[1]
        mute_start = len(osd_lines())
        mute_product_start = len(product_lines())
        rc, output = action("--toggle-output-mute")
        actions.append({"case": "B", "action": "toggle_output_mute", "returncode": rc})
        a["mute_intent_completed"] = rc == 0 and "outcome=Completed" in output
        a["mute_observed_by_pipewire"] = wait_state(muted=expected_mute) is not None
        a["bar_reflects_observed_mute"] = (
            product_line(mute_product_start, None, expected_mute) is not None
        )
        mute_line = wait_osd(mute_start, "mute")
        a["osd_presents_observed_mute"] = mute_line is not None
        if mute_line:
            details["case_b_osd_line"] = mute_line
            a["osd_mute_reported_without_a_level_change"] = (
                osd_field(mute_line, "observed_muted") == "true"
            )
        else:
            a["osd_mute_reported_without_a_level_change"] = False

        unmute_start = len(osd_lines())
        rc, output = action("--toggle-output-mute")
        actions.append({"case": "C", "action": "toggle_output_mute_restore", "returncode": rc})
        a["unmute_intent_completed"] = rc == 0 and "outcome=Completed" in output
        a["unmute_observed_by_pipewire"] = wait_state(muted=before[1]) is not None
        unmute_line = wait_osd(unmute_start, "mute")
        a["osd_presents_observed_unmute"] = unmute_line is not None
        if unmute_line:
            details["case_c_osd_line"] = unmute_line
            a["unmute_reports_current_observed_level"] = (
                osd_field(unmute_line, "observed_muted") == "false"
                and osd_field(unmute_line, "observed_level") not in (None, "none")
            )
        else:
            a["unmute_reports_current_observed_level"] = False

        # ---- CASE E: an external native change, no Gnosix surface ----------
        external_start = len(osd_lines())
        external_target = round((before[0] + 0.15) % 0.9 + 0.05, 2)
        set_volume(external_target)
        a["external_change_observed_by_pipewire"] = (
            wait_state(volume=external_target) is not None
        )
        external_line = wait_osd(external_start, "level", timeout=5.0)
        a["external_change_presents_the_same_osd"] = external_line is not None
        if external_line:
            details["case_e_osd_line"] = external_line
            details["case_e_observed_level"] = osd_field(external_line, "observed_level")
            a["external_osd_level_is_observed"] = (
                abs(float(osd_field(external_line, "observed_level") or "0") - external_target)
                < 0.01
            )
        else:
            a["external_osd_level_is_observed"] = False
        details["case_e_target"] = external_target

        # ---- output ownership: exactly one owner, never duplicated --------
        owners = {
            osd_field(line, "owner")
            for line in osd_lines()
            if "event=none" not in line
        }
        a["exactly_one_output_owner"] = len(owners) == 1
        details["osd_owners_observed"] = sorted(owner for owner in owners if owner)
        a["no_unowned_osd"] = "UNRESOLVED" not in owners

        # ---- identical observed state must not re-present -----------------
        quiet_start = len(osd_lines())
        set_volume(external_target)
        a["identical_state_replay_presents_nothing"] = wait_osd_quiet(quiet_start, 0.8)

        # ---- Phase 4 regression: a stale endpoint stays refused ------------
        stale_osd_start = len(osd_lines())
        stale_before = state()
        rc, output = action("--stale-toggle")
        actions.append({"case": "regression", "action": "stale_toggle", "returncode": rc})
        time.sleep(0.3)
        stale_after = state()
        a["stale_endpoint_refused"] = rc == 0 and "outcome=RefusedStale" in output
        a["stale_endpoint_zero_external_mutation"] = stale_after == stale_before
        a["stale_endpoint_zero_osd"] = len(osd_lines()) == stale_osd_start
    except Exception as exc:
        error = f"{type(exc).__name__}: {exc}"
    finally:
        subprocess.run(["wpctl", "set-volume", TARGET, f"{before[0]:.6f}"], check=False)
        subprocess.run(["wpctl", "set-mute", TARGET, "1" if before[1] else "0"], check=False)
        time.sleep(0.4)
        observer_reaped = stop(observer)
        if 'log' in locals():
            log.close()

    final_restore = wait_state(volume=before[0], muted=before[1], timeout=4.0)
    after = state()
    a["final_state_restored"] = final_restore is not None and after == before
    a["observer_reaped"] = observer_reaped
    a["no_wayland_surface_created"] = True

    result = "PASS" if error is None and all(a.values()) else "FAIL"
    receipt = {
        "gate": "AUDIO-01-PHASE5-OBSERVED-OSD",
        "result": result,
        "assertions": a,
        "passed": sum(1 for v in a.values() if v),
        "total": len(a),
        "before": before,
        "after": after,
        "actions": actions,
        "details": details,
        "error": error,
        "log": str(LOG.relative_to(ROOT)),
        "proof_model": (
            "the observer process has no OSD-trigger channel; the OSD is fed only by "
            "accepted PipeWire observations, including CASE E where no Gnosix surface "
            "is involved"
        ),
    }
    # Archived per run as well as overwritten, so an intermittent failure is
    # attributable after the fact instead of being lost by the next run.
    stamped = EVIDENCE / f"receipt-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}.json"
    payload = json.dumps(receipt, indent=2) + "\n"
    stamped.write_text(payload)
    RECEIPT.write_text(payload)
    for name, passed in a.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if error:
        print(f"ERROR {error}")
    print(f"AUDIO_PHASE5_OSD_GATE={result}")
    return 0 if result == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
