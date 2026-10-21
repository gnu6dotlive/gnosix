//! Source tests for the typed NetworkManager read model.
//!
//! The cases are named for the invariants they defend. Three of them exist
//! because the live host proved the naive version wrong.

use super::*;

#[test]
fn only_verified_device_types_are_named_and_the_rest_are_unsupported() {
    // Verified against a live NetworkManager 1.58 host by cross-checking the
    // raw `DeviceType` against an independent oracle.
    assert_eq!(NetworkDeviceKind::from_raw(1), NetworkDeviceKind::Ethernet);
    assert_eq!(NetworkDeviceKind::from_raw(2), NetworkDeviceKind::Wifi);
    assert_eq!(NetworkDeviceKind::from_raw(13), NetworkDeviceKind::Bridge);
    assert_eq!(NetworkDeviceKind::from_raw(20), NetworkDeviceKind::Veth);
    assert_eq!(
        NetworkDeviceKind::from_raw(29),
        NetworkDeviceKind::Wireguard
    );
    assert_eq!(NetworkDeviceKind::from_raw(30), NetworkDeviceKind::WifiP2p);
    assert_eq!(NetworkDeviceKind::from_raw(32), NetworkDeviceKind::Loopback);

    // A number this build has not verified is never guessed. In particular a
    // copied table claiming `13 = wireguard` would make every bridge on the host
    // masquerade as a VPN overlay.
    for unknown in [0_u32, 6, 11, 14, 40, 99, 1000] {
        assert_eq!(
            NetworkDeviceKind::from_raw(unknown),
            NetworkDeviceKind::Unsupported,
            "raw {unknown} must not be decoded into a named kind"
        );
    }
}

#[test]
fn verified_state_100_is_connected_and_unverified_states_stay_explicit() {
    // This host reports 100 for every activated device. Tables that stop at 90
    // would classify all of them as unknown.
    assert_eq!(
        NetworkDeviceState::from_raw(100),
        NetworkDeviceState::Connected
    );
    assert!(NetworkDeviceState::from_raw(100).is_connected());
    assert!(NetworkDeviceState::from_raw(70).is_connected());
    assert!(NetworkDeviceState::from_raw(50).is_connected());
    assert!(!NetworkDeviceState::from_raw(20).is_connected());
    assert!(NetworkDeviceState::from_raw(40).is_transitional());

    // Unverified values are preserved, never folded into a connected answer.
    assert_eq!(
        NetworkDeviceState::from_raw(37),
        NetworkDeviceState::Unverified(37)
    );
    assert!(!NetworkDeviceState::from_raw(37).is_connected());
    assert_eq!(
        NetworkConnectivity::from_raw(9),
        NetworkConnectivity::Unverified(9)
    );
}

#[test]
fn ssid_is_decoded_from_its_length_prefixed_byte_array() {
    // NetworkManager publishes Ssid as a byte array whose first element is the
    // declared length. The live host's "Cartier50" arrives as
    // 9 67 97 114 116 105 101 114 53 48 -- note the leading 9.
    let bytes = [9, 67, 97, 114, 116, 105, 101, 114, 53, 48];
    assert_eq!(decode_ssid(&bytes), "Cartier50");
    assert_eq!(
        String::from_utf8_lossy(&bytes).into_owned(),
        "\tCartier50",
        "decoding the whole array is the bug: a name the user never sees"
    );

    assert_eq!(decode_ssid(&[]), "");
    // An inconsistent prefix is not guessed around: the whole array is used
    // rather than deciding which half to believe.
    assert_eq!(decode_ssid(&[9, 67, 97]), "\tCa");
    // A declared length of zero is taken at face value only when it matches.
    assert_eq!(decode_ssid(&[0]), "");
    // A non-UTF-8 SSID degrades rather than panicking.
    assert_eq!(decode_ssid(&[2, 0xff, 0xfe]), "\u{fffd}\u{fffd}");
}

#[test]
fn rsn_flags_yield_a_corroborated_mechanism_and_no_invented_cipher() {
    // The live host reports RsnFlags 392 while the independent oracle reports
    // WPA2. The mechanism is therefore corroborated.
    let security = decode_rsn_security(3, 0, 392);
    assert_eq!(security.mechanism, NetworkSecurityMechanism::Rsn);
    assert!(security.authenticated);

    // The cipher bit layout is deliberately NOT decoded. This host exposes a
    // single access point, so there is no second observation to check a bit
    // table against, and a guessed table would be the `DeviceType` mistake a
    // second time -- it would confidently name a cipher the link does not use.
    assert!(security.ciphers.is_empty());
    assert!(!security.ciphers_decoded);
    assert_eq!(security.raw_flags.rsn_flags, 392);
    assert_eq!(security.raw_flags.flags, 3);

    // A non-zero Flags alone is not RSN.
    assert_eq!(
        decode_rsn_security(3, 0, 0).mechanism,
        NetworkSecurityMechanism::Wep
    );
    assert_eq!(
        decode_rsn_security(0, 7, 0).mechanism,
        NetworkSecurityMechanism::Wpa
    );
    assert_eq!(
        decode_rsn_security(0, 0, 0).mechanism,
        NetworkSecurityMechanism::Open
    );
    assert!(
        !decode_rsn_security(0, 0, 0).authenticated,
        "an unencrypted link must not claim to be authenticated"
    );
}

