//! Typed BlueZ read model for Gnosix.
//!
//! External D-Bus vocabulary terminates at the observer boundary. Runtime
//! identity is scoped to the current BlueZ unique owner, and every optional
//! provider fact stays optional rather than being replaced with a fabricated
//! default.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BluetoothObjectRef {
    pub provider_owner: String,
    pub path: String,
}

impl BluetoothObjectRef {
    pub fn new(provider_owner: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            provider_owner: provider_owner.into(),
            path: path.into(),
        }
    }

    pub fn belongs_to(&self, provider_owner: &str) -> bool {
        self.provider_owner == provider_owner
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothProviderState {
    Available,
    Unavailable,
    Reconnecting,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothPowerState {
    Unavailable,
    On,
    Off,
    Enabling,
    Disabling,
    Blocked,
    Unsupported(String),
}

impl BluetoothPowerState {
    pub fn from_raw(raw: &str) -> Self {
        match raw {
            "on" => Self::On,
            "off" => Self::Off,
            "off-enabling" => Self::Enabling,
            "on-disabling" => Self::Disabling,
            "off-blocked" => Self::Blocked,
            other => Self::Unsupported(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Unavailable => "unavailable",
            Self::On => "on",
            Self::Off => "off",
            Self::Enabling => "enabling",
            Self::Disabling => "disabling",
            Self::Blocked => "blocked",
            Self::Unsupported(raw) => raw,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothAdapterSnapshot {
    pub id: BluetoothObjectRef,
    pub powered: bool,
    pub power_state: BluetoothPowerState,
    pub discovering: bool,
    pub pairable: bool,
    pub discoverable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothDeviceSnapshot {
    pub id: BluetoothObjectRef,
    pub adapter: BluetoothObjectRef,
    pub alias: Option<String>,
    pub paired: bool,
    pub bonded: bool,
    pub connected: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub services_resolved: bool,
    pub rssi: Option<i16>,
    /// BlueZ `Icon`: a freedesktop device-class name such as `audio-headset`.
    /// Optional in BlueZ; absent stays absent.
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothSnapshot {
    pub provider: BluetoothProviderState,
    pub generation: u64,
    pub provider_owner: Option<String>,
    pub adapters: BTreeMap<BluetoothObjectRef, BluetoothAdapterSnapshot>,
    pub devices: BTreeMap<BluetoothObjectRef, BluetoothDeviceSnapshot>,
}
