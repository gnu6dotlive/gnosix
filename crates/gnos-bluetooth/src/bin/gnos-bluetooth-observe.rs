use gnos_bluetooth::{BluetoothObservation, run_observer};

fn main() {
    let mut last_line = String::new();
    if let Err(error) = run_observer(move |observation| {
        let line = render(&observation);
        if observation.changed || line != last_line {
            println!("{line}");
            last_line = line;
        }
    }) {
        eprintln!("GNOS_BLUETOOTH_OBSERVER fatal {error}");
        std::process::exit(1);
    }
}

fn render(observation: &BluetoothObservation) -> String {
    let snapshot = &observation.snapshot;
    let powered = snapshot
        .adapters
        .values()
        .filter(|adapter| adapter.powered)
        .count();
    let blocked = snapshot
        .adapters
        .values()
        .filter(|adapter| adapter.power_state.as_str() == "blocked")
        .count();
    let transitioning = snapshot
        .adapters
        .values()
        .filter(|adapter| matches!(adapter.power_state.as_str(), "enabling" | "disabling"))
        .count();
    let connected = snapshot
        .devices
        .values()
        .filter(|device| device.connected)
        .count();
    let paired = snapshot
        .devices
        .values()
        .filter(|device| device.paired)
        .count();
    let bonded = snapshot
        .devices
        .values()
        .filter(|device| device.bonded)
        .count();
    let trusted = snapshot
        .devices
        .values()
        .filter(|device| device.trusted)
        .count();
    let resolved = snapshot
        .devices
        .values()
        .filter(|device| device.services_resolved)
        .count();
    let rssi = snapshot
        .devices
        .values()
        .filter(|device| device.rssi.is_some())
        .count();
    format!(
        "provider={:?} owner={} generation={} baselined={} changed={} adapters={} powered={} blocked={} transitioning={} devices={} connected={} paired={} bonded={} trusted={} services_resolved={} rssi_available={}",
        snapshot.provider,
        snapshot.provider_owner.as_deref().unwrap_or("none"),
        snapshot.generation,
        observation.baselined,
        observation.changed,
        snapshot.adapters.len(),
        powered,
        blocked,
        transitioning,
        snapshot.devices.len(),
        connected,
        paired,
        bonded,
        trusted,
        resolved,
        rssi,
    )
}
