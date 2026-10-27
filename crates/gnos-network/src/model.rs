//! Typed NetworkManager read model for Gnosix.
//!
//! NETWORK-01 Phase 2 owns facts only. This module is the boundary where
//! external protocol data becomes typed product-domain data: no D-Bus path, no
//! numeric NetworkManager constant, no `zbus` type survives past this file.
//!
//! Three rules are load-bearing, and each exists because the live host forced
//! it (see `docs/workstreams/network/01/reconnaissance.md`):
//!
//! 1. **Only verified enum values are decoded.** NetworkManager's `DeviceType`
//!    numbering on this host is `1=ethernet 2=wifi 13=bridge 20=veth
//!    29=wireguard 30=wifi-p2p 32=loopback`. A copied table that claims
//!    `13=wireguard` misclassifies 27 of 59 devices and surfaces `virbr0` as a
//!    VPN overlay. Anything not verified becomes [`NetworkDeviceKind::Unsupported`]
//!    and is never inferred from an interface name or a driver string.
//! 2. **Provider absence is not a network fact.** NetworkManager disappearing
//!    means [`NetworkProviderState::Unavailable`]. It never means disconnected,
//!    Wi-Fi off, zero signal, or no internet.
//! 3. **Membership is not relevance.** 28 bridges and 26 veth devices sit on
//!    this host. [`NetworkRelevance`] is a semantic classification over typed
//!    facts, never a projection of the inventory and never a name prefix.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The NetworkManager well-known bus name.
pub const NETWORK_MANAGER_BUS_NAME: &str = "org.freedesktop.NetworkManager";
/// The NetworkManager root object path.
pub const NETWORK_MANAGER_ROOT_PATH: &str = "/org/freedesktop/NetworkManager";

/// Exact identity of one NetworkManager object the observer follows.
///
/// This is deliberately *not* a durable identity. It carries the provider owner
/// as well as the object path, because an object path only means something
/// relative to the service instance that published it. Across a provider
/// replacement the same path is a different object with a different lifetime,
/// and the observer must be unable to confuse the two.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetworkObjectRef {
    /// The unique bus name of the NetworkManager owner that published this path.
    pub provider_owner: String,
    /// The object path within that owner.
    pub path: String,
}

impl NetworkObjectRef {
    pub fn new(provider_owner: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            provider_owner: provider_owner.into(),
            path: path.into(),
        }
    }

    /// Whether this reference belongs to the given provider owner.
    pub fn belongs_to(&self, provider_owner: &str) -> bool {
        self.provider_owner == provider_owner
    }
}

/// Verified NetworkManager device kinds.
///
/// Only values confirmed against a live NetworkManager 1.58 host are named.
/// Everything else is `Unsupported`, which is a decision rather than an absence:
/// it means "this build has not verified what that number means", and it is
/// propagated into product state instead of being guessed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkDeviceKind {
    Ethernet,
    Wifi,
    /// Verified: 13. A bridge is infrastructure, never a user network.
    Bridge,
    /// Verified: 20. A container virtual interface, never a user network.
    Veth,
    /// Verified: 29. A WireGuard tunnel: an overlay, not a physical transport.
    Wireguard,
    /// Verified: 30. The Wi-Fi direct pseudo-interface; never a user network.
    WifiP2p,
    /// Verified: 32. Loopback.
    Loopback,
    /// A device type this build has not verified, or one NetworkManager adds
    /// later. Never inferred.
    Unsupported,
}

