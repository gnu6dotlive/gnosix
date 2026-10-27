use gnos_bluetooth::{
    BluetoothAdapterSnapshot, BluetoothDeviceSnapshot, BluetoothObjectRef, BluetoothPowerState,
    BluetoothProviderState, BluetoothState,
};

const OWNER_A: &str = ":1.6";
const OWNER_B: &str = ":1.42";

fn id(owner: &str, path: &str) -> BluetoothObjectRef {
    BluetoothObjectRef::new(owner, path)
}

fn adapter(owner: &str) -> BluetoothAdapterSnapshot {
    BluetoothAdapterSnapshot {
        id: id(owner, "/org/bluez/hci0"),
        powered: true,
        power_state: BluetoothPowerState::On,
        discovering: false,
        pairable: true,
        discoverable: false,
    }
}
fn connected_device(owner: &str) -> BluetoothDeviceSnapshot {
    BluetoothDeviceSnapshot {
        id: id(owner, "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"),
        adapter: id(owner, "/org/bluez/hci0"),
        alias: Some("headset".into()),
        paired: true,
        bonded: true,
        connected: true,
        trusted: true,
        blocked: false,
        services_resolved: true,
        rssi: None,
        icon: None,
    }
}

#[test]
fn runtime_identity_is_provider_scoped() {
    assert_ne!(
        id(OWNER_A, "/org/bluez/hci0"),
        id(OWNER_B, "/org/bluez/hci0")
    );
}
#[test]
fn power_state_decoding_is_verified_only() {
    assert_eq!(BluetoothPowerState::from_raw("on"), BluetoothPowerState::On);
    assert_eq!(
        BluetoothPowerState::from_raw("off"),
        BluetoothPowerState::Off
    );
    assert_eq!(
        BluetoothPowerState::from_raw("off-enabling"),
        BluetoothPowerState::Enabling
    );
    assert_eq!(
        BluetoothPowerState::from_raw("on-disabling"),
        BluetoothPowerState::Disabling
    );
    assert_eq!(
        BluetoothPowerState::from_raw("off-blocked"),
        BluetoothPowerState::Blocked
    );
    assert_eq!(
        BluetoothPowerState::from_raw("future-state"),
        BluetoothPowerState::Unsupported("future-state".into())
    );
}
#[test]
fn provider_loss_is_not_adapter_off() {
    let mut state = BluetoothState::new();
    assert!(!state.apply_observation(
        OWNER_A,
        vec![adapter(OWNER_A)],
        vec![connected_device(OWNER_A)]
    ));
    assert_eq!(state.provider(), BluetoothProviderState::Available);
    assert!(state.set_provider_unavailable());
    assert_eq!(state.provider(), BluetoothProviderState::Unavailable);
    assert!(state.snapshot().adapters.is_empty());
    assert!(state.snapshot().devices.is_empty());
}

#[test]
fn provider_replacement_invalidates_runtime_objects() {
    let mut state = BluetoothState::new();
    state.apply_observation(
        OWNER_A,
        vec![adapter(OWNER_A)],
        vec![connected_device(OWNER_A)],
    );
    let old = id(OWNER_A, "/org/bluez/hci0");
    assert!(state.accepts(&old));
    assert!(state.set_provider_owner(Some(OWNER_B)));
    assert_eq!(state.provider(), BluetoothProviderState::Reconnecting);
    assert!(!state.accepts(&old));
    assert!(state.snapshot().adapters.is_empty());
    assert!(!state.baselined());

    assert!(!state.apply_observation(
        OWNER_B,
        vec![adapter(OWNER_B)],
        vec![connected_device(OWNER_B)]
    ));
    assert!(state.baselined());
    assert!(state.accepts(&id(OWNER_B, "/org/bluez/hci0")));
}

#[test]
fn identical_observation_does_not_advance_generation() {
    let mut state = BluetoothState::new();
    assert!(!state.apply_observation(
        OWNER_A,
        vec![adapter(OWNER_A)],
        vec![connected_device(OWNER_A)]
    ));
    let generation = state.generation();
    assert!(!state.apply_observation(
        OWNER_A,
        vec![adapter(OWNER_A)],
        vec![connected_device(OWNER_A)]
    ));
    assert_eq!(state.generation(), generation);

    let mut disconnected = connected_device(OWNER_A);
    disconnected.connected = false;
    assert!(state.apply_observation(OWNER_A, vec![adapter(OWNER_A)], vec![disconnected]));
    assert_eq!(state.generation(), generation + 1);
}

#[test]
fn missing_rssi_stays_absent() {
    let device = connected_device(OWNER_A);
    assert_eq!(device.rssi, None);
}

#[test]
fn available_provider_without_adapter_is_not_adapter_off() {
    let mut no_adapter = BluetoothState::new();
    assert!(!no_adapter.apply_observation(OWNER_A, vec![], vec![]));
    assert_eq!(no_adapter.provider(), BluetoothProviderState::Available);
    assert!(no_adapter.snapshot().adapters.is_empty());

    let mut off = adapter(OWNER_A);
    off.powered = false;
    off.power_state = BluetoothPowerState::Off;
    let mut powered_off = BluetoothState::new();
    assert!(!powered_off.apply_observation(OWNER_A, vec![off], vec![]));
    assert_eq!(powered_off.provider(), BluetoothProviderState::Available);
    assert_eq!(powered_off.snapshot().adapters.len(), 1);
}

#[test]
fn foreign_runtime_objects_are_rejected() {
    let mut state = BluetoothState::new();
    state.apply_observation(
        OWNER_A,
        vec![adapter(OWNER_B)],
        vec![connected_device(OWNER_B)],
    );
    assert!(state.snapshot().adapters.is_empty());
    assert!(state.snapshot().devices.is_empty());
}

#[test]
fn device_without_observed_adapter_is_rejected() {
    let mut state = BluetoothState::new();
    state.apply_observation(OWNER_A, vec![], vec![connected_device(OWNER_A)]);
    assert!(state.snapshot().devices.is_empty());
}
