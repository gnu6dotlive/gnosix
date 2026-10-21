//! Read-only product projection of observed network truth.
//!
//! This is the frozen seam between the network substrate and the Bar. Network
//! facts are reduced to product semantics here, once, so that no consumer needs
//! to know the provider.
//!
//! What the Bar is allowed to know is exactly what is in this file: typed
//! product state. It must never see `org.freedesktop.NetworkManager`, a D-Bus
//! object path, `zbus`, a numeric NetworkManager constant, a connection UUID, or
//! the relevance classifier's rules. A source-level test enforces that on the
//! Bar module.
//!
//! Two separations are load-bearing and are carried forward from the substrate:
//!
//! - **Provider absence is not a network state.** `Unavailable` is its own
//!   product state and never becomes `Disconnected`.
//! - **Connectivity is not transport.** A connected Wi-Fi link on a captive
//!   portal is `Ready` *and* `Portal`; neither field is derived from the other.

use gnos_network::{
    NetworkConnectivity, NetworkPrimaryState, NetworkProviderState, NetworkSecurityMechanism,
    NetworkSignalBand, NetworkSnapshot, NetworkTransport, summarize,
};

/// The observed wireless facts a presentation may report.
///
/// A copy of already-observed truth, retained only to render. It is not an
/// authority and is not consulted by anything that acts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductNetworkWifi {
    pub ssid: String,
    pub strength: u8,
    pub band: NetworkSignalBand,
    pub security: NetworkSecurityMechanism,
    pub authenticated: bool,
}

impl ProductNetworkWifi {
    /// A short, human word for the observed security mechanism.
    ///
    /// Only the mechanism is named. The cipher list is deliberately not decoded
    /// anywhere, because the substrate has not verified the provider's cipher bit
    /// layout and naming one would be a guess presented as a fact.
    pub const fn security_word(&self) -> &'static str {
        match self.security {
            NetworkSecurityMechanism::Open => "ouvert",
            NetworkSecurityMechanism::Wep => "WEP",
            NetworkSecurityMechanism::Wpa => "WPA",
            NetworkSecurityMechanism::Rsn => "WPA2",
            NetworkSecurityMechanism::Unverified => "sécurité inconnue",
        }
    }
}

/// The primary user-facing transport, once one is up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductNetworkEndpoint {
    pub transport: NetworkTransport,
    /// Present only when the transport is Wi-Fi and the link is up.
    pub wifi: Option<ProductNetworkWifi>,
}

/// The product's answer to "is a relevant network path up?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductNetworkPrimary {
    /// The provider is not answering, or is establishing a new baseline.
    /// Explicitly not `Disconnected`.
    Unavailable,
    /// The provider is answering and reports no usable primary transport.
    Disconnected,
    /// The provider is answering and a transport is coming up.
    Connecting,
    /// A transport is up.
    Ready(ProductNetworkEndpoint),
}

/// Everything the Bar may know about the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductNetworkPresentation {
    pub provider: NetworkProviderState,
    pub primary: ProductNetworkPrimary,
    /// The provider's link assessment, carried separately from transport.
    pub connectivity: NetworkConnectivity,
    /// Whether that assessment is backed by a real provider check.
    ///
    /// When false, `Full` means "the provider sees a working link" and must not
    /// be presented as verified internet reachability. On the observed host this
    /// is false.
    pub connectivity_checked: bool,
    /// A meaningful overlay such as WireGuard is up, reported on its own field
    /// so it is never mistaken for the transport.
    pub overlay_active: bool,
    /// Inventory diagnostics. Not product state: they are deliberately absent
    /// from any change comparison, because 28 bridges appearing is not a network
    /// change.
    pub device_count: usize,
    pub relevant_device_count: usize,
}

impl ProductNetworkPresentation {
    pub fn derive(snapshot: &NetworkSnapshot) -> Self {
        let summary = summarize(snapshot);

        let primary = match summary.provider {
            // A provider that is absent or rebuilding has no network to report.
            // Deriving anything here is how a restart becomes a false
            // "disconnected" and then a false "no internet".
            NetworkProviderState::Unavailable | NetworkProviderState::Reconnecting => {
                ProductNetworkPrimary::Unavailable
            }
            NetworkProviderState::Available => match summary.primary_state {
                NetworkPrimaryState::Unavailable => ProductNetworkPrimary::Unavailable,
                NetworkPrimaryState::Connecting => ProductNetworkPrimary::Connecting,
                NetworkPrimaryState::Disconnected => ProductNetworkPrimary::Disconnected,
                NetworkPrimaryState::Connected => {
                    match summary.transport {
                        // A connected provider with no user-facing transport is
                        // not a ready link; an overlay can keep the provider
                        // connected on its own.
                        NetworkTransport::None => ProductNetworkPrimary::Disconnected,
                        transport => ProductNetworkPrimary::Ready(ProductNetworkEndpoint {
                            transport,
                            wifi: summary.wifi.as_ref().map(|facts| ProductNetworkWifi {
                                ssid: facts.ssid.clone(),
                                strength: facts.strength,
                                band: facts.strength_band(),
                                security: facts.security.mechanism,
                                authenticated: facts.security.authenticated,
                            }),
                        }),
                    }
                }
            },
        };

        Self {
            provider: summary.provider,
            primary,
            connectivity: summary.connectivity,
            connectivity_checked: summary.connectivity_checked,
            overlay_active: summary.overlay_active,
            device_count: snapshot.devices.len(),
            // Derived rather than read from the device's cached field, for the
            // same reason `summarize` derives it: a snapshot that did not come
            // through the reducer must not be read as having no relevant device.
            relevant_device_count: snapshot
                .devices
                .values()
                .filter(|device| {
                    gnos_network::classify_relevance(
                        device.kind,
                        device.state,
                        device.managed,
                        device.primary,
                    )
                    .is_user_facing()
                })
                .count(),
        }
    }

    /// Whether the presentation describes a usable primary transport.
    pub fn has_transport(&self) -> bool {
        matches!(self.primary, ProductNetworkPrimary::Ready(_))
    }

    /// Whether connectivity is being reported as restricted.
    ///
    /// `Portal` and `Limited` are distinct observed values and are not merged.
    pub fn is_restricted(&self) -> bool {
        matches!(
            self.connectivity,
            NetworkConnectivity::Portal | NetworkConnectivity::Limited
        )
    }
}

#[cfg(test)]
mod tests;
