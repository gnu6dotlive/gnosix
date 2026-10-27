//! Headless live observer for NETWORK-01 Phase 2 qualification.
//!
//! Drives the production observer and prints one typed line per accepted product
//! transition, so the gate can distinguish "no change" from "a change the gate
//! missed".
//!
//! Read-only. This binary never calls a NetworkManager mutating method and never
//! runs `nmcli`. The stimulus comes from outside; this process only observes.

use gnos_network::{NetworkObservation, NetworkSnapshot, run_observer};
use std::io::Write;

fn main() {
    let mut last = String::new();
    let result = run_observer(move |observation: NetworkObservation| {
        let NetworkObservation {
            snapshot,
            changed,
            baselined,
        } = observation;
        let line = describe(&snapshot);
        // Print when the reducer reports a product-relevant change, or when the
        // rendered line differs from the last one printed.
        //
        // NOT on every observation: `baselined` stays true for the life of a
        // provider owner, and printing on it floods the log with identical lines.
        // That flood is not merely noisy, it is a correctness hazard for any gate
        // that matches positionally, and it did make a Phase 2 run select a
        // pre-restart line as the post-replacement baseline.
        if changed || line != last {
            println!("GNOS_NETWORK changed={changed} baselined={baselined} {line}");
            let _ = std::io::stdout().flush();
            last = line.clone();
        }
    });
    if let Err(error) = result {
        eprintln!("GNOS_NETWORK_FAIL error={error}");
        std::process::exit(1);
    }
}

fn describe(snapshot: &NetworkSnapshot) -> String {
    let summary = gnos_network::summarize(snapshot);
    let wifi = match summary.wifi.as_ref() {
        Some(facts) => format!(
            "ssid={:?} strength={} band={} mechanism={:?} authenticated={} bssid={}",
            facts.ssid,
            facts.strength,
            facts.strength_band().as_str(),
            facts.security.mechanism,
            facts.security.authenticated,
            facts.bssid.as_deref().unwrap_or("none"),
        ),
        None => "ssid=none strength=none".to_owned(),
    };
    let relevant = snapshot
        .devices
        .values()
        .filter(|device| device.relevance.is_user_facing())
        .count();
    format!(
        "owner={} provider={} generation={} primary_state={} transport={} connectivity={} \
connectivity_checked={} overlay={} {} devices={} relevant={} device_paths={}",
        snapshot.provider_owner.as_deref().unwrap_or("none"),
        snapshot.provider.as_str(),
        snapshot.generation,
        summary.primary_state.as_str(),
        summary.transport.as_str(),
        summary.connectivity.as_str(),
        summary.connectivity_checked,
        summary.overlay_active,
        wifi,
        snapshot.devices.len(),
        relevant,
        device_digest(snapshot),
    )
}

/// The provider-scoped runtime identities currently held.
///
/// The gate compares this before and after a provider replacement to prove the
/// previous owner's `Device`, `ActiveConnection` and `AccessPoint` paths were
/// really discarded rather than reused.
fn device_digest(snapshot: &NetworkSnapshot) -> String {
    let mut parts: Vec<String> = snapshot
        .devices
        .keys()
        .map(|reference| format!("{}/{}", reference.provider_owner, reference.path))
        .collect();
    parts.sort();
    parts.join(",")
}
