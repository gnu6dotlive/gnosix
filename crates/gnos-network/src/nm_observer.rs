//! Native event-driven NetworkManager observer.
//!
//! Event-driven, with no polling: it subscribes to the well-known name owner
//! and to NetworkManager's signals, and re-reads typed facts when something may
//! have changed. A 30 s live subscription on the operator host produced exactly
//! two events on a quiet network (`Device.Wireless/Bitrate`,
//! `AccessPoint/Strength`), so no periodic rescan is justified.
//!
//! Nothing here mutates NetworkManager. The only operations are property
//! reads and signal subscription. There is no `nmcli` subprocess anywhere in
//! this crate, and no polling loop.
//!
//! The provider boundary: this is the only file that knows about D-Bus. Every
//! value leaving it is typed and provider-scoped, so no D-Bus path or numeric
//! NetworkManager constant can reach a widget.

use crate::model::*;
use crate::reducer::NetworkState;

const NM: &str = NETWORK_MANAGER_BUS_NAME;
const ROOT: &str = NETWORK_MANAGER_ROOT_PATH;
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
const DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
const DEVICE_WIRELESS_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACTIVE_IFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const ACCESS_POINT_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

#[derive(Debug, thiserror::Error)]
pub enum NetworkObserverError {
    #[error("cannot reach the NetworkManager system service: {0}")]
    Connect(String),
    #[error("NetworkManager call failed: {0}")]
    Call(String),
    #[error("NetworkManager property {interface}.{property} is absent: {error}")]
    Property {
        interface: String,
        property: String,
        error: String,
    },
    #[error("unexpected NetworkManager reply: {0}")]
    UnexpectedReply(&'static str),
}

/// Which optional reads the observer performs.
#[derive(Clone, Copy, Debug)]
pub struct NetworkObserverOptions {
    /// Read each Wi-Fi device's active access point facts.
    pub read_wireless: bool,
}

impl Default for NetworkObserverOptions {
    fn default() -> Self {
        Self {
            read_wireless: true,
        }
    }
}

/// A typed reduction of live NetworkManager facts.
pub struct NetworkObserver {
    state: NetworkState,
    options: NetworkObserverOptions,
}

impl NetworkObserver {
    pub fn new(options: NetworkObserverOptions) -> Self {
        Self {
            state: NetworkState::new(),
            options,
        }
    }

    pub fn state(&self) -> &NetworkState {
        &self.state
    }

    pub fn snapshot(&self) -> NetworkSnapshot {
        self.state.snapshot()
    }

    pub fn summary(&self) -> NetworkSummary {
        self.state.summary()
    }

    /// Records that the well-known name lost its owner.
    ///
    /// Provider loss is never reported as a network state change.
    pub fn observe_provider_gone(&mut self) -> bool {
        self.state.set_provider_unavailable()
    }

    /// Records a new well-known name owner.
    ///
    /// Every runtime identity held so far is discarded, so a restart cannot be
    /// read as a disconnect and cannot leave a stale `Device`,
    /// `ActiveConnection` or `AccessPoint` path in use.
    pub fn observe_provider_owner(&mut self, owner: &str) -> bool {
        self.state.set_provider_owner(Some(owner))
    }

