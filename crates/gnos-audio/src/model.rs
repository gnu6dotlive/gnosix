use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AudioEndpointId {
    pub server_cookie: u32,
    pub object_serial: u64,
}

impl AudioEndpointId {
    pub const fn new(server_cookie: u32, object_serial: u64) -> Self {
        Self {
            server_cookie,
            object_serial,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioEndpointKind {
    Output,
    Input,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NativeVolume(f32);

impl NativeVolume {
    pub fn new(value: f32) -> Result<Self, AudioModelError> {
        if !value.is_finite() || value < 0.0 {
            return Err(AudioModelError::InvalidVolume(value));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    pub fn percent_unclamped(self) -> f32 {
        self.0 * 100.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioVolume {
    scalar: Option<NativeVolume>,
    channels: Vec<NativeVolume>,
    soft_channels: Vec<NativeVolume>,
}

impl AudioVolume {
    pub fn scalar(value: f32) -> Result<Self, AudioModelError> {
        Ok(Self {
            scalar: Some(NativeVolume::new(value)?),
            channels: Vec::new(),
            soft_channels: Vec::new(),
        })
    }

    pub fn from_parts(
        scalar: Option<NativeVolume>,
        channels: Vec<NativeVolume>,
        soft_channels: Vec<NativeVolume>,
    ) -> Self {
        Self {
            scalar,
            channels,
            soft_channels,
        }
    }

    pub fn scalar_value(&self) -> Option<NativeVolume> {
        self.scalar
    }

    pub fn channels(&self) -> &[NativeVolume] {
        &self.channels
    }

    pub fn soft_channels(&self) -> &[NativeVolume] {
        &self.soft_channels
    }

    /// User-facing level using WirePlumber's cubic mixer scale.
    ///
    /// `SPA_PROP_channelVolumes` is the effective linear per-channel volume.
    /// WirePlumber's cubic mixer exposes the first effective channel as
    /// `cbrt(linear)`; when no per-channel value exists, it falls back to the
    /// scalar volume. Values above 1.0 are deliberately preserved.
    pub fn cubic_user_level(&self) -> Option<f32> {
        let linear = self.channels.first().copied().or(self.scalar)?;
        Some(linear.get().cbrt())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioEndpointSnapshot {
    pub id: AudioEndpointId,
    pub kind: AudioEndpointKind,
    pub node_name: String,
    pub display_name: String,
    pub volume: AudioVolume,
    pub muted: bool,
}

impl AudioEndpointSnapshot {
    pub fn new(
        id: AudioEndpointId,
        kind: AudioEndpointKind,
        node_name: String,
        display_name: String,
        volume: AudioVolume,
        muted: bool,
    ) -> Self {
        Self {
            id,
            kind,
            node_name,
            display_name,
            volume,
            muted,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSelection {
    pub name: Option<String>,
    pub resolved: Option<AudioEndpointId>,
    pub metadata_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDefaultRole {
    pub effective: RoleSelection,
    pub configured: RoleSelection,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDefaultRoles {
    pub output: AudioDefaultRole,
    pub input: AudioDefaultRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultRoleKind {
    Output,
    Input,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioProviderState {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioSnapshot {
    pub provider: AudioProviderState,
    pub generation: u64,
    pub endpoints: BTreeMap<AudioEndpointId, AudioEndpointSnapshot>,
    pub defaults: AudioDefaultRoles,
}

#[derive(Clone, Debug)]
pub struct AudioState {
    server_cookie: u32,
    snapshot: AudioSnapshot,
}

impl AudioState {
    pub fn new(server_cookie: u32) -> Self {
        Self {
            server_cookie,
            snapshot: AudioSnapshot {
                provider: AudioProviderState::Available,
                generation: 0,
                endpoints: BTreeMap::new(),
                defaults: AudioDefaultRoles::default(),
            },
        }
    }

    pub const fn server_cookie(&self) -> u32 {
        self.server_cookie
    }

    pub fn snapshot(&self) -> AudioSnapshot {
        self.snapshot.clone()
    }

    pub fn snapshot_ref(&self) -> &AudioSnapshot {
        &self.snapshot
    }

    pub fn set_provider_state(&mut self, provider: AudioProviderState) -> bool {
        if self.snapshot.provider == provider {
            return false;
        }
        self.snapshot.provider = provider;
        self.bump_generation();
        true
    }

    pub fn upsert_endpoint(&mut self, endpoint: AudioEndpointSnapshot) -> bool {
        let changed = self.snapshot.endpoints.get(&endpoint.id) != Some(&endpoint);
        if !changed {
            return false;
        }
        self.snapshot.endpoints.insert(endpoint.id, endpoint);
        self.recompute_roles();
        self.bump_generation();
        true
    }

    pub fn remove_endpoint(&mut self, id: &AudioEndpointId) -> bool {
        if self.snapshot.endpoints.remove(id).is_none() {
            return false;
        }
        self.recompute_roles();
        self.bump_generation();
        true
    }

    pub fn update_endpoint_state(
        &mut self,
        id: &AudioEndpointId,
        volume: AudioVolume,
        muted: bool,
    ) -> bool {
        let Some(endpoint) = self.snapshot.endpoints.get_mut(id) else {
            return false;
        };
        if endpoint.volume == volume && endpoint.muted == muted {
            return false;
        }
        endpoint.volume = volume;
        endpoint.muted = muted;
        self.bump_generation();
        true
    }

    pub fn set_effective_default_name(
        &mut self,
        kind: DefaultRoleKind,
        name: Option<String>,
    ) -> bool {
        let role = self.role_mut(kind);
        if role.effective.name == name && role.effective.metadata_error.is_none() {
            return false;
        }
        role.effective.name = name;
        role.effective.metadata_error = None;
        self.recompute_roles();
        self.bump_generation();
        true
    }

    pub fn set_configured_default_name(
        &mut self,
        kind: DefaultRoleKind,
        name: Option<String>,
    ) -> bool {
        let role = self.role_mut(kind);
        if role.configured.name == name && role.configured.metadata_error.is_none() {
            return false;
        }
        role.configured.name = name;
        role.configured.metadata_error = None;
        self.recompute_roles();
        self.bump_generation();
        true
    }

    pub fn set_default_metadata_error(
        &mut self,
        kind: DefaultRoleKind,
        configured: bool,
        error: String,
    ) -> bool {
        let selection = if configured {
            &mut self.role_mut(kind).configured
        } else {
            &mut self.role_mut(kind).effective
        };
        if selection.name.is_none()
            && selection.resolved.is_none()
            && selection.metadata_error.as_deref() == Some(error.as_str())
        {
            return false;
        }
        selection.name = None;
        selection.resolved = None;
        selection.metadata_error = Some(error);
        self.bump_generation();
        true
    }

    fn role_mut(&mut self, kind: DefaultRoleKind) -> &mut AudioDefaultRole {
        match kind {
            DefaultRoleKind::Output => &mut self.snapshot.defaults.output,
            DefaultRoleKind::Input => &mut self.snapshot.defaults.input,
        }
    }

    fn recompute_roles(&mut self) {
        let output_effective = self.snapshot.defaults.output.effective.name.clone();
        let output_configured = self.snapshot.defaults.output.configured.name.clone();
        let input_effective = self.snapshot.defaults.input.effective.name.clone();
        let input_configured = self.snapshot.defaults.input.configured.name.clone();

        self.snapshot.defaults.output.effective.resolved =
            self.resolve_named(AudioEndpointKind::Output, output_effective.as_deref());
        self.snapshot.defaults.output.configured.resolved =
            self.resolve_named(AudioEndpointKind::Output, output_configured.as_deref());
        self.snapshot.defaults.input.effective.resolved =
            self.resolve_named(AudioEndpointKind::Input, input_effective.as_deref());
        self.snapshot.defaults.input.configured.resolved =
            self.resolve_named(AudioEndpointKind::Input, input_configured.as_deref());
    }

    fn resolve_named(
        &self,
        kind: AudioEndpointKind,
        name: Option<&str>,
    ) -> Option<AudioEndpointId> {
        let name = name?;
        self.snapshot
            .endpoints
            .values()
            .find(|endpoint| endpoint.kind == kind && endpoint.node_name == name)
            .map(|endpoint| endpoint.id)
    }

    fn bump_generation(&mut self) {
        self.snapshot.generation = self.snapshot.generation.saturating_add(1);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioModelError {
    #[error("audio volume must be finite and non-negative, got {0}")]
    InvalidVolume(f32),
    #[error("WirePlumber default metadata is invalid: {0}")]
    InvalidDefaultMetadata(String),
    #[error("PipeWire Props payload is invalid: {0}")]
    InvalidProps(String),
}

#[derive(Deserialize)]
struct WirePlumberDefaultName {
    name: String,
}

pub fn parse_default_node_name(value: &str) -> Result<String, AudioModelError> {
    let parsed: WirePlumberDefaultName = serde_json::from_str(value)
        .map_err(|error| AudioModelError::InvalidDefaultMetadata(error.to_string()))?;
    if parsed.name.is_empty() {
        return Err(AudioModelError::InvalidDefaultMetadata(
            "name must not be empty".into(),
        ));
    }
    Ok(parsed.name)
}
