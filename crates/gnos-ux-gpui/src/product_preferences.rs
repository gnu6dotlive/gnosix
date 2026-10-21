//! GNOS.UX view of the neutral in.gnu.os effective-preference read model.
//!
//! The merge/cross-validation policy lives below GPUI so the future Settings
//! Authority and non-UI consumers can share the exact same semantics.

pub use in_gnu_os_settings_read_model::{
    BarEdgePreference, EffectiveProductPreferences, PreferenceValueSource, ProductColorScheme,
    ProductPreferenceDefaults, ProductPreferenceProjectionState, ProductPreferenceReadError,
    ProductPreferenceResolveError, SystemProductPreferences, read_effective_preferences,
    resolve_effective_preferences, resolve_without_profile_authority, session_output_observation,
};

/// Map validated durable intent onto the already-qualified bounded placement
/// domain. Recovery/unavailable projections intentionally return no target.
pub const fn bar_surface_edge(
    preferences: &EffectiveProductPreferences,
) -> Option<gnos_ux_surface::envelope::SurfaceEdge> {
    use gnos_ux_surface::envelope::SurfaceEdge;
    match preferences.bar_edge {
        Some(BarEdgePreference::Top) => Some(SurfaceEdge::Top),
        Some(BarEdgePreference::Right) => Some(SurfaceEdge::Right),
        Some(BarEdgePreference::Bottom) => Some(SurfaceEdge::Bottom),
        Some(BarEdgePreference::Left) => Some(SurfaceEdge::Left),
        None => None,
    }
}

/// TOPO-01C: the edge the native rail takes. The product default edge is
/// the top, but the native rail's own default is the left edge (the
/// reference rail): only a durable profile choice moves it.
pub fn native_rail_edge(
    preferences: &EffectiveProductPreferences,
) -> Option<gnos_ux_surface::envelope::SurfaceEdge> {
    match preferences.bar_edge_source {
        Some(PreferenceValueSource::ProductDefault) => {
            Some(gnos_ux_surface::envelope::SurfaceEdge::Left)
        }
        _ => bar_surface_edge(preferences),
    }
}

/// Reapply the validated durable Bar projection after system-only preferences refresh.
/// Portal observations own color/motion/text inputs; they must not erase R12-B state.
pub fn preserve_durable_bar_projection(
    mut refreshed: EffectiveProductPreferences,
    current: &EffectiveProductPreferences,
) -> EffectiveProductPreferences {
    refreshed.bar_edge = current.bar_edge;
    refreshed.bar_edge_source = current.bar_edge_source;
    refreshed.bar_overrides = current.bar_overrides.clone();
    refreshed.profile_revision = current.profile_revision;
    refreshed.identity_revision = current.identity_revision;
    refreshed.projection_state = current.projection_state;
    refreshed
}

