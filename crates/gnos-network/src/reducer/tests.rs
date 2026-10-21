//! Source tests for the network observation reducer.
//!
//! These are the invariants the live qualification could not fully close
//! hermetically, plus the churn-suppression behaviour the Docker-heavy host
//! forces.

use super::*;
use crate::model::*;

const OWNER_A: &str = ":1.7";
const OWNER_B: &str = ":1.42";

fn id(owner: &str, path: &str) -> NetworkObjectRef {
    NetworkObjectRef::new(owner, path)
}

fn globals(primary_type: &str) -> NetworkGlobals {
    NetworkGlobals {
        state: NetworkDeviceState::ConnectedGlobal,
        connectivity: NetworkConnectivityAssessment {
            value: NetworkConnectivity::Full,
            checked: false,
        },
        primary_connection: None,
        primary_connection_type: Some(primary_type.to_owned()),
        networking_enabled: true,
        wireless_enabled: true,
    }
}

fn wifi_device(owner: &str, index: u32) -> NetworkDeviceSnapshot {
    let path = format!("/org/freedesktop/NetworkManager/Devices/{index}");
    NetworkDeviceSnapshot {
        primary: true,
        relevance: NetworkRelevance::Unsupported,
        id: id(owner, &path),
        interface: "wlan0".into(),
        kind: NetworkDeviceKind::Wifi,
        state: NetworkDeviceState::Connected,
        managed: true,
        active_connection: Some(id(
            owner,
            &format!("/org/freedesktop/NetworkManager/ActiveConnection/{index}"),
        )),
        settings_connection: None,
        wifi: Some(NetworkWifiFacts {
            access_point: Some(id(owner, "/org/freedesktop/NetworkManager/AccessPoint/1")),
            bssid: Some("0E:AC:8A:58:8B:61".into()),
            ssid: "Cartier50".into(),
            strength: 65,
            frequency_mhz: 5220,
            security: decode_rsn_security(3, 0, 392),
        }),
    }
}

fn bridge(owner: &str, index: u32) -> NetworkDeviceSnapshot {
    NetworkDeviceSnapshot {
        primary: false,
        relevance: NetworkRelevance::Unsupported,
        id: id(
            owner,
            &format!("/org/freedesktop/NetworkManager/Devices/{index}"),
        ),
        interface: format!("br-{index:012x}"),
        kind: NetworkDeviceKind::Bridge,
        state: NetworkDeviceState::Connected,
        managed: true,
        active_connection: Some(id(
            owner,
            &format!("/org/freedesktop/NetworkManager/ActiveConnection/{index}"),
        )),
        settings_connection: None,
        wifi: None,
    }
}

fn overlay(owner: &str) -> NetworkDeviceSnapshot {
    NetworkDeviceSnapshot {
        primary: false,
        relevance: NetworkRelevance::Unsupported,
        id: id(owner, "/org/freedesktop/NetworkManager/Devices/4"),
        interface: "wg-gnu6".into(),
        kind: NetworkDeviceKind::Wireguard,
        state: NetworkDeviceState::Connected,
        managed: true,
        active_connection: Some(id(
            owner,
            "/org/freedesktop/NetworkManager/ActiveConnection/2",
        )),
        settings_connection: None,
        wifi: None,
    }
}

fn base_observation(owner: &str) -> Vec<NetworkDeviceSnapshot> {
    vec![
        wifi_device(owner, 3),
        overlay(owner),
        bridge(owner, 30),
        bridge(owner, 31),
    ]
}

