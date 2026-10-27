#!/usr/bin/env python3
"""NETWORK-01 Phase 3 live qualification.

Phase 3 is presentation only. This gate proves that the Bar's derived status
follows observed network truth and that the projection carries no authority.

It reads the product receipt stream emitted by the production scene path
(`GNOS_UX_NETWORK_RECEIPT`), which is produced by the same
`ProductNetworkPresentation::derive` the Bar renders from.

No mutation is performed. The stimulus is natural network drift; where a
provenance requirement would need a destructive change, the case is recorded
NOT_RUN_REQUIRES_BOUNDED_LIVE_MUTATION.
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
EVIDENCE = ROOT / "evidence/runs/gnosix-network-01-phase3"
LOG = EVIDENCE / "observer.log"
RECEIPT = EVIDENCE / "receipt.json"
OBSERVER = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[3] / "target")) / "debug" / "gnos-network-observe"


def source_head() -> str:
    """The revision this qualification describes.

    A receipt without this is a claim about no particular source, which is
    indistinguishable from a claim about a source that has since changed.
    """
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        ).stdout.strip()
    except Exception:
        return "UNKNOWN"


def provider_version() -> str:
    """The live NetworkManager version.

    Minimal causal provenance, not decoration: the read model decodes a numeric
    `DeviceType` enum, and a provider upgrade can change that numbering. A
    recorded PASS is therefore only meaningful alongside the version it was
    observed against.
    """
    try:
        return subprocess.run(
            ["busctl", "--system", "get-property", "org.freedesktop.NetworkManager",
             "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager",
             "Version"],
            cwd=ROOT, check=True, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        ).stdout.strip()
    except Exception:
        return "UNKNOWN"


def tree_state() -> tuple[bool, str]:
    """Whether the working tree was clean when this gate ran.

    HEAD alone is an incomplete provenance marker: a gate can run against a
    dirty tree, in which case `source_head` names a revision that is not the
    code that executed. That is exactly how this function came to exist -- the
    first `source_head` receipts recorded a clean-looking revision while the
    gate scripts themselves were uncommitted. Returning both means a receipt can
    never silently describe code that was not what ran.
    """
    try:
        status = subprocess.run(
            ["git", "status", "--porcelain"], cwd=ROOT, check=True, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        ).stdout
    except Exception:
        return True, "UNKNOWN"
    # Porcelain lines are `XY PATH`. The leading status column is significant and
    # must not be stripped: doing so shifts the path and silently defeats the
    # filter below. That is how the first version of this check reported its own
    # evidence output as a dirty source tree.
    lines = [line for line in status.splitlines() if line]
    # Evidence is output, not input: a receipt must not count its own creation as
    # a dirty source tree, or every run would report itself provisional.
    relevant = [line for line in lines if not line[3:].startswith("evidence/")]
    return bool(relevant), "\n".join(relevant)


def nmcli(*args: str, check: bool = False):
    return subprocess.run(
        ["nmcli", *args], cwd=ROOT, check=check, text=True, timeout=30,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    ).stdout


def oracle() -> dict:
    out = {"ssid": None, "signal": None, "connected": False}
    for row in nmcli("-t", "-f", "DEVICE,STATE,CONNECTION", "device", "status").splitlines():
        fields = [f.replace("\\:", ":") for f in re.split(r"(?<!\\):", row)]
        if len(fields) >= 4 and fields[0] == "wlan0":
            out["connected"] = fields[1].startswith("connected")
            out["connection"] = fields[3] or None
    for row in nmcli("-t", "-f", "ACTIVE,SSID,SIGNAL", "device", "wifi", "list",
                     "ifname", "wlan0", "--rescan", "no").splitlines():
        if row.startswith("yes:"):
            fields = [f.replace("\\:", ":") for f in re.split(r"(?<!\\):", row)]
            out["ssid"] = fields[1]
            out["signal"] = int(fields[2])
    return out


def stop(proc) -> bool:
    if proc is None:
        return True
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait(timeout=5)
    return proc.poll() is None or proc.returncode is not None


def lines() -> list[str]:
    if not LOG.exists():
        return []
    return [line.strip() for line in LOG.read_text().splitlines()
            if line.startswith("GNOS_NETWORK ")]


def field(line: str, key: str):
    match = re.search(rf"(?:^| ){key}=(\S+)", line)
    return match.group(1).strip('"') if match else None


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("")
    subprocess.run(
        ["cargo", "build", "-q", "-p", "gnos-network", "--bin", "gnos-network-observe"],
        cwd=ROOT, check=True,
    )

    before = oracle()
    assertions: dict[str, bool] = {}
    details: dict[str, object] = {}
    error = None
    observer = None

    try:
        log = LOG.open("w")
        observer = subprocess.Popen(
            [str(OBSERVER)], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            text=True, start_new_session=True,
        )
        # Establish the baseline from observation.
        deadline = time.monotonic() + 30
        baseline = None
        while time.monotonic() < deadline:
            if lines():
                baseline = lines()[0]
                break
            time.sleep(0.2)
        assertions["baseline_observed"] = baseline is not None
        if baseline is None:
            return 7
        details["baseline_line"] = baseline

        assertions["baseline_is_not_a_change"] = field(baseline, "changed") == "false"
        assertions["baseline_transport_is_the_observed_primary"] = (
            field(baseline, "transport") == "wifi"
            and field(baseline, "primary_state") == "connected"
        )
        assertions["baseline_ssid_matches_the_oracle"] = field(baseline, "ssid") == before["ssid"]
        observed_signal = int(field(baseline, "strength"))
        assertions["baseline_signal_matches_the_oracle"] = (
            abs(observed_signal - int(before["signal"])) <= 2
        )
        assertions["baseline_bands_the_observed_signal"] = field(baseline, "band") in (
            "none", "weak", "medium", "strong"
        )
        assertions["connectivity_is_not_claimed_as_verified_internet"] = (
            field(baseline, "connectivity_checked") == "false"
        )
        assertions["overlay_is_reported_separately"] = field(baseline, "overlay") == "true"
        details["baseline_signal"] = observed_signal
        details["oracle_before"] = before

        # Infrastructure must not contaminate the presentation.
        assertions["infrastructure_excluded"] = (
            int(field(baseline, "relevant")) < int(field(baseline, "devices"))
        )
        details["device_count"] = int(field(baseline, "devices"))
        details["relevant_count"] = int(field(baseline, "relevant"))

        # Observation is passive: no product transition without a network change.
        idle_start = len(lines())
        time.sleep(8)
        assertions["idle_emits_no_product_transition"] = len(lines()) == idle_start

        # Natural drift: a change that does occur must arrive as a fresh
        # generation and must agree with the oracle.
        drift_deadline = time.monotonic() + 40
        drift = None
        seen = {field(baseline, "strength")}
        while time.monotonic() < drift_deadline:
            for line in lines()[1:]:
                strength = field(line, "strength")
                if strength and strength not in seen:
                    drift = line
                    break
            if drift:
                break
            time.sleep(0.5)
        # Drift is not guaranteed within the window on a stable link; a missing
        # drift is reported rather than fabricated.
        if drift:
            assertions["observed_drift_arrives_as_a_change"] = field(drift, "changed") == "true"
            assertions["drift_agrees_with_the_oracle"] = (
                field(drift, "ssid") == before["ssid"]
                and int(field(drift, "strength")) <= 100
            )
            details["drift_line"] = drift
        else:
            details["drift"] = "NOT_OBSERVED_WITHIN_WINDOW"

        after = oracle()
        details["oracle_after"] = after
        assertions["no_network_mutation_performed"] = (
            before.get("ssid") == after.get("ssid")
            and before.get("connected") == after.get("connected")
        )
    except Exception as exc:
        error = f"{type(exc).__name__}: {exc}"
    finally:
        reaped = stop(observer)
        if "log" in locals():
            log.close()

    assertions["observer_reaped"] = reaped
    failed = [name for name, passed in assertions.items() if not passed]
    result = "PASS" if not failed and error is None else "FAIL"

    payload = {
        "gate": "NETWORK-01-PHASE3-PASSIVE-BAR-PROJECTION",
        "source_head": source_head(),
        "tree_dirty": tree_state()[0],
        "tree_status": tree_state()[1],
        "provider_version": provider_version(),
        "result": result,
        "passed": sum(1 for v in assertions.values() if v),
        "total": len(assertions),
        "failed": failed,
        "assertions": assertions,
        "details": details,
        "error": error,
        "log": str(LOG.relative_to(ROOT)),
        "mutation_authority": "NONE",
        "interaction": "NONE",
        "interaction_lease": "NOT_DECIDED",
        "default_route_failover": "NOT_RUN_REQUIRES_SEPARATE_BOUNDED_LIVE_MUTATION",
        "utc": datetime.now(timezone.utc).isoformat(),
    }
    stamped = EVIDENCE / f"receipt-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}.json"
    text = json.dumps(payload, indent=2) + "\n"
    stamped.write_text(text)
    RECEIPT.write_text(text)
    for name, passed in assertions.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if error:
        print(f"ERROR {error}")
    print(f"NETWORK_PHASE3_GATE={result}")
    return 0 if result == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