/// Adapt the neutral effective projection to the typed, read-only Settings
/// control model. No write authority is supplied by this bridge.
pub const fn bar_edge_settings_projection(
    preferences: &EffectiveProductPreferences,
) -> in_gnu_os_settings_core::BarEdgeSettingProjection {
    use in_gnu_os_settings_core::{BarEdgeSettingProjection, BarEdgeSettingProvenance};
    let provenance = match preferences.bar_edge_source {
        Some(PreferenceValueSource::ProductDefault) => Some(BarEdgeSettingProvenance::Default),
        Some(PreferenceValueSource::DurableProfile) => Some(BarEdgeSettingProvenance::Explicit),
        Some(PreferenceValueSource::SystemObservation) | None => None,
    };
    BarEdgeSettingProjection {
        effective_edge: preferences.bar_edge,
        provenance,
        profile_revision: preferences.profile_revision,
        identity_revision: preferences.identity_revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_rail_keeps_the_left_edge_until_a_durable_choice_moves_it() {
        use gnos_ux_surface::envelope::SurfaceEdge;
        let mut preferences = EffectiveProductPreferences {
            bar_edge: Some(BarEdgePreference::Top),
            bar_edge_source: Some(PreferenceValueSource::ProductDefault),
            ..EffectiveProductPreferences::default()
        };
        assert_eq!(native_rail_edge(&preferences), Some(SurfaceEdge::Left));
        assert_eq!(bar_surface_edge(&preferences), Some(SurfaceEdge::Top));
        preferences.bar_edge_source = Some(PreferenceValueSource::DurableProfile);
        assert_eq!(native_rail_edge(&preferences), Some(SurfaceEdge::Top));
        preferences.bar_edge = None;
        preferences.bar_edge_source = None;
        assert_eq!(native_rail_edge(&preferences), None);
    }

    #[test]
    fn durable_bar_edges_map_to_the_existing_surface_edge_domain() {
        for (preference, edge) in [
            (
                BarEdgePreference::Top,
                gnos_ux_surface::envelope::SurfaceEdge::Top,
            ),
            (
                BarEdgePreference::Right,
                gnos_ux_surface::envelope::SurfaceEdge::Right,
            ),
            (
                BarEdgePreference::Bottom,
                gnos_ux_surface::envelope::SurfaceEdge::Bottom,
            ),
            (
                BarEdgePreference::Left,
                gnos_ux_surface::envelope::SurfaceEdge::Left,
            ),
        ] {
            let mut effective = resolve_without_profile_authority(
                ProductPreferenceDefaults::default(),
                SystemProductPreferences::default(),
            );
            effective.bar_edge = Some(preference);
            assert_eq!(bar_surface_edge(&effective), Some(edge));
        }
    }

    #[test]
    fn projected_bar_edges_select_the_existing_product_layout_resolver() {
        let viewport = gnos_ux_surface::grammar::SurfaceViewport::try_new(1200., 800.).unwrap();
        for preference in [
            BarEdgePreference::Top,
            BarEdgePreference::Right,
            BarEdgePreference::Bottom,
            BarEdgePreference::Left,
        ] {
            let mut effective = resolve_without_profile_authority(
                ProductPreferenceDefaults::default(),
                SystemProductPreferences::default(),
            );
            effective.bar_edge = Some(preference);
            let edge = bar_surface_edge(&effective).expect("validated preference");
            let layout = crate::product_scene::ProductLayout::resolve_reactive_at_edge(
                viewport, &mut None, false, true, None, edge, 14.,
            )
            .expect("existing product resolver accepts every supported edge");
            assert_eq!(layout.bar_placement.edge(), edge);
        }
    }

    #[test]
    fn recovery_projection_does_not_create_a_bar_placement_target() {
        let mut effective = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
        );
        effective.bar_edge = None;
        effective.bar_edge_source = None;
        assert_eq!(bar_surface_edge(&effective), None);
        assert_eq!(bar_edge_settings_projection(&effective).provenance, None);
    }

    #[test]
    fn settings_projection_preserves_effective_edge_provenance_and_revisions() {
        let mut effective = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
        );
        effective.bar_edge = Some(BarEdgePreference::Right);
        effective.bar_edge_source = Some(PreferenceValueSource::DurableProfile);
        effective.profile_revision = Some(9);
        effective.identity_revision = Some(4);
        assert_eq!(
            bar_edge_settings_projection(&effective),
            in_gnu_os_settings_core::BarEdgeSettingProjection {
                effective_edge: Some(BarEdgePreference::Right),
                provenance: Some(in_gnu_os_settings_core::BarEdgeSettingProvenance::Explicit),
                profile_revision: Some(9),
                identity_revision: Some(4),
            }
        );
    }
    #[test]
    fn system_refresh_preserves_the_complete_durable_bar_projection() {
        let mut current = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
        );
        current.bar_edge = Some(BarEdgePreference::Left);
        current.bar_edge_source = Some(PreferenceValueSource::DurableProfile);
        current.bar_overrides.persistent = Some(false);
        current.bar_overrides.show_on_hover = Some(true);
        current.bar_overrides.tray_compact = Some(false);
        current.profile_revision = Some(17);
        current.identity_revision = Some(8);

        let refreshed = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences {
                color_scheme: Some(ProductColorScheme::Light),
                reduced_motion: Some(true),
                text_scale_basis_points: None,
            },
        );
        let merged = preserve_durable_bar_projection(refreshed, &current);

        assert_eq!(merged.color_scheme, ProductColorScheme::Light);
        assert!(merged.reduced_motion);
        assert_eq!(merged.bar_edge, current.bar_edge);
        assert_eq!(merged.bar_edge_source, current.bar_edge_source);
        assert_eq!(merged.bar_overrides, current.bar_overrides);
        assert_eq!(merged.profile_revision, Some(17));
        assert_eq!(merged.identity_revision, Some(8));
    }
}
