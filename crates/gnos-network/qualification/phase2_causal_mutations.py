"""Phase 2 causal mutations for the NETWORK-01 read model.

Each mutation breaks exactly one invariant and runs the network source tests.
A mutation is accepted only when it turns them RED for the intended reason.

N-8 is included because it is the vertical's most valuable finding: an
unverified or obsolete external enum value must become `Unsupported`, never be
inferred. On the live host, a copied table claiming `13 = wireguard`
misclassifies 27 of 59 devices and surfaces `virbr0` as a VPN overlay.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODEL = ROOT / "crates/gnos-network/src/model.rs"
REDUCER = ROOT / "crates/gnos-network/src/reducer.rs"
PRISTINE_MODEL = Path("/tmp/opencode/net01/model-pristine.rs")
PRISTINE_REDUCER = Path("/tmp/opencode/net01/reducer-pristine.rs")

# (id, file, description, needle, replacement)
MUTATIONS = [
    (
        "N-1",
        REDUCER,
        "map provider disappearance to disconnected",
        """    /// Records that the provider stopped answering.
    ///
    /// Provider loss is not a network fact. It becomes
    /// [`NetworkProviderState::Unavailable`] and never `Disconnected`.
    pub fn set_provider_unavailable(&mut self) -> bool {
        if self.provider == NetworkProviderState::Unavailable && self.owner.is_none() {
            return false;
        }
        self.owner = None;
        self.globals = None;
        self.devices.clear();
        self.active_connections.clear();
        self.settings_connections.clear();
        self.baselined = false;
        self.provider = NetworkProviderState::Unavailable;
        self.bump();
        true
    }""",
        """    /// MUTATION: the last known network facts are kept, so a provider failure
    /// keeps presenting a stale connected link instead of retiring it.
    pub fn set_provider_unavailable(&mut self) -> bool {
        if self.provider == NetworkProviderState::Unavailable && self.owner.is_none() {
            return false;
        }
        self.provider = NetworkProviderState::Unavailable;
        self.bump();
        true
    }""",
    ),
    (
        "N-1b",
        MODEL,
        "summarise provider loss as Disconnected",
        """    if snapshot.provider != NetworkProviderState::Available {
        return NetworkSummary {
            provider: snapshot.provider,
            primary_state: NetworkPrimaryState::Unavailable,""",
        """    if snapshot.provider != NetworkProviderState::Available {
        return NetworkSummary {
            provider: snapshot.provider,
            // MUTATION: a provider failure is treated as a disconnect.
            primary_state: NetworkPrimaryState::Disconnected,""",
    ),
    (
        "N-2",
        MODEL,
        "let a connected Docker bridge become the primary presentation",
        """    let primary_device = snapshot.devices.values().find(|device| {
        device.primary && device.relevance == NetworkRelevance::PhysicalPrimaryCandidate
    });""",
        """    // MUTATION: any connected, primary device wins, bridges included.
    let primary_device = snapshot
        .devices
        .values()
        .find(|device| device.primary && device.state.is_connected());""",
    ),
    (
        "N-3",
        REDUCER,
        "identify the primary activation by its connection type instead of its object",
        """            let is_primary = primary == Some(&id)
                || device
                    .active_connection
                    .as_ref()
                    .is_some_and(|ac| Some(ac) == primary);""",
        """            // MUTATION: an activation is identified by its connection type, a
            // class, so two different activations of the same type are conflated.
            let primary_type = primary.and_then(|target| {
                self.active_connections
                    .get(&target)
                    .map(|entry| entry.connection_type.clone())
            });
            let is_primary = match primary_type.as_deref() {
                Some(wanted) => device
                    .active_connection
                    .as_ref()
                    .and_then(|ac| self.active_connections.get(ac))
                    .is_some_and(|entry| entry.connection_type == wanted),
                None => false,
            };""",
    ),
    (
        "N-4",
        REDUCER,
        "carry runtime object identity across a provider replacement",
        """        self.owner = owner.map(str::to_owned);
        self.globals = None;
        self.devices.clear();
        self.active_connections.clear();
        self.settings_connections.clear();
        self.baselined = false;""",
        """        // MUTATION: the previous owner's runtime identities are kept.
        self.owner = owner.map(str::to_owned);
        self.baselined = false;""",
    ),
    (
        "N-5",
        REDUCER,
        "advance the generation on an irrelevant property update",
        """        if self.summary() == previous_summary {
            // Protocol churn with identical product facts. Docker and libvirt
            // traffic lands here and must not advance the generation.
            return false;
        }""",
        """        // MUTATION: any observation advances the generation.
        let _ = previous_summary;""",
    ),
    (
        "N-6",
        MODEL,
        "let the product model expose raw D-Bus values",
        """pub const NETWORK_MANAGER_BUS_NAME: &str = "org.freedesktop.NetworkManager";""",
        """pub const NETWORK_MANAGER_BUS_NAME: &str = "org.freedesktop.NetworkManager";

// MUTATION: a raw protocol type is re-exported into the product model.
pub use zbus::zvariant::OwnedValue as RawNetworkValue;
#[allow(unused_imports)]
use zbus as dbus;""",
    ),
    (
        "N-7",
        MODEL,
        "treat connectivity FULL as synonymous with a connected transport",
        """    NetworkSummary {
        provider: snapshot.provider,
        primary_state,
        transport,""",
        """    // MUTATION: connectivity is collapsed into the transport field.
    NetworkSummary {
        provider: snapshot.provider,
        primary_state: if connectivity == NetworkConnectivity::Full {
            NetworkPrimaryState::Connected
        } else {
            primary_state
        },
        transport,""",
    ),
    (
        "N-8",
        MODEL,
        "substitute a guessed/obsolete DeviceType table so bridge and veth masquerade",
        """            1 => Self::Ethernet,
            2 => Self::Wifi,
            13 => Self::Bridge,
            20 => Self::Veth,
            29 => Self::Wireguard,
            30 => Self::WifiP2p,
            32 => Self::Loopback,""",
        """            // MUTATION: the obsolete table. 13 becomes WireGuard and 20 becomes
            // L2TP, so all 28 bridges and 26 veth devices are misclassified and
            // virbr0 is presented as a VPN overlay.
            1 => Self::Ethernet,
            2 => Self::Wifi,
            13 => Self::Wireguard,
            20 => Self::Wifi,
            29 => Self::Wireguard,
            30 => Self::WifiP2p,
            32 => Self::Loopback,""",
    ),
    (
        "N-8b",
        MODEL,
        "infer an unknown DeviceType from the interface name",
        """        match raw {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            13 => Self::Bridge,
            20 => Self::Veth,
            29 => Self::Wireguard,
            30 => Self::WifiP2p,
            32 => Self::Loopback,
            _ => Self::Unsupported,
        }""",
        """        match raw {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            13 => Self::Bridge,
            20 => Self::Veth,
            29 => Self::Wireguard,
            30 => Self::WifiP2p,
            32 => Self::Loopback,
            // MUTATION: an unverified value is guessed rather than Unsupported.
            _ => Self::Wifi,
        }""",
    ),
    (
        "N-8c",
        MODEL,
        "read the SSID without honouring the declared length prefix",
        """    let usable = if usize::from(declared) == rest.len() {
        rest
    } else {
        bytes
    };""",
        """    // MUTATION: the length prefix is ignored.
    let usable = bytes;
    let _ = (declared, rest);""",
    ),
]


def run_tests() -> tuple[bool, str]:
    # `touch` before building is load-bearing. `shutil.copy` preserves mtime, so a
    # restored source can keep a timestamp older than the last build and cargo
    # will reuse a stale test binary -- which then reports RED for source that is
    # in fact correct. That is a false failure of the harness itself, and it is
    # worse than no gate because it is indistinguishable from a real one.
    for path in (MODEL, REDUCER):
        os.utime(path, None)
    result = subprocess.run(
        ["cargo", "test", "-p", "gnos-network", "--quiet"],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    return result.returncode == 0, result.stdout


def failing_tests(output: str) -> list[str]:
    names = []
    for line in output.splitlines():
        stripped = line.strip()
        # `--quiet` prints failures as "path::to::test --- FAILED" rather than
        # "test path::to::test ... FAILED".
        if stripped.endswith("--- FAILED"):
            names.append(stripped.split(" --- ")[0].strip())
    return names


def main() -> int:
    for path in (PRISTINE_MODEL, PRISTINE_REDUCER):
        if not path.exists():
            print(f"missing pristine copy at {path}", file=sys.stderr)
            return 2
    results = []
    for name, target, description, needle, replacement in MUTATIONS:
        pristine = {
            str(MODEL): PRISTINE_MODEL,
            str(REDUCER): PRISTINE_REDUCER,
        }[str(target)].read_text()
        if needle not in pristine:
            print(f"[SKIP] {name}: anchor not found in {target.name}")
            results.append({"id": name, "verdict": "SKIP", "description": description})
            continue
        shutil.copyfile(target, f"{target}.pristine")
        try:
            target.write_text(pristine.replace(needle, replacement, 1))
            passed, output = run_tests()
            failures = failing_tests(output)
            # N-6 must fail to COMPILE: exposing a raw protocol type is a type
            # boundary violation, so a compile failure is the RED we want.
            compiled = "error[E" not in output and "error: could not compile" not in output
            red = (not passed) and (failures or not compiled)
            verdict = "RED" if red else ("GREEN" if passed else "AMBIGUOUS")
            print(f"[{verdict}] {name}: {description}")
            if failures:
                for failure in failures[:4]:
                    print(f"         failed: {failure}")
            elif not compiled:
                print("         rejected at compile time")
            results.append({
                "id": name,
                "verdict": verdict,
                "description": description,
                "failing_tests": failures,
                "compiled": compiled,
            })
        finally:
            shutil.move(f"{target}.pristine", target)
            os.utime(target, None)

    passed, output = run_tests()
    results.append({
        "id": "RESTORED",
        "verdict": "GREEN" if passed else "RED",
        "description": "source restored after every mutation",
        "failing_tests": failing_tests(output),
    })
    print(f"[{'GREEN' if passed else 'RED'}] RESTORED: source restored after every mutation")

    verdicts = {r["verdict"] for r in results if r["id"] != "RESTORED"}
    ok = verdicts == {"RED"} and passed
    print(f"NETWORK_PHASE2_CAUSAL={'PASS' if ok else 'FAIL'}")
    print(f"summary: {sum(1 for r in results if r['verdict'] == 'RED')}/{len(results) - 1} RED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
