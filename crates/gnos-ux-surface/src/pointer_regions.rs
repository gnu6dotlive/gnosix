use std::{collections::BTreeSet, error::Error, fmt};

use crate::grammar::{
    BarGeometryFrame, DockGeometryFrame, GeometryError, GeometryRect, SurfaceViewport,
};

pub const POINTER_REGION_MODEL_ID: &str = "gnos.ux.pointer-regions";
pub const POINTER_REGION_MODEL_REVISION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PointerRegionId(String);

impl PointerRegionId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, PointerRegionError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PointerRegionError::EmptyId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PointerRegionRole(String);

impl PointerRegionRole {
    pub fn try_new(value: impl Into<String>) -> Result<Self, PointerRegionError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PointerRegionError::EmptyRole);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PointerRegionError {
    Geometry(GeometryError),
    EmptyId,
    EmptyRole,
    NonPositiveArea { id: String },
    DuplicateId { id: String },
    OutOfBounds { id: String },
    Overlap { first: String, second: String },
    InvalidSourceFrame { surface: &'static str },
}

impl fmt::Display for PointerRegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Geometry(error) => write!(f, "invalid pointer-region geometry: {error}"),
            Self::EmptyId => write!(f, "pointer-region id must not be empty"),
            Self::EmptyRole => write!(f, "pointer-region role must not be empty"),
            Self::NonPositiveArea { id } => {
                write!(f, "pointer-region {id} must have positive area")
            }
            Self::DuplicateId { id } => write!(f, "duplicate pointer-region id: {id}"),
            Self::OutOfBounds { id } => {
                write!(f, "pointer-region {id} exceeds the surface viewport")
            }
            Self::Overlap { first, second } => {
                write!(f, "pointer-regions overlap: {first} and {second}")
            }
            Self::InvalidSourceFrame { surface } => {
                write!(
                    f,
                    "invalid {surface} geometry frame for pointer-region projection"
                )
            }
        }
    }
}

impl Error for PointerRegionError {}

