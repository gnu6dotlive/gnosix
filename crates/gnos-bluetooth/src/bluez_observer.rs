//! Native, read-only BlueZ observer.
//!
//! This is the protocol boundary for BLUETOOTH-01. D-Bus object paths and
//! OwnedValue values never leave this module; consumers receive provider-scoped
//! typed facts only.

use crate::model::{
    BluetoothAdapterSnapshot, BluetoothDeviceSnapshot, BluetoothObjectRef, BluetoothPowerState,
    BluetoothSnapshot,
};
use crate::reducer::BluetoothState;
use std::collections::HashMap;
use zbus::fdo::ManagedObjects;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub const BLUEZ_BUS_NAME: &str = "org.bluez";
const BLUEZ_ROOT_PATH: &str = "/";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";

#[derive(Debug, thiserror::Error)]
pub enum BluetoothObserverError {
    #[error("cannot reach BlueZ on the system bus: {0}")]
    Connect(String),
    #[error("BlueZ call failed: {0}")]
    Call(String),
    #[error("required BlueZ fact is absent or malformed: {0}")]
    Fact(String),
}
#[derive(Clone, Copy, Debug)]
pub struct BluetoothObserverOptions {
    pub read_alias: bool,
}

impl Default for BluetoothObserverOptions {
    fn default() -> Self {
        Self { read_alias: true }
    }
}

pub struct BluetoothObserver {
    state: BluetoothState,
    options: BluetoothObserverOptions,
}

impl BluetoothObserver {
    pub fn new(options: BluetoothObserverOptions) -> Self {
        Self {
            state: BluetoothState::new(),
            options,
        }
    }

    pub fn state(&self) -> &BluetoothState {
        &self.state
    }

    pub fn snapshot(&self) -> BluetoothSnapshot {
        self.state.snapshot()
    }
    pub fn observe_provider_gone(&mut self) -> bool {
        self.state.set_provider_unavailable()
    }

    pub fn observe_provider_owner(&mut self, owner: &str) -> bool {
        self.state.set_provider_owner(Some(owner))
    }

    pub async fn refresh(&mut self) -> Result<bool, BluetoothObserverError> {
        let connection = system_connection().await?;
        self.refresh_on(&connection).await
    }

    /// Re-read BlueZ over an existing connection.
    pub async fn refresh_on(
        &mut self,
        connection: &zbus::Connection,
    ) -> Result<bool, BluetoothObserverError> {
        let Some(owner) = resolve_owner(connection).await? else {
            return Ok(self.state.set_provider_unavailable());
        };
        let managed = read_managed_objects(connection, &owner).await?;
        let (adapters, devices) = decode_managed_objects(&owner, managed, self.options)?;
        Ok(self.state.apply_observation(&owner, adapters, devices))
    }
}

async fn system_connection() -> Result<zbus::Connection, BluetoothObserverError> {
    zbus::Connection::system()
        .await
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))
}

pub async fn resolve_owner(
    connection: &zbus::Connection,
) -> Result<Option<String>, BluetoothObserverError> {
    let name = zbus::names::BusName::try_from(BLUEZ_BUS_NAME.to_owned())
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;
    let proxy = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;
    if !proxy
        .name_has_owner(name.clone())
        .await
        .map_err(|error| BluetoothObserverError::Call(error.to_string()))?
    {
        return Ok(None);
    }
    proxy
        .get_name_owner(name)
        .await
        .map(|owner| Some(owner.to_string()))
        .map_err(|error| BluetoothObserverError::Call(error.to_string()))
}