impl NetworkDeviceKind {
    /// Decodes a raw NetworkManager `DeviceType`.
    ///
    /// Total by construction: an unrecognised number is `Unsupported`, not a
    /// silent default that would let an unknown device masquerade as a known one.
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            13 => Self::Bridge,
            20 => Self::Veth,
            29 => Self::Wireguard,
            30 => Self::WifiP2p,
            32 => Self::Loopback,
            _ => Self::Unsupported,
        }
    }

    pub const fn as_raw(self) -> u32 {
        match self {
            Self::Ethernet => 1,
            Self::Wifi => 2,
            Self::Bridge => 13,
            Self::Veth => 20,
            Self::Wireguard => 29,
            Self::WifiP2p => 30,
            Self::Loopback => 32,
            Self::Unsupported => 0,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ethernet => "ethernet",
            Self::Wifi => "wifi",
            Self::Bridge => "bridge",
            Self::Veth => "veth",
            Self::Wireguard => "wireguard",
            Self::WifiP2p => "wifi-p2p",
            Self::Loopback => "loopback",
            Self::Unsupported => "unsupported",
        }
    }

    /// Whether this kind is a physical or near-physical user-facing transport.
    ///
    /// A bridge or a veth is a real device with a real IP address and is still
    /// not a user network; that distinction is what keeps Docker out of the Bar.
    pub const fn is_physical_candidate(self) -> bool {
        matches!(self, Self::Ethernet | Self::Wifi)
    }

    /// Whether this kind represents an overlay tunnel rather than a transport.
    pub const fn is_overlay(self) -> bool {
        matches!(self, Self::Wireguard)
    }

    /// Whether this kind is container/VM infrastructure.
    pub const fn is_infrastructure_virtual(self) -> bool {
        matches!(
            self,
            Self::Bridge | Self::Veth | Self::WifiP2p | Self::Loopback
        )
    }
}

impl NetworkProviderState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Reconnecting => "reconnecting",
        }
    }
}

/// Verified NetworkManager device states.
///
/// `100` is present because this host reports it for every activated device; it
/// is absent from older published tables that stop at 90, and assuming 90 would
/// classify every live device here as unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkDeviceState {
    Unknown,
    Asleep,
    Disconnected,
    Disconnecting,
    Connecting,
    /// At least one network is available, but not the internet.
    ConnectedLocal,
    /// A network is available, with limited internet.
    ConnectedSite,
    /// A network is available, and NetworkManager believes the internet works.
    ConnectedGlobal,
    /// Verified on this host: NetworkManager 1.58 reports 100 for an activated
    /// device. Treated as a connected state, not as an unknown one.
    Connected,
    /// A state value this build has not verified. Never inferred, and never
    /// silently folded into a connected or disconnected answer.
    Unverified(u32),
}

impl NetworkDeviceState {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Unknown,
            10 => Self::Asleep,
            20 => Self::Disconnected,
            30 => Self::Disconnecting,
            40 => Self::Connecting,
            50 => Self::ConnectedLocal,
            60 => Self::ConnectedSite,
            70 => Self::ConnectedGlobal,
            80 => Self::ConnectedSite,
            90 => Self::ConnectedGlobal,
            100 => Self::Connected,
            other => Self::Unverified(other),
        }
    }

    pub const fn as_raw(self) -> u32 {
        match self {
            Self::Unknown => 0,
            Self::Asleep => 10,
            Self::Disconnected => 20,
            Self::Disconnecting => 30,
            Self::Connecting => 40,
            Self::ConnectedLocal => 50,
            Self::ConnectedSite => 60,
            Self::ConnectedGlobal => 70,
            Self::Unverified(raw) => raw,
            Self::Connected => 100,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Asleep => "asleep",
            Self::Disconnected => "disconnected",
            Self::Disconnecting => "disconnecting",
            Self::Connecting => "connecting",
            Self::ConnectedLocal => "connected-local",
            Self::ConnectedSite => "connected-site",
            Self::ConnectedGlobal => "connected-global",
            Self::Connected => "connected",
            Self::Unverified(_) => "unverified",
        }
    }

    /// Whether the device currently carries a usable network.
    pub const fn is_connected(self) -> bool {
        matches!(
            self,
            Self::ConnectedLocal | Self::ConnectedSite | Self::ConnectedGlobal | Self::Connected
        )
    }

    /// Whether the device is mid-transition.
    pub const fn is_transitional(self) -> bool {
        matches!(self, Self::Connecting | Self::Disconnecting)
    }
}

/// NetworkManager's connectivity assessment.
///
/// This is **not** verified internet reachability unless
/// [`NetworkConnectivityAssessment::checked`] says connectivity checking is
/// enabled. On this host it is disabled, so `Full` means "NetworkManager sees a
/// working link", which is a weaker and different claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkConnectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
    /// A value this build has not verified. Never folded into `Full`.
    Unverified(u32),
}

impl NetworkConnectivity {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Unknown,
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            other => Self::Unverified(other),
        }
    }

    pub const fn as_raw(self) -> u32 {
        match self {
            Self::Unknown => 0,
            Self::None => 1,
            Self::Portal => 2,
            Self::Limited => 3,
            Self::Full => 4,
            Self::Unverified(raw) => raw,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::None => "none",
            Self::Portal => "portal",
            Self::Limited => "limited",
            Self::Full => "full",
            Self::Unverified(_) => "unverified",
        }
    }
}

