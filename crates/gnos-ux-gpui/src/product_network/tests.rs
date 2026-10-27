//! Source tests for the read-only network product presentation.
//!
//! These defend the two separations the substrate established, and the rule that
//! the Bar cannot reach the protocol.

use super::*;
use gnos_network::{
    NetworkConnectivityAssessment, NetworkDeviceKind, NetworkDeviceSnapshot, NetworkDeviceState,
    NetworkGlobals, NetworkObjectRef, NetworkRelevance, NetworkWifiFacts, decode_rsn_security,
};
use std::collections::BTreeMap;

const OWNER: &str = ":1.7";

fn id(path: &str) -> NetworkObjectRef {
    NetworkObjectRef::new(OWNER, path)
}

fn wifi_facts(ssid: &str, strength: u8, muted_security: bool) -> NetworkWifiFacts {
    NetworkWifiFacts {
        access_point: Some(id("/org/freedesktop/NetworkManager/AccessPoint/1")),
        bssid: Some("0E:AC:8A:58:8B:61".into()),
        ssid: ssid.into(),
        strength,
        frequency_mhz: 5220,
        security: decode_rsn_security(3, 0, if muted_security { 0 } else { 392 }),
    }
}

fn wifi_device(state: NetworkDeviceState, strength: u8) -> NetworkDeviceSnapshot {
    NetworkDeviceSnapshot {
        id: id("/org/freedesktop/NetworkManager/Devices/3"),
        interface: "wlan0".into(),
        kind: NetworkDeviceKind::Wifi,
        state,
        managed: true,
        primary: true,
        active_connection: Some(id("/org/freedesktop/NetworkManager/ActiveConnection/4")),
        settings_connection: None,
        relevance: NetworkRelevance::Unsupported,
        wifi: state
            .is_connected()
            .then(|| wifi_facts("Cartier50", strength, false)),
    }
}

fn ethernet_device(state: NetworkDeviceState, primary: bool) -> NetworkDeviceSnapshot {
    NetworkDeviceSnapshot {
        id: id("/org/freedesktop/NetworkManager/Devices/2"),
        interface: "enp3s0".into(),
        kind: NetworkDeviceKind::Ethernet,
        state,
        managed: true,
        primary,
        active_connection: state
            .is_connected()
            .then(|| id("/org/freedesktop/NetworkManager/ActiveConnection/9")),
        settings_connection: None,
        relevance: NetworkRelevance::Unsupported,
        wifi: None,
    }
}

fn overlay_device(state: NetworkDeviceState) -> NetworkDeviceSnapshot {
    NetworkDeviceSnapshot {
        id: id("/org/freedesktop/NetworkManager/Devices/4"),
        interface: "wg-gnu6".into(),
        kind: NetworkDeviceKind::Wireguard,
        state,
        managed: true,
        primary: false,
        active_connection: state
            .is_connected()
            .then(|| id("/org/freedesktop/NetworkManager/ActiveConnection/2")),
        settings_connection: None,
        relevance: NetworkRelevance::Unsupported,
        wifi: None,
    }
}

/// Infrastructure devices. The host has 28 bridges and 26 veth; they must never
/// reach presentation.
fn infrastructure(count: usize) -> Vec<NetworkDeviceSnapshot> {
    (0..count)
        .map(|index| NetworkDeviceSnapshot {
            id: id(&format!(
                "/org/freedesktop/NetworkManager/Devices/{}",
                100 + index
            )),
            interface: format!("br-{index:012x}"),
            kind: NetworkDeviceKind::Bridge,
            state: NetworkDeviceState::Connected,
            managed: true,
            primary: false,
            active_connection: Some(id(&format!(
                "/org/freedesktop/NetworkManager/ActiveConnection/{}",
                200 + index
            ))),
            settings_connection: None,
            relevance: NetworkRelevance::Unsupported,
            wifi: None,
        })
        .collect()
}

fn globals(
    state: NetworkDeviceState,
    connectivity: NetworkConnectivity,
    checked: bool,
) -> NetworkGlobals {
    NetworkGlobals {
        state,
        connectivity: NetworkConnectivityAssessment {
            value: connectivity,
            checked,
        },
        primary_connection: Some(id("/org/freedesktop/NetworkManager/ActiveConnection/4")),
        primary_connection_type: Some("802-11-wireless".into()),
        networking_enabled: true,
        wireless_enabled: true,
    }
}

