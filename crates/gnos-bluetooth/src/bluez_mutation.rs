//! SURF-01A (U01): Bluetooth power, one typed power.
//!
//! A request names an observed adapter and the state it wants. It is
//! revalidated against the observed snapshot first: the adapter must still
//! be observed, under the same provider owner; a radio blocked by rfkill is
//! refused (BlueZ cannot power it); a radio already in that state is
//! refused. Otherwise it writes the adapter's `Powered`, addressed to the
//! observed owner. The product shows the new state when the observer
//! reports it, never because the request returned.

use crate::{BluetoothObjectRef, BluetoothPowerState, BluetoothSnapshot};

const ADAPTER_IFACE: &str = "org.bluez.Adapter1";

/// Why a Bluetooth power request was not sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothPowerRefusal {
    /// The adapter is no longer observed, or under another owner.
    AdapterGone,
    /// rfkill blocks the radio.
    Blocked,
    AlreadySatisfied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothPowerOutcome {
    Accepted,
    Refused(BluetoothPowerRefusal),
    Denied(String),
    TransportFailure(String),
}

pub fn revalidate_power(
    snapshot: &BluetoothSnapshot,
    adapter: &BluetoothObjectRef,
    enable: bool,
) -> Result<(), BluetoothPowerRefusal> {
    if snapshot.provider_owner.as_deref() != Some(adapter.provider_owner.as_str()) {
        return Err(BluetoothPowerRefusal::AdapterGone);
    }
    let Some(observed) = snapshot.adapters.get(adapter) else {
        return Err(BluetoothPowerRefusal::AdapterGone);
    };
    if enable && observed.power_state == BluetoothPowerState::Blocked {
        return Err(BluetoothPowerRefusal::Blocked);
    }
    if observed.powered == enable {
        return Err(BluetoothPowerRefusal::AlreadySatisfied);
    }
    Ok(())
}

pub fn classify_power_error(name: &str, message: &str) -> BluetoothPowerOutcome {
    let denied = name.ends_with("AccessDenied")
        || name.ends_with("NotAuthorized")
        || name.ends_with("NotPermitted");
    if denied {
        BluetoothPowerOutcome::Denied(format!("{name}: {message}"))
    } else {
        BluetoothPowerOutcome::TransportFailure(format!("{name}: {message}"))
    }
}

/// Revalidate, then ask BlueZ (on `connection`, the system bus) to power
/// `adapter` on or off.
pub async fn request_power(
    connection: &zbus::Connection,
    snapshot: &BluetoothSnapshot,
    adapter: &BluetoothObjectRef,
    enable: bool,
) -> BluetoothPowerOutcome {
    if let Err(refusal) = revalidate_power(snapshot, adapter, enable) {
        return BluetoothPowerOutcome::Refused(refusal);
    }
    let reply = connection
        .call_method(
            Some(adapter.provider_owner.as_str()),
            adapter.path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(
                ADAPTER_IFACE,
                "Powered",
                zbus::zvariant::Value::from(enable),
            ),
        )
        .await;
    match reply {
        Ok(_) => BluetoothPowerOutcome::Accepted,
        Err(zbus::Error::MethodError(name, message, _)) => {
            classify_power_error(name.as_str(), message.as_deref().unwrap_or(""))
        }
        Err(error) => BluetoothPowerOutcome::TransportFailure(error.to_string()),
    }
}

/// SETTINGS-02C: what asking a paired device to connect or disconnect
/// came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothConnectOutcome {
    Requested,
    /// The device is gone, not paired, blocked or already in that state, or
    /// its owner changed since the request was made.
    Refused(&'static str),
    Failed(String),
}

/// Revalidate against the snapshot, then ask BlueZ to connect (or
/// disconnect) a paired device. The result is seen by observation.
pub async fn request_device_connection(
    connection: &zbus::Connection,
    snapshot: &BluetoothSnapshot,
    device: &BluetoothObjectRef,
    connect: bool,
) -> BluetoothConnectOutcome {
    match revalidate_connection(snapshot, device, connect) {
        Ok(()) => {}
        Err(reason) => return BluetoothConnectOutcome::Refused(reason),
    }
    match connection
        .call_method(
            Some(device.provider_owner.as_str()),
            device.path.as_str(),
            Some("org.bluez.Device1"),
            if connect { "Connect" } else { "Disconnect" },
            &(),
        )
        .await
    {
        Ok(_) => BluetoothConnectOutcome::Requested,
        Err(error) => BluetoothConnectOutcome::Failed(error.to_string()),
    }
}

