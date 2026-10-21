use gnos_audio::{
    AudioEndpointId, AudioEndpointKind, AudioProviderState, AudioSnapshot, RoleSelection,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ProductAudioEndpoint {
    pub id: AudioEndpointId,
    pub label: String,
    pub level: Option<f32>,
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProductAudioEndpointState {
    ProviderUnavailable,
    DefaultUnresolved {
        role_name: Option<String>,
        metadata_error: Option<String>,
    },
    Ready(ProductAudioEndpoint),
}

impl ProductAudioEndpointState {
    /// Whether an observed endpoint backs this state.
    ///
    /// A caller comparing two outputs wants to know whether both are showing real
    /// observed audio, without pattern-matching the variant at every site.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductAudioPresentation {
    pub output: ProductAudioEndpointState,
    pub input: ProductAudioEndpointState,
}

impl ProductAudioPresentation {
    pub fn derive(snapshot: &AudioSnapshot) -> Self {
        if snapshot.provider == AudioProviderState::Unavailable {
            return Self {
                output: ProductAudioEndpointState::ProviderUnavailable,
                input: ProductAudioEndpointState::ProviderUnavailable,
            };
        }
        Self {
            output: derive_endpoint(
                snapshot,
                &snapshot.defaults.output.effective,
                AudioEndpointKind::Output,
            ),
            input: derive_endpoint(
                snapshot,
                &snapshot.defaults.input.effective,
                AudioEndpointKind::Input,
            ),
        }
    }
}

fn derive_endpoint(
    snapshot: &AudioSnapshot,
    role: &RoleSelection,
    kind: AudioEndpointKind,
) -> ProductAudioEndpointState {
    let Some(id) = role.resolved else {
        return ProductAudioEndpointState::DefaultUnresolved {
            role_name: role.name.clone(),
            metadata_error: role.metadata_error.clone(),
        };
    };
    let Some(endpoint) = snapshot
        .endpoints
        .get(&id)
        .filter(|endpoint| endpoint.kind == kind)
    else {
        return ProductAudioEndpointState::DefaultUnresolved {
            role_name: role.name.clone(),
            metadata_error: role.metadata_error.clone(),
        };
    };
    ProductAudioEndpointState::Ready(ProductAudioEndpoint {
        id,
        label: endpoint.display_name.clone(),
        level: endpoint.volume.cubic_user_level(),
        muted: endpoint.muted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_audio::{
        AudioDefaultRoles, AudioEndpointId, AudioEndpointKind, AudioEndpointSnapshot,
        AudioProviderState, AudioSnapshot, AudioVolume, NativeVolume, RoleSelection,
    };
    use std::collections::BTreeMap;

    fn sink_snapshot(level_linear: f32, muted: bool) -> AudioSnapshot {
        let id = AudioEndpointId::new(7, 42);
        let endpoint = AudioEndpointSnapshot::new(
            id,
            AudioEndpointKind::Output,
            "sink.a".into(),
            "Built-in Audio".into(),
            AudioVolume::from_parts(
                None,
                vec![NativeVolume::new(level_linear).unwrap(); 2],
                vec![NativeVolume::new(1.0).unwrap(); 2],
            ),
            muted,
        );
        let mut endpoints = BTreeMap::new();
        endpoints.insert(id, endpoint);
        let mut defaults = AudioDefaultRoles::default();
        defaults.output.effective = RoleSelection {
            name: Some("sink.a".into()),
            resolved: Some(id),
            metadata_error: None,
        };
        AudioSnapshot {
            provider: AudioProviderState::Available,
            generation: 1,
            endpoints,
            defaults,
        }
    }

    #[test]
    fn unavailable_provider_is_not_presented_as_zero_volume() {
        let snapshot = AudioSnapshot {
            provider: AudioProviderState::Unavailable,
            generation: 0,
            endpoints: BTreeMap::new(),
            defaults: AudioDefaultRoles::default(),
        };
        let presentation = ProductAudioPresentation::derive(&snapshot);
        assert!(matches!(
            presentation.output,
            ProductAudioEndpointState::ProviderUnavailable
        ));
    }

    #[test]
    fn resolved_output_projects_cubic_user_level() {
        let presentation = ProductAudioPresentation::derive(&sink_snapshot(0.729, false));
        let ProductAudioEndpointState::Ready(output) = presentation.output else {
            panic!("resolved output must be ready");
        };
        assert_eq!(output.label, "Built-in Audio");
        assert!((output.level.unwrap() - 0.9).abs() < 0.0001);
        assert!(!output.muted);
    }

    #[test]
    fn mute_is_preserved_without_destroying_level() {
        let presentation = ProductAudioPresentation::derive(&sink_snapshot(0.729, true));
        let ProductAudioEndpointState::Ready(output) = presentation.output else {
            panic!("resolved output must be ready");
        };
        assert!(output.muted);
        assert!((output.level.unwrap() - 0.9).abs() < 0.0001);
    }

    #[test]
    fn unresolved_default_is_distinct_from_provider_unavailable() {
        let mut snapshot = sink_snapshot(0.729, false);
        snapshot.defaults.output.effective.resolved = None;
        let presentation = ProductAudioPresentation::derive(&snapshot);
        assert_eq!(
            presentation.output,
            ProductAudioEndpointState::DefaultUnresolved {
                role_name: Some("sink.a".into()),
                metadata_error: None,
            }
        );
    }
}