    /// Enumerates live facts and folds them in.
    ///
    /// The first call establishes a baseline and returns `false`: enumeration is
    /// not a network change. Returns whether product-relevant truth changed.
    pub async fn refresh(&mut self) -> Result<bool, NetworkObserverError> {
        let connection = system_connection().await?;
        let Some(owner) = resolve_owner(&connection).await? else {
            return Ok(self.state.set_provider_unavailable());
        };

        let mut globals = read_globals(&connection, &owner).await?;
        let device_paths = read_object_path_list(&connection, &owner, ROOT, NM, "Devices").await?;
        let active_paths =
            read_object_path_list(&connection, &owner, ROOT, NM, "ActiveConnections").await?;

        let primary_path =
            read_object_path(&connection, &owner, ROOT, NM, "PrimaryConnection").await?;
        let primary = primary_path.map(|path| NetworkObjectRef::new(owner.clone(), path));
        globals.primary_connection = primary.clone();

        let mut devices = Vec::with_capacity(device_paths.len());
        for path in &device_paths {
            if let Some(device) =
                read_device(&connection, &owner, path, self.options, primary.as_ref()).await?
            {
                devices.push(device);
            }
        }

        let mut active = Vec::with_capacity(active_paths.len());
        for path in &active_paths {
            if let Some(entry) = read_active_connection(&connection, &owner, path).await? {
                active.push(entry);
            }
        }

        Ok(self
            .state
            .apply_observation(&owner, Some(globals), devices, active))
    }
}

async fn system_connection() -> Result<zbus::Connection, NetworkObserverError> {
    zbus::Connection::system()
        .await
        .map_err(|error| NetworkObserverError::Connect(error.to_string()))
}

/// The unique owner of the NetworkManager well-known name, if any.
pub async fn resolve_owner(
    connection: &zbus::Connection,
) -> Result<Option<String>, NetworkObserverError> {
    let name = zbus::names::BusName::try_from(NM.to_owned())
        .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
    let proxy = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
    if !proxy
        .name_has_owner(name.clone())
        .await
        .map_err(|error| NetworkObserverError::Call(error.to_string()))?
    {
        return Ok(None);
    }
    proxy
        .get_name_owner(name)
        .await
        .map(|owner| Some(owner.to_string()))
        .map_err(|error| NetworkObserverError::Call(error.to_string()))
}

/// Reads one property through the D-Bus Properties interface.
///
/// An absent property is `None`, never a default. NetworkManager 1.58 has no
/// root `StateReason`, and folding that into a zero would fabricate a fact.
async fn read_property(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<Option<zbus::zvariant::OwnedValue>, NetworkObserverError> {
    let proxy = zbus::Proxy::new(connection, owner, path, PROPERTIES_IFACE)
        .await
        .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
    match proxy.call("Get", &(interface, property)).await {
        Ok(value) => Ok(Some(value)),
        // A vanished object is an appearance/disappearance event, not a
        // failure. NetworkManager destroys a device or activation object and the
        // bus reports it as `UnknownMethod` with "Object does not exist at path",
        // which is the same class of thing as an unknown property. Reporting it
        // as an error would make ordinary device teardown look like a fault and
        // would abort the rest of that enumeration.
        Err(zbus::Error::MethodError(_, name, detail))
            if is_absent(name.as_deref(), Some(&detail.to_string())) =>
        {
            Ok(None)
        }
        Err(error) => Err(NetworkObserverError::Property {
            interface: interface.to_owned(),
            property: property.to_owned(),
            error: error.to_string(),
        }),
    }
}

/// Whether a failed read means "this fact is not published" rather than "the
/// read broke".
fn is_absent(name: Option<&str>, detail: Option<&str>) -> bool {
    if matches!(
        name,
        Some("org.freedesktop.DBus.Error.UnknownProperty")
            | Some("org.freedesktop.DBus.Error.UnknownObject")
            | Some("org.freedesktop.DBus.Error.UnknownInterface")
            | Some("org.freedesktop.DBus.Error.UnknownMethod")
    ) {
        return true;
    }
    detail.is_some_and(|detail| {
        detail.contains("Object does not exist")
            || detail.contains("was not exported")
            || detail.contains("No such interface")
            || detail.contains("No such object")
    })
}

/// Reads a property that this build requires; absence is an error, not a zero.
async fn require_property<T>(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<T, NetworkObserverError>
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
    T::Error: std::fmt::Debug,
{
    let value = read_property(connection, owner, path, interface, property)
        .await?
        .ok_or_else(|| NetworkObserverError::Property {
            interface: interface.to_owned(),
            property: property.to_owned(),
            error: "absent".to_owned(),
        })?;
    T::try_from(value).map_err(|_| NetworkObserverError::UnexpectedReply("required property"))
}

/// Reads a property that this build tolerates being absent.
async fn optional_property<T>(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    property: &str,
    fallback: T,
) -> Result<T, NetworkObserverError>
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
    T::Error: std::fmt::Debug,
{
    match read_property(connection, owner, path, interface, property).await? {
        Some(value) => Ok(T::try_from(value).unwrap_or(fallback)),
        None => Ok(fallback),
    }
}

async fn read_object_path(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<Option<String>, NetworkObserverError> {
    // An object path must be decoded as an object path. Reading it as a `String`
    // fails the conversion and silently falls back, which made every
    // `ActiveConnection`, `PrimaryConnection` and `ActiveAccessPoint` read as
    // absent: the network looked connected with no transport and no SSID, and
    // nothing anywhere reported a missing property.
    let value: zbus::zvariant::OwnedObjectPath = optional_property(
        connection,
        owner,
        path,
        interface,
        property,
        zbus::zvariant::OwnedObjectPath::try_from("/").expect("the root object path is valid"),
    )
    .await?;
    let text = value.to_string();
    Ok((!text.is_empty() && text != "/").then_some(text))
}

async fn read_object_path_list(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<Vec<String>, NetworkObserverError> {
    let value: Vec<zbus::zvariant::OwnedObjectPath> =
        require_property(connection, owner, path, interface, property).await?;
    Ok(value.into_iter().map(|path| path.to_string()).collect())
}

async fn read_globals(
    connection: &zbus::Connection,
    owner: &str,
) -> Result<NetworkGlobals, NetworkObserverError> {
    let state: u32 = require_property(connection, owner, ROOT, NM, "State").await?;
    let connectivity: u32 = require_property(connection, owner, ROOT, NM, "Connectivity").await?;
    // Verified present on this host as false, which is why `Full` must not be
    // presented as working internet.
    let checked: bool = optional_property(
        connection,
        owner,
        ROOT,
        NM,
        "ConnectivityCheckEnabled",
        false,
    )
    .await?;
    let primary_connection_type: String = optional_property(
        connection,
        owner,
        ROOT,
        NM,
        "PrimaryConnectionType",
        String::new(),
    )
    .await?;
    let networking_enabled: bool =
        optional_property(connection, owner, ROOT, NM, "NetworkingEnabled", false).await?;
    let wireless_enabled: bool =
        optional_property(connection, owner, ROOT, NM, "WirelessEnabled", false).await?;

    Ok(NetworkGlobals {
        state: NetworkDeviceState::from_raw(state),
        connectivity: NetworkConnectivityAssessment {
            value: NetworkConnectivity::from_raw(connectivity),
            checked,
        },
        primary_connection: None,
        primary_connection_type: (!primary_connection_type.is_empty())
            .then_some(primary_connection_type),
        networking_enabled,
        wireless_enabled,
    })
}

async fn read_device(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    options: NetworkObserverOptions,
    primary: Option<&NetworkObjectRef>,
) -> Result<Option<NetworkDeviceSnapshot>, NetworkObserverError> {
    // Absent DeviceType means the object vanished between enumeration and read:
    // an appearance/disappearance event, not an error, and never a zero. A
    // device whose type is unreadable is dropped rather than invented.
    let Some(raw_type) = read_property(connection, owner, path, DEVICE_IFACE, "DeviceType")
        .await?
        .and_then(|value| <u32 as TryFrom<_>>::try_from(value).ok())
    else {
        return Ok(None);
    };
    let kind = NetworkDeviceKind::from_raw(raw_type);
    let interface: String = optional_property(
        connection,
        owner,
        path,
        DEVICE_IFACE,
        "Interface",
        String::new(),
    )
    .await?;
    let state: u32 = optional_property(connection, owner, path, DEVICE_IFACE, "State", 0).await?;
    let managed: bool =
        optional_property(connection, owner, path, DEVICE_IFACE, "Managed", false).await?;
    let active_path =
        read_object_path(connection, owner, path, DEVICE_IFACE, "ActiveConnection").await?;
    let active_connection = active_path.map(|path| NetworkObjectRef::new(owner.to_owned(), path));

    let wifi = if options.read_wireless && kind == NetworkDeviceKind::Wifi {
        read_wifi_facts(connection, owner, path).await?
    } else {
        None
    };

    Ok(Some(NetworkDeviceSnapshot {
        primary: primary.is_some() && primary == active_connection.as_ref(),
        relevance: NetworkRelevance::Unsupported,
        id: NetworkObjectRef::new(owner.to_owned(), path.to_owned()),
        interface,
        kind,
        state: NetworkDeviceState::from_raw(state),
        managed,
        active_connection,
        settings_connection: None,
        wifi,
    }))
}

async fn read_wifi_facts(
    connection: &zbus::Connection,
    owner: &str,
    device_path: &str,
) -> Result<Option<NetworkWifiFacts>, NetworkObserverError> {
    // Read as an object path for the same reason as every other path property.
    let ap_object: zbus::zvariant::OwnedObjectPath = optional_property(
        connection,
        owner,
        device_path,
        DEVICE_WIRELESS_IFACE,
        "ActiveAccessPoint",
        zbus::zvariant::OwnedObjectPath::try_from("/").expect("the root object path is valid"),
    )
    .await?;
    let ap_path = ap_object.to_string();
    if ap_path.is_empty() || ap_path == "/" {
        // Associated with no access point. Explicitly not "signal zero".
        return Ok(None);
    }

    // One GetAll observes a single consistent version of the access point. An AP
    // object is replaced on roam or reassociation, and a burst of individual
    // reads can straddle a replacement and fail.
    let proxy = zbus::Proxy::new(connection, owner, ap_path.as_str(), PROPERTIES_IFACE)
        .await
        .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
    let all: std::collections::BTreeMap<String, zbus::zvariant::OwnedValue> = proxy
        .call("GetAll", &(ACCESS_POINT_IFACE,))
        .await
        .map_err(|error| NetworkObserverError::Call(error.to_string()))?;

    let ssid_bytes = all
        .get("Ssid")
        .and_then(|value| <Vec<u8> as TryFrom<_>>::try_from(value.clone()).ok())
        .unwrap_or_default();
    // Numeric access points are typed per property by NetworkManager: `Strength`
    // is a `y` (byte), while `Frequency` and `MaxBitrate` are `u`. Reading every
    // one as `u32` fails the conversion and yields 0, which would have reported
    // a permanent zero signal.
    let u32_at = |key: &str| -> u32 {
        let Some(value) = all.get(key) else {
            return 0;
        };
        <u32 as TryFrom<_>>::try_from(value.clone())
            .ok()
            .or_else(|| {
                <u8 as TryFrom<_>>::try_from(value.clone())
                    .ok()
                    .map(u32::from)
            })
            .or_else(|| {
                <u16 as TryFrom<_>>::try_from(value.clone())
                    .ok()
                    .map(u32::from)
            })
            .or_else(|| {
                <i32 as TryFrom<_>>::try_from(value.clone())
                    .ok()
                    .map(|v| v.max(0) as u32)
            })
            .unwrap_or(0)
    };
    let string_at = |key: &str| -> Option<String> {
        all.get(key)
            .and_then(|value| <String as TryFrom<_>>::try_from(value.clone()).ok())
    };

    Ok(Some(NetworkWifiFacts {
        access_point: Some(NetworkObjectRef::new(owner.to_owned(), ap_path)),
        bssid: string_at("HwAddress"),
        ssid: decode_ssid(&ssid_bytes),
        strength: u8::try_from(u32_at("Strength")).unwrap_or(u8::MAX),
        frequency_mhz: u32_at("Frequency"),
        security: decode_rsn_security(u32_at("Flags"), u32_at("WpaFlags"), u32_at("RsnFlags")),
    }))
}

async fn read_active_connection(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
) -> Result<Option<NetworkActiveConnectionSnapshot>, NetworkObserverError> {
    // A vanished activation is an appearance/disappearance event, not a zero
    // and not an empty connection type.
    let Some(connection_type) = read_property(connection, owner, path, ACTIVE_IFACE, "Type")
        .await?
        .and_then(|value| <String as TryFrom<_>>::try_from(value).ok())
    else {
        return Ok(None);
    };
    if connection_type.is_empty() {
        return Ok(None);
    }
    let state: u32 = optional_property(connection, owner, path, ACTIVE_IFACE, "State", 0).await?;
    let default_ipv4: bool =
        optional_property(connection, owner, path, ACTIVE_IFACE, "Default", false).await?;
    // The durable profile identity, read as a path reference only. The settings
    // object itself is never opened, because it holds secrets.
    let settings = read_object_path(connection, owner, path, ACTIVE_IFACE, "Connection")
        .await?
        .map(|value| NetworkObjectRef::new(owner.to_owned(), value));

    Ok(Some(NetworkActiveConnectionSnapshot {
        id: NetworkObjectRef::new(owner.to_owned(), path.to_owned()),
        connection_type,
        devices: Vec::new(),
        default_ipv4,
        state: NetworkActiveConnectionState::from_raw(state),
        settings_connection: settings,
    }))
}

/// Every object path the provider currently publishes.
///
/// Used by an observer process to decide what to subscribe to. It re-reads the
/// inventory on each call, so a device that appeared since the last call is
/// picked up without a restart.
pub async fn published_object_paths(
    connection: &zbus::Connection,
    owner: &str,
) -> Result<Vec<String>, NetworkObserverError> {
    let mut paths = vec![ROOT.to_owned()];
    paths.extend(read_object_path_list(connection, owner, ROOT, NM, "Devices").await?);
    paths.extend(read_object_path_list(connection, owner, ROOT, NM, "ActiveConnections").await?);
    Ok(paths)
}
