//! Native PipeWire read-side audio substrate for Gnosix.
//!
//! Phase 2 deliberately owns facts only: live endpoint identity, WirePlumber
//! default roles, volume/mute state, and semantic snapshot publication.
//! SURF-04B adds a read-only capture of the default sink's monitor for the
//! desktop visualiser (`visualiser`).

mod model;
mod pipewire_mutation;
mod pipewire_observer;
mod visualiser;

pub use model::*;
pub use pipewire_mutation::*;
pub use pipewire_observer::*;
pub use visualiser::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_identity_is_server_cookie_plus_object_serial() {
        let a = AudioEndpointId::new(77, 1001);
        let b = AudioEndpointId::new(77, 1002);
        let restarted = AudioEndpointId::new(78, 1001);
        assert_ne!(a, b);
        assert_ne!(a, restarted);
    }

    #[test]
    fn default_metadata_parses_node_name_without_inventing_identity() {
        let parsed =
            parse_default_node_name(r#"{"name":"alsa_output.pci-0000_00_1f.3.analog-stereo"}"#)
                .expect("valid WirePlumber metadata");
        assert_eq!(parsed, "alsa_output.pci-0000_00_1f.3.analog-stereo");
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn malformed_default_metadata_is_rejected() {
        assert!(parse_default_node_name("not-json").is_err());
        assert!(parse_default_node_name(r#"{"name":7}"#).is_err());
    }

    #[test]
    fn native_volume_preserves_amplification_above_one() {
        let volume = NativeVolume::new(1.25).expect("finite non-negative volume");
        assert_eq!(volume.get(), 1.25);
        assert_eq!(volume.percent_unclamped(), 125.0);
    }

    #[test]
    fn mute_is_independent_from_nonzero_volume() {
        let endpoint = fixture_endpoint("sink", AudioEndpointKind::Output, 0.8, true);
        assert_eq!(endpoint.volume.scalar_value().unwrap().get(), 0.8);
        assert!(endpoint.muted);
    }

    #[test]
    fn cubic_user_level_matches_wireplumber_observation() {
        let linear = NativeVolume::new(0.7289852).unwrap();
        let volume = AudioVolume::from_parts(
            None,
            vec![linear, linear],
            vec![NativeVolume::new(1.0).unwrap(); 2],
        );
        let level = volume.cubic_user_level().expect("channel volume exists");
        assert!((level - 0.9).abs() < 0.0001, "level={level}");
    }

    #[test]
    fn cubic_user_level_preserves_amplification_above_one() {
        let volume =
            AudioVolume::from_parts(None, vec![NativeVolume::new(1.728).unwrap()], Vec::new());
        let level = volume.cubic_user_level().unwrap();
        assert!((level - 1.2).abs() < 0.0001, "level={level}");
    }
}

#[cfg(test)]
mod reducer_tests {
    use super::*;

    #[test]
    fn unresolved_effective_default_stays_explicit() {
        let mut state = AudioState::new(77);
        state.set_effective_default_name(DefaultRoleKind::Output, Some("missing.sink".into()));
        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.defaults.output.effective.name.as_deref(),
            Some("missing.sink")
        );
        assert_eq!(snapshot.defaults.output.effective.resolved, None);
    }

    #[test]
    fn malformed_role_metadata_is_explicit_and_clears_stale_resolution() {
        let mut state = AudioState::new(77);
        state.set_effective_default_name(DefaultRoleKind::Output, Some("sink.a".into()));
        let endpoint = fixture_endpoint("sink.a", AudioEndpointKind::Output, 0.5, false);
        state.upsert_endpoint(endpoint);
        assert!(
            state
                .snapshot()
                .defaults
                .output
                .effective
                .resolved
                .is_some()
        );
        state.set_default_metadata_error(DefaultRoleKind::Output, false, "bad json".into());
        let role = state.snapshot().defaults.output.effective;
        assert_eq!(role.name, None);
        assert_eq!(role.resolved, None);
        assert_eq!(role.metadata_error.as_deref(), Some("bad json"));
    }

    #[test]
    fn adding_matching_endpoint_resolves_role_without_replacing_identity() {
        let mut state = AudioState::new(77);
        state.set_effective_default_name(DefaultRoleKind::Output, Some("sink.a".into()));
        let endpoint = fixture_endpoint("sink.a", AudioEndpointKind::Output, 0.5, false);
        let expected = endpoint.id;
        state.upsert_endpoint(endpoint);
        assert_eq!(
            state.snapshot().defaults.output.effective.resolved,
            Some(expected)
        );
    }
}

#[cfg(test)]
mod generation_tests {
    use super::*;

    #[test]
    fn duplicate_endpoint_update_does_not_advance_generation() {
        let mut state = AudioState::new(77);
        let endpoint = fixture_endpoint("sink.a", AudioEndpointKind::Output, 0.5, false);
        assert!(state.upsert_endpoint(endpoint.clone()));
        let generation = state.snapshot().generation;
        assert!(!state.upsert_endpoint(endpoint));
        assert_eq!(state.snapshot().generation, generation);
    }

    #[test]
    fn provider_unavailability_is_explicit_and_semantic() {
        let mut state = AudioState::new(77);
        let generation = state.snapshot().generation;
        assert!(state.set_provider_state(AudioProviderState::Unavailable));
        assert_eq!(state.snapshot().provider, AudioProviderState::Unavailable);
        assert_eq!(state.snapshot().generation, generation + 1);
        assert!(!state.set_provider_state(AudioProviderState::Unavailable));
    }

    #[test]
    fn semantic_volume_change_advances_generation_once() {
        let mut state = AudioState::new(77);
        let endpoint = fixture_endpoint("sink.a", AudioEndpointKind::Output, 0.5, false);
        let id = endpoint.id;
        state.upsert_endpoint(endpoint);
        let generation = state.snapshot().generation;
        assert!(state.update_endpoint_state(&id, AudioVolume::scalar(0.6).unwrap(), false));
        assert_eq!(state.snapshot().generation, generation + 1);
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn removing_default_endpoint_keeps_effective_role_but_unresolves_identity() {
        let mut state = AudioState::new(77);
        state.set_effective_default_name(DefaultRoleKind::Output, Some("sink.a".into()));
        let endpoint = fixture_endpoint("sink.a", AudioEndpointKind::Output, 0.5, false);
        let id = endpoint.id;
        state.upsert_endpoint(endpoint);
        assert_eq!(
            state.snapshot().defaults.output.effective.resolved.as_ref(),
            Some(&id)
        );
        assert!(state.remove_endpoint(&id));
        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.defaults.output.effective.name.as_deref(),
            Some("sink.a")
        );
        assert_eq!(snapshot.defaults.output.effective.resolved, None);
    }
}

#[cfg(test)]
fn fixture_endpoint(
    name: &str,
    kind: AudioEndpointKind,
    volume: f32,
    muted: bool,
) -> AudioEndpointSnapshot {
    AudioEndpointSnapshot::new(
        AudioEndpointId::new(77, name.bytes().map(u64::from).sum()),
        kind,
        name.into(),
        name.into(),
        AudioVolume::scalar(volume).unwrap(),
        muted,
    )
}