impl From<GeometryError> for PointerRegionError {
    fn from(value: GeometryError) -> Self {
        Self::Geometry(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointerRegion {
    id: PointerRegionId,
    role: PointerRegionRole,
    bounds: GeometryRect,
    decoratable: bool,
}

impl PointerRegion {
    pub fn try_new(
        id: PointerRegionId,
        role: PointerRegionRole,
        bounds: GeometryRect,
        decoratable: bool,
    ) -> Result<Self, PointerRegionError> {
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return Err(PointerRegionError::NonPositiveArea {
                id: id.as_str().to_owned(),
            });
        }
        Ok(Self {
            id,
            role,
            bounds,
            decoratable,
        })
    }

    pub fn id(&self) -> &PointerRegionId {
        &self.id
    }

    pub fn role(&self) -> &PointerRegionRole {
        &self.role
    }

    pub fn bounds(&self) -> GeometryRect {
        self.bounds
    }

    pub const fn decoratable(&self) -> bool {
        self.decoratable
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointerRegionSet {
    viewport: SurfaceViewport,
    regions: Vec<PointerRegion>,
}

impl PointerRegionSet {
    pub fn try_new(
        viewport: SurfaceViewport,
        mut regions: Vec<PointerRegion>,
    ) -> Result<Self, PointerRegionError> {
        let viewport = viewport.validate()?;
        let surface_bounds = GeometryRect::try_new(0.0, 0.0, viewport.width, viewport.height, 0.0)?;
        regions.sort_by(|left, right| left.id.cmp(&right.id));

        let mut ids = BTreeSet::new();
        for region in &regions {
            let id = region.id.as_str();
            if !ids.insert(id.to_owned()) {
                return Err(PointerRegionError::DuplicateId { id: id.to_owned() });
            }
            if !surface_bounds.contains(region.bounds) {
                return Err(PointerRegionError::OutOfBounds { id: id.to_owned() });
            }
        }

        for left_index in 0..regions.len() {
            for right_index in (left_index + 1)..regions.len() {
                let left = &regions[left_index];
                let right = &regions[right_index];
                if overlaps_with_area(left.bounds, right.bounds) {
                    return Err(PointerRegionError::Overlap {
                        first: left.id.as_str().to_owned(),
                        second: right.id.as_str().to_owned(),
                    });
                }
            }
        }

        Ok(Self { viewport, regions })
    }

    pub const fn viewport(&self) -> SurfaceViewport {
        self.viewport
    }

    pub fn regions(&self) -> &[PointerRegion] {
        &self.regions
    }

    pub fn surface_local_rects(&self) -> impl Iterator<Item = GeometryRect> + '_ {
        self.regions.iter().map(|region| region.bounds)
    }
}

/// Project exact immutable placement facts; the scene supplies viewport ownership.
pub fn project_resolved_bar_pointer_regions(
    viewport: SurfaceViewport,
    placement: super::grammar::ResolvedBarPlacement,
) -> Result<PointerRegionSet, PointerRegionError> {
    let body = placement.allocation();
    let bounds = if body.radius() > 0.0 {
        crate::grammar::shared_envelope::rounded_input(body)
    } else {
        vec![body]
    };
    let mut regions = bounds
        .into_iter()
        .enumerate()
        .map(|(index, bounds)| {
            PointerRegion::try_new(
                PointerRegionId::try_new(if index == 0 {
                    "bar.input.surface".to_string()
                } else {
                    format!("bar.input.surface.band.{index}")
                })?,
                PointerRegionRole::try_new("surface")?,
                bounds,
                true,
            )
        })
        .collect::<Result<Vec<_>, PointerRegionError>>()?;
    if let Some(bounds) = placement.sensing_region() {
        regions.push(PointerRegion::try_new(
            PointerRegionId::try_new("bar.input.reveal")?,
            PointerRegionRole::try_new("reveal")?,
            bounds,
            false,
        )?);
    }
    PointerRegionSet::try_new(viewport, regions)
}

pub fn project_bar_pointer_regions(
    frame: &BarGeometryFrame,
) -> Result<PointerRegionSet, PointerRegionError> {
    if !frame.validate().is_valid() {
        return Err(PointerRegionError::InvalidSourceFrame { surface: "bar" });
    }

    let region = PointerRegion::try_new(
        PointerRegionId::try_new("bar.input.envelope")?,
        PointerRegionRole::try_new("surface")?,
        frame.envelope,
        true,
    )?;

    PointerRegionSet::try_new(frame.viewport, vec![region])
}

pub fn project_dock_pointer_regions(
    frame: &DockGeometryFrame,
) -> Result<PointerRegionSet, PointerRegionError> {
    if !frame.validate().is_valid() {
        return Err(PointerRegionError::InvalidSourceFrame { surface: "dock" });
    }

    let mut regions = Vec::with_capacity(2);
    if frame.visible_envelope.height() > 0.5 {
        regions.push(PointerRegion::try_new(
            PointerRegionId::try_new("dock.input.visible")?,
            PointerRegionRole::try_new("surface")?,
            frame.visible_envelope,
            true,
        )?);
    }
    regions.push(PointerRegion::try_new(
        PointerRegionId::try_new("dock.input.reveal")?,
        PointerRegionRole::try_new("reveal")?,
        frame.reveal_region,
        false,
    )?);

    PointerRegionSet::try_new(frame.viewport, regions)
}

fn overlaps_with_area(left: GeometryRect, right: GeometryRect) -> bool {
    left.left() < right.right()
        && right.left() < left.right()
        && left.top() < right.bottom()
        && right.top() < left.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        envelope::{LogicalOffset, LogicalPx, OpacityPercent, VisualState},
        grammar::{BarGeometryRecipe, DockGeometryRecipe, SurfaceGeometryRecipe, UnitInterval},
    };

    fn region(id: &str, role: &str, left: f32, top: f32, right: f32, bottom: f32) -> PointerRegion {
        PointerRegion::try_new(
            PointerRegionId::try_new(id).expect("id"),
            PointerRegionRole::try_new(role).expect("role"),
            GeometryRect::try_new(left, top, right, bottom, 0.0).expect("bounds"),
            true,
        )
        .expect("region")
    }

    #[test]
    fn accepts_disjoint_surface_local_declarations() {
        let set = PointerRegionSet::try_new(
            SurfaceViewport::try_new(640.0, 160.0).expect("viewport"),
            vec![
                region("right", "status", 340.0, 20.0, 620.0, 140.0),
                region("left", "action", 20.0, 20.0, 300.0, 140.0),
            ],
        )
        .expect("valid set");

        assert_eq!(set.regions()[0].id().as_str(), "left");
        assert_eq!(set.regions()[1].id().as_str(), "right");
        assert_eq!(set.surface_local_rects().count(), 2);
    }

    #[test]
    fn rejects_duplicate_identity() {
        let result = PointerRegionSet::try_new(
            SurfaceViewport::try_new(100.0, 100.0).expect("viewport"),
            vec![
                region("same", "a", 0.0, 0.0, 40.0, 40.0),
                region("same", "b", 60.0, 60.0, 100.0, 100.0),
            ],
        );
        assert!(matches!(
            result,
            Err(PointerRegionError::DuplicateId { .. })
        ));
    }

    #[test]
    fn rejects_overlap_and_out_of_bounds() {
        let viewport = SurfaceViewport::try_new(100.0, 100.0).expect("viewport");
        let overlap = PointerRegionSet::try_new(
            viewport,
            vec![
                region("a", "action", 0.0, 0.0, 60.0, 60.0),
                region("b", "status", 50.0, 50.0, 100.0, 100.0),
            ],
        );
        assert!(matches!(overlap, Err(PointerRegionError::Overlap { .. })));

        let outside =
            PointerRegionSet::try_new(viewport, vec![region("a", "action", 0.0, 0.0, 101.0, 40.0)]);
        assert!(matches!(
            outside,
            Err(PointerRegionError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn touching_edges_are_not_overlap() {
        let result = PointerRegionSet::try_new(
            SurfaceViewport::try_new(100.0, 50.0).expect("viewport"),
            vec![
                region("a", "action", 0.0, 0.0, 50.0, 50.0),
                region("b", "status", 50.0, 0.0, 100.0, 50.0),
            ],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_empty_identity_role_and_zero_area() {
        assert!(matches!(
            PointerRegionId::try_new("  "),
            Err(PointerRegionError::EmptyId)
        ));
        assert!(matches!(
            PointerRegionRole::try_new(""),
            Err(PointerRegionError::EmptyRole)
        ));

        let zero = PointerRegion::try_new(
            PointerRegionId::try_new("zero").expect("id"),
            PointerRegionRole::try_new("action").expect("role"),
            GeometryRect::try_new(10.0, 10.0, 10.0, 20.0, 0.0).expect("bounds"),
            true,
        );
        assert!(matches!(
            zero,
            Err(PointerRegionError::NonPositiveArea { .. })
        ));
    }

    #[test]
    fn bar_projection_uses_shared_geometry_frame_without_reauthoring_bounds() {
        let viewport = SurfaceViewport::try_new(1200.0, 150.0).expect("viewport");
        let frame = BarGeometryRecipe::qualifier_baseline()
            .resolve(
                viewport,
                VisualState {
                    opacity: OpacityPercent::new(92),
                    radius: LogicalPx(24),
                    margin: LogicalPx(48),
                    translation: LogicalOffset { x: 0, y: 18 },
                    bridge_width: LogicalPx(0),
                    bridge_neck_width: LogicalPx(0),
                },
            )
            .expect("bar frame");
        let set = project_bar_pointer_regions(&frame).expect("projection");

        assert_eq!(set.viewport(), viewport);
        assert_eq!(set.regions().len(), 1);
        assert_eq!(set.regions()[0].id().as_str(), "bar.input.envelope");
        assert_eq!(set.regions()[0].role().as_str(), "surface");
        assert_eq!(set.regions()[0].bounds(), frame.envelope);
        assert!(set.regions()[0].decoratable());
    }

    #[test]
    fn dock_projection_removes_visible_region_when_autohide_reaches_hidden() {
        let viewport = SurfaceViewport::try_new(1200.0, 720.0).expect("viewport");
        let recipe = DockGeometryRecipe::atlas_baseline();
        let visible = recipe
            .resolve(viewport, UnitInterval::ONE)
            .expect("visible frame");
        let hidden = recipe
            .resolve(viewport, UnitInterval::ZERO)
            .expect("hidden frame");

        let visible_set = project_dock_pointer_regions(&visible).expect("visible projection");
        let hidden_set = project_dock_pointer_regions(&hidden).expect("hidden projection");

        assert_eq!(visible_set.regions().len(), 2);
        assert_eq!(hidden_set.regions().len(), 1);
        assert_eq!(hidden_set.regions()[0].id().as_str(), "dock.input.reveal");
        assert!(!hidden_set.regions()[0].decoratable());
        assert_eq!(hidden_set.regions()[0].bounds(), hidden.reveal_region);
    }

    #[test]
    fn projections_recompute_from_each_viewport_without_stale_geometry() {
        let recipe = DockGeometryRecipe::atlas_baseline();
        let wide = recipe
            .resolve(
                SurfaceViewport::try_new(1200.0, 720.0).expect("wide viewport"),
                UnitInterval::ONE,
            )
            .expect("wide frame");
        let narrow = recipe
            .resolve(
                SurfaceViewport::try_new(900.0, 560.0).expect("narrow viewport"),
                UnitInterval::ONE,
            )
            .expect("narrow frame");

        let wide_set = project_dock_pointer_regions(&wide).expect("wide projection");
        let narrow_set = project_dock_pointer_regions(&narrow).expect("narrow projection");

        assert_eq!(wide_set.viewport(), wide.viewport);
        assert_eq!(narrow_set.viewport(), narrow.viewport);
        assert_ne!(
            wide_set.regions()[0].bounds(),
            narrow_set.regions()[0].bounds()
        );
        assert_eq!(
            narrow_set
                .regions()
                .iter()
                .find(|region| region.id().as_str() == "dock.input.visible")
                .expect("visible region")
                .bounds(),
            narrow.visible_envelope
        );
    }

    #[test]
    fn invalid_source_frames_are_rejected_before_projection() {
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");
        let mut frame = DockGeometryRecipe::atlas_baseline()
            .resolve(viewport, UnitInterval::ONE)
            .expect("frame");
        frame.input_regions.clear();

        assert!(matches!(
            project_dock_pointer_regions(&frame),
            Err(PointerRegionError::InvalidSourceFrame { surface: "dock" })
        ));
    }
}
