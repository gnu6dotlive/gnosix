//! BlueZ observation reducer.
//!
//! Provider replacement invalidates every runtime object. Enumeration establishes
//! a baseline; only later semantic differences advance the generation.

use crate::model::{
    BluetoothAdapterSnapshot, BluetoothDeviceSnapshot, BluetoothObjectRef, BluetoothProviderState,
    BluetoothSnapshot,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct BluetoothState {
    owner: Option<String>,
    provider: BluetoothProviderState,
    generation: u64,
    adapters: BTreeMap<BluetoothObjectRef, BluetoothAdapterSnapshot>,
    devices: BTreeMap<BluetoothObjectRef, BluetoothDeviceSnapshot>,
    baselined: bool,
}

impl BluetoothState {
    pub fn new() -> Self {
        Self {
            owner: None,
            provider: BluetoothProviderState::Unavailable,
            generation: 0,
            adapters: BTreeMap::new(),
            devices: BTreeMap::new(),
            baselined: false,
        }
    }
    pub fn provider(&self) -> BluetoothProviderState {
        self.provider
    }

    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn baselined(&self) -> bool {
        self.baselined
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn snapshot(&self) -> BluetoothSnapshot {
        BluetoothSnapshot {
            provider: self.provider,
            generation: self.generation,
            provider_owner: self.owner.clone(),
            adapters: self.adapters.clone(),
            devices: self.devices.clone(),
        }
    }

    pub fn accepts(&self, candidate: &BluetoothObjectRef) -> bool {
        self.baselined
            && self
                .owner
                .as_deref()
                .is_some_and(|owner| candidate.belongs_to(owner))
    }
    pub fn set_provider_owner(&mut self, owner: Option<&str>) -> bool {
        if self.owner.as_deref() == owner {
            return false;
        }
        self.owner = owner.map(str::to_owned);
        self.adapters.clear();
        self.devices.clear();
        self.baselined = false;
        self.provider = if owner.is_some() {
            BluetoothProviderState::Reconnecting
        } else {
            BluetoothProviderState::Unavailable
        };
        self.bump();
        true
    }

    pub fn set_provider_unavailable(&mut self) -> bool {
        if self.provider == BluetoothProviderState::Unavailable && self.owner.is_none() {
            return false;
        }
        self.owner = None;
        self.adapters.clear();
        self.devices.clear();
        self.baselined = false;
        self.provider = BluetoothProviderState::Unavailable;
        self.bump();
        true
    }
    pub fn apply_observation(
        &mut self,
        owner: &str,
        adapters: Vec<BluetoothAdapterSnapshot>,
        devices: Vec<BluetoothDeviceSnapshot>,
    ) -> bool {
        if self.owner.as_deref() != Some(owner) {
            self.owner = Some(owner.to_owned());
            self.adapters.clear();
            self.devices.clear();
            self.baselined = false;
            self.provider = BluetoothProviderState::Reconnecting;
            self.bump();
        }

        let before = (self.adapters.clone(), self.devices.clone());
        let was_baselined = self.baselined;

        let adapters: BTreeMap<_, _> = adapters
            .into_iter()
            .filter(|adapter| adapter.id.belongs_to(owner))
            .map(|adapter| (adapter.id.clone(), adapter))
            .collect();
        let devices = devices
            .into_iter()
            .filter(|device| {
                device.id.belongs_to(owner)
                    && device.adapter.belongs_to(owner)
                    && adapters.contains_key(&device.adapter)
            })
            .map(|device| (device.id.clone(), device))
            .collect();
        self.adapters = adapters;
        self.devices = devices;
        self.provider = BluetoothProviderState::Available;

        if !was_baselined {
            self.baselined = true;
            return false;
        }

        if before == (self.adapters.clone(), self.devices.clone()) {
            return false;
        }

        self.bump();
        true
    }

    fn bump(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }
}
impl Default for BluetoothState {
    fn default() -> Self {
        Self::new()
    }
}