async fn read_managed_objects(
    connection: &zbus::Connection,
    owner: &str,
) -> Result<ManagedObjects, BluetoothObserverError> {
    let proxy = zbus::fdo::ObjectManagerProxy::builder(connection)
        .destination(owner.to_owned())
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?
        .path(BLUEZ_ROOT_PATH)
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?
        .build()
        .await
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;
    proxy
        .get_managed_objects()
        .await
        .map_err(|error| BluetoothObserverError::Call(error.to_string()))
}
fn decode_managed_objects(
    owner: &str,
    managed: ManagedObjects,
    options: BluetoothObserverOptions,
) -> Result<(Vec<BluetoothAdapterSnapshot>, Vec<BluetoothDeviceSnapshot>), BluetoothObserverError> {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();

    for (path, interfaces) in managed {
        let path = path.to_string();
        for (interface, properties) in interfaces {
            match interface.as_str() {
                ADAPTER_IFACE => adapters.push(decode_adapter(owner, &path, &properties)?),
                DEVICE_IFACE => devices.push(decode_device(owner, &path, &properties, options)?),
                _ => {}
            }
        }
    }

    Ok((adapters, devices))
}

fn decode_adapter(
    owner: &str,
    path: &str,
    properties: &HashMap<String, OwnedValue>,
) -> Result<BluetoothAdapterSnapshot, BluetoothObserverError> {
    let powered = required::<bool>(properties, "Powered")?;
    let power_state = optional::<String>(properties, "PowerState")?
        .map(|raw| BluetoothPowerState::from_raw(&raw))
        .unwrap_or(BluetoothPowerState::Unavailable);
    Ok(BluetoothAdapterSnapshot {
        id: BluetoothObjectRef::new(owner, path),
        powered,
        power_state,
        discovering: required(properties, "Discovering")?,
        pairable: required(properties, "Pairable")?,
        discoverable: required(properties, "Discoverable")?,
    })
}

fn decode_device(
    owner: &str,
    path: &str,
    properties: &HashMap<String, OwnedValue>,
    options: BluetoothObserverOptions,
) -> Result<BluetoothDeviceSnapshot, BluetoothObserverError> {
    let adapter_path = required::<OwnedObjectPath>(properties, "Adapter")?;
    Ok(BluetoothDeviceSnapshot {
        id: BluetoothObjectRef::new(owner, path),
        adapter: BluetoothObjectRef::new(owner, adapter_path.to_string()),
        alias: if options.read_alias {
            optional::<String>(properties, "Alias")?
        } else {
            None
        },
        paired: required(properties, "Paired")?,
        bonded: required(properties, "Bonded")?,
        connected: required(properties, "Connected")?,
        trusted: required(properties, "Trusted")?,
        blocked: required(properties, "Blocked")?,
        services_resolved: required(properties, "ServicesResolved")?,
        rssi: optional::<i16>(properties, "RSSI")?,
        icon: optional::<String>(properties, "Icon")?,
    })
}
fn optional<T>(
    properties: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<Option<T>, BluetoothObserverError>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::fmt::Debug,
{
    let Some(value) = properties.get(name) else {
        return Ok(None);
    };
    T::try_from(value.clone())
        .map(Some)
        .map_err(|_| BluetoothObserverError::Fact(name.to_owned()))
}

fn required<T>(
    properties: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<T, BluetoothObserverError>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::fmt::Debug,
{
    optional(properties, name)?.ok_or_else(|| BluetoothObserverError::Fact(name.to_owned()))
}

pub async fn published_object_paths(
    connection: &zbus::Connection,
    owner: &str,
) -> Result<Vec<String>, BluetoothObserverError> {
    let managed = read_managed_objects(connection, owner).await?;
    let mut paths = Vec::new();
    for (path, interfaces) in managed {
        if interfaces
            .keys()
            .any(|name| matches!(name.as_str(), ADAPTER_IFACE | DEVICE_IFACE))
        {
            paths.push(path.to_string());
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_optional_fact_stays_absent() {
        let properties = HashMap::new();
        let rssi = optional::<i16>(&properties, "RSSI").expect("optional read");
        assert_eq!(rssi, None);
    }

    #[test]
    fn unknown_power_state_stays_explicit() {
        assert_eq!(
            BluetoothPowerState::from_raw("future-state"),
            BluetoothPowerState::Unsupported("future-state".into())
        );
    }
}
