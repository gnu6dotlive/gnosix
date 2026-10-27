//! The network observation reducer.
//!
//! This is where the two hardest NETWORK-01 invariants are enforced:
//!
//! - **Provider replacement invalidates runtime provenance.** Object paths are
//!   only meaningful relative to the service instance that published them, so a
//!   changed provider owner discards every runtime identity and forces a fresh
//!   baseline. There is no code path that carries a `Device`,
//!   `ActiveConnection` or `AccessPoint` path across an owner change.
//! - **Semantic churn is suppressed.** 28 bridges and 26 veth devices on this
//!   host emit property changes continuously while 25 containers run, and an
//!   unchanged network is genuinely quiet. The generation advances only when
//!   product-relevant facts change.

use crate::model::{
    NetworkActiveConnectionSnapshot, NetworkDeviceSnapshot, NetworkGlobals, NetworkObjectRef,
    NetworkProviderState, NetworkSnapshot, NetworkSummary, classify_relevance, summarize,
};

/// Accumulates observations into the current truth.
///
/// Every mutating method reports whether it advanced the product generation, so
/// a caller can drive a notification without a second comparison.
#[derive(Clone, Debug)]
pub struct NetworkState {
    /// The provider owner whose runtime identities we currently hold.
    owner: Option<String>,
    provider: NetworkProviderState,
    generation: u64,
    globals: Option<NetworkGlobals>,
    devices: std::collections::BTreeMap<NetworkObjectRef, NetworkDeviceSnapshot>,
    active_connections:
        std::collections::BTreeMap<NetworkObjectRef, NetworkActiveConnectionSnapshot>,
    settings_connections: std::collections::BTreeMap<NetworkObjectRef, NetworkObjectRef>,
    /// True once a baseline exists for the current owner. Before that, an
    /// observation establishes truth rather than announcing a change.
    baselined: bool,
}

impl NetworkState {
    pub fn new() -> Self {
        Self {
            owner: None,
            provider: NetworkProviderState::Unavailable,
            generation: 0,
            globals: None,
            devices: Default::default(),
            active_connections: Default::default(),
            settings_connections: Default::default(),
            baselined: false,
        }
    }

