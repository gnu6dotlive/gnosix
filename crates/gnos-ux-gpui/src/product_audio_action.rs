use gnos_audio::{AudioEndpointId, AudioEndpointKind, AudioProviderState, AudioSnapshot};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProductVolume(f32);

impl ProductVolume {
    pub fn new(value: f32) -> Result<Self, AudioIntentError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(AudioIntentError::VolumeOutOfRange(value));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeDelta(f32);

impl VolumeDelta {
    pub fn new(value: f32) -> Result<Self, AudioIntentError> {
        if !value.is_finite() {
            return Err(AudioIntentError::InvalidDelta(value));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactAudioEndpointRef {
    pub id: AudioEndpointId,
    pub kind: AudioEndpointKind,
}

impl ExactAudioEndpointRef {
    pub const fn output(id: AudioEndpointId) -> Self {
        Self {
            id,
            kind: AudioEndpointKind::Output,
        }
    }

    pub const fn input(id: AudioEndpointId) -> Self {
        Self {
            id,
            kind: AudioEndpointKind::Input,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioIntent {
    SetOutputVolume {
        target: ExactAudioEndpointRef,
        level: ProductVolume,
    },
    AdjustOutputVolume {
        target: ExactAudioEndpointRef,
        delta: VolumeDelta,
    },
    ToggleOutputMute {
        target: ExactAudioEndpointRef,
    },
    ToggleInputMute {
        target: ExactAudioEndpointRef,
    },
    /// SETTINGS-02B: make this endpoint the default for its kind.
    SelectDefault {
        target: ExactAudioEndpointRef,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioResolvedMutation {
    SetUserVolume {
        target: AudioEndpointId,
        level: ProductVolume,
        channels: usize,
    },
    SetMute {
        target: AudioEndpointId,
        kind: AudioEndpointKind,
        muted: bool,
    },
    SelectDefault {
        target: AudioEndpointId,
        kind: AudioEndpointKind,
        node_name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioIntentRefusal {
    ProviderUnavailable,
    EndpointGone,
    DefaultRoleChanged,
    KindMismatch,
    VolumeUnavailable,
    ServerReplaced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioIntentOutcome {
    Completed,
    RefusedStale(AudioIntentRefusal),
    TransportFailure(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioIntentError {
    VolumeOutOfRange(f32),
    InvalidDelta(f32),
}

pub fn validate_intent(
    snapshot: &AudioSnapshot,
    intent: &AudioIntent,
) -> Result<(), AudioIntentRefusal> {
    resolve_target(snapshot, intent).map(|_| ())
}

pub fn dispatch_intent(snapshot: &AudioSnapshot, intent: &AudioIntent) -> AudioIntentOutcome {
    let resolved = match resolve_intent(snapshot, intent) {
        Ok(resolved) => resolved,
        Err(reason) => return AudioIntentOutcome::RefusedStale(reason),
    };
    if let AudioResolvedMutation::SelectDefault {
        target,
        kind,
        node_name,
    } = &resolved
    {
        return match gnos_audio::execute_select_default(
            gnos_audio::ExactAudioTarget {
                id: *target,
                kind: *kind,
            },
            node_name,
        ) {
            Ok(outcome) => map_native_outcome(outcome),
            Err(error) => AudioIntentOutcome::TransportFailure(error.to_string()),
        };
    }
    let (target, mutation) = match resolved {
        AudioResolvedMutation::SetUserVolume {
            target,
            level,
            channels,
        } => (
            gnos_audio::ExactAudioTarget {
                id: target,
                kind: AudioEndpointKind::Output,
            },
            gnos_audio::ExactAudioMutation::SetUserVolume {
                level: level.get(),
                channels,
            },
        ),
        AudioResolvedMutation::SetMute {
            target,
            kind,
            muted,
        } => (
            gnos_audio::ExactAudioTarget { id: target, kind },
            gnos_audio::ExactAudioMutation::SetMute { muted },
        ),
        AudioResolvedMutation::SelectDefault { .. } => unreachable!("carried out above"),
    };
    match gnos_audio::execute_exact_mutation(target, mutation) {
        Ok(outcome) => map_native_outcome(outcome),
        Err(error) => AudioIntentOutcome::TransportFailure(error.to_string()),
    }
}

fn map_native_outcome(outcome: gnos_audio::ExactAudioMutationOutcome) -> AudioIntentOutcome {
    match outcome {
        gnos_audio::ExactAudioMutationOutcome::Completed => AudioIntentOutcome::Completed,
        gnos_audio::ExactAudioMutationOutcome::RefusedStaleServer => {
            AudioIntentOutcome::RefusedStale(AudioIntentRefusal::ServerReplaced)
        }
        gnos_audio::ExactAudioMutationOutcome::TargetAbsent => {
            AudioIntentOutcome::RefusedStale(AudioIntentRefusal::EndpointGone)
        }
        gnos_audio::ExactAudioMutationOutcome::KindMismatch => {
            AudioIntentOutcome::RefusedStale(AudioIntentRefusal::KindMismatch)
        }
    }
}

pub fn resolve_intent(
    snapshot: &AudioSnapshot,
    intent: &AudioIntent,
) -> Result<AudioResolvedMutation, AudioIntentRefusal> {
    let endpoint = resolve_target(snapshot, intent)?;
    match intent {
        AudioIntent::SetOutputVolume { level, .. } => Ok(AudioResolvedMutation::SetUserVolume {
            target: endpoint.id,
            level: *level,
            channels: channel_count(endpoint),
        }),
        AudioIntent::AdjustOutputVolume { delta, .. } => {
            let current = endpoint
                .volume
                .cubic_user_level()
                .ok_or(AudioIntentRefusal::VolumeUnavailable)?;
            let desired = (current + delta.get()).clamp(0.0, 1.0);
            Ok(AudioResolvedMutation::SetUserVolume {
                target: endpoint.id,
                level: ProductVolume::new(desired).expect("clamped product volume"),
                channels: channel_count(endpoint),
            })
        }
        AudioIntent::ToggleOutputMute { .. } | AudioIntent::ToggleInputMute { .. } => {
            Ok(AudioResolvedMutation::SetMute {
                target: endpoint.id,
                kind: endpoint.kind,
                muted: !endpoint.muted,
            })
        }
        AudioIntent::SelectDefault { .. } => Ok(AudioResolvedMutation::SelectDefault {
            target: endpoint.id,
            kind: endpoint.kind,
            node_name: endpoint.node_name.clone(),
        }),
    }
}

fn channel_count(endpoint: &gnos_audio::AudioEndpointSnapshot) -> usize {
    endpoint.volume.channels().len().max(1)
}

fn resolve_target<'a>(
    snapshot: &'a AudioSnapshot,
    intent: &AudioIntent,
) -> Result<&'a gnos_audio::AudioEndpointSnapshot, AudioIntentRefusal> {
    if snapshot.provider != AudioProviderState::Available {
        return Err(AudioIntentRefusal::ProviderUnavailable);
    }
    let target = match intent {
        AudioIntent::SetOutputVolume { target, .. }
        | AudioIntent::AdjustOutputVolume { target, .. }
        | AudioIntent::ToggleOutputMute { target }
        | AudioIntent::ToggleInputMute { target } => target,
        // Any present endpoint of the named kind may become the default.
        AudioIntent::SelectDefault { target } => {
            let endpoint = snapshot
                .endpoints
                .get(&target.id)
                .ok_or(AudioIntentRefusal::EndpointGone)?;
            if endpoint.kind != target.kind {
                return Err(AudioIntentRefusal::KindMismatch);
            }
            return Ok(endpoint);
        }
    };
    let expected_kind = match intent {
        AudioIntent::ToggleInputMute { .. } => AudioEndpointKind::Input,
        _ => AudioEndpointKind::Output,
    };
    if target.kind != expected_kind {
        return Err(AudioIntentRefusal::KindMismatch);
    }
    let role = match expected_kind {
        AudioEndpointKind::Output => &snapshot.defaults.output.effective,
        AudioEndpointKind::Input => &snapshot.defaults.input.effective,
    };
    match role.resolved {
        Some(id) if id != target.id => return Err(AudioIntentRefusal::DefaultRoleChanged),
        None if snapshot.endpoints.contains_key(&target.id) => {
            return Err(AudioIntentRefusal::DefaultRoleChanged);
        }
        None => return Err(AudioIntentRefusal::EndpointGone),
        Some(_) => {}
    }
    let endpoint = snapshot
        .endpoints
        .get(&target.id)
        .ok_or(AudioIntentRefusal::EndpointGone)?;
    if endpoint.kind != expected_kind {
        return Err(AudioIntentRefusal::KindMismatch);
    }
    Ok(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_audio::{
        AudioEndpointId, AudioEndpointKind, AudioEndpointSnapshot, AudioState, AudioVolume,
        DefaultRoleKind, NativeVolume,
    };

    fn snapshot(level: f32, muted: bool) -> gnos_audio::AudioSnapshot {
        let mut state = AudioState::new(7);
        let endpoint = AudioEndpointSnapshot::new(
            AudioEndpointId::new(7, 42),
            AudioEndpointKind::Output,
            "sink.a".into(),
            "Built-in Audio".into(),
            AudioVolume::from_parts(
                None,
                vec![NativeVolume::new(level.powi(3)).unwrap(); 2],
                vec![NativeVolume::new(1.0).unwrap(); 2],
            ),
            muted,
        );
        state.upsert_endpoint(endpoint);
        state.set_effective_default_name(DefaultRoleKind::Output, Some("sink.a".into()));
        state.snapshot()
    }

    #[test]
    fn any_present_endpoint_of_its_kind_can_become_the_default() {
        let snapshot = snapshot(0.5, false);
        let id = AudioEndpointId::new(7, 42);
        assert_eq!(
            resolve_intent(
                &snapshot,
                &AudioIntent::SelectDefault {
                    target: ExactAudioEndpointRef::output(id)
                }
            ),
            Ok(AudioResolvedMutation::SelectDefault {
                target: id,
                kind: AudioEndpointKind::Output,
                node_name: "sink.a".into(),
            })
        );
        assert_eq!(
            validate_intent(
                &snapshot,
                &AudioIntent::SelectDefault {
                    target: ExactAudioEndpointRef::input(id)
                }
            ),
            Err(AudioIntentRefusal::KindMismatch)
        );
        assert_eq!(
            validate_intent(
                &snapshot,
                &AudioIntent::SelectDefault {
                    target: ExactAudioEndpointRef::output(AudioEndpointId::new(7, 9))
                }
            ),
            Err(AudioIntentRefusal::EndpointGone)
        );
    }

    #[test]
    fn current_exact_output_binding_resolves_set_volume() {
        let snapshot = snapshot(0.9, false);
        let binding = ExactAudioEndpointRef::output(AudioEndpointId::new(7, 42));
        let intent = AudioIntent::SetOutputVolume {
            target: binding,
            level: ProductVolume::new(0.75).unwrap(),
        };
        assert_eq!(
            resolve_intent(&snapshot, &intent).unwrap(),
            AudioResolvedMutation::SetUserVolume {
                target: AudioEndpointId::new(7, 42),
                level: ProductVolume::new(0.75).unwrap(),
                channels: 2,
            }
        );
    }

    #[test]
    fn stale_binding_never_falls_through_to_replacement_default() {
        let old = ExactAudioEndpointRef::output(AudioEndpointId::new(7, 42));
        let mut state = AudioState::new(7);
        let replacement = AudioEndpointSnapshot::new(
            AudioEndpointId::new(7, 99),
            AudioEndpointKind::Output,
            "sink.b".into(),
            "Replacement".into(),
            AudioVolume::scalar(0.5).unwrap(),
            false,
        );
        state.upsert_endpoint(replacement);
        state.set_effective_default_name(DefaultRoleKind::Output, Some("sink.b".into()));
        let snapshot = state.snapshot();
        let intent = AudioIntent::ToggleOutputMute { target: old };
        assert_eq!(
            validate_intent(&snapshot, &intent),
            Err(AudioIntentRefusal::DefaultRoleChanged)
        );
    }

    #[test]
    fn adjust_volume_uses_observed_level_and_clamps_to_initial_policy() {
        let snapshot = snapshot(0.99, false);
        let target = ExactAudioEndpointRef::output(AudioEndpointId::new(7, 42));
        let intent = AudioIntent::AdjustOutputVolume {
            target,
            delta: VolumeDelta::new(0.05).unwrap(),
        };
        let resolved = resolve_intent(&snapshot, &intent).unwrap();
        let AudioResolvedMutation::SetUserVolume { level, .. } = resolved else {
            panic!("adjust must resolve to exact set-volume mutation");
        };
        assert_eq!(level.get(), 1.0);
    }

    #[test]
    fn volume_policy_rejects_amplification_for_phase4_actions() {
        assert!(ProductVolume::new(1.01).is_err());
        assert!(ProductVolume::new(1.0).is_ok());
        assert!(ProductVolume::new(0.0).is_ok());
    }

    #[test]
    fn toggle_mute_resolves_from_observed_truth() {
        let snapshot = snapshot(0.9, true);
        let target = ExactAudioEndpointRef::output(AudioEndpointId::new(7, 42));
        assert_eq!(
            resolve_intent(&snapshot, &AudioIntent::ToggleOutputMute { target }).unwrap(),
            AudioResolvedMutation::SetMute {
                target: AudioEndpointId::new(7, 42),
                kind: AudioEndpointKind::Output,
                muted: false,
            }
        );
    }

    #[test]
    fn disappeared_endpoint_is_refused_without_retargeting() {
        let mut snapshot = snapshot(0.9, false);
        let target = ExactAudioEndpointRef::output(AudioEndpointId::new(7, 42));
        snapshot.endpoints.remove(&target.id);
        assert_eq!(
            validate_intent(&snapshot, &AudioIntent::ToggleOutputMute { target }),
            Err(AudioIntentRefusal::EndpointGone)
        );
    }

    #[test]
    fn wrong_endpoint_kind_is_refused() {
        let snapshot = snapshot(0.9, false);
        let target = ExactAudioEndpointRef::input(AudioEndpointId::new(7, 42));
        assert_eq!(
            validate_intent(&snapshot, &AudioIntent::ToggleOutputMute { target }),
            Err(AudioIntentRefusal::KindMismatch)
        );
    }

    #[test]
    fn unavailable_provider_refuses_intent() {
        let mut snapshot = snapshot(0.9, false);
        snapshot.provider = AudioProviderState::Unavailable;
        let target = ExactAudioEndpointRef::output(AudioEndpointId::new(7, 42));
        assert_eq!(
            validate_intent(&snapshot, &AudioIntent::ToggleOutputMute { target }),
            Err(AudioIntentRefusal::ProviderUnavailable)
        );
    }

    #[test]
    fn adjust_refuses_when_observed_volume_is_unavailable() {
        let mut snapshot = snapshot(0.9, false);
        let id = AudioEndpointId::new(7, 42);
        snapshot.endpoints.get_mut(&id).unwrap().volume =
            AudioVolume::from_parts(None, vec![], vec![]);
        let intent = AudioIntent::AdjustOutputVolume {
            target: ExactAudioEndpointRef::output(id),
            delta: VolumeDelta::new(0.05).unwrap(),
        };
        assert_eq!(
            resolve_intent(&snapshot, &intent),
            Err(AudioIntentRefusal::VolumeUnavailable)
        );
    }

    #[test]
    fn native_outcomes_map_to_product_outcomes_without_retry_semantics() {
        assert_eq!(
            map_native_outcome(gnos_audio::ExactAudioMutationOutcome::Completed),
            AudioIntentOutcome::Completed
        );
        assert_eq!(
            map_native_outcome(gnos_audio::ExactAudioMutationOutcome::RefusedStaleServer),
            AudioIntentOutcome::RefusedStale(AudioIntentRefusal::ServerReplaced)
        );
        assert_eq!(
            map_native_outcome(gnos_audio::ExactAudioMutationOutcome::TargetAbsent),
            AudioIntentOutcome::RefusedStale(AudioIntentRefusal::EndpointGone)
        );
        assert_eq!(
            map_native_outcome(gnos_audio::ExactAudioMutationOutcome::KindMismatch),
            AudioIntentOutcome::RefusedStale(AudioIntentRefusal::KindMismatch)
        );
    }
}
