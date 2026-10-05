//! Bounded cardinal Bar endpoints. Placement is geometry intent, not persistence.
use super::{GeometryError, GeometryRect, SurfaceViewport, Vector2};
use crate::{envelope::SurfaceEdge, scene::SceneReservationIntent};

#[derive(Clone, Copy, Debug, PartialEq)]
/// Signed geometric basis; tangent sign never determines semantic child order.
pub struct EdgeBasis {
    pub edge: SurfaceEdge,
    pub tangent: Vector2,
    pub inward_normal: Vector2,
}
impl EdgeBasis {
    pub fn for_edge(edge: SurfaceEdge) -> Self {
        let (tangent, inward) = match edge {
            SurfaceEdge::Top => ((1., 0.), (0., 1.)),
            SurfaceEdge::Right => ((0., 1.), (-1., 0.)),
            SurfaceEdge::Bottom => ((-1., 0.), (0., -1.)),
            SurfaceEdge::Left => ((0., -1.), (1., 0.)),
        };
        Self {
            edge,
            tangent: Vector2::new(tangent.0, tangent.1),
            inward_normal: Vector2::new(inward.0, inward.1),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarOrientation {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Physical alignment: Start is left on horizontal edges, top on vertical edges.
pub enum AlongEdgeAlignment {
    Start,
    Center,
    End,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarReservationPolicy {
    Overlay,
    ReserveOccupiedExtent,
}
#[derive(Clone, Copy, Debug, PartialEq)]
/// Only the uncovered edge-to-body gap becomes additional input.
pub enum BarSensingPolicy {
    None,
    EdgeStrip { thickness: f32 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
/// Output-local geometry intent. Output association belongs to SceneFrame.
pub struct BarPlacement {
    pub edge: SurfaceEdge,
    pub along_edge_alignment: AlongEdgeAlignment,
    pub thickness: f32,
    pub span: f32,
    pub inset: f32,
    pub reservation_policy: BarReservationPolicy,
    pub sensing_policy: BarSensingPolicy,
}
/// Immutable result: consumers use these facts rather than reconstructing bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedBarPlacement {
    edge: SurfaceEdge,
    allocation: GeometryRect,
    sensing_region: Option<GeometryRect>,
    reservation: SceneReservationIntent,
}
impl ResolvedBarPlacement {
    pub fn edge(self) -> SurfaceEdge {
        self.edge
    }
    pub fn orientation(self) -> BarOrientation {
        match self.edge {
            SurfaceEdge::Top | SurfaceEdge::Bottom => BarOrientation::Horizontal,
            _ => BarOrientation::Vertical,
        }
    }
    pub fn allocation(self) -> GeometryRect {
        self.allocation
    }
    pub fn inward_direction(self) -> Vector2 {
        EdgeBasis::for_edge(self.edge).inward_normal
    }
    pub fn sensing_region(self) -> Option<GeometryRect> {
        self.sensing_region
    }
    pub fn reservation(self) -> SceneReservationIntent {
        self.reservation
    }
    /// Presentation radius is supplied by the product recipe, never by placement intent.
    pub fn with_radius(self, radius: f32) -> Result<Self, GeometryError> {
        Ok(Self {
            allocation: GeometryRect::try_new(
                self.allocation.left(),
                self.allocation.top(),
                self.allocation.right(),
                self.allocation.bottom(),
                radius,
            )?,
            ..self
        })
    }
}
impl BarPlacement {
    pub fn resolve(self, viewport: SurfaceViewport) -> Result<ResolvedBarPlacement, GeometryError> {
        viewport.validate()?;
        let horizontal = matches!(self.edge, SurfaceEdge::Top | SurfaceEdge::Bottom);
        let (along, cross) = if horizontal {
            (viewport.width, viewport.height)
        } else {
            (viewport.height, viewport.width)
        };
        for (name, value, valid, requirement) in [
            ("bar.thickness", self.thickness, self.thickness > 0., "> 0"),
            (
                "bar.span",
                self.span,
                self.span > 0. && self.span <= along,
                "> 0 and <= edge length",
            ),
            (
                "bar.inset",
                self.inset,
                self.inset >= 0. && self.inset + self.thickness <= cross,
                ">= 0 and inset + thickness <= cross extent",
            ),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
            if !valid {
                return Err(GeometryError::InvalidParameter {
                    name,
                    value,
                    requirement,
                });
            }
        }
        let origin = match self.along_edge_alignment {
            AlongEdgeAlignment::Start => 0.,
            AlongEdgeAlignment::Center => (along - self.span) * 0.5,
            AlongEdgeAlignment::End => along - self.span,
        };
        let rect = |inset: f32, thickness: f32| {
            let (l, t, r, b) = match self.edge {
                SurfaceEdge::Top => (origin, inset, origin + self.span, inset + thickness),
                SurfaceEdge::Right => (
                    viewport.width - inset - thickness,
                    origin,
                    viewport.width - inset,
                    origin + self.span,
                ),
                SurfaceEdge::Bottom => (
                    origin,
                    viewport.height - inset - thickness,
                    origin + self.span,
                    viewport.height - inset,
                ),
                SurfaceEdge::Left => (inset, origin, inset + thickness, origin + self.span),
            };
            GeometryRect::try_new(l, t, r, b, 0.)
        };
        let allocation = rect(self.inset, self.thickness)?;
        if allocation.width() <= 0.0 || allocation.height() <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "bar.allocation",
                value: self.thickness,
                requirement: "positive representable area",
            });
        }
        let sensing_region = match self.sensing_policy {
            BarSensingPolicy::None => None,
            BarSensingPolicy::EdgeStrip { thickness } => {
                if !thickness.is_finite() {
                    return Err(GeometryError::NonFinite("bar.sensing.thickness"));
                }
                if thickness <= 0. || thickness > cross {
                    return Err(GeometryError::InvalidParameter {
                        name: "bar.sensing.thickness",
                        value: thickness,
                        requirement: "> 0 and <= cross extent",
                    });
                }
                // Only the uncovered edge-to-body gap is sensing input. A body
                // touching the edge already supplies that input; never overlap it.
                let uncovered = thickness.min(self.inset);
                if uncovered > 0. {
                    let sensing = rect(0., uncovered)?;
                    if sensing.width() <= 0.0 || sensing.height() <= 0.0 {
                        return Err(GeometryError::InvalidParameter {
                            name: "bar.sensing.region",
                            value: uncovered,
                            requirement: "positive representable area",
                        });
                    }
                    Some(sensing)
                } else {
                    None
                }
            }
        };
        let mut reservation = SceneReservationIntent::NONE;
        if self.reservation_policy == BarReservationPolicy::ReserveOccupiedExtent {
            let extent = self.inset + self.thickness;
            match self.edge {
                SurfaceEdge::Top => reservation.top = extent,
                SurfaceEdge::Right => reservation.right = extent,
                SurfaceEdge::Bottom => reservation.bottom = extent,
                SurfaceEdge::Left => reservation.left = extent,
            }
        }
        Ok(ResolvedBarPlacement {
            edge: self.edge,
            allocation,
            sensing_region,
            reservation,
        })
    }
}
