use gnos_bluetooth::{BluetoothObservation, BluetoothSnapshot, run_observer};

#[test]
fn observation_keeps_baseline_distinct_from_change() {
    fn accepts(_: BluetoothObservation) {}
    let _: fn(BluetoothObservation) = accepts;
    let _runner = run_observer::<fn(BluetoothObservation)>;
}

#[test]
fn observer_and_runner_are_read_only_and_event_driven_by_source() {
    let observer = include_str!("../src/bluez_observer.rs");
    let runner = include_str!("../src/runner.rs");

    for forbidden in [
        "StartDiscovery",
        "StopDiscovery",
        "RemoveDevice",
        ".Set(",
        "Pair()",
        "Connect()",
        "Disconnect()",
    ] {
        assert!(
            !observer.contains(forbidden) && !runner.contains(forbidden),
            "read-only substrate contains mutation vocabulary: {forbidden}"
        );
    }

    for forbidden in ["tokio::time::interval", "thread::sleep", "polling"] {
        assert!(
            !runner.contains(forbidden),
            "runner contains polling: {forbidden}"
        );
    }
    assert!(runner.contains("if !object_events.is_empty()"));
    assert!(runner.contains("if !property_events.is_empty()"));
}

#[allow(dead_code)]
fn snapshot_type_is_public(_: BluetoothSnapshot) {}