    pub fn provider(&self) -> NetworkProviderState {
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

    pub fn snapshot(&self) -> NetworkSnapshot {
        NetworkSnapshot {
            provider: self.provider,
            generation: self.generation,
            provider_owner: self.owner.clone(),
            globals: self.globals.clone(),
            devices: self.devices.clone(),
            active_connections: self.active_connections.clone(),
            settings_connections: self.settings_connections.clone(),
        }
    }

    pub fn summary(&self) -> NetworkSummary {
        summarize(&self.snapshot())
    }

    /// Whether a runtime identity from `candidate` may still be trusted.
    ///
    /// Used by the observer to reject a stale object outright instead of
    /// merging it into current truth.
    pub fn accepts(&self, candidate: &NetworkObjectRef) -> bool {
        self.baselined
            && self
                .owner
                .as_deref()
                .is_some_and(|owner| candidate.belongs_to(owner))
    }

    /// Records a provider owner change.
    ///
    /// A changed owner invalidates every runtime identity: the same object path
    /// under a new owner is a different object with a different lifetime. The
    /// next observation therefore establishes a baseline rather than announcing a
    /// change, which is what prevents a restart from being reported as a
    /// disconnect.
    pub fn set_provider_owner(&mut self, owner: Option<&str>) -> bool {
        if self.owner.as_deref() == owner {
            return false;
        }
        self.owner = owner.map(str::to_owned);
        self.globals = None;
        self.devices.clear();
        self.active_connections.clear();
        self.settings_connections.clear();
        self.baselined = false;
        self.provider = if owner.is_some() {
            NetworkProviderState::Reconnecting
        } else {
            NetworkProviderState::Unavailable
        };
        self.bump();
        true
    }

    /// Records that the provider stopped answering.
    ///
    /// Provider loss is not a network fact. It becomes
    /// [`NetworkProviderState::Unavailable`] and never `Disconnected`.
    pub fn set_provider_unavailable(&mut self) -> bool {
        if self.provider == NetworkProviderState::Unavailable && self.owner.is_none() {
            return false;
        }
        self.owner = None;
        self.globals = None;
        self.devices.clear();
        self.active_connections.clear();
        self.settings_connections.clear();
        self.baselined = false;
        self.provider = NetworkProviderState::Unavailable;
        self.bump();
        true
    }

    /// Replaces the observed facts in one atomic step.
    ///
    /// Applies a whole enumeration and reports whether product-relevant truth
    /// changed. This is the only way facts enter the state, which keeps
    /// classification and churn suppression in one place.
    pub fn apply_observation(
        &mut self,
        owner: &str,
        globals: Option<NetworkGlobals>,
        devices: Vec<NetworkDeviceSnapshot>,
        active_connections: Vec<NetworkActiveConnectionSnapshot>,
    ) -> bool {
        if self.owner.as_deref() != Some(owner) {
            self.owner = Some(owner.to_owned());
            self.globals = None;
            self.devices.clear();
            self.active_connections.clear();
            self.settings_connections.clear();
            self.baselined = false;
            self.provider = NetworkProviderState::Reconnecting;
            self.bump();
        }

        let previous_summary = self.summary();
        let was_baselined = self.baselined;

        self.globals = globals;
        self.devices = devices
            .into_iter()
            .map(|device| {
                // Classification is derived here, once, from typed facts.
                let relevance =
                    classify_relevance(device.kind, device.state, device.managed, device.primary);
                (
                    device.id.clone(),
                    NetworkDeviceSnapshot {
                        relevance,
                        ..without_stale_wireless(device)
                    },
                )
            })
            .collect();
        self.settings_connections = active_connections
            .iter()
            .filter_map(|entry| {
                entry
                    .settings_connection
                    .clone()
                    .map(|settings| (entry.id.clone(), settings))
            })
            .collect();
        self.active_connections = active_connections
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect();
        self.provider = NetworkProviderState::Available;

        if !was_baselined {
            // Enumeration establishes truth. It is not a network change, so it
            // must not flash product state.
            self.baselined = true;
            return false;
        }

        if self.summary() == previous_summary {
            // Protocol churn with identical product facts. Docker and libvirt
            // traffic lands here and must not advance the generation.
            return false;
        }
        self.bump();
        true
    }

    /// Applies a single device's re-read facts, for a targeted signal.
    ///
    /// Returns whether product truth changed. A change to a device the product
    /// does not present is a no-op by construction, because the comparison is on
    /// the derived summary rather than on the inventory.
    pub fn apply_device(&mut self, device: NetworkDeviceSnapshot) -> bool {
        if !self.baselined || !self.accepts(&device.id) {
            return false;
        }
        let Some(previous) = self.devices.get(&device.id) else {
            return false;
        };
        if *previous == device {
            return false;
        }
        // The comparison is against the state before this update, so a change
        // that does not alter the derived summary is still suppressed.
        let before = self.summary();
        // An unchanged device object is dropped rather than inserted, so a
        // vanished object cannot linger as stale truth.
        if !device.state.is_connected() && device.active_connection.is_none() {
            self.devices.remove(&device.id);
        } else {
            let relevance =
                classify_relevance(device.kind, device.state, device.managed, device.primary);
            self.devices.insert(
                device.id.clone(),
                NetworkDeviceSnapshot {
                    relevance,
                    ..device
                },
            );
        }
        if self.summary() == before {
            return false;
        }
        self.bump();
        true
    }

    /// Applies re-read globals, for a root-object property change.
    pub fn apply_globals(&mut self, globals: NetworkGlobals) -> bool {
        if !self.baselined {
            return false;
        }
        if self.globals.as_ref() == Some(&globals) {
            return false;
        }
        self.globals = Some(globals);
        self.bump();
        true
    }

    /// Re-resolves which devices the provider marks primary.
    ///
    /// The provider can change `PrimaryConnection` without any device object
    /// changing, so primary-ness is recomputed from the current global fact
    /// rather than being assumed to follow a device edit.
    pub fn reclassify_primary(&mut self, primary: Option<&NetworkObjectRef>) -> bool {
        if !self.baselined {
            return false;
        }
        let before = self.summary();
        let mut changed = false;
        let owners: Vec<NetworkObjectRef> = self.devices.keys().cloned().collect();
        for id in owners {
            let Some(device) = self.devices.get(&id) else {
                continue;
            };
            let is_primary = primary == Some(&id)
                || device
                    .active_connection
                    .as_ref()
                    .is_some_and(|ac| Some(ac) == primary);
            if device.primary != is_primary {
                let mut updated = device.clone();
                updated.primary = is_primary;
                let relevance =
                    classify_relevance(updated.kind, updated.state, updated.managed, is_primary);
                updated.relevance = relevance;
                self.devices.insert(id, updated);
                changed = true;
            }
        }
        if changed && self.summary() != before {
            self.bump();
            return true;
        }
        false
    }

    fn bump(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }
}

/// Drops wireless facts from a device that is no longer associated.
///
/// NetworkManager keeps publishing `ActiveConnection` and `ActiveAccessPoint`
/// for a moment while a wireless link goes down. Retaining those facts would
/// leave a stale SSID and a stale signal strength on a device whose link is
/// already gone -- the live gate caught a frame reporting
/// `primary_state=disconnected` next to a real-looking SSID. A disconnected
/// device has no access point, so the facts are retired at the boundary rather
/// than filtered out later.
fn without_stale_wireless(mut device: NetworkDeviceSnapshot) -> NetworkDeviceSnapshot {
    if !device.state.is_connected() {
        device.wifi = None;
    }
    device
}

impl Default for NetworkState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
