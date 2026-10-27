//! SURF-01A (U01): the Wi-Fi radio, one typed power.
//!
//! A request names the state it wants (on or off). It is revalidated
//! against the observed snapshot first: no provider, a different provider
//! owner than the one observed, or the radio already in that state, and
//! nothing is sent. Otherwise it writes NetworkManager's `WirelessEnabled`,
//! addressed to the observed owner (never to whoever holds the name now).
//! The outcome is the provider's answer only: the product shows the new
//! state when the observer reports it, never because the request returned.

use crate::{NETWORK_MANAGER_BUS_NAME, NETWORK_MANAGER_ROOT_PATH, NetworkSnapshot};

const NM_IFACE: &str = NETWORK_MANAGER_BUS_NAME;

/// Why a Wi-Fi request was not sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WirelessRefusal {
    /// No provider, or no global state observed yet.
    ProviderUnavailable,
    /// The radio is already in the requested state.
    AlreadySatisfied,
}

/// The provider's answer to a sent request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WirelessOutcome {
    /// Accepted by the provider; the observer will report the new state.
    Accepted,
    Refused(WirelessRefusal),
    /// The provider (or its authorization agent) refused it.
    Denied(String),
    /// The request did not reach a provider, or the answer was unreadable.
    TransportFailure(String),
}

/// The observed owner a request for `enable` may be sent to.
pub fn revalidate_wireless(
    snapshot: &NetworkSnapshot,
    enable: bool,
) -> Result<String, WirelessRefusal> {
    let (Some(owner), Some(globals)) = (&snapshot.provider_owner, &snapshot.globals) else {
        return Err(WirelessRefusal::ProviderUnavailable);
    };
    if globals.wireless_enabled == enable {
        return Err(WirelessRefusal::AlreadySatisfied);
    }
    Ok(owner.clone())
}

/// Map a provider error name to a denial or a transport failure.
pub fn classify_wireless_error(name: &str, message: &str) -> WirelessOutcome {
    let denied = name.ends_with("AccessDenied")
        || name.ends_with("PermissionDenied")
        || name.ends_with("NotAuthorized")
        || name.contains("PolicyKit");
    if denied {
        WirelessOutcome::Denied(format!("{name}: {message}"))
    } else {
        WirelessOutcome::TransportFailure(format!("{name}: {message}"))
    }
}

/// Revalidate, then ask NetworkManager (on `connection`, the system bus) to
/// turn the Wi-Fi radio on or off.
pub async fn request_wireless(
    connection: &zbus::Connection,
    snapshot: &NetworkSnapshot,
    enable: bool,
) -> WirelessOutcome {
    let owner = match revalidate_wireless(snapshot, enable) {
        Ok(owner) => owner,
        Err(refusal) => return WirelessOutcome::Refused(refusal),
    };
    let reply = connection
        .call_method(
            Some(owner.as_str()),
            NETWORK_MANAGER_ROOT_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(
                NM_IFACE,
                "WirelessEnabled",
                zbus::zvariant::Value::from(enable),
            ),
        )
        .await;
    match reply {
        Ok(_) => WirelessOutcome::Accepted,
        Err(zbus::Error::MethodError(name, message, _)) => {
            classify_wireless_error(name.as_str(), message.as_deref().unwrap_or(""))
        }
        Err(error) => WirelessOutcome::TransportFailure(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NetworkConnectivity, NetworkConnectivityAssessment, NetworkDeviceState, NetworkGlobals,
        NetworkState,
    };

    fn snapshot(owner: Option<&str>, wireless: Option<bool>) -> NetworkSnapshot {
        let mut s = NetworkState::new().snapshot();
        s.provider_owner = owner.map(str::to_owned);
        s.globals = wireless.map(|w| NetworkGlobals {
            state: NetworkDeviceState::Disconnected,
            connectivity: NetworkConnectivityAssessment {
                value: NetworkConnectivity::Unknown,
                checked: false,
            },
            primary_connection: None,
            primary_connection_type: None,
            networking_enabled: true,
            wireless_enabled: w,
        });
        s
    }

    #[test]
    fn a_request_is_sent_only_to_the_observed_provider_and_only_to_change_the_radio() {
        assert_eq!(
            revalidate_wireless(&snapshot(None, Some(true)), false),
            Err(WirelessRefusal::ProviderUnavailable)
        );
        assert_eq!(
            revalidate_wireless(&snapshot(Some(":1.7"), None), false),
            Err(WirelessRefusal::ProviderUnavailable)
        );
        assert_eq!(
            revalidate_wireless(&snapshot(Some(":1.7"), Some(true)), true),
            Err(WirelessRefusal::AlreadySatisfied)
        );
        assert_eq!(
            revalidate_wireless(&snapshot(Some(":1.7"), Some(true)), false),
            Ok(":1.7".to_owned())
        );
    }

    #[test]
    fn authorization_errors_are_denials_and_the_rest_transport() {
        assert!(matches!(
            classify_wireless_error("org.freedesktop.NetworkManager.PermissionDenied", "no"),
            WirelessOutcome::Denied(_)
        ));
        assert!(matches!(
            classify_wireless_error("org.freedesktop.DBus.Error.AccessDenied", "no"),
            WirelessOutcome::Denied(_)
        ));
        assert!(matches!(
            classify_wireless_error("org.freedesktop.DBus.Error.ServiceUnknown", "gone"),
            WirelessOutcome::TransportFailure(_)
        ));
    }
}