// N-2: a connected Docker bridge must never become the primary presentation.
#[test]
fn a_connected_bridge_never_becomes_the_primary_presentation() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    let summary = state.summary();
    assert_eq!(summary.transport, NetworkTransport::Wifi);
    assert!(
        summary.overlay_active,
        "the WireGuard overlay is reported separately"
    );
    // Inventory counts are diagnostics on the snapshot, deliberately absent from
    // the summary: if they were in the summary, bridge churn would look like a
    // product change.
    let snapshot = state.snapshot();
    assert_eq!(snapshot.devices.len(), 4);
    assert_eq!(
        snapshot
            .devices
            .values()
            .filter(|device| device.relevance.is_user_facing())
            .count(),
        2,
        "only the Wi-Fi and the overlay are user-facing"
    );

    // Now make every bridge "primary". None of them may win.
    let devices = vec![wifi_device(OWNER_A, 3), overlay(OWNER_A), {
        let mut b = bridge(OWNER_A, 30);
        b.primary = true;
        b
    }];
    let mut state = NetworkState::new();
    state.apply_observation(OWNER_A, Some(globals("bridge")), devices, Vec::new());
    assert_eq!(
        state.summary().transport,
        NetworkTransport::Wifi,
        "a bridge cannot become the user-facing transport"
    );
}

// N-1: provider disappearance is not "disconnected".
#[test]
fn provider_disappearance_is_unavailable_not_disconnected() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    assert_eq!(
        state.summary().primary_state,
        NetworkPrimaryState::Connected
    );

    assert!(state.set_provider_unavailable());
    let summary = state.summary();
    assert_eq!(summary.provider, NetworkProviderState::Unavailable);
    assert_eq!(summary.primary_state, NetworkPrimaryState::Unavailable);
    assert_ne!(summary.primary_state, NetworkPrimaryState::Disconnected);
    assert_eq!(summary.transport, NetworkTransport::None);
    assert!(
        summary.wifi.is_none(),
        "a provider failure must not leave a stale signal or SSID on screen"
    );
    // No runtime identity survives.
    assert!(state.snapshot().devices.is_empty());
    assert!(state.snapshot().globals.is_none());
}

// N-4: runtime object identity must not survive a provider replacement.
#[test]
fn a_provider_replacement_invalidates_all_runtime_provenance() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    let old_device = id(OWNER_A, "/org/freedesktop/NetworkManager/Devices/3");
    let old_ap = id(OWNER_A, "/org/freedesktop/NetworkManager/AccessPoint/1");
    assert!(state.accepts(&old_device));
    assert!(state.accepts(&old_ap));

    assert!(state.set_provider_owner(Some(OWNER_B)));
    assert_eq!(state.provider(), NetworkProviderState::Reconnecting);
    assert!(
        state.snapshot().devices.is_empty(),
        "every runtime identity is discarded on replacement"
    );
    assert!(state.snapshot().globals.is_none());
    assert!(!state.baselined());
    assert!(
        !state.accepts(&old_device),
        "an object path from the previous owner is no longer trustworthy"
    );
    assert!(!state.accepts(&old_ap));
    // And it is not reported as disconnected while reconnecting.
    assert_eq!(
        state.summary().primary_state,
        NetworkPrimaryState::Unavailable,
        "a replacement is a provider event, never a network state"
    );

    // The first observation under the new owner is a fresh baseline, not a change.
    assert!(!state.apply_observation(
        OWNER_B,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_B),
        Vec::new()
    ));
    assert!(state.baselined());
    assert_eq!(
        state.summary().primary_state,
        NetworkPrimaryState::Connected
    );
    assert!(
        state.accepts(&id(OWNER_B, "/org/freedesktop/NetworkManager/Devices/3")),
        "the same path under the new owner is a different, now-current object"
    );
    assert!(!state.accepts(&old_device));
}

// N-5: irrelevant property churn must not advance the generation.
#[test]
fn identical_facts_and_infrastructure_churn_do_not_advance_the_generation() {
    let mut state = NetworkState::new();
    assert!(!state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new()
    ));
    let generation = state.generation();
    assert_eq!(generation, 1, "a baseline publishes once");

    // Same product facts, byte-for-byte the same inventory.
    assert!(!state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new()
    ));
    assert_eq!(state.generation(), generation);

    // Docker and libvirt traffic: more bridges appear and disappear.
    let mut churned = base_observation(OWNER_A);
    for index in 40..55 {
        churned.push(bridge(OWNER_A, index));
    }
    assert!(!state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        churned,
        Vec::new()
    ));
    assert_eq!(
        state.generation(),
        generation,
        "28 bridges appearing must not look like a network change"
    );
    let snapshot = state.snapshot();
    assert_eq!(snapshot.devices.len(), 19, "the inventory still grew");
    assert_eq!(
        snapshot
            .devices
            .values()
            .filter(|device| device.relevance.is_user_facing())
            .count(),
        2,
        "and the user-facing set did not"
    );
}

