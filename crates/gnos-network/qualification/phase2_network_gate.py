#!/usr/bin/env python3
"""NETWORK-01 Phase 2 live qualification.

Read model, relevance classifier, event-driven observation, and the two
authorized bounded mutations:

  CASE A  Wi-Fi disconnect / reconnect  (authorized)
  CASE B  NetworkManager restart        (authorized; needs operator polkit auth)

`nmcli` is used only as stimulus and as an independent oracle. It is never a
production path: the observer under test is the Gnosix binary.

Restoration is mandatory. If the exact pre-existing connection cannot be
restored, the gate reports LIVE_FAILED_RESTORATION and stops mutating.
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
EVIDENCE = ROOT / "evidence/runs/gnosix-network-01-phase2"
LOG = EVIDENCE / "observer.log"
RECEIPT = EVIDENCE / "receipt.json"
OBSERVER = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[3] / "target")) / "debug" / "gnos-network-observe"
NM_SERVICE = "NetworkManager.service"
TARGET_IFACE = "wlan0"


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


def run(*args: str, check: bool = True, timeout: float = 60.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args, cwd=ROOT, check=check, text=True, timeout=timeout,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )


def nmcli(*args: str, check: bool = True, timeout: float = 30.0):
    return run("nmcli", *args, check=check, timeout=timeout)


def oracle_state() -> dict:
    """Independent network truth from nmcli."""
    out = {"ssid": None, "signal": None, "connected": False, "type": None}
    rows = nmcli("-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device", "status",
                 check=False).stdout.splitlines()
    for row in rows:
        fields = [f.replace("\\:", ":") for f in re.split(r"(?<!\\):", row)]
        if len(fields) < 4:
            continue
        device, dtype, state, connection = fields[0], fields[1], fields[2], fields[3]
        if device == TARGET_IFACE:
            out.update(type=dtype, connected=state.startswith("connected"),
                       connection=connection or None)
    wifi = nmcli("-t", "-f", "ACTIVE,SSID,BSSID,SIGNAL", "device", "wifi", "list",
                 "ifname", TARGET_IFACE, "--rescan", "no", check=False).stdout.splitlines()
    for row in wifi:
        if row.startswith("yes:"):
            fields = [f.replace("\\:", ":") for f in re.split(r"(?<!\\):", row)]
            out.update(ssid=fields[1], signal=int(fields[3]))
    return out


def observer_lines() -> list[str]:
    if not LOG.exists():
        return []
    return [line.strip() for line in LOG.read_text().splitlines()
            if line.startswith("GNOS_NETWORK ")]


def field(line: str, key: str) -> str | None:
    match = re.search(rf"(?:^| ){key}=(\S+)", line)
    if not match:
        return None
    return match.group(1).strip('"')


def wait_line(start: int, predicate, timeout: float, description: str):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        lines = observer_lines()[start:]
        for line in lines:
            if predicate(line):
                return line
        time.sleep(0.1)
    return None


def wait_generation(start: int, timeout: float, description: str):
    return wait_line(start, lambda l: field(l, "generation") is not None, timeout, description)


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
    return proc.poll() is not None


def restart_networkmanager() -> tuple[bool, str]:
    """Restart NetworkManager through polkit, never sudo.

    The timeout is deliberately short. polkit authentication is interactive, and
    an unbounded wait would hold the whole gate open behind a prompt nobody
    answered -- and would look like a hung qualification rather than an
    authorisation that could not be granted non-interactively.
    """
    try:
        result = run("pkexec", "systemctl", "restart", NM_SERVICE, check=False, timeout=45)
    except subprocess.TimeoutExpired:
        return False, "pkexec timed out waiting for an interactive polkit authentication"
    return result.returncode == 0, result.stdout.strip()[-400:]


def main() -> int:
    # The two mutations are separated so an unavailable authorisation for one can
    # never hold the other hostage.
    case_b = "--case-a-only" not in sys.argv
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    LOG.write_text("")
    run("cargo", "build", "-q", "-p", "gnos-network", "--bin", "gnos-network-observe")

    assertions: dict[str, bool] = {}
    details: dict[str, object] = {}
    actions: list[dict] = []
    error = None
    status = "PASS"

    baseline_oracle = oracle_state()
    details["baseline_oracle"] = baseline_oracle
    assertions["baseline_has_a_resolvable_primary"] = bool(baseline_oracle["ssid"])
    exact_connection = baseline_oracle["connection"]
    details["exact_connection_to_restore"] = exact_connection

    observer = None
    try:
        log = LOG.open("w")
        observer = subprocess.Popen(
            [str(OBSERVER)], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            text=True, start_new_session=True,
        )
        # Initial enumeration establishes a baseline and is not a change.
        first = wait_line(0, lambda l: field(l, "baselined") == "true", 30, "baseline")
        assertions["baseline_established"] = first is not None
        if first is None:
            return 7
        baseline_generation = int(field(first, "generation"))
        details["baseline_line"] = first
        assertions["baseline_is_not_a_change"] = baseline_generation == 1
        assertions["baseline_reports_the_verified_wifi_truth"] = (
            field(first, "transport") == "wifi"
            and field(first, "ssid") == baseline_oracle["ssid"]
            and abs(int(field(first, "strength")) - int(baseline_oracle["signal"])) <= 2
        )
        assertions["baseline_connectivity_is_not_claimed_as_internet"] = (
            field(first, "connectivity") == "full"
            and field(first, "connectivity_checked") == "false"
        )
        assertions["baseline_overlay_reported_separately"] = field(first, "overlay") == "true"
        baseline_owner = field(first, "owner")
        baseline_paths = field(first, "device_paths")
        details["baseline_owner"] = baseline_owner
        details["baseline_device_count"] = int(field(first, "devices"))
        details["baseline_relevant_count"] = int(field(first, "relevant"))

        # Infrastructure must not be the presentation.
        assertions["infrastructure_excluded_from_presentation"] = (
            int(field(first, "relevant")) < int(field(first, "devices"))
        )

        # Idle churn must not churn generations.
        idle_start = len(observer_lines())
        time.sleep(6)
        idle_lines = observer_lines()[idle_start:]
        assertions["idle_emits_no_product_transition"] = len(idle_lines) == 0

        # ---- CASE A: authorized Wi-Fi disconnect / reconnect -------------
        mark = len(observer_lines())
        before_state = oracle_state()
        nmcli("device", "disconnect", TARGET_IFACE, check=False)
        actions.append({"case": "A", "action": "device disconnect", "target": TARGET_IFACE})
        lost = wait_line(
            mark,
            lambda l: field(l, "primary_state") in ("disconnected", "connecting"),
            25, "wifi loss",
        )
        assertions["wifi_loss_reflected_in_product_truth"] = lost is not None
        if lost:
            details["case_a_lost_line"] = lost
            assertions["wifi_loss_reports_no_signal_or_ssid"] = (
                field(lost, "ssid") == "none"
            )
            assertions["wifi_loss_is_not_a_provider_failure"] = (
                field(lost, "provider") == "available"
            )
            assertions["wifi_loss_keeps_the_overlay_separate"] = (
                field(lost, "overlay") == "true"
            )

        # Restore the exact pre-existing connection.
        mark = len(observer_lines())
        nmcli("device", "connect", TARGET_IFACE, check=False)
        actions.append({"case": "A", "action": "device connect", "target": TARGET_IFACE})
        recovered = wait_line(
            mark,
            lambda l: field(l, "primary_state") == "connected" and field(l, "ssid") == baseline_oracle["ssid"],
            40, "wifi recovery",
        )
        assertions["wifi_recovery_reflected"] = recovered is not None
        if recovered:
            details["case_a_recovered_line"] = recovered
            after_oracle = oracle_state()
            assertions["wifi_recovery_agrees_with_oracle"] = (
                after_oracle["ssid"] == field(recovered, "ssid")
                and after_oracle["connected"]
            )
            assertions["wifi_recovery_is_a_fresh_snapshot"] = (
                int(field(recovered, "generation"))
                > int(field(lost, "generation")) if lost else True
            )

        if not (oracle_state().get("ssid") == baseline_oracle["ssid"]):
            status = "LIVE_FAILED_RESTORATION"
            details["restoration_failure"] = "wifi did not return to its baseline connection"
            raise RuntimeError(status)

        # ---- CASE B: authorized NetworkManager restart --------------------
        mark = len(observer_lines())
        if not case_b:
            status = "NOT_RUN_REQUIRES_OPERATOR_POLKIT_AUTHORIZATION"
            details["case_b_reason"] = "run without --case-a-only to attempt the authorized restart"
            assertions["networkmanager_restart_performed"] = False
            ok, restart_output = False, "skipped: --case-a-only"
        else:
            ok, restart_output = restart_networkmanager()
        details["networkmanager_restart"] = {
            "performed": ok,
            "output": restart_output,
        }
        actions.append({"case": "B", "action": "pkexec systemctl restart", "ok": ok})

        if not ok:
            # Not faked, and not silently skipped.
            status = "NOT_RUN_REQUIRES_OPERATOR_POLKIT_AUTHORIZATION"
            details["case_b_reason"] = (
                "pkexec systemctl restart NetworkManager.service did not complete "
                "without an interactive polkit authentication; no rule permits it "
                "non-interactively for this session"
            )
            assertions["networkmanager_restart_performed"] = False
        else:
            assertions["networkmanager_restart_performed"] = True
            unavailable = wait_line(
                mark,
                lambda l: field(l, "provider") in ("unavailable", "reconnecting"),
                20, "provider loss",
            )
            assertions["provider_loss_observed"] = unavailable is not None
            if unavailable:
                details["case_b_unavailable_line"] = unavailable
                assertions["provider_loss_is_not_disconnected"] = (
                    field(unavailable, "primary_state") == "unavailable"
                    and field(unavailable, "transport") == "none"
                )
                assertions["provider_loss_reports_no_signal_or_ssid"] = (
                    field(unavailable, "ssid") == "none"
                )
                assertions["provider_loss_discards_runtime_paths"] = (
                    field(unavailable, "device_paths") != baseline_paths
                )

            # A fresh baseline must belong to a DIFFERENT provider owner.
            #
            # Waiting only for "available and baselined and the same SSID" is too
            # weak: the observer legitimately emits further lines for the current
            # owner between the wifi recovery and the restart, and one of those
            # would be matched as the post-replacement baseline. The assertion
            # then fails against a system that is in fact correct. The owner is
            # the fact that distinguishes the two, so the predicate names it.
            fresh = wait_line(
                mark,
                lambda l: field(l, "provider") == "available"
                and field(l, "baselined") == "true"
                and field(l, "ssid") == baseline_oracle["ssid"]
                and field(l, "owner") not in (None, baseline_owner),
                60, "fresh baseline",
            )
            assertions["fresh_owner_and_baseline_after_replacement"] = fresh is not None
            if fresh:
                details["case_b_fresh_line"] = fresh
                new_owner = field(fresh, "owner")
                assertions["new_owner_acquired"] = new_owner not in (None, baseline_owner)
                assertions["old_runtime_paths_not_reused"] = (
                    field(fresh, "device_paths") != baseline_paths
                    and all(path.startswith(new_owner + "/")
                            for path in field(fresh, "device_paths").split(",") if path)
                )
                assertions["recovery_restores_normal_truth"] = (
                    field(fresh, "transport") == "wifi"
                    and field(fresh, "primary_state") == "connected"
                    and field(fresh, "overlay") == "true"
                )

        # ---- final restoration check -------------------------------------
        final = oracle_state()
        details["final_oracle"] = final
        assertions["final_state_restored"] = (
            final.get("ssid") == baseline_oracle["ssid"]
            and final.get("connected") is True
        )
    except Exception as exc:
        error = f"{type(exc).__name__}: {exc}"
    finally:
        # Mandatory restoration, regardless of outcome. Only the exact
        # connections that were active before the gate are reactivated, and no
        # profile is ever edited.
        if status != "LIVE_FAILED_RESTORATION":
            nmcli("device", "connect", TARGET_IFACE, check=False)
            if exact_connection:
                nmcli("connection", "up", "id", exact_connection, check=False)
            time.sleep(3)
        observer_reaped = stop(observer)
        if "log" in locals():
            log.close()

    assertions["observer_reaped"] = observer_reaped
    failed = [name for name, passed in assertions.items() if not passed]
    # A gate that could not run an authorized mutation is not a PASS.
    result = "PASS" if not failed and status == "PASS" else ("FAIL" if status == "PASS" else status)

    payload = {
        "gate": "NETWORK-01-PHASE2-READ-MODEL-AND-PROVIDER-LIFECYCLE",
        "source_head": source_head(),
        "tree_dirty": tree_state()[0],
        "tree_status": tree_state()[1],
        "provider_version": provider_version(),
        "result": result,
        "status": status,
        "passed": sum(1 for v in assertions.values() if v),
        "total": len(assertions),
        "failed": failed,
        "assertions": assertions,
        "actions": actions,
        "details": details,
        "error": error,
        "log": str(LOG.relative_to(ROOT)),
        "default_route_failover": "NOT_RUN_REQUIRES_SEPARATE_BOUNDED_LIVE_MUTATION",
        "case_a_only_run": not case_b,
        "interaction_lease": "NOT_DECIDED",
        "network_mutation_authority": "NONE",
        "polling": "NONE",
        "production_nmcli": "NONE",
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
    print(f"NETWORK_PHASE2_GATE={result}")
    return 0 if result == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