fn revalidate_connection(
    snapshot: &BluetoothSnapshot,
    device: &BluetoothObjectRef,
    connect: bool,
) -> Result<(), &'static str> {
    if snapshot.provider_owner.as_deref() != Some(device.provider_owner.as_str()) {
        return Err("provider changed");
    }
    let Some(observed) = snapshot.devices.get(device) else {
        return Err("device gone");
    };
    if !observed.paired {
        return Err("not paired");
    }
    if observed.blocked {
        return Err("blocked");
    }
    if observed.connected == connect {
        return Err("already in that state");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BluetoothAdapterSnapshot, BluetoothState};

    #[test]
    fn a_device_connects_only_when_present_paired_unblocked_and_not_already_so() {
        let adapter = BluetoothObjectRef::new(":1.4", "/org/bluez/hci0");
        let id = BluetoothObjectRef::new(":1.4", "/org/bluez/hci0/dev_AA");
        let mut s = BluetoothState::new().snapshot();
        s.provider_owner = Some(":1.4".to_owned());
        assert_eq!(revalidate_connection(&s, &id, true), Err("device gone"));
        let device = crate::BluetoothDeviceSnapshot {
            id: id.clone(),
            adapter,
            alias: Some("Buds".into()),
            paired: true,
            bonded: true,
            connected: false,
            trusted: false,
            blocked: false,
            services_resolved: false,
            rssi: None,
            icon: None,
        };
        s.devices.insert(id.clone(), device.clone());
        assert_eq!(revalidate_connection(&s, &id, true), Ok(()));
        assert_eq!(
            revalidate_connection(&s, &id, false),
            Err("already in that state")
        );
        s.devices.get_mut(&id).unwrap().paired = false;
        assert_eq!(revalidate_connection(&s, &id, true), Err("not paired"));
        s.devices.insert(id.clone(), device);
        s.devices.get_mut(&id).unwrap().blocked = true;
        assert_eq!(revalidate_connection(&s, &id, true), Err("blocked"));
        s.provider_owner = Some(":1.9".to_owned());
        assert_eq!(
            revalidate_connection(&s, &id, true),
            Err("provider changed")
        );
    }

    fn snapshot(
        powered: bool,
        state: BluetoothPowerState,
    ) -> (BluetoothSnapshot, BluetoothObjectRef) {
        let adapter = BluetoothObjectRef::new(":1.4", "/org/bluez/hci0");
        let mut s = BluetoothState::new().snapshot();
        s.provider_owner = Some(":1.4".to_owned());
        s.adapters.insert(
            adapter.clone(),
            BluetoothAdapterSnapshot {
                id: adapter.clone(),
                powered,
                power_state: state,
                discovering: false,
                pairable: false,
                discoverable: false,
            },
        );
        (s, adapter)
    }

    #[test]
    fn power_requests_are_revalidated_against_the_observed_adapter() {
        let (s, adapter) = snapshot(false, BluetoothPowerState::Off);
        assert_eq!(revalidate_power(&s, &adapter, true), Ok(()));
        assert_eq!(
            revalidate_power(&s, &adapter, false),
            Err(BluetoothPowerRefusal::AlreadySatisfied)
        );
        let other = BluetoothObjectRef::new(":1.9", "/org/bluez/hci0");
        assert_eq!(
            revalidate_power(&s, &other, true),
            Err(BluetoothPowerRefusal::AdapterGone)
        );
        let (blocked, adapter) = snapshot(false, BluetoothPowerState::Blocked);
        assert_eq!(
            revalidate_power(&blocked, &adapter, true),
            Err(BluetoothPowerRefusal::Blocked)
        );
        // Powering off a blocked radio is still a plain request.
        let (on_blocked, adapter) = snapshot(true, BluetoothPowerState::Blocked);
        assert_eq!(revalidate_power(&on_blocked, &adapter, false), Ok(()));
    }

    #[test]
    fn authorization_errors_are_denials() {
        assert!(matches!(
            classify_power_error("org.bluez.Error.NotAuthorized", "no"),
            BluetoothPowerOutcome::Denied(_)
        ));
        assert!(matches!(
            classify_power_error("org.bluez.Error.Failed", "rfkill"),
            BluetoothPowerOutcome::TransportFailure(_)
        ));
    }
}
