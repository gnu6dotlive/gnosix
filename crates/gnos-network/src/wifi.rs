//! SETTINGS-02C: the Wi-Fi networks in range, and connecting to one.
//!
//! A one-shot read of NetworkManager (never polled): each Wi-Fi device's
//! access points, and the saved Wi-Fi connections. Connecting uses a saved
//! connection, or an open network; a secured network that is not saved
//! needs a passphrase, which Settings cannot take yet, so it is reported
//! as such rather than attempted.

use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const NM_BUS: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const PROPS: &str = "org.freedesktop.DBus.Properties";
const DEVICE_TYPE_WIFI: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// Signal strength, 0..=100.
    pub strength: u8,
    pub secured: bool,
    /// A saved connection exists for this SSID.
    pub saved: bool,
    pub active: bool,
    /// What a connect request needs.
    pub device: String,
    pub access_point: String,
    pub saved_connection: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiConnectOutcome {
    Requested,
    /// A secured network with no saved connection needs a passphrase.
    NeedsPassphrase,
    Failed(String),
}

impl WifiNetwork {
    /// What connecting would do.
    pub fn can_connect_without_passphrase(&self) -> bool {
        !self.secured || self.saved
    }
}

/// Strongest first, the active one at the top, one entry per SSID.
pub fn arrange(mut networks: Vec<WifiNetwork>) -> Vec<WifiNetwork> {
    networks.retain(|n| !n.ssid.is_empty());
    networks.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.strength.cmp(&a.strength))
            .then(a.ssid.cmp(&b.ssid))
    });
    let mut seen = std::collections::HashSet::new();
    networks.retain(|n| seen.insert(n.ssid.clone()));
    networks
}

async fn get<T: TryFrom<OwnedValue>>(
    bus: &zbus::Connection,
    path: &str,
    iface: &str,
    name: &str,
) -> Option<T> {
    let reply = bus
        .call_method(Some(NM_BUS), path, Some(PROPS), "Get", &(iface, name))
        .await
        .ok()?;
    let value: OwnedValue = reply.body().deserialize().ok()?;
    T::try_from(value).ok()
}

async fn paths(bus: &zbus::Connection, path: &str, iface: &str, method: &str) -> Vec<String> {
    let Ok(reply) = bus
        .call_method(Some(NM_BUS), path, Some(iface), method, &())
        .await
    else {
        return Vec::new();
    };
    reply
        .body()
        .deserialize::<Vec<OwnedObjectPath>>()
        .map(|v| v.into_iter().map(|p| p.to_string()).collect())
        .unwrap_or_default()
}

/// The SSIDs of the saved Wi-Fi connections, with each one's path.
async fn saved_wifi(bus: &zbus::Connection) -> HashMap<String, String> {
    let mut saved = HashMap::new();
    for path in paths(
        bus,
        NM_SETTINGS_PATH,
        "org.freedesktop.NetworkManager.Settings",
        "ListConnections",
    )
    .await
    {
        let Ok(reply) = bus
            .call_method(
                Some(NM_BUS),
                path.as_str(),
                Some("org.freedesktop.NetworkManager.Settings.Connection"),
                "GetSettings",
                &(),
            )
            .await
        else {
            continue;
        };
        let Ok(settings) = reply
            .body()
            .deserialize::<HashMap<String, HashMap<String, OwnedValue>>>()
        else {
            continue;
        };
        let Some(wifi) = settings.get("802-11-wireless") else {
            continue;
        };
        if let Some(ssid) = wifi
            .get("ssid")
            .and_then(|v| Vec::<u8>::try_from(v.try_clone().ok()?).ok())
        {
            saved.insert(crate::decode_ssid(&ssid), path);
        }
    }
    saved
}

/// The networks in range on every Wi-Fi device.
pub async fn list_networks(bus: &zbus::Connection) -> Vec<WifiNetwork> {
    let saved = saved_wifi(bus).await;
    let mut networks = Vec::new();
    for device in paths(bus, NM_PATH, "org.freedesktop.NetworkManager", "GetDevices").await {
        let kind: Option<u32> = get(
            bus,
            &device,
            "org.freedesktop.NetworkManager.Device",
            "DeviceType",
        )
        .await;
        if kind != Some(DEVICE_TYPE_WIFI) {
            continue;
        }
        let wireless = "org.freedesktop.NetworkManager.Device.Wireless";
        let active: Option<OwnedObjectPath> =
            get(bus, &device, wireless, "ActiveAccessPoint").await;
        for ap in paths(bus, &device, wireless, "GetAllAccessPoints").await {
            let iface = "org.freedesktop.NetworkManager.AccessPoint";
            let ssid: Option<Vec<u8>> = get(bus, &ap, iface, "Ssid").await;
            let strength: Option<u8> = get(bus, &ap, iface, "Strength").await;
            let flags: Option<u32> = get(bus, &ap, iface, "Flags").await;
            let wpa: Option<u32> = get(bus, &ap, iface, "WpaFlags").await;
            let rsn: Option<u32> = get(bus, &ap, iface, "RsnFlags").await;
            let (Some(ssid), Some(strength)) = (ssid, strength) else {
                continue;
            };
            let ssid = crate::decode_ssid(&ssid);
            networks.push(WifiNetwork {
                secured: flags.unwrap_or(0) & 1 != 0
                    || wpa.unwrap_or(0) != 0
                    || rsn.unwrap_or(0) != 0,
                saved: saved.contains_key(&ssid),
                saved_connection: saved.get(&ssid).cloned(),
                active: active.as_ref().is_some_and(|a| a.as_str() == ap),
                strength: strength.min(100),
                device: device.clone(),
                access_point: ap,
                ssid,
            });
        }
    }
    arrange(networks)
}