#[test]
fn a_relevant_change_advances_the_generation_exactly_once() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    let generation = state.generation();

    let mut devices = base_observation(OWNER_A);
    devices[0].state = NetworkDeviceState::Disconnected;
    devices[0].wifi = None;
    assert!(state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        devices,
        Vec::new()
    ));
    assert_eq!(state.generation(), generation + 1);
    let summary = state.summary();
    assert_eq!(summary.primary_state, NetworkPrimaryState::Disconnected);
    assert_eq!(
        summary.transport,
        NetworkTransport::Wifi,
        "the primary transport is still Wi-Fi; it is the state that moved"
    );
    assert!(
        summary.wifi.is_none(),
        "a dropped link presents no signal or SSID"
    );
    assert!(
        summary.overlay_active,
        "losing Wi-Fi must not silently claim the overlay is the transport"
    );
}

// N-7: connectivity is not transport identity.
#[test]
fn connectivity_is_carried_separately_from_transport() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    let summary = state.summary();
    assert_eq!(summary.transport, NetworkTransport::Wifi);
    assert_eq!(summary.primary_state, NetworkPrimaryState::Connected);
    assert_eq!(summary.connectivity, NetworkConnectivity::Full);
    assert!(
        !summary.connectivity_checked,
        "this host has ConnectivityCheckEnabled=false, so Full is not verified internet"
    );

    // A portal on a connected Wi-Fi link is connected AND portal.
    let mut portal = globals("802-11-wireless");
    portal.connectivity = NetworkConnectivityAssessment {
        value: NetworkConnectivity::Portal,
        checked: true,
    };
    state.apply_observation(OWNER_A, Some(portal), base_observation(OWNER_A), Vec::new());
    let summary = state.summary();
    assert_eq!(summary.primary_state, NetworkPrimaryState::Connected);
    assert_eq!(summary.transport, NetworkTransport::Wifi);
    assert_eq!(summary.connectivity, NetworkConnectivity::Portal);
    assert!(summary.connectivity_checked);
}

#[test]
fn a_disconnected_ethernet_port_is_not_the_primary_transport() {
    // This host has enp3s0 present, managed, and disconnected.
    let devices = vec![
        wifi_device(OWNER_A, 3),
        NetworkDeviceSnapshot {
            primary: false,
            relevance: NetworkRelevance::Unsupported,
            id: id(OWNER_A, "/org/freedesktop/NetworkManager/Devices/2"),
            interface: "enp3s0".into(),
            kind: NetworkDeviceKind::Ethernet,
            state: NetworkDeviceState::Disconnected,
            managed: true,
            active_connection: None,
            settings_connection: None,
            wifi: None,
        },
    ];
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        devices,
        Vec::new(),
    );
    let summary = state.summary();
    assert_eq!(summary.transport, NetworkTransport::Wifi);
    assert_eq!(summary.primary_state, NetworkPrimaryState::Connected);
    let snapshot = state.snapshot();
    let ethernet = snapshot
        .devices
        .values()
        .find(|device| device.kind == NetworkDeviceKind::Ethernet)
        .expect("ethernet present");
    assert_eq!(
        ethernet.relevance,
        NetworkRelevance::PhysicalPrimaryCandidate
    );
    assert!(
        !ethernet.state.is_connected(),
        "a wired port existing is not wired being primary"
    );
}