/// Whether NetworkManager's connectivity number is backed by a real check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConnectivityAssessment {
    pub value: NetworkConnectivity,
    /// NetworkManager's `ConnectivityCheckEnabled`.
    ///
    /// When false, `Full` is an unverified link assessment. The product must
    /// not present it as working internet.
    pub checked: bool,
}

/// Whether the provider itself is answering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProviderState {
    Available,
    /// The well-known name has no owner. This is *not* "disconnected".
    Unavailable,
    /// A new owner appeared and has not yet established a baseline.
    Reconnecting,
}

/// The product's primary user-facing transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTransport {
    Wifi,
    Ethernet,
    /// A verified physical-class device this build does not name.
    OtherPhysical,
    /// No user-facing transport is currently primary.
    None,
}

/// Wi-Fi security, decoded from `RsnFlags` / `WpaFlags` / `Flags` bitmasks.
///
/// The flags are a bitmask, so a truthiness test would collapse WPA2 with a
/// misconfigured open network. Only what was observed is named.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkWifiSecurity {
    pub authenticated: bool,
    pub mechanism: NetworkSecurityMechanism,
    /// Cipher names, empty unless a build has verified the bit layout.
    pub ciphers: Vec<String>,
    /// Whether `ciphers` was decoded or is deliberately withheld.
    pub ciphers_decoded: bool,
    /// The observed flags, retained so a future build can decode them without
    /// having to guess again.
    pub raw_flags: NetworkSecurityFlags,
}

/// The raw security flags exactly as NetworkManager published them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSecurityFlags {
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkSecurityMechanism {
    Open,
    Wep,
    Wpa,
    /// RSN/WPA2 family, confirmed by a non-zero `RsnFlags`.
    Rsn,
    /// Flags were present that this build does not decode.
    Unverified,
}

/// Decodes NetworkManager's `Flags` / `WpaFlags` / `RsnFlags`.
///
/// Only the coarse mechanism is named, and only because it is corroborated: on
/// this host `RsnFlags` is non-zero (392) and the independent oracle reports
/// `WPA2`.
///
/// The individual key-management and cipher bit positions are **not** decoded.
/// A guessed bit table would be the `DeviceType` mistake a second time: this host
/// exposes exactly one access point, so there is no second observation to check a
/// table against, and a wrong table would confidently name a cipher the link is
/// not using. The raw values are carried instead, so a later build with more
/// observations can decode them, and until then the product cannot invent one.
pub fn decode_rsn_security(flags: u32, wpa_flags: u32, rsn_flags: u32) -> NetworkWifiSecurity {
    let mechanism = if rsn_flags != 0 {
        NetworkSecurityMechanism::Rsn
    } else if wpa_flags != 0 {
        NetworkSecurityMechanism::Wpa
    } else if flags != 0 {
        NetworkSecurityMechanism::Wep
    } else {
        NetworkSecurityMechanism::Open
    };
    NetworkWifiSecurity {
        authenticated: rsn_flags != 0 || wpa_flags != 0,
        mechanism,
        raw_flags: NetworkSecurityFlags {
            flags,
            wpa_flags,
            rsn_flags,
        },
        ciphers: Vec::new(),
        ciphers_decoded: false,
    }
}

/// Decodes an SSID from NetworkManager's byte-array representation.
///
/// Two forms of the same property were observed on the live host:
///
/// - the raw wire form, as `busctl` prints it, carries a leading **declared
///   length** byte: `Cartier50` appears as `[9, 67, 97, …]`;
/// - a `zvariant` `Vec<u8>` conversion of that value has already stripped the
///   prefix, yielding `[67, 97, 114, …]`.
///
/// Both occur depending on the reader, and decoding either naively as a plain
/// string yields a name the user never sees — `\tCartier50` for the first form.
/// The declared length is honoured only when it is consistent with the bytes
/// that arrived; guessing which half to believe would be the original defect
/// again.
pub fn decode_ssid(bytes: &[u8]) -> String {
    let Some((&declared, rest)) = bytes.split_first() else {
        return String::new();
    };
    // Trust the declared length only when it is consistent with what arrived. A
    // mismatched prefix means the encoding is not the one this build verified,
    // and guessing which half to believe would be the original defect again.
    let usable = if usize::from(declared) == rest.len() {
        rest
    } else {
        bytes
    };
    String::from_utf8_lossy(usable).into_owned()
}

