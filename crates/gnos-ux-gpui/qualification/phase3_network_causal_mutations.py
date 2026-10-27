#!/usr/bin/env python3
"""NETWORK-01 Phase 3 causal mutations.

Phase 3 is presentation only, so the mutations defend presentation invariants:
that the Bar cannot reach the protocol, that provider absence is never rendered
as a normal state, and that connectivity is never folded into transport.

Each mutation is applied, the product tests are run, and the source is restored
whether it passed or failed. `touch` before building is load-bearing: a restored
file keeps its mtime under `shutil.copy`, and a stale test binary reports RED for
correct source.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BAR = ROOT / "crates/gnos-ux-gpui/src/product_bar.rs"
PRESENTATION = ROOT / "crates/gnos-ux-gpui/src/product_network.rs"
PRISTINE_BAR = Path("/tmp/opencode/net01/bar-pristine.rs")
PRISTINE_PRESENTATION = Path("/tmp/opencode/net01/presentation-pristine.rs")

# (id, file, description, needle, replacement)
MUTATIONS = [
    (
        "N6-BAR",
        BAR,
        "let the Bar import the protocol directly",
        """use gpui::App;""",
        """use gpui::App;
// MUTATION: the Bar reaches the provider protocol directly.
use gnos_network::zbus;
use gnos_network::NetworkSnapshot as RawNetworkProtocolSnapshot;""",
    ),
    (
        "P-1",
        PRESENTATION,
        "render provider absence as a disconnected network",
        """            NetworkProviderState::Unavailable | NetworkProviderState::Reconnecting => {
                ProductNetworkPrimary::Unavailable
            }""",
        """            // MUTATION: provider absence is rendered as a disconnected network.
            NetworkProviderState::Unavailable | NetworkProviderState::Reconnecting => {
                ProductNetworkPrimary::Disconnected
            }""",
    ),
    (
        "P-2",
        PRESENTATION,
        "let an overlay become the transport",
        """                        NetworkTransport::None => ProductNetworkPrimary::Disconnected,
                        transport => ProductNetworkPrimary::Ready(ProductNetworkEndpoint {""",
        """                        // MUTATION: the overlay is allowed to be the transport.
                        transport => ProductNetworkPrimary::Ready(ProductNetworkEndpoint {""",
    ),
    (
        "P-3",
        PRESENTATION,
        "fold connectivity into transport",
        """    pub fn is_restricted(&self) -> bool {
        matches!(
            self.connectivity,
            NetworkConnectivity::Portal | NetworkConnectivity::Limited
        )
    }""",
        """    // MUTATION: connectivity is read back out of the transport.
    pub fn is_restricted(&self) -> bool {
        false
    }""",
    ),
    (
        "N6-MODEL",
        PRESENTATION,
        "expose a raw protocol value through the product projection",
        """use gnos_network::{
    NetworkConnectivity,""",
        """// MUTATION: a raw protocol value crosses into the product projection.
pub use gnos_network::NetworkObjectRef as RawProductNetworkReference;
use gnos_network::{
    NetworkConnectivity,""",
    ),
]


def run_tests() -> tuple[bool, str]:
    for path in (BAR, PRESENTATION):
        os.utime(path, None)
    result = subprocess.run(
        ["cargo", "test", "-p", "gnos-ux-gpui", "--lib", "--features", "component-lab",
         "--quiet", "network"],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    return result.returncode == 0, result.stdout


def failing_tests(output: str) -> list[str]:
    names = []
    for line in output.splitlines():
        stripped = line.strip()
        if stripped.endswith("--- FAILED"):
            names.append(stripped.split(" --- ")[0].strip())
    return names


def main() -> int:
    for path in (PRISTINE_BAR, PRISTINE_PRESENTATION):
        if not path.exists():
            print(f"missing pristine copy at {path}", file=sys.stderr)
            return 2

    pristine = {str(BAR): PRISTINE_BAR, str(PRESENTATION): PRISTINE_PRESENTATION}
    results = []
    for name, target, description, needle, replacement in MUTATIONS:
        source = pristine[str(target)].read_text()
        if needle not in source:
            print(f"[SKIP] {name}: anchor not found in {target.name}")
            results.append({"id": name, "verdict": "SKIP", "description": description})
            continue
        shutil.copyfile(target, f"{target}.pristine")
        try:
            target.write_text(source.replace(needle, replacement, 1))
            passed, output = run_tests()
            failures = failing_tests(output)
            compiled = "error[E" not in output and "error: could not compile" not in output
            red = (not passed) and (bool(failures) or not compiled)
            verdict = "RED" if red else ("GREEN" if passed else "AMBIGUOUS")
            print(f"[{verdict}] {name}: {description}")
            for failure in failures[:4]:
                print(f"         failed: {failure}")
            if not failures and not compiled:
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
    print(f"NETWORK_PHASE3_CAUSAL={'PASS' if ok else 'FAIL'}")
    print(f"summary: {sum(1 for r in results if r['verdict'] == 'RED')}/"
          f"{len(results) - 1} RED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