#[test]
fn the_durable_profile_is_never_the_activation_identity() {
    let activation = NetworkActiveConnectionSnapshot {
        id: id(
            OWNER_A,
            "/org/freedesktop/NetworkManager/ActiveConnection/4",
        ),
        connection_type: "802-11-wireless".into(),
        devices: vec![id(OWNER_A, "/org/freedesktop/NetworkManager/Devices/3")],
        default_ipv4: true,
        state: NetworkActiveConnectionState::Activated,
        settings_connection: Some(id(OWNER_A, "/org/freedesktop/NetworkManager/Settings/1")),
    };
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        vec![activation.clone()],
    );
    let snapshot = state.snapshot();
    assert_eq!(
        snapshot.settings_connections.get(&activation.id),
        Some(&id(OWNER_A, "/org/freedesktop/NetworkManager/Settings/1")),
        "the profile is recorded beside the activation, not inside it"
    );
    assert_ne!(activation.id, activation.settings_connection.unwrap());
}

#[test]
fn a_single_targeted_device_update_changes_truth_once() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    let generation = state.generation();

    let mut updated = wifi_device(OWNER_A, 3);
    updated.wifi.as_mut().expect("wifi").strength = 30;
    assert!(state.apply_device(updated));
    assert_eq!(state.generation(), generation + 1);
    assert_eq!(
        state.summary().wifi.expect("wifi").strength_band(),
        NetworkSignalBand::Weak
    );

    // The same update again is a no-op.
    let generation = state.generation();
    let mut repeated = wifi_device(OWNER_A, 3);
    repeated.wifi.as_mut().expect("wifi").strength = 30;
    assert!(!state.apply_device(repeated));
    assert_eq!(state.generation(), generation);

    // A change to a device from a previous owner is refused outright.
    let mut stale = wifi_device(OWNER_A, 3);
    stale.id = id(OWNER_B, "/org/freedesktop/NetworkManager/Devices/3");
    stale.wifi.as_mut().expect("wifi").strength = 10;
    assert!(!state.apply_device(stale));
}

#[test]
fn primary_reassignment_is_recomputed_from_the_global_fact() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    assert_eq!(state.summary().transport, NetworkTransport::Wifi);
    let generation = state.generation();

    // The provider now names the overlay as primary. Nothing about either
    // device changed, so this must be re-derived rather than assumed.
    assert!(state.reclassify_primary(Some(&id(
        OWNER_A,
        "/org/freedesktop/NetworkManager/ActiveConnection/2"
    ))));
    assert_eq!(state.generation(), generation + 1);
    assert_eq!(
        state.summary().transport,
        NetworkTransport::None,
        "an overlay is reported separately and never as the transport"
    );
    assert_eq!(
        state.summary().primary_state,
        NetworkPrimaryState::Disconnected,
        "a physical candidate that is not the primary connection is not evidence of a link"
    );
    assert!(
        state.summary().overlay_active,
        "and the overlay is still reported on its own field"
    );
}