fn snapshot(
    provider: NetworkProviderState,
    globals: Option<NetworkGlobals>,
    devices: Vec<NetworkDeviceSnapshot>,
) -> NetworkSnapshot {
    NetworkSnapshot {
        provider,
        generation: 1,
        provider_owner: Some(OWNER.into()),
        globals,
        devices: devices
            .into_iter()
            .map(|device| (device.id.clone(), device))
            .collect::<BTreeMap<_, _>>(),
        active_connections: BTreeMap::new(),
        settings_connections: BTreeMap::new(),
    }
}

fn ready_wifi(strength: u8) -> NetworkSnapshot {
    snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::ConnectedGlobal,
            NetworkConnectivity::Full,
            false,
        )),
        vec![
            wifi_device(NetworkDeviceState::Connected, strength),
            overlay_device(NetworkDeviceState::Connected),
            ethernet_device(NetworkDeviceState::Disconnected, false),
        ],
    )
}

// A provider that is not answering must never be presented as a disconnected
// network, and must not leave a stale transport, SSID or signal on screen.
#[test]
fn provider_absence_is_unavailable_and_carries_no_stale_truth() {
    for provider in [
        NetworkProviderState::Unavailable,
        NetworkProviderState::Reconnecting,
    ] {
        let presentation =
            ProductNetworkPresentation::derive(&snapshot(provider, None, Vec::new()));
        assert_eq!(presentation.primary, ProductNetworkPrimary::Unavailable);
        assert_ne!(presentation.primary, ProductNetworkPrimary::Disconnected);
        assert_eq!(presentation.connectivity, NetworkConnectivity::Unknown);
        assert!(!presentation.overlay_active);
        assert!(!presentation.has_transport());
    }
}

#[test]
fn a_ready_wifi_link_projects_observed_ssid_strength_and_security() {
    let presentation = ProductNetworkPresentation::derive(&ready_wifi(68));
    let ProductNetworkPrimary::Ready(endpoint) = &presentation.primary else {
        panic!("a connected Wi-Fi link is ready");
    };
    assert_eq!(endpoint.transport, NetworkTransport::Wifi);
    let wifi = endpoint.wifi.as_ref().expect("wifi facts");
    assert_eq!(wifi.ssid, "Cartier50");
    assert_eq!(wifi.strength, 68);
    assert_eq!(wifi.band, NetworkSignalBand::Medium);
    assert_eq!(wifi.security, NetworkSecurityMechanism::Rsn);
    assert!(wifi.authenticated);
    assert!(
        presentation.overlay_active,
        "the overlay is reported separately"
    );
}

#[test]
fn signal_strength_bands_are_a_pure_function_of_the_observation() {
    for (strength, band) in [
        (0_u8, NetworkSignalBand::None),
        (20, NetworkSignalBand::Weak),
        (65, NetworkSignalBand::Medium),
        (90, NetworkSignalBand::Strong),
    ] {
        let presentation = ProductNetworkPresentation::derive(&ready_wifi(strength));
        let ProductNetworkPrimary::Ready(endpoint) = &presentation.primary else {
            panic!("ready");
        };
        assert_eq!(
            endpoint.wifi.as_ref().expect("wifi").band,
            band,
            "{strength}"
        );
    }
}

// N-7 at the product boundary: connectivity is carried alongside transport and
// is never derived from it, nor it from connectivity.
#[test]
fn connectivity_is_not_transport_identity() {
    let portal = snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::ConnectedGlobal,
            NetworkConnectivity::Portal,
            true,
        )),
        vec![wifi_device(NetworkDeviceState::Connected, 70)],
    );
    let presentation = ProductNetworkPresentation::derive(&portal);
    assert!(
        presentation.has_transport(),
        "a portal does not remove the transport"
    );
    assert_eq!(presentation.connectivity, NetworkConnectivity::Portal);
    assert!(presentation.is_restricted());

    // And the reverse: a connected transport with a disabled check is Full but
    // explicitly not verified internet.
    let presentation = ProductNetworkPresentation::derive(&ready_wifi(70));
    assert_eq!(presentation.connectivity, NetworkConnectivity::Full);
    assert!(
        !presentation.connectivity_checked,
        "Full without a check must not claim verified internet"
    );
    assert!(!presentation.is_restricted());
}