/// The connection settings for a WPA-PSK network, as NetworkManager's
/// `AddAndActivateConnection` takes them.
pub fn passphrase_settings(
    ssid: &str,
    passphrase: &str,
) -> HashMap<String, HashMap<String, Value<'static>>> {
    let section = |pairs: Vec<(&str, Value<'static>)>| {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<HashMap<_, _>>()
    };
    HashMap::from([
        (
            "connection".to_owned(),
            section(vec![
                ("type", Value::from("802-11-wireless".to_owned())),
                ("id", Value::from(ssid.to_owned())),
            ]),
        ),
        (
            "802-11-wireless".to_owned(),
            section(vec![("ssid", Value::from(ssid.as_bytes().to_vec()))]),
        ),
        (
            "802-11-wireless-security".to_owned(),
            section(vec![
                ("key-mgmt", Value::from("wpa-psk".to_owned())),
                ("psk", Value::from(passphrase.to_owned())),
            ]),
        ),
    ])
}

/// Connect to a secured network that is not saved, with its passphrase (8
/// to 63 characters). The passphrase is handed to NetworkManager and kept
/// nowhere else.
pub async fn connect_with_passphrase(
    bus: &zbus::Connection,
    network: &WifiNetwork,
    passphrase: &str,
) -> WifiConnectOutcome {
    if !(8..=63).contains(&passphrase.chars().count()) {
        return WifiConnectOutcome::Failed("a passphrase has 8 to 63 characters".into());
    }
    let (Ok(device), Ok(ap)) = (
        OwnedObjectPath::try_from(network.device.as_str()),
        OwnedObjectPath::try_from(network.access_point.as_str()),
    ) else {
        return WifiConnectOutcome::Failed("bad path".into());
    };
    match bus
        .call_method(
            Some(NM_BUS),
            NM_PATH,
            Some("org.freedesktop.NetworkManager"),
            "AddAndActivateConnection",
            &(passphrase_settings(&network.ssid, passphrase), device, ap),
        )
        .await
    {
        Ok(_) => WifiConnectOutcome::Requested,
        Err(error) => WifiConnectOutcome::Failed(error.to_string()),
    }
}

/// Ask NetworkManager to connect to `network`.
pub async fn connect(bus: &zbus::Connection, network: &WifiNetwork) -> WifiConnectOutcome {
    let root = OwnedObjectPath::try_from("/").expect("root path");
    let Ok(device) = OwnedObjectPath::try_from(network.device.as_str()) else {
        return WifiConnectOutcome::Failed("bad device path".into());
    };
    let reply = if let Some(saved) = &network.saved_connection {
        let Ok(saved) = OwnedObjectPath::try_from(saved.as_str()) else {
            return WifiConnectOutcome::Failed("bad connection path".into());
        };
        bus.call_method(
            Some(NM_BUS),
            NM_PATH,
            Some("org.freedesktop.NetworkManager"),
            "ActivateConnection",
            &(saved, device, root),
        )
        .await
    } else if !network.secured {
        let Ok(ap) = OwnedObjectPath::try_from(network.access_point.as_str()) else {
            return WifiConnectOutcome::Failed("bad access point path".into());
        };
        let empty: HashMap<String, HashMap<String, Value>> = HashMap::new();
        bus.call_method(
            Some(NM_BUS),
            NM_PATH,
            Some("org.freedesktop.NetworkManager"),
            "AddAndActivateConnection",
            &(empty, device, ap),
        )
        .await
    } else {
        return WifiConnectOutcome::NeedsPassphrase;
    };
    match reply {
        Ok(_) => WifiConnectOutcome::Requested,
        Err(error) => WifiConnectOutcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(ssid: &str, strength: u8, active: bool) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.into(),
            strength,
            secured: true,
            saved: false,
            active,
            device: "/d".into(),
            access_point: format!("/ap/{ssid}{strength}"),
            saved_connection: None,
        }
    }

    #[test]
    fn networks_are_active_first_then_strongest_one_per_ssid() {
        let out = arrange(vec![
            net("b", 40, false),
            net("a", 80, false),
            net("b", 70, false),
            net("", 99, false),
            net("c", 10, true),
        ]);
        let order: Vec<_> = out.iter().map(|n| (n.ssid.as_str(), n.strength)).collect();
        assert_eq!(order, [("c", 10), ("a", 80), ("b", 70)]);
    }

    #[test]
    fn passphrase_settings_name_the_network_and_hold_the_key_only_in_security() {
        let s = passphrase_settings("Home", "correct horse");
        assert_eq!(s["connection"]["id"], Value::from("Home".to_owned()));
        assert_eq!(s["802-11-wireless"]["ssid"], Value::from(b"Home".to_vec()));
        assert_eq!(
            s["802-11-wireless-security"]["psk"],
            Value::from("correct horse".to_owned())
        );
        assert!(!s["connection"].contains_key("psk"));
    }

    #[test]
    fn a_secured_network_needs_a_saved_connection_to_connect_without_a_passphrase() {
        let mut n = net("x", 50, false);
        assert!(!n.can_connect_without_passphrase());
        n.saved = true;
        assert!(n.can_connect_without_passphrase());
        n.saved = false;
        n.secured = false;
        assert!(n.can_connect_without_passphrase());
    }
}