// N-3: two activations of the same connection type are different objects. Only
// the provider's exact object reference may select one of them; a connection
// type is a class and cannot be an identity.
#[test]
fn two_activations_of_one_type_are_not_conflated() {
    let wifi_ac = NetworkActiveConnectionSnapshot {
        id: id(
            OWNER_A,
            "/org/freedesktop/NetworkManager/ActiveConnection/4",
        ),
        connection_type: "802-11-wireless".into(),
        devices: Vec::new(),
        default_ipv4: true,
        state: NetworkActiveConnectionState::Activated,
        settings_connection: Some(id(OWNER_A, "/org/freedesktop/NetworkManager/Settings/1")),
    };
    let other_wifi_ac = NetworkActiveConnectionSnapshot {
        id: id(
            OWNER_A,
            "/org/freedesktop/NetworkManager/ActiveConnection/9",
        ),
        connection_type: "802-11-wireless".into(),
        devices: Vec::new(),
        default_ipv4: false,
        state: NetworkActiveConnectionState::Activated,
        settings_connection: Some(id(OWNER_A, "/org/freedesktop/NetworkManager/Settings/7")),
    };

    let mut first = wifi_device(OWNER_A, 3);
    first.active_connection = Some(id(
        OWNER_A,
        "/org/freedesktop/NetworkManager/ActiveConnection/4",
    ));
    let mut second = wifi_device(OWNER_A, 9);
    second.interface = "wlan1".into();
    second.active_connection = Some(id(
        OWNER_A,
        "/org/freedesktop/NetworkManager/ActiveConnection/9",
    ));
    // The provider has not yet promoted either; only the named one may become
    // primary.
    first.primary = false;
    second.primary = false;
    let devices = vec![first, second];

    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        devices,
        vec![wifi_ac, other_wifi_ac],
    );
    // The provider promotes the *other* activation of the same type. Both
    // devices are Wi-Fi of the same connection type, so a type-based match
    // would mark both primary; only the exact object may. The derived summary
    // is unchanged here (both are Wi-Fi), so the assertion is on the
    // per-device identity rather than on the aggregate.
    state.reclassify_primary(Some(&id(
        OWNER_A,
        "/org/freedesktop/NetworkManager/ActiveConnection/9",
    )));

    let snapshot = state.snapshot();
    let first = snapshot
        .devices
        .values()
        .find(|device| device.interface == "wlan0")
        .expect("wlan0 present");
    let second = snapshot
        .devices
        .values()
        .find(|device| device.interface == "wlan1")
        .expect("wlan1 present");
    assert!(!first.primary, "only the named activation is primary");
    assert!(second.primary);
    assert_eq!(
        snapshot.settings_connections.get(&id(
            OWNER_A,
            "/org/freedesktop/NetworkManager/ActiveConnection/9"
        )),
        Some(&id(OWNER_A, "/org/freedesktop/NetworkManager/Settings/7")),
        "the durable profile follows its own activation, not the other one"
    );
}

// A wireless link that is going down must not keep presenting its SSID or its
// signal. NetworkManager keeps publishing the access point for a moment after the
// association is gone, and the live gate caught a frame that reported
// `primary_state=disconnected` next to a real-looking SSID.
#[test]
fn a_down_link_presents_no_stale_ssid_or_signal() {
    let mut state = NetworkState::new();
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        base_observation(OWNER_A),
        Vec::new(),
    );
    let summary = state.summary();
    assert_eq!(summary.primary_state, NetworkPrimaryState::Connected);
    assert_eq!(summary.wifi.as_ref().expect("wifi").ssid, "Cartier50");

    // The device drops its association but the provider is still publishing the
    // access point: this is the frame the live gate saw.
    let mut devices = base_observation(OWNER_A);
    devices[0].state = NetworkDeviceState::Disconnected;
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        devices,
        Vec::new(),
    );

    let summary = state.summary();
    assert_eq!(summary.primary_state, NetworkPrimaryState::Disconnected);
    assert_eq!(
        summary.transport,
        NetworkTransport::Wifi,
        "the primary connection is still Wi-Fi; primary_state is what says it is down"
    );
    assert!(
        summary.wifi.is_none(),
        "a dead link must not report an SSID or a signal strength"
    );
    let snapshot = state.snapshot();
    let wifi_device = snapshot
        .devices
        .values()
        .find(|device| device.kind == NetworkDeviceKind::Wifi)
        .expect("wifi device present");
    assert!(
        wifi_device.wifi.is_none(),
        "the stale access point is not retained on the device either"
    );
    assert!(
        summary.overlay_active,
        "the overlay is still reported on its own field"
    );

    // And the shape the live host actually produces: once the provider clears
    // `ActiveConnection`, there is no primary transport at all.
    let mut cleared = base_observation(OWNER_A);
    cleared[0].active_connection = None;
    cleared[0].primary = false;
    state.apply_observation(
        OWNER_A,
        Some(globals("802-11-wireless")),
        cleared,
        Vec::new(),
    );
    let summary = state.summary();
    assert_eq!(summary.transport, NetworkTransport::None);
    assert_eq!(summary.primary_state, NetworkPrimaryState::Disconnected);
    assert!(summary.wifi.is_none());
}