#[test]
fn a_connected_provider_without_a_transport_is_not_ready() {
    // The overlay keeps the provider connected, but it is not a transport.
    let overlay_only = snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::ConnectedGlobal,
            NetworkConnectivity::Full,
            false,
        )),
        vec![overlay_device(NetworkDeviceState::Connected)],
    );
    let presentation = ProductNetworkPresentation::derive(&overlay_only);
    assert_eq!(presentation.primary, ProductNetworkPrimary::Disconnected);
    assert!(presentation.overlay_active);
    assert!(!presentation.has_transport());
}

#[test]
fn a_disconnected_ethernet_port_is_not_a_ready_transport() {
    let presentation = ProductNetworkPresentation::derive(&snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::Disconnected,
            NetworkConnectivity::None,
            false,
        )),
        vec![ethernet_device(NetworkDeviceState::Disconnected, false)],
    ));
    assert_eq!(presentation.primary, ProductNetworkPrimary::Disconnected);
    assert!(!presentation.has_transport());
}

#[test]
fn a_wired_primary_reports_ethernet_without_wireless_facts() {
    let wired = snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::ConnectedGlobal,
            NetworkConnectivity::Full,
            true,
        )),
        vec![ethernet_device(NetworkDeviceState::Connected, true)],
    );
    let presentation = ProductNetworkPresentation::derive(&wired);
    let ProductNetworkPrimary::Ready(endpoint) = &presentation.primary else {
        panic!("wired is ready");
    };
    assert_eq!(endpoint.transport, NetworkTransport::Ethernet);
    assert!(endpoint.wifi.is_none());
}

#[test]
fn a_transitional_device_projects_connecting() {
    let presentation = ProductNetworkPresentation::derive(&snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::Connecting,
            NetworkConnectivity::Limited,
            false,
        )),
        vec![wifi_device(NetworkDeviceState::Connecting, 0)],
    ));
    assert_eq!(presentation.primary, ProductNetworkPrimary::Connecting);
    assert!(!presentation.has_transport());
}

// Infrastructure must never reach presentation, however many of them there are.
#[test]
fn infrastructure_never_reaches_presentation() {
    let mut devices = vec![
        wifi_device(NetworkDeviceState::Connected, 70),
        overlay_device(NetworkDeviceState::Connected),
    ];
    devices.extend(infrastructure(50));
    let presentation = ProductNetworkPresentation::derive(&snapshot(
        NetworkProviderState::Available,
        Some(globals(
            NetworkDeviceState::ConnectedGlobal,
            NetworkConnectivity::Full,
            false,
        )),
        devices,
    ));
    assert_eq!(presentation.device_count, 52);
    assert_eq!(presentation.relevant_device_count, 2);
    let ProductNetworkPrimary::Ready(endpoint) = &presentation.primary else {
        panic!("ready");
    };
    assert_eq!(
        endpoint.transport,
        NetworkTransport::Wifi,
        "a bridge cannot become the presentation"
    );
}

// The Bar must not be able to reach the protocol. Asserted on the module source,
// with comments stripped, because a type-level test would not notice a
// re-export.
#[test]
fn the_product_projection_does_not_name_the_protocol() {
    let source = include_str!("../product_network.rs")
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with('*') && !trimmed.starts_with("/*")
        })
        .collect::<Vec<_>>()
        .join("\n");
    // `NetworkObjectRef` is included deliberately: it carries a provider owner
    // and an object path, so exposing it from the projection would put D-Bus
    // identity one re-export away from the Bar. `NetworkSnapshot` is NOT
    // forbidden, because `derive` must accept the observed facts as its input.
    for forbidden in [
        "zbus",
        "zvariant",
        "OwnedValue",
        "ObjectPath",
        "NetworkObjectRef",
        "org.freedesktop",
        "nmcli",
        "DeviceType",
        "RsnFlags",
    ] {
        assert!(
            !source.contains(forbidden),
            "the product projection must not reference {forbidden}"
        );
    }
}
