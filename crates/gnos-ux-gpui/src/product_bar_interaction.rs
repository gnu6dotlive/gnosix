use crate::product_preferences::EffectiveProductPreferences;
use gnos_ux_surface::{
    grammar::{Point2, ResolvedBarPlacement, SurfaceViewport},
    pointer_regions::project_resolved_bar_pointer_regions,
    scene::{SceneFrame, SceneReservationIntent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarRevealMode {
    AlwaysVisible,
    HoverReveal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarInteractionPolicy {
    mode: BarRevealMode,
}

impl BarInteractionPolicy {
    pub const fn always_visible() -> Self {
        Self {
            mode: BarRevealMode::AlwaysVisible,
        }
    }

    pub const fn hover_reveal() -> Self {
        Self {
            mode: BarRevealMode::HoverReveal,
        }
    }

    pub fn from_preferences(preferences: &EffectiveProductPreferences) -> Self {
        if preferences.bar_overrides.persistent == Some(false)
            && preferences.bar_overrides.show_on_hover == Some(true)
        {
            Self::hover_reveal()
        } else {
            Self::always_visible()
        }
    }

    pub const fn mode(self) -> BarRevealMode {
        self.mode
    }

    pub const fn initial_state(self) -> BarRevealState {
        match self.mode {
            BarRevealMode::AlwaysVisible => BarRevealState::Visible,
            BarRevealMode::HoverReveal => BarRevealState::Hidden,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarRevealState {
    Hidden,
    Visible,
}

impl BarRevealState {
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarPointerZone {
    Outside,
    Reveal,
    Surface,
}

pub const fn next_state(
    policy: BarInteractionPolicy,
    current: BarRevealState,
    pointer_zone: BarPointerZone,
    panel_open: bool,
) -> BarRevealState {
    match policy.mode {
        BarRevealMode::AlwaysVisible => BarRevealState::Visible,
        BarRevealMode::HoverReveal => {
            if panel_open
                || matches!(pointer_zone, BarPointerZone::Reveal)
                || (current.is_visible() && matches!(pointer_zone, BarPointerZone::Surface))
            {
                BarRevealState::Visible
            } else {
                BarRevealState::Hidden
            }
        }
    }
}

pub fn pointer_zone(
    viewport: SurfaceViewport,
    placement: ResolvedBarPlacement,
    painted_state: BarRevealState,
    pointer: Point2,
) -> BarPointerZone {
    let Ok(regions) = project_resolved_bar_pointer_regions(viewport, placement) else {
        return BarPointerZone::Outside;
    };
    // The retained semantic allocation is not input while its Bar is unpainted.
    // Other surfaces (notably the bottom Dock) may occupy those same bounds.
    if painted_state.is_visible()
        && regions.regions().iter().any(|region| {
            region.role().as_str() == "surface"
                && region.bounds().contains_point(pointer.x(), pointer.y())
        })
    {
        return BarPointerZone::Surface;
    }
    if regions.regions().iter().any(|region| {
        region.role().as_str() == "reveal"
            && region.bounds().contains_point(pointer.x(), pointer.y())
    }) {
        BarPointerZone::Reveal
    } else {
        BarPointerZone::Outside
    }
}

pub fn project_frame(
    mut frame: SceneFrame,
    viewport: SurfaceViewport,
    placement: ResolvedBarPlacement,
    state: BarRevealState,
) -> SceneFrame {
    if state.is_visible() {
        return frame;
    }
    if let Ok(regions) = project_resolved_bar_pointer_regions(viewport, placement) {
        let surface_regions = regions
            .regions()
            .iter()
            .filter(|region| region.role().as_str() == "surface")
            .map(|region| region.bounds())
            .collect::<Vec<_>>();
        frame
            .input_regions
            .retain(|input| !surface_regions.iter().any(|surface| surface == input));
    }
    frame.reservation = SceneReservationIntent::NONE;
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_preferences::{
        ProductPreferenceDefaults, SystemProductPreferences, resolve_without_profile_authority,
    };
    use gnos_ux_surface::grammar::GeometryRect;

    fn center(rect: GeometryRect) -> Point2 {
        Point2::new(
            (rect.left() + rect.right()) * 0.5,
            (rect.top() + rect.bottom()) * 0.5,
        )
    }

    fn sensor_center(rect: GeometryRect) -> Point2 {
        center(rect)
    }
    fn body_center(rect: GeometryRect) -> Point2 {
        center(rect)
    }

    fn rectangles_overlap(a: GeometryRect, b: GeometryRect) -> bool {
        a.left() < b.right() && a.right() > b.left() && a.top() < b.bottom() && a.bottom() > b.top()
    }

    fn preferences(
        persistent: Option<bool>,
        show_on_hover: Option<bool>,
    ) -> crate::product_preferences::EffectiveProductPreferences {
        let mut preferences = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
        );
        preferences.bar_overrides.persistent = persistent;
        preferences.bar_overrides.show_on_hover = show_on_hover;
        preferences
    }

    #[test]
    fn hidden_bar_surface_cannot_reveal_without_edge_sensor() {
        assert_eq!(
            next_state(
                BarInteractionPolicy::hover_reveal(),
                BarRevealState::Hidden,
                BarPointerZone::Surface,
                false,
            ),
            BarRevealState::Hidden,
            "an unpainted Bar body is not a reveal sensor",
        );
    }

    #[test]
    fn hidden_bottom_dock_hover_does_not_reveal_or_displace_dock() {
        use gnos_ux_surface::{
            envelope::SurfaceEdge,
            grammar::{BarReservationPolicy, BarSensingPolicy},
        };
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let layout = crate::product_scene::ProductLayout::resolve_reactive_at_edge_with_policies(
            viewport,
            &mut solver,
            false,
            true,
            None,
            SurfaceEdge::Bottom,
            14.0,
            BarReservationPolicy::Overlay,
            BarSensingPolicy::EdgeStrip { thickness: 16.0 },
        )
        .unwrap();
        let dock_pointer = center(layout.dock);
        assert!(
            layout
                .bar
                .contains_point(dock_pointer.x(), dock_pointer.y())
        );
        let zone = pointer_zone(
            viewport,
            layout.bar_placement,
            BarRevealState::Hidden,
            dock_pointer,
        );
        assert_eq!(zone, BarPointerZone::Outside);
        assert_eq!(
            next_state(
                BarInteractionPolicy::hover_reveal(),
                BarRevealState::Hidden,
                zone,
                false
            ),
            BarRevealState::Hidden,
            "moving horizontally onto the hidden-state Dock must not reveal the overlapping invisible Bar",
        );
    }

    #[test]
    fn absent_or_incomplete_overrides_preserve_the_existing_visible_bar() {
        for (persistent, show_on_hover) in [
            (None, None),
            (Some(false), None),
            (Some(false), Some(false)),
            (Some(true), Some(true)),
            (Some(true), Some(false)),
        ] {
            let policy =
                BarInteractionPolicy::from_preferences(&preferences(persistent, show_on_hover));
            assert_eq!(policy.mode(), BarRevealMode::AlwaysVisible);
            assert_eq!(policy.initial_state(), BarRevealState::Visible);
        }
    }

    #[test]
    fn explicit_nonpersistent_plus_hover_enables_bounded_auto_hide() {
        let policy = BarInteractionPolicy::from_preferences(&preferences(Some(false), Some(true)));
        assert_eq!(policy.mode(), BarRevealMode::HoverReveal);
        assert_eq!(policy.initial_state(), BarRevealState::Hidden);
    }

    #[test]
    fn edge_sensor_reveals_and_painted_surface_holds_the_auto_hidden_bar() {
        let policy = BarInteractionPolicy::hover_reveal();
        assert_eq!(
            next_state(
                policy,
                BarRevealState::Hidden,
                BarPointerZone::Reveal,
                false
            ),
            BarRevealState::Visible
        );
        assert_eq!(
            next_state(
                policy,
                BarRevealState::Hidden,
                BarPointerZone::Surface,
                false
            ),
            BarRevealState::Hidden
        );
        assert_eq!(
            next_state(
                policy,
                BarRevealState::Visible,
                BarPointerZone::Surface,
                false
            ),
            BarRevealState::Visible
        );
    }

    #[test]
    fn leaving_hides_only_when_no_product_panel_holds_the_bar_open() {
        let policy = BarInteractionPolicy::hover_reveal();
        assert_eq!(
            next_state(
                policy,
                BarRevealState::Visible,
                BarPointerZone::Outside,
                false
            ),
            BarRevealState::Hidden
        );
        assert_eq!(
            next_state(
                policy,
                BarRevealState::Visible,
                BarPointerZone::Outside,
                true
            ),
            BarRevealState::Visible
        );
    }

    #[test]
    fn always_visible_policy_ignores_pointer_departure() {
        let policy = BarInteractionPolicy::always_visible();
        assert_eq!(
            next_state(
                policy,
                BarRevealState::Hidden,
                BarPointerZone::Outside,
                false
            ),
            BarRevealState::Visible
        );
    }
    #[test]
    fn hover_reveal_projects_a_non_overlapping_sensor_on_every_cardinal_edge() {
        use gnos_ux_surface::{
            envelope::SurfaceEdge,
            grammar::{BarSensingPolicy, SurfaceViewport},
        };
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        for edge in [
            SurfaceEdge::Top,
            SurfaceEdge::Right,
            SurfaceEdge::Bottom,
            SurfaceEdge::Left,
        ] {
            let placement = crate::product_bar::presentation_placement_with_sensing(
                viewport,
                edge,
                14.0,
                BarSensingPolicy::EdgeStrip { thickness: 16.0 },
            )
            .unwrap()
            .resolve(viewport)
            .unwrap();
            let sensor = placement.sensing_region().expect("hover reveal sensor");
            let body = placement.allocation();
            assert_eq!(
                pointer_zone(
                    viewport,
                    placement,
                    BarRevealState::Hidden,
                    sensor_center(sensor)
                ),
                BarPointerZone::Reveal
            );
            assert_eq!(
                pointer_zone(
                    viewport,
                    placement,
                    BarRevealState::Visible,
                    body_center(body)
                ),
                BarPointerZone::Surface
            );
            assert!(!rectangles_overlap(sensor, body));
            assert_eq!(
                pointer_zone(
                    viewport,
                    placement,
                    BarRevealState::Hidden,
                    body_center(body)
                ),
                BarPointerZone::Outside,
            );
            match edge {
                SurfaceEdge::Top => assert_eq!(sensor.top(), 0.0),
                SurfaceEdge::Right => assert_eq!(sensor.right(), viewport.width),
                SurfaceEdge::Bottom => assert_eq!(sensor.bottom(), viewport.height),
                SurfaceEdge::Left => assert_eq!(sensor.left(), 0.0),
            }
        }
    }

    #[test]
    fn hidden_frame_keeps_bar_identity_and_reveal_input_but_drops_surface_input_and_reservation() {
        use gnos_ux_surface::{
            envelope::SurfaceEdge,
            grammar::{BarSensingPolicy, SurfaceViewport},
            scene::{MonitorSceneId, SceneEpoch, SceneNodeId, SceneReservationIntent},
        };
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let layout = crate::product_scene::ProductLayout::resolve_reactive_at_edge_with_sensing(
            viewport,
            &mut solver,
            false,
            true,
            None,
            SurfaceEdge::Top,
            14.0,
            BarSensingPolicy::EdgeStrip { thickness: 16.0 },
        )
        .unwrap();
        let frame = layout.frame(MonitorSceneId::new(9), SceneEpoch::new(3));
        let projected = project_frame(
            frame,
            viewport,
            layout.bar_placement,
            BarRevealState::Hidden,
        );
        let regions = gnos_ux_surface::pointer_regions::project_resolved_bar_pointer_regions(
            viewport,
            layout.bar_placement,
        )
        .unwrap();
        let reveal = regions
            .regions()
            .iter()
            .find(|r| r.role().as_str() == "reveal")
            .unwrap()
            .bounds();
        let surfaces = regions
            .regions()
            .iter()
            .filter(|r| r.role().as_str() == "surface")
            .map(|r| r.bounds())
            .collect::<Vec<_>>();
        assert!(projected.node(SceneNodeId::BAR).is_some());
        assert!(projected.input_regions.contains(&reveal));
        assert!(
            surfaces
                .iter()
                .all(|surface| !projected.input_regions.contains(surface))
        );
        assert_eq!(projected.reservation, SceneReservationIntent::NONE);
        assert!(projected.validate().is_valid());
    }

    #[test]
    fn visible_frame_preserves_surface_input_and_full_reservation() {
        use gnos_ux_surface::{
            envelope::SurfaceEdge,
            grammar::{BarSensingPolicy, SurfaceViewport},
            scene::{MonitorSceneId, SceneEpoch},
        };
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let layout = crate::product_scene::ProductLayout::resolve_reactive_at_edge_with_sensing(
            viewport,
            &mut solver,
            false,
            true,
            None,
            SurfaceEdge::Left,
            14.0,
            BarSensingPolicy::EdgeStrip { thickness: 16.0 },
        )
        .unwrap();
        let frame = layout.frame(MonitorSceneId::new(9), SceneEpoch::new(4));
        let expected_input = frame.input_regions.clone();
        let expected_reservation = frame.reservation;
        let projected = project_frame(
            frame,
            viewport,
            layout.bar_placement,
            BarRevealState::Visible,
        );
        assert_eq!(projected.input_regions, expected_input);
        assert_eq!(projected.reservation, expected_reservation);
    }
}
