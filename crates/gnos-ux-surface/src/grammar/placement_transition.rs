//! Bounded cardinal Bar relocation over the perimeter of one output.
//!
//! An edge change is represented as a slide to a shared corner followed by a
//! connected two-arm transfer. The arms are fragments of one Bar surface; the
//! semantic node and controls remain owned by the caller.

use super::{GeometryError, GeometryRect, ResolvedBarPlacement, SurfaceViewport, UnitInterval};
use crate::{envelope::SurfaceEdge, scene::SceneReservationIntent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarPlacementTransitionPhase {
    SourceSlide,
    CornerTransfer,
    PerimeterSlide,
    TargetSlide,
    RetargetHandoff,
    Settled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarTransitionCorner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarTransitionArm {
    pub edge: SurfaceEdge,
    pub bounds: GeometryRect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedBarTransitionFrame {
    pub progress: UnitInterval,
    pub phase: BarPlacementTransitionPhase,
    pub active_edge: SurfaceEdge,
    pub reservation_edge: SurfaceEdge,
    pub reservation: SceneReservationIntent,
    /// Bounding box of the painted arms; consumers must paint each arm, not
    /// this box as a filled rectangle.
    pub allocation: GeometryRect,
    pub source_allocation: GeometryRect,
    pub target_allocation: GeometryRect,
    /// Location for the one semantic content/control subtree.
    pub active_allocation: GeometryRect,
    pub arms: Vec<BarTransitionArm>,
    /// 0 is horizontal content layout, 1 is vertical.
    pub orientation_progress: UnitInterval,
}

impl ResolvedBarTransitionFrame {
    pub fn input_regions(&self) -> Vec<GeometryRect> {
        disjoint_rect_union(
            &self
                .arms
                .iter()
                .flat_map(|arm| rounded_rect_input(arm.bounds))
                .collect::<Vec<_>>(),
        )
    }

    pub fn is_settled(&self) -> bool {
        self.phase == BarPlacementTransitionPhase::Settled
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SegmentKind {
    Slide {
        edge: SurfaceEdge,
        from: GeometryRect,
        to: GeometryRect,
    },
    Corner {
        corner: BarTransitionCorner,
        from_edge: SurfaceEdge,
        to_edge: SurfaceEdge,
        span: f32,
        from_thickness: f32,
        to_thickness: f32,
        from_inset: f32,
        to_inset: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    phase: BarPlacementTransitionPhase,
    kind: SegmentKind,
    weight: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarPlacementTransition {
    viewport: SurfaceViewport,
    source: ResolvedBarPlacement,
    source_geometry: GeometryRect,
    target: ResolvedBarPlacement,
    segments: Vec<Segment>,
    total_weight: f32,
    retarget_start: Option<ResolvedBarTransitionFrame>,
}

impl BarPlacementTransition {
    pub fn new(
        viewport: SurfaceViewport,
        source: ResolvedBarPlacement,
        target: ResolvedBarPlacement,
    ) -> Result<Self, GeometryError> {
        viewport.validate()?;
        validate_endpoint(viewport, source.allocation())?;
        validate_endpoint(viewport, target.allocation())?;
        let mut transition = Self {
            viewport,
            source,
            source_geometry: source.allocation(),
            target,
            segments: Vec::new(),
            total_weight: 0.0,
            retarget_start: None,
        };
        transition.build()?;
        Ok(transition)
    }

    /// Rebase from a live frame without restarting at the stale endpoint.
    /// Additional arms retract during a short handoff while the active arm
    /// becomes the origin for the next perimeter route.
    pub fn retarget_from_frame(
        viewport: SurfaceViewport,
        current: &ResolvedBarTransitionFrame,
        current_edge_semantics: ResolvedBarPlacement,
        target: ResolvedBarPlacement,
    ) -> Result<Self, GeometryError> {
        if current.arms.is_empty() || current.active_edge != current_edge_semantics.edge() {
            return Err(GeometryError::InvalidParameter {
                name: "bar.transition.retarget",
                value: current.arms.len() as f32,
                requirement: "at least one active arm matching source semantics",
            });
        }
        let mut next = Self::new(viewport, current_edge_semantics, target)?;
        next.source_geometry = current.active_allocation;
        next.retarget_start = Some(current.clone());
        next.segments.clear();
        next.total_weight = 0.0;
        validate_endpoint(viewport, next.source_geometry)?;
        next.build()?;
        Ok(next)
    }

    pub fn source(&self) -> ResolvedBarPlacement {
        self.source
    }
    pub fn target(&self) -> ResolvedBarPlacement {
        self.target
    }

    pub fn evaluate(
        &self,
        progress: UnitInterval,
    ) -> Result<ResolvedBarTransitionFrame, GeometryError> {
        if let Some(start) = &self.retarget_start {
            let p = progress.value();
            if p == 0.0 {
                let mut frame = start.clone();
                frame.progress = progress;
                return Ok(frame);
            }
            let handoff = 0.1;
            let routed_progress =
                UnitInterval::clamped(((p - handoff) / (1.0 - handoff)).clamp(0.0, 1.0));
            let mut frame = self.evaluate_route(routed_progress)?;
            if p < handoff {
                let amount = 1.0 - smoothstep(p / handoff);
                for arm in &start.arms {
                    if arm.edge != start.active_edge && amount > 0.01 {
                        frame.arms.push(BarTransitionArm {
                            edge: arm.edge,
                            bounds: scale_about_center(arm.bounds, amount)?,
                        });
                    }
                }
                frame.phase = BarPlacementTransitionPhase::RetargetHandoff;
            }
            frame.progress = progress;
            frame.allocation = frame
                .arms
                .iter()
                .skip(1)
                .fold(frame.arms[0].bounds, |acc, arm| union(acc, arm.bounds));
            if p < 0.5 {
                frame.reservation = start.reservation;
                frame.reservation_edge = start.reservation_edge;
            }
            frame.orientation_progress = UnitInterval::clamped(
                start.orientation_progress.value()
                    + (orientation(self.target.edge()) - start.orientation_progress.value()) * p,
            );
            return Ok(frame);
        }
        self.evaluate_route(progress)
    }

    fn evaluate_route(
        &self,
        progress: UnitInterval,
    ) -> Result<ResolvedBarTransitionFrame, GeometryError> {
        let p = progress.value();
        if p == 0.0 {
            return Ok(endpoint_at(self.source, self.source_geometry, progress));
        }
        if p == 1.0 {
            return Ok(endpoint(self.target, progress));
        }
        if self.segments.is_empty() {
            return Ok(endpoint(self.target, progress));
        }

        let distance = p * self.total_weight;
        let mut prefix = 0.0;
        let mut chosen = self.segments.last().expect("non-empty");
        let mut local = 1.0;
        for segment in &self.segments {
            let next = prefix + segment.weight;
            if distance <= next {
                chosen = segment;
                local = ((distance - prefix) / segment.weight).clamp(0.0, 1.0);
                break;
            }
            prefix = next;
        }
        let t = smoothstep(local);
        let (arms, active_edge, _active_allocation) = match chosen.kind {
            SegmentKind::Slide { edge, from, to } => {
                let bounds = lerp_rect(from, to, t)?;
                (vec![BarTransitionArm { edge, bounds }], edge, bounds)
            }
            SegmentKind::Corner {
                corner,
                from_edge,
                to_edge,
                span,
                from_thickness,
                to_thickness,
                from_inset,
                to_inset,
            } => {
                let thickness = lerp(from_thickness, to_thickness, t);
                let inset = lerp(from_inset, to_inset, t);
                let from_len = span * (1.0 - t);
                let to_len = span * t;
                let mut arms = Vec::with_capacity(2);
                if from_len > 0.01 {
                    arms.push(BarTransitionArm {
                        edge: from_edge,
                        bounds: corner_arm(
                            self.viewport,
                            from_edge,
                            corner,
                            from_len,
                            thickness,
                            inset,
                        )?,
                    });
                }
                if to_len > 0.01 {
                    arms.push(BarTransitionArm {
                        edge: to_edge,
                        bounds: corner_arm(
                            self.viewport,
                            to_edge,
                            corner,
                            to_len,
                            thickness,
                            inset,
                        )?,
                    });
                }
                let active_edge = if t < 0.5 { from_edge } else { to_edge };
                let active = arms
                    .iter()
                    .find(|arm| arm.edge == active_edge)
                    .or_else(|| arms.first())
                    .ok_or(GeometryError::InvalidParameter {
                        name: "bar.transition.arm",
                        value: t,
                        requirement: "at least one positive arm",
                    })?
                    .bounds;
                (arms, active_edge, active)
            }
        };
        let radius = lerp(
            self.source.allocation().radius(),
            self.target.allocation().radius(),
            p,
        );
        let arms = arms
            .into_iter()
            .map(|mut arm| {
                let r = radius
                    .min(arm.bounds.width() * 0.5)
                    .min(arm.bounds.height() * 0.5);
                arm.bounds = GeometryRect::try_new(
                    arm.bounds.left(),
                    arm.bounds.top(),
                    arm.bounds.right(),
                    arm.bounds.bottom(),
                    r,
                )
                .expect("bounded transition arm radius");
                arm
            })
            .collect::<Vec<_>>();
        let active_allocation = arms
            .iter()
            .find(|arm| arm.edge == active_edge)
            .or_else(|| arms.first())
            .expect("transition has an active arm")
            .bounds;
        let allocation = arms
            .iter()
            .skip(1)
            .fold(arms[0].bounds, |acc, arm| union(acc, arm.bounds));
        let reservation_edge = if p < 0.5 {
            self.source.edge()
        } else {
            self.target.edge()
        };
        let reservation = if p < 0.5 {
            self.source.reservation()
        } else {
            self.target.reservation()
        };
        Ok(ResolvedBarTransitionFrame {
            progress,
            phase: chosen.phase,
            active_edge,
            reservation_edge,
            reservation,
            allocation,
            source_allocation: self.source.allocation(),
            target_allocation: self.target.allocation(),
            active_allocation,
            arms,
            orientation_progress: UnitInterval::clamped(
                orientation(self.source.edge())
                    + (orientation(self.target.edge()) - orientation(self.source.edge())) * p,
            ),
        })
    }

    fn build(&mut self) -> Result<(), GeometryError> {
        let source_edge = self.source.edge();
        let target_edge = self.target.edge();
        let source = self.source_geometry;
        let target = self.target.allocation();
        if source_edge == target_edge {
            if source != target {
                self.push(
                    BarPlacementTransitionPhase::SourceSlide,
                    SegmentKind::Slide {
                        edge: source_edge,
                        from: source,
                        to: target,
                    },
                    rect_distance(source, target),
                );
            }
            return Ok(());
        }

        let route = shortest_route(self.viewport, source_edge, target_edge, source, target);
        let corners = route
            .windows(2)
            .map(|pair| shared_corner(pair[0], pair[1]).expect("route edges are adjacent"))
            .collect::<Vec<_>>();
        let source_span = span(source_edge, source);
        let target_span = span(target_edge, target);
        let common_span = source_span.min(target_span);
        let source_thickness = thickness(source_edge, source);
        let target_thickness = thickness(target_edge, target);
        let source_inset = edge_inset(self.viewport, source_edge, source);
        let target_inset = edge_inset(self.viewport, target_edge, target);
        let turns = corners.len() as f32;

        let first = corners[0];
        let first_rect = corner_arm(
            self.viewport,
            source_edge,
            first,
            common_span,
            source_thickness,
            source_inset,
        )?;
        self.push(
            BarPlacementTransitionPhase::SourceSlide,
            SegmentKind::Slide {
                edge: source_edge,
                from: source,
                to: first_rect,
            },
            rect_distance(source, first_rect),
        );

        for (i, corner) in corners.iter().copied().enumerate() {
            let from_edge = route[i];
            let to_edge = route[i + 1];
            let a = i as f32 / turns;
            let b = (i + 1) as f32 / turns;
            let ta = lerp(source_thickness, target_thickness, a);
            let tb = lerp(source_thickness, target_thickness, b);
            let ia = lerp(source_inset, target_inset, a);
            let ib = lerp(source_inset, target_inset, b);
            let phase = BarPlacementTransitionPhase::CornerTransfer;
            self.push(
                phase,
                SegmentKind::Corner {
                    corner,
                    from_edge,
                    to_edge,
                    span: common_span,
                    from_thickness: ta,
                    to_thickness: tb,
                    from_inset: ia,
                    to_inset: ib,
                },
                std::f32::consts::FRAC_PI_2 * (common_span + (ta + tb) * 0.5).max(1.0),
            );
            if i + 1 < corners.len() {
                let next = corners[i + 1];
                let edge = to_edge;
                let t = tb;
                let inset = ib;
                let from = corner_arm(self.viewport, edge, corner, common_span, t, inset)?;
                let to = corner_arm(self.viewport, edge, next, common_span, t, inset)?;
                self.push(
                    BarPlacementTransitionPhase::PerimeterSlide,
                    SegmentKind::Slide { edge, from, to },
                    rect_distance(from, to),
                );
            }
        }
        let last = *corners.last().expect("different edges have a turn");
        let final_corner = corner_arm(
            self.viewport,
            target_edge,
            last,
            common_span,
            target_thickness,
            target_inset,
        )?;
        self.push(
            BarPlacementTransitionPhase::TargetSlide,
            SegmentKind::Slide {
                edge: target_edge,
                from: final_corner,
                to: target,
            },
            rect_distance(final_corner, target),
        );
        Ok(())
    }

    fn push(&mut self, phase: BarPlacementTransitionPhase, kind: SegmentKind, weight: f32) {
        let weight = weight.max(0.001);
        self.total_weight += weight;
        self.segments.push(Segment {
            phase,
            kind,
            weight,
        });
    }
}

fn validate_endpoint(viewport: SurfaceViewport, rect: GeometryRect) -> Result<(), GeometryError> {
    if rect.left() < 0.0
        || rect.top() < 0.0
        || rect.right() > viewport.width
        || rect.bottom() > viewport.height
    {
        return Err(GeometryError::InvalidParameter {
            name: "bar.transition.endpoint",
            value: rect.width(),
            requirement: "inside viewport",
        });
    }
    Ok(())
}

fn endpoint(placement: ResolvedBarPlacement, progress: UnitInterval) -> ResolvedBarTransitionFrame {
    endpoint_at(placement, placement.allocation(), progress)
}

fn endpoint_at(
    placement: ResolvedBarPlacement,
    rect: GeometryRect,
    progress: UnitInterval,
) -> ResolvedBarTransitionFrame {
    ResolvedBarTransitionFrame {
        progress,
        phase: BarPlacementTransitionPhase::Settled,
        active_edge: placement.edge(),
        reservation_edge: placement.edge(),
        reservation: placement.reservation(),
        allocation: rect,
        source_allocation: rect,
        target_allocation: rect,
        active_allocation: rect,
        arms: vec![BarTransitionArm {
            edge: placement.edge(),
            bounds: rect,
        }],
        orientation_progress: UnitInterval::clamped(orientation(placement.edge())),
    }
}

fn orientation(edge: SurfaceEdge) -> f32 {
    if matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom) {
        0.0
    } else {
        1.0
    }
}
fn span(edge: SurfaceEdge, rect: GeometryRect) -> f32 {
    if matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom) {
        rect.width()
    } else {
        rect.height()
    }
}
fn thickness(edge: SurfaceEdge, rect: GeometryRect) -> f32 {
    if matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom) {
        rect.height()
    } else {
        rect.width()
    }
}
fn edge_inset(viewport: SurfaceViewport, edge: SurfaceEdge, rect: GeometryRect) -> f32 {
    match edge {
        SurfaceEdge::Top => rect.top(),
        SurfaceEdge::Right => viewport.width - rect.right(),
        SurfaceEdge::Bottom => viewport.height - rect.bottom(),
        SurfaceEdge::Left => rect.left(),
    }
}

fn shared_corner(a: SurfaceEdge, b: SurfaceEdge) -> Option<BarTransitionCorner> {
    match (a, b) {
        (SurfaceEdge::Top, SurfaceEdge::Right) | (SurfaceEdge::Right, SurfaceEdge::Top) => {
            Some(BarTransitionCorner::TopRight)
        }
        (SurfaceEdge::Right, SurfaceEdge::Bottom) | (SurfaceEdge::Bottom, SurfaceEdge::Right) => {
            Some(BarTransitionCorner::BottomRight)
        }
        (SurfaceEdge::Bottom, SurfaceEdge::Left) | (SurfaceEdge::Left, SurfaceEdge::Bottom) => {
            Some(BarTransitionCorner::BottomLeft)
        }
        (SurfaceEdge::Left, SurfaceEdge::Top) | (SurfaceEdge::Top, SurfaceEdge::Left) => {
            Some(BarTransitionCorner::TopLeft)
        }
        _ => None,
    }
}

fn shortest_route(
    viewport: SurfaceViewport,
    from: SurfaceEdge,
    to: SurfaceEdge,
    source: GeometryRect,
    target: GeometryRect,
) -> Vec<SurfaceEdge> {
    const CW: [SurfaceEdge; 4] = [
        SurfaceEdge::Top,
        SurfaceEdge::Right,
        SurfaceEdge::Bottom,
        SurfaceEdge::Left,
    ];
    let start = CW.iter().position(|edge| *edge == from).unwrap();
    let end = CW.iter().position(|edge| *edge == to).unwrap();
    let cw_steps = (end + 4 - start) % 4;
    let ccw_steps = (start + 4 - end) % 4;
    let cw = route_steps(start, cw_steps, 1);
    let ccw = route_steps(start, ccw_steps, -1);
    let cost = |route: &[SurfaceEdge]| {
        let first = shared_corner(route[0], route[1]).unwrap();
        let last = shared_corner(route[route.len() - 2], route[route.len() - 1]).unwrap();
        distance_to_corner(viewport, route[0], source, first)
            + route
                .iter()
                .skip(1)
                .take(route.len().saturating_sub(2))
                .map(|edge| edge_length(*edge, viewport))
                .sum::<f32>()
            + distance_from_corner(viewport, to, target, last)
    };
    if cw_steps == 0 {
        return vec![from, to];
    }
    let cw_cost = cost(&cw);
    let ccw_cost = cost(&ccw);
    // Stable tie: clockwise in Top, Right, Bottom, Left order.
    if cw_cost <= ccw_cost + 0.001 { cw } else { ccw }
}

fn route_steps(start: usize, steps: usize, direction: isize) -> Vec<SurfaceEdge> {
    const CW: [SurfaceEdge; 4] = [
        SurfaceEdge::Top,
        SurfaceEdge::Right,
        SurfaceEdge::Bottom,
        SurfaceEdge::Left,
    ];
    (0..=steps)
        .map(|n| CW[(start as isize + direction * n as isize).rem_euclid(4) as usize])
        .collect()
}

fn edge_length(edge: SurfaceEdge, viewport: SurfaceViewport) -> f32 {
    if matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom) {
        viewport.width
    } else {
        viewport.height
    }
}
fn along_center(edge: SurfaceEdge, rect: GeometryRect) -> f32 {
    if matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom) {
        (rect.left() + rect.right()) * 0.5
    } else {
        (rect.top() + rect.bottom()) * 0.5
    }
}
fn corner_along(viewport: SurfaceViewport, edge: SurfaceEdge, corner: BarTransitionCorner) -> f32 {
    match edge {
        SurfaceEdge::Top | SurfaceEdge::Bottom => {
            if matches!(
                corner,
                BarTransitionCorner::TopLeft | BarTransitionCorner::BottomLeft
            ) {
                0.0
            } else {
                viewport.width
            }
        }
        SurfaceEdge::Left | SurfaceEdge::Right => {
            if matches!(
                corner,
                BarTransitionCorner::TopLeft | BarTransitionCorner::TopRight
            ) {
                0.0
            } else {
                viewport.height
            }
        }
    }
}
fn distance_to_corner(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    rect: GeometryRect,
    corner: BarTransitionCorner,
) -> f32 {
    (along_center(edge, rect) - corner_along(viewport, edge, corner)).abs()
}
fn distance_from_corner(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    rect: GeometryRect,
    corner: BarTransitionCorner,
) -> f32 {
    distance_to_corner(viewport, edge, rect, corner)
}

fn corner_arm(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    corner: BarTransitionCorner,
    length: f32,
    thickness: f32,
    inset: f32,
) -> Result<GeometryRect, GeometryError> {
    let length = length.max(0.01);
    let (left, top, right, bottom) = match edge {
        SurfaceEdge::Top => {
            let x = if matches!(
                corner,
                BarTransitionCorner::TopLeft | BarTransitionCorner::BottomLeft
            ) {
                0.0
            } else {
                viewport.width - length
            };
            (x, inset, x + length, inset + thickness)
        }
        SurfaceEdge::Bottom => {
            let x = if matches!(
                corner,
                BarTransitionCorner::TopLeft | BarTransitionCorner::BottomLeft
            ) {
                0.0
            } else {
                viewport.width - length
            };
            (
                x,
                viewport.height - inset - thickness,
                x + length,
                viewport.height - inset,
            )
        }
        SurfaceEdge::Left => {
            let y = if matches!(
                corner,
                BarTransitionCorner::TopLeft | BarTransitionCorner::TopRight
            ) {
                0.0
            } else {
                viewport.height - length
            };
            (inset, y, inset + thickness, y + length)
        }
        SurfaceEdge::Right => {
            let y = if matches!(
                corner,
                BarTransitionCorner::TopLeft | BarTransitionCorner::TopRight
            ) {
                0.0
            } else {
                viewport.height - length
            };
            (
                viewport.width - inset - thickness,
                y,
                viewport.width - inset,
                y + length,
            )
        }
    };
    GeometryRect::try_new(left, top, right, bottom, 0.0)
}
fn lerp_rect(a: GeometryRect, b: GeometryRect, t: f32) -> Result<GeometryRect, GeometryError> {
    GeometryRect::try_new(
        lerp(a.left(), b.left(), t),
        lerp(a.top(), b.top(), t),
        lerp(a.right(), b.right(), t),
        lerp(a.bottom(), b.bottom(), t),
        lerp(a.radius(), b.radius(), t),
    )
}

fn scale_about_center(rect: GeometryRect, scale: f32) -> Result<GeometryRect, GeometryError> {
    let half_width = (rect.width() * scale * 0.5).max(0.005);
    let half_height = (rect.height() * scale * 0.5).max(0.005);
    let center_x = (rect.left() + rect.right()) * 0.5;
    let center_y = (rect.top() + rect.bottom()) * 0.5;
    GeometryRect::try_new(
        center_x - half_width,
        center_y - half_height,
        center_x + half_width,
        center_y + half_height,
        rect.radius().min(half_width).min(half_height),
    )
}

fn rounded_rect_input(rect: GeometryRect) -> Vec<GeometryRect> {
    let radius = rect
        .radius()
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);
    if radius <= 0.0 {
        return vec![rect];
    }
    let mut regions = Vec::with_capacity(3);
    let side_left = (rect.left() + radius).min(rect.right());
    let side_right = (rect.right() - radius).max(rect.left());
    let top_bottom = (rect.top() + radius).min(rect.bottom());
    if side_right > side_left && top_bottom > rect.top() {
        regions.push(
            GeometryRect::try_new(side_left, rect.top(), side_right, top_bottom, 0.0)
                .expect("positive top rounded-input band"),
        );
    }
    let middle_bottom = (rect.bottom() - radius).max(rect.top());
    if rect.right() > rect.left() && middle_bottom > top_bottom {
        regions.push(
            GeometryRect::try_new(rect.left(), top_bottom, rect.right(), middle_bottom, 0.0)
                .expect("positive middle rounded-input band"),
        );
    }
    if side_right > side_left && rect.bottom() > middle_bottom {
        regions.push(
            GeometryRect::try_new(side_left, middle_bottom, side_right, rect.bottom(), 0.0)
                .expect("positive bottom rounded-input band"),
        );
    }
    regions
}
fn union(a: GeometryRect, b: GeometryRect) -> GeometryRect {
    GeometryRect::try_new(
        a.left().min(b.left()),
        a.top().min(b.top()),
        a.right().max(b.right()),
        a.bottom().max(b.bottom()),
        a.radius().max(b.radius()),
    )
    .unwrap()
}
fn rect_distance(a: GeometryRect, b: GeometryRect) -> f32 {
    (a.left() - b.left()).abs()
        + (a.top() - b.top()).abs()
        + (a.width() - b.width()).abs()
        + (a.height() - b.height()).abs()
}

fn disjoint_rect_union(rects: &[GeometryRect]) -> Vec<GeometryRect> {
    let mut xs = rects
        .iter()
        .flat_map(|r| [r.left(), r.right()])
        .collect::<Vec<_>>();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < f32::EPSILON);
    let mut strips = Vec::<GeometryRect>::new();
    for pair in xs.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        if right <= left {
            continue;
        }
        let mid = (left + right) * 0.5;
        let mut ys = rects
            .iter()
            .filter(|r| r.left() < mid && mid < r.right())
            .map(|r| (r.top(), r.bottom()))
            .collect::<Vec<_>>();
        ys.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged = Vec::<(f32, f32)>::new();
        for (top, bottom) in ys {
            if let Some(last) = merged.last_mut()
                && top <= last.1
            {
                last.1 = last.1.max(bottom);
                continue;
            }
            merged.push((top, bottom));
        }
        for (top, bottom) in merged {
            strips.push(
                GeometryRect::try_new(left, top, right, bottom, 0.0)
                    .expect("positive disjoint input strip"),
            );
        }
    }
    strips
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::{
        AlongEdgeAlignment, BarPlacement, BarReservationPolicy, BarSensingPolicy,
    };

    const EDGES: [SurfaceEdge; 4] = [
        SurfaceEdge::Top,
        SurfaceEdge::Right,
        SurfaceEdge::Bottom,
        SurfaceEdge::Left,
    ];
    const PROGRESS: [f32; 7] = [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

    fn endpoint(viewport: SurfaceViewport, edge: SurfaceEdge) -> ResolvedBarPlacement {
        let horizontal = matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom);
        BarPlacement {
            edge,
            along_edge_alignment: AlongEdgeAlignment::Center,
            thickness: 48.0,
            span: if horizontal {
                viewport.width.min(760.0) - 32.0
            } else {
                viewport.height.min(760.0) - 32.0
            },
            inset: 16.0,
            reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
            sensing_policy: BarSensingPolicy::None,
        }
        .resolve(viewport)
        .unwrap()
        .with_radius(14.0)
        .unwrap()
    }

    #[test]
    fn all_ordered_edge_pairs_are_finite_bounded_deterministic_and_endpoint_exact() {
        for (width, height) in [(1280.0, 800.0), (800.0, 1280.0), (720.0, 720.0)] {
            let viewport = SurfaceViewport::try_new(width, height).unwrap();
            for source_edge in EDGES {
                for target_edge in EDGES {
                    if source_edge == target_edge {
                        continue;
                    }
                    let source = endpoint(viewport, source_edge);
                    let target = endpoint(viewport, target_edge);
                    let transition = BarPlacementTransition::new(viewport, source, target).unwrap();
                    for p in PROGRESS {
                        let progress = UnitInterval::try_new(p).unwrap();
                        let frame = transition.evaluate(progress).unwrap();
                        let rect = frame.allocation;
                        assert!(
                            rect.left().is_finite()
                                && rect.top().is_finite()
                                && rect.right().is_finite()
                                && rect.bottom().is_finite()
                        );
                        assert!(viewport_rect(viewport).contains(rect));
                        assert!(!frame.arms.is_empty());
                        let input = frame.input_regions();
                        for (i, left) in input.iter().enumerate() {
                            assert!(viewport_rect(viewport).contains(*left));
                            for right in input.iter().skip(i + 1) {
                                assert!(
                                    left.right() <= right.left()
                                        || right.right() <= left.left()
                                        || left.bottom() <= right.top()
                                        || right.bottom() <= left.top()
                                );
                            }
                        }
                        let again = transition.evaluate(progress).unwrap();
                        assert_eq!(frame, again);
                        let nonzero = [
                            frame.reservation.top,
                            frame.reservation.right,
                            frame.reservation.bottom,
                            frame.reservation.left,
                        ]
                        .into_iter()
                        .filter(|v| *v > 0.0)
                        .count();
                        assert!(nonzero <= 1);
                    }
                    assert_eq!(
                        transition.evaluate(UnitInterval::ZERO).unwrap().allocation,
                        source.allocation()
                    );
                    assert_eq!(
                        transition.evaluate(UnitInterval::ONE).unwrap().allocation,
                        target.allocation()
                    );
                }
            }
        }
    }

    #[test]
    fn reservation_switches_once_at_half_and_matches_target() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        for source_edge in EDGES {
            for target_edge in EDGES {
                if source_edge == target_edge {
                    continue;
                }
                let transition = BarPlacementTransition::new(
                    viewport,
                    endpoint(viewport, source_edge),
                    endpoint(viewport, target_edge),
                )
                .unwrap();
                assert_eq!(
                    transition
                        .evaluate(UnitInterval::try_new(0.499).unwrap())
                        .unwrap()
                        .reservation_edge,
                    source_edge
                );
                let at = transition
                    .evaluate(UnitInterval::try_new(0.5).unwrap())
                    .unwrap();
                assert_eq!(at.reservation_edge, target_edge);
                assert_eq!(
                    at.reservation,
                    endpoint(viewport, target_edge).reservation()
                );
                assert_eq!(
                    transition.evaluate(UnitInterval::ONE).unwrap().reservation,
                    endpoint(viewport, target_edge).reservation()
                );
            }
        }
    }

    #[test]
    fn rounded_corner_input_excludes_transparent_paint_corners() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        for edge in EDGES {
            let resolved = endpoint(viewport, edge);
            let frame = BarPlacementTransition::new(viewport, resolved, endpoint(viewport, edge))
                .unwrap()
                .evaluate(UnitInterval::ZERO)
                .unwrap();
            let body = resolved.allocation();
            let input = frame.input_regions();
            assert!(
                !input
                    .iter()
                    .any(|r| r.contains_point(body.left() + 0.5, body.top() + 0.5)),
                "{edge:?} rounded corner passes through"
            );
            assert!(
                input.iter().any(|r| r.contains_point(
                    (body.left() + body.right()) * 0.5,
                    (body.top() + body.bottom()) * 0.5
                )),
                "{edge:?} body remains interactive"
            );
        }
    }

    #[test]
    fn same_edge_transition_is_a_single_slide() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut target = BarPlacement {
            edge: SurfaceEdge::Top,
            along_edge_alignment: AlongEdgeAlignment::End,
            thickness: 48.0,
            span: 600.0,
            inset: 20.0,
            reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
            sensing_policy: BarSensingPolicy::None,
        }
        .resolve(viewport)
        .unwrap();
        target = target.with_radius(12.0).unwrap();
        let source = endpoint(viewport, SurfaceEdge::Top);
        let transition = BarPlacementTransition::new(viewport, source, target).unwrap();
        let frame = transition
            .evaluate(UnitInterval::try_new(0.5).unwrap())
            .unwrap();
        assert_eq!(frame.phase, BarPlacementTransitionPhase::SourceSlide);
        assert_eq!(frame.active_edge, SurfaceEdge::Top);
    }

    #[test]
    fn retarget_rebases_from_current_geometry_without_endpoint_restart() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let top = endpoint(viewport, SurfaceEdge::Top);
        let left = endpoint(viewport, SurfaceEdge::Left);
        let right = endpoint(viewport, SurfaceEdge::Right);
        let first = BarPlacementTransition::new(viewport, top, left).unwrap();
        let current = first.evaluate(UnitInterval::try_new(0.1).unwrap()).unwrap();
        assert_eq!(
            current.arms.len(),
            1,
            "retarget sample is a single-arm slide phase"
        );
        let retargeted =
            BarPlacementTransition::retarget_from_frame(viewport, &current, top, right).unwrap();
        let rebased = retargeted.evaluate(UnitInterval::ZERO).unwrap();
        assert_eq!(rebased.allocation, current.active_allocation);
        let next = retargeted
            .evaluate(UnitInterval::try_new(0.001).unwrap())
            .unwrap();
        assert!((next.active_allocation.left() - rebased.active_allocation.left()).abs() < 0.1);
        let settled = retargeted.evaluate(UnitInterval::ONE).unwrap();
        assert_eq!(settled.allocation, right.allocation());
    }

    #[test]
    fn retarget_from_corner_handoff_retracts_extra_arm_without_geometry_jump() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let top = endpoint(viewport, SurfaceEdge::Top);
        let left = endpoint(viewport, SurfaceEdge::Left);
        let right = endpoint(viewport, SurfaceEdge::Right);
        let first = BarPlacementTransition::new(viewport, top, left).unwrap();
        let current = first
            .evaluate(UnitInterval::try_new(0.25).unwrap())
            .unwrap();
        assert_eq!(current.arms.len(), 2);
        let retargeted =
            BarPlacementTransition::retarget_from_frame(viewport, &current, top, right).unwrap();
        let at_start = retargeted.evaluate(UnitInterval::ZERO).unwrap();
        assert_eq!(at_start.allocation, current.allocation);
        let near_start = retargeted
            .evaluate(UnitInterval::try_new(0.001).unwrap())
            .unwrap();
        assert!(rect_distance(at_start.allocation, near_start.allocation) < 0.1);
        assert_eq!(
            retargeted.evaluate(UnitInterval::ONE).unwrap().allocation,
            right.allocation()
        );
    }

    fn viewport_rect(viewport: SurfaceViewport) -> GeometryRect {
        GeometryRect::try_new(0.0, 0.0, viewport.width, viewport.height, 0.0).unwrap()
    }
}
