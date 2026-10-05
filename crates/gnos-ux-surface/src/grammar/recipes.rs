use crate::envelope::VisualState;

use super::{
    BarGeometryFrame, DockGeometryFrame, GeometryError, GeometryRect, RecipeIdentity,
    ResponsiveLength, SurfaceViewport, UnitInterval,
    molecules::{
        envelope_only_input, reveal_strip_below, visible_from_bottom, visible_plus_reveal_input,
    },
};

pub trait SurfaceGeometryRecipe<State> {
    type Frame;

    fn identity(&self) -> RecipeIdentity;

    fn resolve(
        &self,
        viewport: SurfaceViewport,
        state: State,
    ) -> Result<Self::Frame, GeometryError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectRecipe {
    pub left: ResponsiveLength,
    pub top: ResponsiveLength,
    pub right: ResponsiveLength,
    pub bottom: ResponsiveLength,
    pub radius: ResponsiveLength,
}

impl RectRecipe {
    pub fn resolve(self, viewport: SurfaceViewport) -> Result<GeometryRect, GeometryError> {
        GeometryRect::try_new(
            self.left.resolve(viewport)?,
            self.top.resolve(viewport)?,
            self.right.resolve(viewport)?,
            self.bottom.resolve(viewport)?,
            self.radius.resolve(viewport)?,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockGeometryRecipe {
    pub envelope: RectRecipe,
    pub reveal_height: ResponsiveLength,
    pub minimum_visible_input_extent: f32,
}

impl DockGeometryRecipe {
    pub const ID: &'static str = "gnos.ux.recipe.dock.atlas-baseline";
    pub const REVISION: u32 = 1;

    pub const fn atlas_baseline() -> Self {
        Self {
            envelope: RectRecipe {
                left: ResponsiveLength::width(1.0 / 30.0),
                top: ResponsiveLength::height(0.70),
                right: ResponsiveLength::width(29.0 / 30.0),
                bottom: ResponsiveLength::height(31.0 / 35.0),
                radius: ResponsiveLength::min_axis(2.0 / 35.0),
            },
            reveal_height: ResponsiveLength::height(1.0 / 105.0).with_min(1.0),
            minimum_visible_input_extent: 0.5,
        }
    }
}

impl Default for DockGeometryRecipe {
    fn default() -> Self {
        Self::atlas_baseline()
    }
}

impl SurfaceGeometryRecipe<UnitInterval> for DockGeometryRecipe {
    type Frame = DockGeometryFrame;

    fn identity(&self) -> RecipeIdentity {
        RecipeIdentity {
            id: Self::ID,
            revision: Self::REVISION,
        }
    }

    fn resolve(
        &self,
        viewport: SurfaceViewport,
        visible_fraction: UnitInterval,
    ) -> Result<Self::Frame, GeometryError> {
        viewport.validate()?;
        if !self.minimum_visible_input_extent.is_finite() {
            return Err(GeometryError::NonFinite(
                "dock.minimum_visible_input_extent",
            ));
        }
        if self.minimum_visible_input_extent < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "dock.minimum_visible_input_extent",
                value: self.minimum_visible_input_extent,
                requirement: ">= 0",
            });
        }
        let envelope = self.envelope.resolve(viewport)?;
        let visible_envelope = visible_from_bottom(envelope, visible_fraction)?;
        let reveal_region = reveal_strip_below(envelope, viewport, self.reveal_height)?;
        let input_regions = visible_plus_reveal_input(
            visible_envelope,
            reveal_region,
            self.minimum_visible_input_extent,
        )?;

        let frame = DockGeometryFrame {
            recipe: self.identity(),
            viewport,
            envelope,
            visible_envelope,
            reveal_region,
            input_regions,
            visible_fraction,
        };
        debug_assert!(frame.validate().is_valid());
        Ok(frame)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarGeometryRecipe {
    pub base_height: f32,
    pub radius_height_divisor: f32,
    pub maximum_margin_fraction: f32,
}

impl BarGeometryRecipe {
    pub const ID: &'static str = "gnos.ux.recipe.bar.qualifier-baseline";
    pub const REVISION: u32 = 1;

    pub const fn qualifier_baseline() -> Self {
        Self {
            base_height: 44.0,
            radius_height_divisor: 6.0,
            maximum_margin_fraction: 0.20,
        }
    }
}

impl Default for BarGeometryRecipe {
    fn default() -> Self {
        Self::qualifier_baseline()
    }
}

impl SurfaceGeometryRecipe<VisualState> for BarGeometryRecipe {
    type Frame = BarGeometryFrame;

    fn identity(&self) -> RecipeIdentity {
        RecipeIdentity {
            id: Self::ID,
            revision: Self::REVISION,
        }
    }

    fn resolve(
        &self,
        viewport: SurfaceViewport,
        visual: VisualState,
    ) -> Result<Self::Frame, GeometryError> {
        viewport.validate()?;

        for (name, value) in [
            ("bar.base_height", self.base_height),
            ("bar.radius_height_divisor", self.radius_height_divisor),
            ("bar.maximum_margin_fraction", self.maximum_margin_fraction),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if self.base_height <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "bar.base_height",
                value: self.base_height,
                requirement: "> 0",
            });
        }
        if self.radius_height_divisor <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "bar.radius_height_divisor",
                value: self.radius_height_divisor,
                requirement: "> 0",
            });
        }
        if self.maximum_margin_fraction < 0.0 || self.maximum_margin_fraction > 0.5 {
            return Err(GeometryError::InvalidParameter {
                name: "bar.maximum_margin_fraction",
                value: self.maximum_margin_fraction,
                requirement: "0..=0.5",
            });
        }

        let margin = (visual.margin.0 as f32).min(viewport.width * self.maximum_margin_fraction);
        let radius_requested = visual.radius.0 as f32;
        let height_delta = radius_requested / self.radius_height_divisor;
        let height = (self.base_height + height_delta)
            .min(viewport.height)
            .max(0.0);
        let width = (viewport.width - margin * 2.0).max(1.0);
        let y = (visual.translation.y.max(0) as f32).min((viewport.height - height).max(0.0));
        let placement = super::BarPlacement {
            edge: crate::envelope::SurfaceEdge::Top,
            along_edge_alignment: super::AlongEdgeAlignment::Center,
            thickness: height,
            span: width,
            inset: y,
            reservation_policy: super::BarReservationPolicy::ReserveOccupiedExtent,
            sensing_policy: super::BarSensingPolicy::None,
        }
        .resolve(viewport)?
        .with_radius(radius_requested)?;
        let envelope = placement.allocation();
        let input_regions = envelope_only_input(envelope);

        let frame = BarGeometryFrame {
            placement,
            recipe: self.identity(),
            viewport,
            envelope,
            input_regions,
        };
        debug_assert!(frame.validate().is_valid());
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use crate::envelope::{LogicalOffset, LogicalPx, OpacityPercent};

    use super::*;

    fn standard_visual() -> VisualState {
        VisualState {
            opacity: OpacityPercent::new(92),
            radius: LogicalPx(0),
            margin: LogicalPx(0),
            translation: LogicalOffset { x: 0, y: 0 },
            bridge_width: LogicalPx(0),
            bridge_neck_width: LogicalPx(0),
        }
    }

    fn hug_visual() -> VisualState {
        VisualState {
            opacity: OpacityPercent::new(92),
            radius: LogicalPx(24),
            margin: LogicalPx(48),
            translation: LogicalOffset { x: 0, y: 18 },
            bridge_width: LogicalPx(0),
            bridge_neck_width: LogicalPx(0),
        }
    }

    #[test]
    fn dock_recipe_matches_atlas_2_baseline_at_both_proven_viewports() {
        let recipe = DockGeometryRecipe::atlas_baseline();

        let wide = recipe
            .resolve(
                SurfaceViewport::try_new(1200.0, 720.0).expect("viewport"),
                UnitInterval::ONE,
            )
            .expect("wide");
        assert!((wide.envelope.left() - 40.0).abs() < 0.001);
        assert!((wide.envelope.top() - 504.0).abs() < 0.001);
        assert!((wide.envelope.right() - 1160.0).abs() < 0.001);
        assert!((wide.envelope.bottom() - 637.7143).abs() < 0.01);
        assert!((wide.envelope.radius() - 41.142857).abs() < 0.01);

        let narrow = recipe
            .resolve(
                SurfaceViewport::try_new(900.0, 560.0).expect("viewport"),
                UnitInterval::ONE,
            )
            .expect("narrow");
        assert!((narrow.envelope.left() - 30.0).abs() < 0.001);
        assert!((narrow.envelope.top() - 392.0).abs() < 0.001);
        assert!((narrow.envelope.right() - 870.0).abs() < 0.001);
        assert!((narrow.envelope.bottom() - 496.0).abs() < 0.001);
        assert!((narrow.envelope.radius() - 32.0).abs() < 0.001);
    }

    #[test]
    fn bar_recipe_matches_existing_standard_and_hug_baselines() {
        let recipe = BarGeometryRecipe::qualifier_baseline();
        let viewport = SurfaceViewport::try_new(1200.0, 720.0).expect("viewport");

        let standard = recipe
            .resolve(viewport, standard_visual())
            .expect("standard");
        assert_eq!(standard.envelope.left(), 0.0);
        assert_eq!(standard.envelope.top(), 0.0);
        assert_eq!(standard.envelope.width(), 1200.0);
        assert_eq!(standard.envelope.height(), 44.0);
        assert_eq!(standard.envelope.radius(), 0.0);

        let hug = recipe.resolve(viewport, hug_visual()).expect("hug");
        assert_eq!(hug.envelope.left(), 48.0);
        assert_eq!(hug.envelope.top(), 18.0);
        assert_eq!(hug.envelope.width(), 1104.0);
        assert_eq!(hug.envelope.height(), 48.0);
        assert_eq!(hug.envelope.radius(), 24.0);
    }

    #[test]
    fn invalid_recipe_parameters_fail_fast() {
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");

        let invalid_bar = BarGeometryRecipe {
            base_height: 44.0,
            radius_height_divisor: 0.0,
            maximum_margin_fraction: 0.20,
        };
        assert!(matches!(
            invalid_bar.resolve(viewport, hug_visual()),
            Err(GeometryError::InvalidParameter {
                name: "bar.radius_height_divisor",
                ..
            })
        ));

        let invalid_margin = BarGeometryRecipe {
            base_height: 44.0,
            radius_height_divisor: 6.0,
            maximum_margin_fraction: 0.75,
        };
        assert!(matches!(
            invalid_margin.resolve(viewport, standard_visual()),
            Err(GeometryError::InvalidParameter {
                name: "bar.maximum_margin_fraction",
                ..
            })
        ));

        let invalid_dock = DockGeometryRecipe {
            minimum_visible_input_extent: -1.0,
            ..DockGeometryRecipe::atlas_baseline()
        };
        assert!(matches!(
            invalid_dock.resolve(viewport, UnitInterval::ONE),
            Err(GeometryError::InvalidParameter {
                name: "dock.minimum_visible_input_extent",
                ..
            })
        ));
    }

    #[test]
    fn recipes_are_deterministic_for_identical_inputs() {
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");
        let dock = DockGeometryRecipe::default();
        let a = dock
            .resolve(viewport, UnitInterval::try_new(0.5).expect("fraction"))
            .expect("a");
        let b = dock
            .resolve(viewport, UnitInterval::try_new(0.5).expect("fraction"))
            .expect("b");
        assert_eq!(a, b);

        let bar = BarGeometryRecipe::default();
        assert_eq!(
            bar.resolve(viewport, hug_visual()).expect("bar a"),
            bar.resolve(viewport, hug_visual()).expect("bar b")
        );
    }
}