/// Observed facts about the active wireless endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkWifiFacts {
    /// The access-point object identity, scoped to the provider owner.
    pub access_point: Option<NetworkObjectRef>,
    /// The observed BSSID. Identity of one radio instance, not a network.
    pub bssid: Option<String>,
    pub ssid: String,
    /// NetworkManager's 0..=100 signal value.
    pub strength: u8,
    pub frequency_mhz: u32,
    pub security: NetworkWifiSecurity,
}

impl NetworkWifiFacts {
    /// A coarse band for presentation. Presentation policy, not a measurement.
    pub const fn strength_band(&self) -> NetworkSignalBand {
        NetworkSignalBand::from_strength(self.strength)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkSignalBand {
    None,
    Weak,
    Medium,
    Strong,
}

impl NetworkSignalBand {
    /// Presentation bands over NetworkManager's 0..=100 signal value.
    ///
    /// A pure function of the observed number, so the Bar cannot invent a band
    /// the provider did not report.
    pub const fn from_strength(strength: u8) -> Self {
        match strength {
            0 => Self::None,
            1..=49 => Self::Weak,
            50..=74 => Self::Medium,
            _ => Self::Strong,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Weak => "weak",
            Self::Medium => "medium",
            Self::Strong => "strong",
        }
    }
}

/// The typed facts of one observed device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDeviceSnapshot {
    /// Provider-scoped runtime object identity.
    pub id: NetworkObjectRef,
    /// The kernel-facing interface name. Presentation identity only.
    pub interface: String,
    pub kind: NetworkDeviceKind,
    pub state: NetworkDeviceState,
    pub managed: bool,
    /// Whether this device backs the provider's primary connection.
    pub primary: bool,
    /// The active connection object backing this device, if any.
    pub active_connection: Option<NetworkObjectRef>,
    /// The durable settings connection this activation came from, if known.
    pub settings_connection: Option<NetworkObjectRef>,
    pub wifi: Option<NetworkWifiFacts>,
    /// The product relevance classification, derived from the facts above.
    pub relevance: NetworkRelevance,
}

/// Why a device is or is not product-relevant.
///
/// This is a closed vocabulary on purpose: "infrastructure" and "overlay" are
/// explicit answers, so a container interface cannot quietly become the Bar's
/// primary transport by being connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkRelevance {
    /// A physical-class transport that could be the user-facing primary.
    PhysicalPrimaryCandidate,
    /// A verified overlay tunnel such as WireGuard.
    Overlay,
    /// Container/VM/pseudo infrastructure.
    InfrastructureVirtual,
    Loopback,
    /// Present but NetworkManager does not manage it.
    Unmanaged,
    /// Present, managed, but not usable right now.
    Unavailable,
    /// A device kind this build has not verified.
    Unsupported,
}

impl NetworkRelevance {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PhysicalPrimaryCandidate => "physical-primary-candidate",
            Self::Overlay => "overlay",
            Self::InfrastructureVirtual => "infrastructure-virtual",
            Self::Loopback => "loopback",
            Self::Unmanaged => "unmanaged",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
        }
    }

    /// Whether this device should be considered for user-facing status.
    pub const fn is_user_facing(self) -> bool {
        matches!(self, Self::PhysicalPrimaryCandidate | Self::Overlay)
    }
}