#[test]
fn relevance_never_depends_on_an_interface_name_prefix() {
    // Docker, libvirt and a hand-made bridge are classified identically,
    // because classification reads device kind only.
    for name in ["docker0", "virbr0", "br-3c3a78200294", "br-", "br-anything"] {
        assert_eq!(
            classify_relevance(
                NetworkDeviceKind::Bridge,
                NetworkDeviceState::Connected,
                true,
                false
            ),
            NetworkRelevance::InfrastructureVirtual,
            "{name} is infrastructure by kind, not by its name"
        );
    }
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::Veth,
            NetworkDeviceState::Asleep,
            false,
            false
        ),
        NetworkRelevance::InfrastructureVirtual
    );
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::WifiP2p,
            NetworkDeviceState::Disconnecting,
            true,
            false
        ),
        NetworkRelevance::InfrastructureVirtual
    );
}

#[test]
fn wireguard_is_an_overlay_and_loopback_is_never_user_facing() {
    // This host's wg-gnu6 is a WireGuard connection type, not a `Vpn` service.
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::Wireguard,
            NetworkDeviceState::Connected,
            true,
            false
        ),
        NetworkRelevance::Overlay
    );
    // An overlay stays relevant while down: "the tunnel is down" is user-facing.
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::Wireguard,
            NetworkDeviceState::Disconnected,
            true,
            false
        ),
        NetworkRelevance::Overlay
    );
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::Loopback,
            NetworkDeviceState::Connected,
            true,
            false
        ),
        NetworkRelevance::Loopback
    );
    assert!(!NetworkRelevance::Loopback.is_user_facing());
    assert!(!NetworkRelevance::InfrastructureVirtual.is_user_facing());
}

#[test]
fn an_unmanaged_or_unverified_device_is_not_a_candidate() {
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::Ethernet,
            NetworkDeviceState::Connected,
            false,
            false
        ),
        NetworkRelevance::Unmanaged
    );
    assert_eq!(
        classify_relevance(
            NetworkDeviceKind::Unsupported,
            NetworkDeviceState::Connected,
            true,
            true
        ),
        NetworkRelevance::Unsupported
    );
    assert!(!NetworkRelevance::Unmanaged.is_user_facing());
    assert!(!NetworkRelevance::Unsupported.is_user_facing());
}

#[test]
fn object_identity_is_scoped_to_the_provider_owner() {
    let before = NetworkObjectRef::new(":1.7", "/org/freedesktop/NetworkManager/Devices/3");
    let after = NetworkObjectRef::new(":1.42", "/org/freedesktop/NetworkManager/Devices/3");
    assert_ne!(before, after);
    assert!(before.belongs_to(":1.7"));
    assert!(!before.belongs_to(":1.42"));
}

#[test]
fn signal_bands_are_a_pure_function_of_the_observed_number() {
    assert_eq!(NetworkSignalBand::from_strength(0), NetworkSignalBand::None);
    assert_eq!(
        NetworkSignalBand::from_strength(20),
        NetworkSignalBand::Weak
    );
    assert_eq!(
        NetworkSignalBand::from_strength(65),
        NetworkSignalBand::Medium
    );
    assert_eq!(
        NetworkSignalBand::from_strength(90),
        NetworkSignalBand::Strong
    );
}

// N-6: the product model must not be able to see the protocol. Type-level
// assertions would not catch a future edit that re-exported a raw value, so this
// checks the module's own source with comments stripped.
#[test]
fn the_product_model_does_not_import_or_name_the_dbus_protocol() {
    let source = include_str!("../model.rs")
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with('*') && !trimmed.starts_with("/*")
        })
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "zbus",
        "zvariant",
        "OwnedValue",
        "OwnedObjectPath",
        "use dbus",
        "get_property",
        "GetAll",
        "Message",
    ] {
        assert!(
            !source.contains(forbidden),
            "the typed product model must not reference {forbidden}"
        );
    }
}

// N-3: two activations of the same connection type are different objects and
// must never be conflated. Only the object identity distinguishes them.
#[test]
fn two_activations_of_one_type_are_distinct_objects() {
    let first = NetworkObjectRef::new(":1.7", "/org/freedesktop/NetworkManager/ActiveConnection/4");
    let second =
        NetworkObjectRef::new(":1.7", "/org/freedesktop/NetworkManager/ActiveConnection/9");
    // Both carry the connection type "802-11-wireless"; the type is a class, not
    // an identity.
    assert_ne!(first, second);
    let mut set = std::collections::BTreeSet::new();
    set.insert(first.clone());
    set.insert(second.clone());
    assert_eq!(set.len(), 2, "one type, two runtime objects");
}
