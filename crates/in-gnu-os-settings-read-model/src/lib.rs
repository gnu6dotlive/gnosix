//! Read-only effective settings projection for Gnosix.
//!
//! This crate composes the already-qualified R11 output-identity and R12
//! desktop-profile read paths. It selects no production filesystem paths,
//! issues no writer, exposes no mutation endpoint, and owns no UI.
//!
//! The future settings authority can own the stores and publish this effective
//! read model to GNOS.UX and other consumers.

use gnosix_desktop_model::DesktopOutputProjection;
use in_gnu_os_desktop_profiles::{
    ColorSchemePreference, DesktopPreferences, DesktopProfileStore, ProfileProjectionOutcome,
    ReducedMotionPreference, SessionProfileProjectionError,
};
use in_gnu_os_output_identity::{BindingEvidence, OutputObservation};
use in_gnu_os_output_identity_projection::{
    DurableSessionOutputAssociation, SessionOutputObservation,
};
use sha2::{Digest, Sha256};
use std::fmt;

pub use in_gnu_os_desktop_profiles::{
    BarEdgePreference, BarPreferenceOverrides, WorkspaceDisplayCount, WorkspaceDisplayCountError,
    WorkspaceDisplayMode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductColorScheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreferenceValueSource {
    ProductDefault,
    SystemObservation,
    DurableProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductPreferenceDefaults {
    pub color_scheme: ProductColorScheme,
    pub reduced_motion: bool,
    pub text_scale_basis_points: u32,
    pub bar_edge: BarEdgePreference,
}

impl Default for ProductPreferenceDefaults {
    fn default() -> Self {
        Self {
            color_scheme: ProductColorScheme::Dark,
            reduced_motion: false,
            text_scale_basis_points: 10_000,
            bar_edge: BarEdgePreference::Top,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SystemProductPreferences {
    pub color_scheme: Option<ProductColorScheme>,
    pub reduced_motion: Option<bool>,
    pub text_scale_basis_points: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductPreferenceProjectionState {
    AuthorityUnavailable,
    NoDurableBinding,
    ProfileDocumentAbsent,
    NoProfileSelection,
    Selected,
    RecoveryRequired,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveProductPreferences {
    pub color_scheme: ProductColorScheme,
    pub color_scheme_source: PreferenceValueSource,
    pub reduced_motion: bool,
    pub reduced_motion_source: PreferenceValueSource,
    pub text_scale_basis_points: u32,
    pub text_scale_source: PreferenceValueSource,
    /// None only when the profile projection is invalid/unavailable and no
    /// truthful effective Bar placement can be presented.
    pub bar_edge: Option<BarEdgePreference>,
    pub bar_edge_source: Option<PreferenceValueSource>,
    /// Exact durable DONOR-11 overrides. Empty means no explicit override;
    /// projection_state distinguishes that from unavailable/recovery states.
    pub bar_overrides: BarPreferenceOverrides,
    pub projection_state: ProductPreferenceProjectionState,
    pub identity_revision: Option<u64>,
    pub profile_revision: Option<u64>,
}

impl Default for EffectiveProductPreferences {
    fn default() -> Self {
        Self::resolve(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            ProductPreferenceProjectionState::AuthorityUnavailable,
            None,
            None,
            None,
        )
    }
}

impl EffectiveProductPreferences {
    fn resolve(
        defaults: ProductPreferenceDefaults,
        system: SystemProductPreferences,
        projection_state: ProductPreferenceProjectionState,
        identity_revision: Option<u64>,
        profile_revision: Option<u64>,
        profile: Option<&DesktopPreferences>,
    ) -> Self {
        let profile_color = profile.and_then(|preferences| preferences.visual.color_scheme);
        let (color_scheme, color_scheme_source) = match profile_color {
            Some(ColorSchemePreference::Light) => (
                ProductColorScheme::Light,
                PreferenceValueSource::DurableProfile,
            ),
            Some(ColorSchemePreference::Dark) => (
                ProductColorScheme::Dark,
                PreferenceValueSource::DurableProfile,
            ),
            Some(ColorSchemePreference::FollowSystem) | None => system.color_scheme.map_or(
                (defaults.color_scheme, PreferenceValueSource::ProductDefault),
                |value| (value, PreferenceValueSource::SystemObservation),
            ),
        };

        let profile_motion = profile.and_then(|preferences| preferences.motion.reduced_motion);
        let (reduced_motion, reduced_motion_source) = match profile_motion {
            Some(ReducedMotionPreference::Reduce) => (true, PreferenceValueSource::DurableProfile),
            Some(ReducedMotionPreference::FollowSystem) | None => system.reduced_motion.map_or(
                (
                    defaults.reduced_motion,
                    PreferenceValueSource::ProductDefault,
                ),
                |value| (value, PreferenceValueSource::SystemObservation),
            ),
        };

        let profile_text_scale = profile
            .and_then(|preferences| preferences.accessibility.text_scale)
            .map(|scale| scale.basis_points());
        let (text_scale_basis_points, text_scale_source) = profile_text_scale.map_or_else(
            || {
                system.text_scale_basis_points.map_or(
                    (
                        defaults.text_scale_basis_points,
                        PreferenceValueSource::ProductDefault,
                    ),
                    |value| (value, PreferenceValueSource::SystemObservation),
                )
            },
            |value| (value, PreferenceValueSource::DurableProfile),
        );

        let profile_bar_edge = profile.and_then(|preferences| preferences.bar.edge);
        let (bar_edge, bar_edge_source) = match projection_state {
            ProductPreferenceProjectionState::RecoveryRequired
            | ProductPreferenceProjectionState::Unavailable => (None, None),
            _ => profile_bar_edge.map_or(
                (
                    Some(defaults.bar_edge),
                    Some(PreferenceValueSource::ProductDefault),
                ),
                |edge| (Some(edge), Some(PreferenceValueSource::DurableProfile)),
            ),
        };
        let bar_overrides = match projection_state {
            ProductPreferenceProjectionState::RecoveryRequired
            | ProductPreferenceProjectionState::Unavailable => BarPreferenceOverrides::default(),
            _ => profile.map_or_else(BarPreferenceOverrides::default, |preferences| {
                preferences.bar.overrides()
            }),
        };

        Self {
            color_scheme,
            color_scheme_source,
            reduced_motion,
            reduced_motion_source,
            text_scale_basis_points,
            text_scale_source,
            bar_edge,
            bar_edge_source,
            bar_overrides,
            projection_state,
            identity_revision,
            profile_revision,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductPreferenceResolveError {
    ProfileWithoutDurableBinding,
    DurableOutputMismatch,
    IdentityRevisionMismatch,
}

#[derive(Debug)]
pub enum ProductPreferenceReadError {
    Projection(SessionProfileProjectionError),
    Resolve(ProductPreferenceResolveError),
}

impl fmt::Display for ProductPreferenceReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => write!(formatter, "settings projection failed: {error}"),
            Self::Resolve(error) => write!(formatter, "effective settings rejected: {error:?}"),
        }
    }
}

impl std::error::Error for ProductPreferenceReadError {}

/// Construct the R11 session observation from the descriptors already present
/// in the exact current desktop projection.
///
/// Connector/make/model remain corroborating context only. A durable binding is
/// possible only when the observer supplied a non-empty serial, represented as
/// an opaque SHA-256 SerialFingerprint.
pub fn session_output_observation(
    projection: &DesktopOutputProjection,
) -> SessionOutputObservation {
    let evidence = projection
        .output
        .descriptors
        .serial
        .as_deref()
        .filter(|serial| !serial.trim().is_empty())
        .map(|serial| BindingEvidence::SerialFingerprint(Sha256::digest(serial.as_bytes()).into()))
        .into_iter()
        .collect::<Vec<_>>();

    let mut observation =
        OutputObservation::new(evidence).with_connector(projection.output.connector.as_str());
    if let (Some(make), Some(model)) = (
        projection.output.descriptors.make.as_deref(),
        projection.output.descriptors.model.as_deref(),
    ) {
        observation = observation.with_make_model(make, model);
    }

    SessionOutputObservation::for_output(projection.output.output_hub_id, observation)
}

/// Read one effective preference snapshot through the profile store's own bound
/// R11 authority.
///
/// A missing profile store is a valid authority-unavailable state: defaults and
/// system observations remain usable, but no durable-profile claim is made.
pub fn resolve_without_profile_authority(
    defaults: ProductPreferenceDefaults,
    system: SystemProductPreferences,
) -> EffectiveProductPreferences {
    EffectiveProductPreferences::resolve(
        defaults,
        system,
        ProductPreferenceProjectionState::AuthorityUnavailable,
        None,
        None,
        None,
    )
}

pub fn read_effective_preferences(
    defaults: ProductPreferenceDefaults,
    system: SystemProductPreferences,
    projection: &DesktopOutputProjection,
    profile_store: Option<&DesktopProfileStore>,
) -> Result<EffectiveProductPreferences, ProductPreferenceReadError> {
    let Some(profile_store) = profile_store else {
        return Ok(EffectiveProductPreferences::resolve(
            defaults,
            system,
            ProductPreferenceProjectionState::AuthorityUnavailable,
            None,
            None,
            None,
        ));
    };

    let observed = session_output_observation(projection);
    let session_projection = profile_store
        .project_session_output(projection, &observed)
        .map_err(ProductPreferenceReadError::Projection)?;

    resolve_effective_preferences(
        defaults,
        system,
        session_projection.association.as_ref(),
        session_projection.profile.as_ref(),
    )
    .map_err(ProductPreferenceReadError::Resolve)
}

pub fn resolve_effective_preferences(
    defaults: ProductPreferenceDefaults,
    system: SystemProductPreferences,
    association: Option<&DurableSessionOutputAssociation>,
    profile_projection: Option<&ProfileProjectionOutcome>,
) -> Result<EffectiveProductPreferences, ProductPreferenceResolveError> {
    let Some(association) = association else {
        if profile_projection.is_some() {
            return Err(ProductPreferenceResolveError::ProfileWithoutDurableBinding);
        }
        return Ok(EffectiveProductPreferences::resolve(
            defaults,
            system,
            ProductPreferenceProjectionState::NoDurableBinding,
            None,
            None,
            None,
        ));
    };

    let Some(profile_projection) = profile_projection else {
        return Ok(EffectiveProductPreferences::resolve(
            defaults,
            system,
            ProductPreferenceProjectionState::NoProfileSelection,
            Some(association.registry_revision),
            None,
            None,
        ));
    };

    let (output_hub_id, identity_revision, profile_revision, state, profile) =
        match profile_projection {
            ProfileProjectionOutcome::DocumentAbsent {
                output_hub_id,
                identity_revision,
            } => (
                *output_hub_id,
                *identity_revision,
                None,
                ProductPreferenceProjectionState::ProfileDocumentAbsent,
                None,
            ),
            ProfileProjectionOutcome::NoSelection {
                output_hub_id,
                profile_revision,
                identity_revision,
            } => (
                *output_hub_id,
                *identity_revision,
                Some(*profile_revision),
                ProductPreferenceProjectionState::NoProfileSelection,
                None,
            ),
            ProfileProjectionOutcome::Selected {
                output_hub_id,
                profile_revision,
                identity_revision,
                profile,
            } => (
                *output_hub_id,
                *identity_revision,
                Some(*profile_revision),
                ProductPreferenceProjectionState::Selected,
                Some(&profile.preferences),
            ),
            ProfileProjectionOutcome::RecoveryRequired {
                output_hub_id,
                identity_revision,
                ..
            } => (
                *output_hub_id,
                *identity_revision,
                None,
                ProductPreferenceProjectionState::RecoveryRequired,
                None,
            ),
            ProfileProjectionOutcome::Unavailable {
                output_hub_id,
                identity_revision,
                ..
            } => (
                *output_hub_id,
                *identity_revision,
                None,
                ProductPreferenceProjectionState::Unavailable,
                None,
            ),
        };

    if output_hub_id != association.durable_output_hub_id {
        return Err(ProductPreferenceResolveError::DurableOutputMismatch);
    }
    if identity_revision != association.registry_revision {
        return Err(ProductPreferenceResolveError::IdentityRevisionMismatch);
    }

    Ok(EffectiveProductPreferences::resolve(
        defaults,
        system,
        state,
        Some(identity_revision),
        profile_revision,
        profile,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnosix_desktop_model::{
        ConnectorName, DesktopContextGeneration, DesktopOutputProjection, LogicalPoint,
        LogicalSize, OutputHubId, OutputState, OutputTransform, PhysicalOutputDescriptors,
        PhysicalPixelSize, ScaleMilli,
    };
    use in_gnu_os_desktop_profiles::{
        AccessibilityPreferences, DesktopProfile, DesktopProfileId, MotionPreferences,
        ProfileCandidateIssue, ProfileRecoveryIssue, TextScale, VisualPreferences,
    };
    use in_gnu_os_output_identity::DurableOutputHubId;
    use std::collections::BTreeSet;

    pub(super) fn association(revision: u64) -> DurableSessionOutputAssociation {
        DurableSessionOutputAssociation {
            session_output_hub_id: OutputHubId::from_bytes([1; 16]),
            durable_output_hub_id: DurableOutputHubId::from_bytes([2; 16]),
            registry_revision: revision,
            matched_evidence: BTreeSet::from([BindingEvidence::SerialFingerprint([3; 32])]),
        }
    }

    pub(super) fn profile_projection(
        identity_revision: u64,
        color_scheme: Option<ColorSchemePreference>,
        reduced_motion: Option<ReducedMotionPreference>,
        text_scale: Option<u32>,
    ) -> ProfileProjectionOutcome {
        ProfileProjectionOutcome::Selected {
            output_hub_id: DurableOutputHubId::from_bytes([2; 16]),
            profile_revision: 11,
            identity_revision,
            profile: DesktopProfile {
                id: DesktopProfileId::from_bytes([4; 16]),
                name: "Studio".to_owned(),
                preferences: DesktopPreferences {
                    visual: VisualPreferences { color_scheme },
                    motion: MotionPreferences { reduced_motion },
                    accessibility: AccessibilityPreferences {
                        text_scale: text_scale.map(|value| TextScale::try_from(value).unwrap()),
                    },
                    bar: Default::default(),
                },
            },
        }
    }

    fn projection(serial: Option<&str>) -> DesktopOutputProjection {
        DesktopOutputProjection {
            generation: DesktopContextGeneration(3),
            output: OutputState {
                output_hub_id: OutputHubId::from_bytes([1; 16]),
                connector: ConnectorName::new("DP-1").unwrap(),
                descriptors: PhysicalOutputDescriptors {
                    make: Some("Example".to_owned()),
                    model: Some("Panel".to_owned()),
                    serial: serial.map(ToOwned::to_owned),
                },
                active_workspace_id: None,
                physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
                logical_position: LogicalPoint::ORIGIN,
                logical_size: LogicalSize::new(1920, 1080).unwrap(),
                scale: ScaleMilli::from_milli(1000).unwrap(),
                transform: OutputTransform::Normal,
            },
            workspaces: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn serial_descriptor_becomes_stable_r11_evidence_but_connector_alone_does_not() {
        let with_serial = session_output_observation(&projection(Some("SERIAL-42")));
        assert_eq!(
            with_serial.session_output_hub_id(),
            OutputHubId::from_bytes([1; 16])
        );
        assert_eq!(
            with_serial.observation(),
            &OutputObservation::new([BindingEvidence::SerialFingerprint(
                Sha256::digest(b"SERIAL-42").into()
            )])
            .with_connector("DP-1")
            .with_make_model("Example", "Panel")
        );

        let without_serial = session_output_observation(&projection(None));
        assert_eq!(
            without_serial.observation(),
            &OutputObservation::new([])
                .with_connector("DP-1")
                .with_make_model("Example", "Panel")
        );
    }

    #[test]
    fn profile_overrides_win_and_keep_point_in_time_revisions() {
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences {
                color_scheme: Some(ProductColorScheme::Light),
                reduced_motion: Some(false),
                text_scale_basis_points: Some(11_000),
            },
            Some(&association(7)),
            Some(&profile_projection(
                7,
                Some(ColorSchemePreference::Dark),
                Some(ReducedMotionPreference::Reduce),
                Some(12_500),
            )),
        )
        .unwrap();

        assert_eq!(effective.color_scheme, ProductColorScheme::Dark);
        assert_eq!(
            effective.color_scheme_source,
            PreferenceValueSource::DurableProfile
        );
        assert!(effective.reduced_motion);
        assert_eq!(
            effective.reduced_motion_source,
            PreferenceValueSource::DurableProfile
        );
        assert_eq!(effective.text_scale_basis_points, 12_500);
        assert_eq!(effective.identity_revision, Some(7));
        assert_eq!(effective.profile_revision, Some(11));
    }

    #[test]
    fn explicit_bar_edge_is_projected_with_profile_and_r11_revisions() {
        let mut selected = profile_projection(7, None, None, None);
        let ProfileProjectionOutcome::Selected { profile, .. } = &mut selected else {
            unreachable!();
        };
        profile.preferences.bar.edge = Some(BarEdgePreference::Right);

        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            Some(&association(7)),
            Some(&selected),
        )
        .unwrap();

        assert_eq!(effective.bar_edge, Some(BarEdgePreference::Right));
        assert_eq!(
            effective.bar_edge_source,
            Some(PreferenceValueSource::DurableProfile)
        );
        assert_eq!(effective.profile_revision, Some(11));
        assert_eq!(effective.identity_revision, Some(7));
    }

    #[test]
    fn missing_bar_edge_projects_top_with_product_default_provenance() {
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            Some(&association(7)),
            Some(&profile_projection(7, None, None, None)),
        )
        .unwrap();

        assert_eq!(effective.bar_edge, Some(BarEdgePreference::Top));
        assert_eq!(
            effective.bar_edge_source,
            Some(PreferenceValueSource::ProductDefault)
        );
    }

    #[test]
    fn follow_system_is_distinct_from_explicit_override() {
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences {
                color_scheme: Some(ProductColorScheme::Light),
                reduced_motion: Some(false),
                text_scale_basis_points: None,
            },
            Some(&association(7)),
            Some(&profile_projection(
                7,
                Some(ColorSchemePreference::FollowSystem),
                Some(ReducedMotionPreference::FollowSystem),
                None,
            )),
        )
        .unwrap();

        assert_eq!(effective.color_scheme, ProductColorScheme::Light);
        assert_eq!(
            effective.color_scheme_source,
            PreferenceValueSource::SystemObservation
        );
        assert!(!effective.reduced_motion);
        assert_eq!(
            effective.reduced_motion_source,
            PreferenceValueSource::SystemObservation
        );
    }

    #[test]
    fn recovery_never_projects_previous_profile_values() {
        let association = association(7);
        let projection = ProfileProjectionOutcome::RecoveryRequired {
            output_hub_id: association.durable_output_hub_id,
            identity_revision: 7,
            issue: ProfileRecoveryIssue {
                current: ProfileCandidateIssue::InvalidJson,
                previous: None,
                initialized_marker: true,
                pending_candidates: Vec::new(),
            },
        };
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences {
                reduced_motion: Some(true),
                ..SystemProductPreferences::default()
            },
            Some(&association),
            Some(&projection),
        )
        .unwrap();

        assert_eq!(
            effective.projection_state,
            ProductPreferenceProjectionState::RecoveryRequired
        );
        assert_eq!(effective.bar_edge, None);
        assert_eq!(effective.bar_edge_source, None);
        assert!(effective.reduced_motion);
        assert_eq!(
            effective.reduced_motion_source,
            PreferenceValueSource::SystemObservation
        );
    }

    #[test]
    fn mismatched_r11_revision_fails_closed() {
        let error = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            Some(&association(8)),
            Some(&profile_projection(7, None, None, None)),
        )
        .unwrap_err();
        assert_eq!(
            error,
            ProductPreferenceResolveError::IdentityRevisionMismatch
        );
    }

    #[test]
    fn profile_without_durable_binding_is_rejected() {
        let projection = profile_projection(7, None, None, None);
        assert_eq!(
            resolve_effective_preferences(
                ProductPreferenceDefaults::default(),
                SystemProductPreferences::default(),
                None,
                Some(&projection),
            ),
            Err(ProductPreferenceResolveError::ProfileWithoutDurableBinding)
        );
    }

    #[test]
    fn authority_unavailable_uses_system_then_defaults_without_claiming_binding() {
        let effective = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences {
                color_scheme: Some(ProductColorScheme::Light),
                reduced_motion: Some(true),
                text_scale_basis_points: Some(12_500),
            },
        );

        assert_eq!(
            effective.projection_state,
            ProductPreferenceProjectionState::AuthorityUnavailable
        );
        assert_eq!(effective.color_scheme, ProductColorScheme::Light);
        assert!(effective.reduced_motion);
        assert_eq!(effective.text_scale_basis_points, 12_500);
        assert_eq!(effective.identity_revision, None);
    }
}

#[cfg(test)]
mod donor11_projection_tests {
    use super::*;
    use in_gnu_os_desktop_profiles::{
        BarPreferenceOverrides, WorkspaceDisplayCount, WorkspaceDisplayMode,
    };

    fn selected_with(overrides: BarPreferenceOverrides) -> ProfileProjectionOutcome {
        let mut selected = super::tests::profile_projection(7, None, None, None);
        let ProfileProjectionOutcome::Selected { profile, .. } = &mut selected else {
            unreachable!();
        };
        profile.preferences.bar.set_overrides(overrides);
        selected
    }

    #[test]
    fn donor_bar_overrides_project_exactly_without_product_defaults() {
        let overrides = BarPreferenceOverrides {
            persistent: Some(false),
            show_on_hover: Some(true),
            workspace_count: Some(WorkspaceDisplayCount::try_from(9_i64).unwrap()),
            workspace_display: Some(WorkspaceDisplayMode::Shapes),
            tray_compact: Some(false),
        };
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            Some(&super::tests::association(7)),
            Some(&selected_with(overrides)),
        )
        .unwrap();
        assert_eq!(effective.bar_overrides, overrides);
    }

    #[test]
    fn absent_donor_overrides_remain_absent_and_recovery_leaks_nothing() {
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            Some(&super::tests::association(7)),
            Some(&selected_with(BarPreferenceOverrides::default())),
        )
        .unwrap();
        assert_eq!(effective.bar_overrides, BarPreferenceOverrides::default());

        let association = super::tests::association(7);
        let recovery = ProfileProjectionOutcome::RecoveryRequired {
            output_hub_id: association.durable_output_hub_id,
            identity_revision: 7,
            issue: in_gnu_os_desktop_profiles::ProfileRecoveryIssue {
                current: in_gnu_os_desktop_profiles::ProfileCandidateIssue::InvalidJson,
                previous: None,
                initialized_marker: true,
                pending_candidates: Vec::new(),
            },
        };
        let effective = resolve_effective_preferences(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
            Some(&association),
            Some(&recovery),
        )
        .unwrap();
        assert_eq!(effective.bar_overrides, BarPreferenceOverrides::default());
        assert_eq!(
            effective.projection_state,
            ProductPreferenceProjectionState::RecoveryRequired
        );
    }
}