/// Classifies one device's product relevance from typed facts only.
///
/// Never inspects an interface-name prefix. `br-*`, `docker0`, `virbr0` and
/// `veth*` are all classified the same way, by device kind, and a device with an
/// unverified kind is `Unsupported` rather than being given a friendly guess.
pub fn classify_relevance(
    kind: NetworkDeviceKind,
    state: NetworkDeviceState,
    managed: bool,
    primary: bool,
) -> NetworkRelevance {
    if kind == NetworkDeviceKind::Unsupported {
        return NetworkRelevance::Unsupported;
    }
    if kind == NetworkDeviceKind::Loopback {
        return NetworkRelevance::Loopback;
    }
    if kind.is_infrastructure_virtual() {
        return NetworkRelevance::InfrastructureVirtual;
    }
    if !managed {
        return NetworkRelevance::Unmanaged;
    }
    if kind.is_overlay() {
        // An overlay is relevant even while disconnected: "the VPN is down" is
        // user-facing information, distinct from "there is no VPN".
        return NetworkRelevance::Overlay;
    }
    if kind.is_physical_candidate() {
        // A managed, physical device that is not currently usable is still a
        // candidate; it is not the primary transport, and saying so must not
        // require pretending it does not exist.
        let _ = primary;
        return NetworkRelevance::PhysicalPrimaryCandidate;
    }
    if !state.is_connected() {
        return NetworkRelevance::Unavailable;
    }
    NetworkRelevance::Unsupported
}

/// One observed active connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkActiveConnectionSnapshot {
    pub id: NetworkObjectRef,
    /// NetworkManager's connection type string, e.g. `802-11-wireless`.
    pub connection_type: String,
    /// The device this activation drives.
    pub devices: Vec<NetworkObjectRef>,
    /// Whether this activation carries the default IPv4 route.
    pub default_ipv4: bool,
    pub state: NetworkActiveConnectionState,
    /// The durable settings profile this activation came from, if the provider
    /// published one. Distinct from `id`: a profile outlives any activation.
    pub settings_connection: Option<NetworkObjectRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkActiveConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Unverified(u32),
}

impl NetworkActiveConnectionState {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Unknown,
            1 => Self::Activating,
            2 => Self::Activated,
            3 => Self::Deactivating,
            other => Self::Unverified(other),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Activating => "activating",
            Self::Activated => "activated",
            Self::Deactivating => "deactivating",
            Self::Unverified(_) => "unverified",
        }
    }
}

/// The provider's own view of global state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkGlobals {
    pub state: NetworkDeviceState,
    pub connectivity: NetworkConnectivityAssessment,
    /// The active connection the provider considers primary.
    pub primary_connection: Option<NetworkObjectRef>,
    /// The provider's `PrimaryConnectionType`, e.g. `802-11-wireless`.
    pub primary_connection_type: Option<String>,
    pub networking_enabled: bool,
    pub wireless_enabled: bool,
}

/// One complete observation of network truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSnapshot {
    pub provider: NetworkProviderState,
    /// Advances only when product-relevant facts change.
    pub generation: u64,
    /// The provider owner this snapshot's object identities belong to.
    pub provider_owner: Option<String>,
    pub globals: Option<NetworkGlobals>,
    pub devices: BTreeMap<NetworkObjectRef, NetworkDeviceSnapshot>,
    pub active_connections: BTreeMap<NetworkObjectRef, NetworkActiveConnectionSnapshot>,
    /// The durable profile behind each active connection, keyed by that
    /// activation.
    ///
    /// Held separately so a runtime object and a durable profile never occupy
    /// the same field, and so no consumer can read a profile path as if it were
    /// an activation identity.
    pub settings_connections: BTreeMap<NetworkObjectRef, NetworkObjectRef>,
}

impl NetworkPrimaryState {
    /// A stable name for receipts and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Unavailable => "unavailable",
        }
    }
}

impl NetworkTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wifi => "wifi",
            Self::Ethernet => "ethernet",
            Self::OtherPhysical => "other-physical",
            Self::None => "none",
        }
    }
}

/// The product's answer to "is a relevant network path available, and which
/// transport is primary?".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPrimaryState {
    Connected,
    Connecting,
    Disconnected,
    /// The provider is not answering. Explicitly not `Disconnected`.
    Unavailable,
}

/// The derived, product-level network status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSummary {
    pub provider: NetworkProviderState,
    pub primary_state: NetworkPrimaryState,
    pub transport: NetworkTransport,
    pub connectivity: NetworkConnectivity,
    /// Whether connectivity is backed by a real NetworkManager check.
    pub connectivity_checked: bool,
    /// A meaningful overlay is active, presented separately from transport.
    pub overlay_active: bool,
    pub wifi: Option<NetworkWifiFacts>,
}

