use gnos_bluetooth::{BluetoothObserver, BluetoothObserverOptions, BluetoothProviderState};

#[test]
fn observer_starts_without_inventing_provider_truth() {
    let observer = BluetoothObserver::new(BluetoothObserverOptions::default());
    assert_eq!(
        observer.state().provider(),
        BluetoothProviderState::Unavailable
    );
    assert!(!observer.state().baselined());
    assert!(observer.snapshot().adapters.is_empty());
    assert!(observer.snapshot().devices.is_empty());
}

#[test]
fn product_model_does_not_import_the_bluez_protocol() {
    let model = include_str!("../src/model.rs");
    let reducer = include_str!("../src/reducer.rs");
    for forbidden in [
        "zbus",
        "zvariant",
        "OwnedValue",
        "org.bluez",
        "ObjectManager",
    ] {
        assert!(
            !model.contains(forbidden) && !reducer.contains(forbidden),
            "protocol vocabulary leaked into typed model: {forbidden}"
        );
    }
}