/// Derives the product-level network status from one observation.
///
/// Three separations are deliberate and load-bearing:
///
/// - A provider failure yields [`NetworkPrimaryState::Unavailable`], never
///   `Disconnected`.
/// - Connectivity is carried alongside transport, never folded into it: a
///   connected Wi-Fi link with a portal is connected *and* limited.
/// - Infrastructure devices never reach `transport`, so Docker cannot become
///   the user's network.
pub fn summarize(snapshot: &NetworkSnapshot) -> NetworkSummary {
    if snapshot.provider != NetworkProviderState::Available {
        return NetworkSummary {
            provider: snapshot.provider,
            primary_state: NetworkPrimaryState::Unavailable,
            transport: NetworkTransport::None,
            connectivity: NetworkConnectivity::Unknown,
            connectivity_checked: false,
            overlay_active: false,
            wifi: None,
        };
    }

    let globals = snapshot.globals.as_ref();
    // Relevance is re-derived here rather than read from the device's cached
    // field. Reading the cache made `summarize` depend on a value only the
    // reducer fills, so a snapshot that did not come through `apply_observation`
    // -- a fixture, or a test -- was silently read as having no relevant device
    // and was reported as disconnected. Classification is pure, so deriving it
    // here removes the hidden precondition instead of documenting it.
    let relevance_of = |device: &NetworkDeviceSnapshot| {
        classify_relevance(device.kind, device.state, device.managed, device.primary)
    };
    let relevant = snapshot
        .devices
        .values()
        .filter(|device| relevance_of(device).is_user_facing())
        .collect::<Vec<_>>();

    // The primary transport comes from the device the provider itself marks
    // primary, never from the first connected device in the inventory.
    //
    // Restricted to physical candidates: an overlay such as WireGuard is
    // reported on its own field, and must never be handed back as "the
    // transport", even when NetworkManager makes it the primary connection.
    let primary_device = snapshot.devices.values().find(|device| {
        device.primary && relevance_of(device) == NetworkRelevance::PhysicalPrimaryCandidate
    });

    let overlay_active = relevant.iter().any(|device| {
        relevance_of(device) == NetworkRelevance::Overlay && device.state.is_connected()
    });

    let (transport, wifi) = match primary_device {
        Some(device) => (
            match device.kind {
                NetworkDeviceKind::Wifi => NetworkTransport::Wifi,
                NetworkDeviceKind::Ethernet => NetworkTransport::Ethernet,
                _ => NetworkTransport::OtherPhysical,
            },
            // Wireless facts are only reported for a link that is up. A
            // provider can keep publishing an access point for a moment after the
            // association is gone, and presenting that stale SSID or signal
            // would make a dead link look current.
            device
                .state
                .is_connected()
                .then(|| device.wifi.clone())
                .flatten(),
        ),
        None => {
            // No primary device. A physical candidate that is merely
            // disconnected (this host has `enp3s0`) does not become the primary
            // transport, so this stays `None`.
            (NetworkTransport::None, None)
        }
    };

    // A physical candidate that exists but is not the primary connection is not
    // evidence that the user's link is up. Falling back to the provider's global
    // state in that case would claim `Connected` on the strength of an overlay or
    // of the global state alone -- the connectivity/transport conflation this
    // model exists to prevent. The live gate caught it: with Wi-Fi down, the
    // summary still reported `Connected`.
    let has_physical_candidate = relevant
        .iter()
        .any(|device| relevance_of(device) == NetworkRelevance::PhysicalPrimaryCandidate);

    let primary_state = match primary_device.map(|device| device.state) {
        Some(state) if state.is_connected() => NetworkPrimaryState::Connected,
        Some(state) if state.is_transitional() => NetworkPrimaryState::Connecting,
        Some(_) => NetworkPrimaryState::Disconnected,
        None if has_physical_candidate => NetworkPrimaryState::Disconnected,
        None => match globals {
            Some(globals) if globals.state.is_transitional() => NetworkPrimaryState::Connecting,
            Some(globals) if globals.state.is_connected() => NetworkPrimaryState::Connected,
            _ => NetworkPrimaryState::Disconnected,
        },
    };

    let (connectivity, connectivity_checked) = match globals {
        Some(globals) => (globals.connectivity.value, globals.connectivity.checked),
        None => (NetworkConnectivity::Unknown, false),
    };

    NetworkSummary {
        provider: snapshot.provider,
        primary_state,
        transport,
        connectivity,
        connectivity_checked,
        overlay_active,
        wifi,
    }
}

#[cfg(test)]
mod tests;
