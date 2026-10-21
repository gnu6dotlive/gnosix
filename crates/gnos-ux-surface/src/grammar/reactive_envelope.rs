//! Pure, ephemeral dynamics for a parent-derived workspace envelope.
//! Semantic identity and presentation intent remain inputs owned by callers.

use super::{
    CubicJoinPair, GeometryError, GeometryRect, PhaseChannel, PhaseResponseProfile, Point2,
    SecondOrderChannel, SecondOrderSpec, SettlePolicy, SurfaceViewport, TemporalResponseMode,
    UnitInterval,
    connective_hull::{ConnectiveHullGeometry, ConnectiveHullInput},
    step_second_order_pair,
};

/// Advance a bounded geometry phase through the shared second-order phase primitive.
/// The caller owns the topology decision and supplies role-specific response values.
pub fn step_reactive_phase(
    phase: &mut f32,
    velocity: &mut f32,
    target: f32,
    full: SecondOrderSpec,
    reduced: SecondOrderSpec,
    mode: TemporalResponseMode,
    dt: f32,
) -> Result<(), GeometryError> {
    let mut channel = PhaseChannel::clamped(*phase, *velocity)?;
    channel.step(
        UnitInterval::try_new(target)?,
        PhaseResponseProfile::new(full, reduced, SettlePolicy::new(0.0005, 0.01, true)),
        mode,
        dt,
    )?;
    *phase = channel.phase().value();
    *velocity = channel.velocity();
    Ok(())
}

/// Explicit spatial samples for the E11 three-node deformation primitive.
/// Coordinates and limits come from the consumer, never from a Dock or viewport policy.
#[derive(Clone, Copy, Debug)]
pub struct LocalDeformationInput {
    pub node_x: [f32; 3],
    pub pointer_x: f32,
    pub directional_drive: [f32; 3],
    pub locality_sigma: f32,
    pub elasticity_gain: f32,
    pub propagation: f32,
    pub target_limit: f32,
    pub position_limit: f32,
}

/// Advance local displacement and velocity together using the prior field for propagation.
/// A rejected input leaves both arrays unchanged.
pub fn step_local_deformation(
    positions: &mut [f32; 3],
    velocities: &mut [f32; 3],
    input: LocalDeformationInput,
    dynamics: SecondOrderSpec,
    dt: f32,
) -> Result<(), GeometryError> {
    for value in input
        .node_x
        .into_iter()
        .chain(input.directional_drive)
        .chain([
            input.pointer_x,
            input.locality_sigma,
            input.elasticity_gain,
            input.propagation,
            input.target_limit,
            input.position_limit,
        ])
    {
        if !value.is_finite() {
            return Err(GeometryError::NonFinite("local_deformation.input"));
        }
    }
    for (name, value, strictly_positive) in [
        ("local_deformation.sigma", input.locality_sigma, true),
        ("local_deformation.elasticity", input.elasticity_gain, false),
        ("local_deformation.propagation", input.propagation, false),
        ("local_deformation.target_limit", input.target_limit, false),
        (
            "local_deformation.position_limit",
            input.position_limit,
            false,
        ),
    ] {
        if value < 0.0 || (strictly_positive && value == 0.0) {
            return Err(GeometryError::InvalidParameter {
                name,
                value,
                requirement: if strictly_positive { "> 0" } else { ">= 0" },
            });
        }
    }
    let prior = *positions;
    let mut next = *positions;
    let mut next_velocity = *velocities;
    for index in 0..3 {
        let distance = input.pointer_x - input.node_x[index];
        let weight = (-0.5 * (distance / input.locality_sigma).powi(2)).exp();
        let adjacent = match index {
            0 | 2 => prior[1],
            _ => (prior[0] + prior[2]) * 0.5,
        };
        let target = (input.directional_drive[index] * input.elasticity_gain * weight
            + (adjacent - prior[index]) * input.propagation * 0.42)
            .clamp(-input.target_limit, input.target_limit);
        super::step_second_order_pair(
            &mut next[index],
            &mut next_velocity[index],
            target,
            dynamics,
            dt,
        )?;
        next[index] = next[index].clamp(-input.position_limit, input.position_limit);
    }
    *positions = next;
    *velocities = next_velocity;
    Ok(())
}

/// E11 shoulder lag, independent of the role or orientation of the parent.
pub fn step_shoulder_response(
    position: &mut f32,
    velocity: &mut f32,
    child_center: f32,
    child_velocity: f32,
    propagation: f32,
    dynamics: SecondOrderSpec,
    dt: f32,
) -> Result<(), GeometryError> {
    if !propagation.is_finite() || propagation < 0.0 {
        return Err(GeometryError::InvalidParameter {
            name: "shoulder.propagation",
            value: propagation,
            requirement: "finite and >= 0",
        });
    }
    let target = child_center - child_velocity * 0.028 * propagation;
    super::step_second_order_pair(position, velocity, target, dynamics.scaled(0.48, 0.68)?, dt)?;
    Ok(())
}

/// Consumer-selected stretch-to-width mapping. No Dock/AppStack dimensions.
#[derive(Clone, Copy, Debug)]
pub struct NeckResponseInput {
    pub gap: f32,
    pub axial_velocity: f32,
    pub velocity_lead: f32,
    pub rest_width: f32,
    pub minimum_width: f32,
    pub maximum_width: f32,
    pub stretch_gain: f32,
}

impl NeckResponseInput {
    pub fn target(self) -> Result<f32, GeometryError> {
        for value in [
            self.gap,
            self.axial_velocity,
            self.velocity_lead,
            self.rest_width,
            self.minimum_width,
            self.maximum_width,
            self.stretch_gain,
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite("neck.input"));
            }
        }
        if self.gap < 0.0
            || self.velocity_lead < 0.0
            || self.stretch_gain < 0.0
            || self.minimum_width < 0.0
            || self.minimum_width > self.rest_width
            || self.rest_width > self.maximum_width
        {
            return Err(GeometryError::InvalidParameter {
                name: "neck.mapping",
                value: self.rest_width,
                requirement: "nonnegative gap/gains and 0 <= minimum <= rest <= maximum",
            });
        }
        let stretch = self.gap + self.axial_velocity.abs() * self.velocity_lead;
        let target = self.rest_width - stretch * self.stretch_gain;
        if !target.is_finite() {
            return Err(GeometryError::NonFinite("neck.target"));
        }
        Ok(target.clamp(self.minimum_width, self.rest_width))
    }
}

pub fn step_neck_response(
    width: &mut f32,
    velocity: &mut f32,
    input: NeckResponseInput,
    dynamics: SecondOrderSpec,
    dt: f32,
) -> Result<f32, GeometryError> {
    let target = input.target()?;
    super::step_second_order_pair(width, velocity, target, dynamics.scaled(0.56, 0.72)?, dt)?;
    *width = width.clamp(input.minimum_width, input.maximum_width);
    Ok(target)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactiveAttachment {
    Fused,
    Bridged,
    Detached,
}

/// Product attachment semantics. Auto retains geometry-driven hysteresis;
/// explicit intent resolves geometry for that relation without inferring it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReactiveAttachmentIntent {
    #[default]
    Auto,
    Fused,
    Bridged,
    Detached,
}

/// Which way a derived body grows away from its parent joint.
///
/// Semantic state, readable as such. Not a `bool reverse` and not a sign
/// multiplier handed to callers: which way a body extrudes is a property of
/// the body, and the solver's own geometry is expressed relative to a joint
/// with a direction of travel rather than relative to a world axis.
///
/// `Downward` is the default so every existing caller keeps the behaviour it
/// was written against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReactiveGrowthDirection {
    /// The body grows from the parent's bottom edge, downward.
    #[default]
    Downward,
    /// The body grows from the parent's top edge, upward.
    Upward,
}

impl ReactiveGrowthDirection {
    /// +1 when growth proceeds toward increasing world y, -1 when it proceeds
    /// toward decreasing world y.
    pub(crate) const fn sign(self) -> f32 {
        match self {
            Self::Downward => 1.0,
            Self::Upward => -1.0,
        }
    }

    /// The parent's edge the derived body grows off.
    pub(crate) const fn joint(self, parent: GeometryRect) -> f32 {
        match self {
            Self::Downward => parent.bottom(),
            Self::Upward => parent.top(),
        }
    }

    /// The parent's other edge, away from the joint.
    pub(crate) const fn joint_opposite(self, parent: GeometryRect) -> f32 {
        match self {
            Self::Downward => parent.top(),
            Self::Upward => parent.bottom(),
        }
    }
}

/// A direction-aware vertical frame for one derived body.
///
/// The solver's physics is directional already: it is written as "a distance
/// from the parent's bottom edge, growing downward". Rather than keep two
/// copies of that physics, the body is solved in a local space where the joint
/// is `y = 0` and growth proceeds toward positive local y, then mapped back to
/// world coordinates. Every existing downward expression keeps its meaning, and
/// upward needs no second implementation of any of it.
///
/// For `Downward` this is the identity on world deltas, which is what keeps the
/// pre-existing behaviour numerically unchanged.
#[derive(Clone, Copy, Debug)]
struct GrowthFrame {
    joint_y: f32,
    sign: f32,
    /// The world y of the far bound of the available region, along the growth axis.
    available_far: f32,
    /// The world y of the available region's near bound, along the growth axis.
    available_near: f32,
}

impl GrowthFrame {
    fn new(
        direction: ReactiveGrowthDirection,
        parent: GeometryRect,
        available: GeometryRect,
    ) -> Self {
        Self {
            joint_y: direction.joint(parent),
            sign: direction.sign(),
            available_far: match direction {
                ReactiveGrowthDirection::Downward => available.bottom(),
                ReactiveGrowthDirection::Upward => available.top(),
            },
            available_near: match direction {
                ReactiveGrowthDirection::Downward => available.top(),
                ReactiveGrowthDirection::Upward => available.bottom(),
            },
        }
    }

    /// The available region's near bound along the growth axis.
    fn near_available(&self) -> f32 {
        self.available_near
    }

    /// World y to local y, with the joint at 0 and growth toward positive.
    fn local(&self, world_y: f32) -> f32 {
        (world_y - self.joint_y) * self.sign
    }

    /// Local y back to world y.
    fn world(&self, local_y: f32) -> f32 {
        self.joint_y + self.sign * local_y
    }

    /// The world edge of another rectangle that faces the joint.
    fn facing_edge(&self, rect: GeometryRect) -> f32 {
        if self.sign > 0.0 {
            rect.top()
        } else {
            rect.bottom()
        }
    }

    /// The world edge of another rectangle that is away from the joint.
    fn far_edge(&self, rect: GeometryRect) -> f32 {
        if self.sign > 0.0 {
            rect.bottom()
        } else {
            rect.top()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReactiveEnvelopeInput {
    pub attachment_intent: ReactiveAttachmentIntent,
    pub parent: GeometryRect,
    pub available: GeometryRect,
    pub viewport: SurfaceViewport,
    pub target_open: bool,
    pub pointer: Option<Point2>,
    pub neighbor: Option<GeometryRect>,
    pub reduced_motion: bool,
    /// Which way the derived body grows away from the parent's joint.
    ///
    /// Defaults to `Downward`, which is what every caller written before
    /// directionality existed means.
    pub growth: ReactiveGrowthDirection,
}

#[derive(Clone, Copy, Debug)]
pub struct ReactiveEnvelopeTuning {
    pub response: f32,
    pub damping: f32,
    pub elasticity: f32,
    pub propagation: f32,
    pub shape_preservation: f32,
    pub edge_affinity: f32,
    pub bilateral_contour_gain: f32,
    pub neighbor_compression_gain: f32,
    pub fusion_hysteresis_radius_ratio: f32,
    pub detach_strain_ratio: f32,
    pub reattach_strain_ratio: f32,
}

impl ReactiveEnvelopeTuning {
    /// Internal experimental values; not a user preference contract.
    pub const fn candidate_default() -> Self {
        Self {
            response: 92.0,
            damping: 13.0,
            elasticity: 0.78,
            propagation: 0.62,
            shape_preservation: 0.62,
            edge_affinity: 1.0,
            bilateral_contour_gain: 0.18,
            neighbor_compression_gain: 0.18,
            // CANDIDATE_DEFAULT: relative exit margin above the 2R fusion threshold.
            fusion_hysteresis_radius_ratio: 0.25,
            detach_strain_ratio: 0.04,
            reattach_strain_ratio: 0.025,
        }
    }

    fn validate(self) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("reactive.response", self.response),
            ("reactive.damping", self.damping),
            ("reactive.elasticity", self.elasticity),
            ("reactive.propagation", self.propagation),
            ("reactive.shape_preservation", self.shape_preservation),
            ("reactive.edge_affinity", self.edge_affinity),
            (
                "reactive.bilateral_contour_gain",
                self.bilateral_contour_gain,
            ),
            (
                "reactive.neighbor_compression_gain",
                self.neighbor_compression_gain,
            ),
            (
                "reactive.fusion_hysteresis_radius_ratio",
                self.fusion_hysteresis_radius_ratio,
            ),
            ("reactive.detach_strain_ratio", self.detach_strain_ratio),
            ("reactive.reattach_strain_ratio", self.reattach_strain_ratio),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
            if value < 0.0 || (name.ends_with("_ratio") && value > 1.0) {
                return Err(GeometryError::InvalidParameter {
                    name,
                    value,
                    requirement: "0..=1 for ratios, otherwise >= 0",
                });
            }
        }
        Ok(self)
    }
}

/// Normalized horizontal edge pressure, split into translation and strain.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReactiveEdgeResponse {
    pub left_pressure: f32,
    pub right_pressure: f32,
    pub directional_bias: f32,
    pub bilateral_confinement: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReactiveEnvelopeFrame {
    pub hull: ConnectiveHullGeometry,
    pub geometry: GeometryRect,
    pub paint_bounds: GeometryRect,
    pub input_bounds: GeometryRect,
    pub top_contour: [Point2; 5],
    pub connector: CubicJoinPair,
    pub connector_bounds: GeometryRect,
    pub connector_width: f32,
    pub connector_presence: f32,
    pub attachment: ReactiveAttachment,
    pub attachment_phase: f32,
    pub release_phase: f32,
    pub reattach_phase: f32,
    pub openness: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub neck_width: f32,
    pub deformation: f32,
    pub constraint_pressure: f32,
    pub edge_response: ReactiveEdgeResponse,
    pub neighbor_pressure: f32,
    pub neighbor_compression: f32,
    pub neighbor_compression_target: f32,
}

impl ReactiveEnvelopeFrame {
    fn transposed(self) -> Self {
        use super::transposition::{point, rect};
        let c = self.connector;
        Self {
            hull: self.hull.transposed(),
            geometry: rect(self.geometry),
            paint_bounds: rect(self.paint_bounds),
            input_bounds: rect(self.input_bounds),
            top_contour: self.top_contour.map(point),
            connector: CubicJoinPair::from_controls(
                point(c.p0()),
                point(c.c1()),
                point(c.c2()),
                point(c.p1()),
                point(c.c3()),
                point(c.c4()),
                point(c.p2()),
            ),
            connector_bounds: rect(self.connector_bounds),
            velocity_x: self.velocity_y,
            velocity_y: self.velocity_x,
            ..self
        }
    }

    /// Conservative, 4px-tessellated input rectangles inside the painted hull.
    ///
    /// The top boundary uses the same smooth cubic interpolation between contour
    /// samples as GPUI painting. Rounded side and bottom corners are kept clear.
    pub fn shape_input_regions(&self) -> Vec<GeometryRect> {
        self.hull.input_regions(4.0)
    }

    pub fn contains_shape_input(&self, x: f32, y: f32) -> bool {
        self.shape_input_regions()
            .iter()
            .any(|region| region.contains_point(x, y))
    }
}

/// Stateful solver for one derived child; reconstructible from current geometry.
#[derive(Clone, Copy, Debug)]
pub struct ReactiveEnvelopeSolver {
    axes_transposed: bool,
    openness: SecondOrderChannel,
    last_parent: GeometryRect,
    local_deformation: [f32; 3],
    local_velocity: [f32; 3],
    shoulder_center: f32,
    shoulder_velocity: f32,
    child_center_x: f32,
    child_center_target: f32,
    child_velocity_x: f32,
    neck_width: f32,
    neck_velocity: f32,
    bilateral_confinement: SecondOrderChannel,
    neighbor_compression: SecondOrderChannel,
    attachment: ReactiveAttachment,
    attachment_phase: PhaseChannel,
    coupling_settled: bool,
}

impl ReactiveEnvelopeSolver {
    pub fn new(parent: GeometryRect, _viewport: SurfaceViewport) -> Result<Self, GeometryError> {
        Ok(Self {
            axes_transposed: false,
            openness: SecondOrderChannel::new(0.0, 0.0),
            last_parent: parent,
            local_deformation: [0.0; 3],
            local_velocity: [0.0; 3],
            shoulder_center: (parent.left() + parent.right()) * 0.5,
            shoulder_velocity: 0.0,
            child_center_x: (parent.left() + parent.right()) * 0.5,
            child_center_target: (parent.left() + parent.right()) * 0.5,
            child_velocity_x: 0.0,
            neck_width: parent.width() * 0.30,
            neck_velocity: 0.0,
            bilateral_confinement: SecondOrderChannel::new(0.0, 0.0),
            neighbor_compression: SecondOrderChannel::new(0.0, 0.0),
            attachment: ReactiveAttachment::Fused,
            attachment_phase: PhaseChannel::clamped(1.0, 0.0)?,
            coupling_settled: true,
        })
    }

    pub fn reset_for_parent(&mut self, parent: GeometryRect) {
        let old_center = (self.last_parent.left() + self.last_parent.right()) * 0.5;
        let new_center = (parent.left() + parent.right()) * 0.5;
        let scale = parent.width() / self.last_parent.width().max(f32::EPSILON);
        self.shoulder_center += new_center - old_center;
        self.child_center_x += new_center - old_center;
        self.child_center_target += new_center - old_center;
        self.child_velocity_x *= scale;
        self.neck_width *= scale;
        self.neck_velocity *= scale;
        self.last_parent = parent;
    }

    /// The timer keeps painting until the geometry reaches its presentation target.
    pub fn is_settled_for(&self, target_open: bool) -> bool {
        let target = if target_open { 1.0 } else { 0.0 };
        self.openness.position() == target
            && self.openness.velocity() == 0.0
            && self.local_deformation == [0.0; 3]
            && self.local_velocity == [0.0; 3]
            && self.child_center_x == self.child_center_target
            && self.child_velocity_x == 0.0
            && self.coupling_settled
    }

    /// Reuse the vertical mechanics in a cardinal parent-local frame. Edge
    /// changes reset local channels; no edge-relocation animation is implied.
    pub fn step_from_edge(
        &mut self,
        mut input: ReactiveEnvelopeInput,
        edge: crate::envelope::SurfaceEdge,
        dt: f32,
        tuning: ReactiveEnvelopeTuning,
    ) -> Result<ReactiveEnvelopeFrame, GeometryError> {
        use super::transposition::{point, rect};
        use crate::envelope::SurfaceEdge;
        let transpose = matches!(edge, SurfaceEdge::Left | SurfaceEdge::Right);
        if transpose {
            input.parent = rect(input.parent);
            input.available = rect(input.available);
            input.viewport = SurfaceViewport::try_new(input.viewport.height, input.viewport.width)?;
            input.pointer = input.pointer.map(point);
            input.neighbor = input.neighbor.map(rect);
        }
        input.growth = match edge {
            SurfaceEdge::Top | SurfaceEdge::Left => ReactiveGrowthDirection::Downward,
            SurfaceEdge::Bottom | SurfaceEdge::Right => ReactiveGrowthDirection::Upward,
        };
        if self.axes_transposed != transpose {
            *self = Self::new(input.parent, input.viewport)?;
            self.axes_transposed = transpose;
        }
        let frame = self.step_local(input, dt, tuning)?;
        Ok(if transpose { frame.transposed() } else { frame })
    }

    pub fn step(
        &mut self,
        input: ReactiveEnvelopeInput,
        dt: f32,
        tuning: ReactiveEnvelopeTuning,
    ) -> Result<ReactiveEnvelopeFrame, GeometryError> {
        if self.axes_transposed {
            *self = Self::new(input.parent, input.viewport)?;
        }
        self.step_local(input, dt, tuning)
    }

    fn step_local(
        &mut self,
        input: ReactiveEnvelopeInput,
        dt: f32,
        tuning: ReactiveEnvelopeTuning,
    ) -> Result<ReactiveEnvelopeFrame, GeometryError> {
        input.viewport.validate()?;
        tuning.validate()?;
        if !dt.is_finite() {
            return Err(GeometryError::NonFinite("reactive_envelope.dt"));
        }
        if dt <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "reactive_envelope.dt",
                value: dt,
                requirement: "> 0",
            });
        }
        if input.parent != self.last_parent {
            self.reset_for_parent(input.parent);
        }
        let target = if input.target_open { 1.0 } else { 0.0 };
        let velocity_y = if input.reduced_motion {
            self.openness.set_state(target, 0.0)?;
            0.0
        } else {
            let velocity = self
                .openness
                .step(
                    target,
                    SecondOrderSpec::new(tuning.response, tuning.damping),
                    dt,
                )?
                .velocity;
            if (self.openness.position() - target).abs() < 0.001 && velocity.abs() < 0.02 {
                self.openness.set_state(target, 0.0)?;
                0.0
            } else {
                velocity
            }
        };
        let open = self.openness.position().clamp(0.0, 1.0);
        // Direction-aware vertical frame. The body is solved in a local space
        // with the parent joint at y = 0 and growth toward positive local y, so
        // every relation below is written once and holds for both directions.
        // For `Downward` the transform is the identity on world deltas, which is
        // what keeps the pre-existing behaviour numerically unchanged.
        let frame = GrowthFrame::new(input.growth, input.parent, input.available);
        // The body's two world edges as (near, far) along the growth axis.
        // `top` is always the upper edge, because every rectangle the solver
        // emits is normalized. Which of the two edges sits at the joint, and
        // which is further along the growth direction, depends on the direction:
        // growing upward puts the far edge at `top - height`.
        let body_edges = |top: f32, height: f32| -> (f32, f32) {
            if frame.sign > 0.0 {
                (top, top + height)
            } else {
                (top, top - height)
            }
        };
        let base_height = frame.local(frame.available_far).clamp(0.0, 156.0);
        let mut height = base_height * open;
        let parent_center_x = (input.parent.left() + input.parent.right()) * 0.5;
        let half_width = input.parent.width() * 0.325;
        let parent_half_width = input.parent.width() * 0.5;
        if input.parent.left() < input.available.left()
            || input.parent.right() > input.available.right()
        {
            return Err(GeometryError::InvalidClamp {
                min: input.available.left() + parent_half_width,
                max: input.available.right() - parent_half_width,
            });
        }
        let mut min_center = input.available.left() + half_width;
        let mut max_center = input.available.right() - half_width;
        if input.attachment_intent == ReactiveAttachmentIntent::Fused {
            // The shared joint belongs to the parent's straight edge. Keep the
            // narrower child's side edges inside that span while it deforms.
            min_center = min_center.max(input.parent.left() + input.parent.radius() + half_width);
            max_center = max_center.min(input.parent.right() - input.parent.radius() - half_width);
        }
        if min_center > max_center {
            return Err(GeometryError::InvalidClamp {
                min: min_center,
                max: max_center,
            });
        }
        // CANDIDATE_DEFAULT: a small neutral span gives the child one exact
        // resting center even when the pointer stops a few pixels off-center.
        let pointer_neutral_span = input.parent.width() * 0.03;
        let requested_shift = input.pointer.map_or(0.0, |pointer| {
            let offset = pointer.x() - parent_center_x;
            let active_offset = offset.signum() * (offset.abs() - pointer_neutral_span).max(0.0);
            (active_offset * 0.12).clamp(-input.parent.width() * 0.07, input.parent.width() * 0.07)
        });
        let requested_center = parent_center_x
            + if input.target_open {
                requested_shift * open
            } else {
                0.0
            };
        // Candidate soft-edge response: as the derived body enters 65% of its
        // half-width from an available-region edge, redirect part of its
        // lateral target inward. The hard clamp below remains the final safety
        // bound; this response is only ephemeral presentation dynamics.
        // Spatial pressure uses the parent footprint; the narrower child can
        // then express the resulting strain without inventing a lateral shift.
        let edge_range = parent_half_width * 0.65;
        let left_clearance = requested_center - parent_half_width - input.available.left();
        let right_clearance = input.available.right() - (requested_center + parent_half_width);
        let edge_response = ReactiveEdgeResponse {
            left_pressure: ((edge_range - left_clearance) / edge_range).clamp(0.0, 1.0),
            right_pressure: ((edge_range - right_clearance) / edge_range).clamp(0.0, 1.0),
            directional_bias: 0.0,
            bilateral_confinement: 0.0,
        };
        let edge_response = ReactiveEdgeResponse {
            directional_bias: edge_response.right_pressure - edge_response.left_pressure,
            bilateral_confinement: edge_response
                .left_pressure
                .min(edge_response.right_pressure),
            ..edge_response
        };
        let edge_recoil =
            edge_response.directional_bias * input.parent.width() * 0.11 * tuning.edge_affinity;
        let center_target = (requested_center - edge_recoil).clamp(min_center, max_center);
        let mut pressure = (requested_center - center_target).abs() * tuning.edge_affinity;
        let confinement_target = edge_response.bilateral_confinement * open;
        if input.reduced_motion {
            self.bilateral_confinement
                .set_state(confinement_target, 0.0)?;
        } else {
            let confinement_step = self.bilateral_confinement.step(
                confinement_target,
                SecondOrderSpec::new(tuning.response, tuning.damping).scaled(0.72, 0.86)?,
                dt,
            )?;
            if (confinement_step.position - confinement_target).abs() < 0.001
                && confinement_step.velocity.abs() < 0.02
            {
                self.bilateral_confinement
                    .set_state(confinement_target, 0.0)?;
            }
        }
        let confinement_strain =
            self.bilateral_confinement.position() * base_height * tuning.bilateral_contour_gain;
        pressure += edge_response.bilateral_confinement
            * open
            * base_height
            * tuning.bilateral_contour_gain;
        self.child_center_target = center_target;
        if input.reduced_motion {
            self.child_center_x = center_target;
            self.child_velocity_x = 0.0;
        } else {
            step_second_order_pair(
                &mut self.child_center_x,
                &mut self.child_velocity_x,
                center_target,
                SecondOrderSpec::new(tuning.response, tuning.damping).scaled(0.48, 0.68)?,
                dt,
            )?;
            if (self.child_center_x - center_target).abs() < 0.01
                && self.child_velocity_x.abs() < 0.10
            {
                self.child_center_x = center_target;
                self.child_velocity_x = 0.0;
            }
        }
        let mut center_x = self.child_center_x;
        let mut child_velocity_x = self.child_velocity_x;
        if input.reduced_motion {
            self.local_deformation = [0.0; 3];
            self.local_velocity = [0.0; 3];
        } else {
            // CANDIDATE_DEFAULT spatial mapping. The kernel is shared with E11;
            // the workspace chooses sample geometry and transition drive.
            let offset = input.parent.width() * 0.25;
            let transition_drive = if input.target_open {
                -(1.0 - open).max(0.0).powf(0.25)
            } else {
                open.powf(0.25)
            };
            step_local_deformation(
                &mut self.local_deformation,
                &mut self.local_velocity,
                LocalDeformationInput {
                    node_x: [center_x - offset, center_x, center_x + offset],
                    pointer_x: input.pointer.map_or(center_x, |point| point.x()),
                    directional_drive: std::array::from_fn(|index| {
                        let side = (index as f32 - 1.0).clamp(-1.0, 1.0);
                        transition_drive * base_height + child_velocity_x * side * 0.12
                    }),
                    locality_sigma: (input.parent.width() * 0.22).max(1.0),
                    elasticity_gain: tuning.elasticity
                        * (1.0 - tuning.shape_preservation).clamp(0.0, 1.0),
                    propagation: tuning.propagation,
                    target_limit: base_height * 0.12,
                    position_limit: base_height * 0.14,
                },
                SecondOrderSpec::new(tuning.response, tuning.damping).scaled(2.2, 1.37)?,
                dt,
            )?;
            if velocity_y == 0.0
                && self.local_deformation.iter().all(|v| v.abs() < 0.001)
                && self.local_velocity.iter().all(|v| v.abs() < 0.02)
            {
                self.local_deformation = [0.0; 3];
                self.local_velocity = [0.0; 3];
            }
        }
        let local_deformation = self
            .local_deformation
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f32, f32::max)
            / base_height.max(1.0);
        let mut deformation = local_deformation.max(confinement_strain / base_height.max(1.0));
        // CANDIDATE_DEFAULT: give the connective contour enough depth for the
        // shoulder/neck profile to read as geometry, not a short connector.
        // CANDIDATE_DEFAULT: the resting bridge has a little more than 2R
        // separation so the shared-radius roundovers fit without overlap.
        let rest_attachment_gap = (input.parent.width() * 0.075).max(input.parent.radius() * 2.5);
        let resolved_gap = if input.attachment_intent == ReactiveAttachmentIntent::Fused {
            0.0
        } else {
            rest_attachment_gap
        };
        let top = frame.world(resolved_gap * open);
        let clamped_center = center_x.clamp(min_center, max_center);
        pressure += (center_x - clamped_center).abs() * tuning.edge_affinity;
        if clamped_center != center_x {
            child_velocity_x = 0.0;
        }
        center_x = clamped_center;
        self.child_center_x = center_x;
        self.child_velocity_x = child_velocity_x;
        let neighbor_pressure = input.neighbor.map_or(0.0, |neighbor| {
            let horizontally_overlapping =
                center_x + half_width > neighbor.left() && center_x - half_width < neighbor.right();
            // In local space the neighbour sits beyond the body in both
            // directions - below it when growing downward, above it when
            // growing upward - so the same two comparisons hold either way.
            let (body_near, body_far) = body_edges(top, height);
            if !horizontally_overlapping
                || frame.local(frame.far_edge(neighbor)) <= frame.local(body_near)
            {
                return 0.0;
            }
            let soft_range = (base_height * 0.65).max(1.0);
            let clearance = frame.local(frame.facing_edge(neighbor)) - frame.local(body_far);
            ((soft_range - clearance) / soft_range).clamp(0.0, 1.0) * open
        });
        let neighbor_compression_target =
            neighbor_pressure * base_height * tuning.neighbor_compression_gain;
        if input.reduced_motion {
            self.neighbor_compression
                .set_state(neighbor_compression_target, 0.0)?;
        } else {
            let step = self.neighbor_compression.step(
                neighbor_compression_target,
                SecondOrderSpec::new(tuning.response, tuning.damping).scaled(0.72, 0.86)?,
                dt,
            )?;
            if (step.position - neighbor_compression_target).abs() < 0.001
                && step.velocity.abs() < 0.02
            {
                self.neighbor_compression
                    .set_state(neighbor_compression_target, 0.0)?;
            }
        }
        let neighbor_compression = self.neighbor_compression.position().clamp(0.0, height);
        height -= neighbor_compression;
        deformation = deformation.max(neighbor_compression / base_height.max(1.0));
        pressure += neighbor_compression_target;
        if let Some(neighbor) = input.neighbor
            && center_x + half_width > neighbor.left()
            && center_x - half_width < neighbor.right()
        {
            let (body_near, body_far) = body_edges(top, height);
            let neighbor_near = frame.local(frame.facing_edge(neighbor));
            let neighbor_far = frame.local(frame.far_edge(neighbor));
            if neighbor_near < frame.local(body_far) && neighbor_far > frame.local(body_near) {
                // How far the body intrudes past the neighbour's facing edge,
                // measured along the growth axis.
                pressure += frame.local(body_far) - neighbor_near;
                height = height.min((neighbor_near - frame.local(body_near)).max(0.0));
            }
        }
        // The far bound along the growth axis: `available.bottom` when growing
        // downward, `available.top` when growing upward. Overshoot is measured
        // in local space, so viewport penetration is refused in both directions
        // without either one special-casing the other.
        let (body_near, body_far) = body_edges(top, height);
        let far_bound = frame.local(frame.available_far);
        if frame.local(body_far) > far_bound {
            pressure += frame.local(body_far) - far_bound;
            height = (far_bound - frame.local(body_near)).max(0.0);
        }
        // Exact shared token at rest; the child contour may deform while moving.
        let radius = input.parent.radius() * (1.0 - deformation * 0.08).clamp(0.75, 1.0);
        // Normalized from the two edges rather than from `top + height`, so an
        // upward body is emitted with the same validity guarantee as a downward
        // one. For `Downward` this is the original expression unchanged.
        let (edge_near, edge_far) = body_edges(top, height);
        let geometry = GeometryRect::try_new(
            center_x - half_width,
            edge_near.min(edge_far),
            center_x + half_width,
            edge_near.max(edge_far),
            radius,
        )?;
        // The contour belongs on the body's edge that faces the parent joint,
        // which is its bottom edge when the body grows downward and its top edge
        // when it grows upward. Anchoring it on `geometry.top()` unconditionally
        // would put the attachment contour on the far edge for an upward body.
        // `outer_contour` below reflects these points onto the far edge.
        let contour_anchor = frame.facing_edge(geometry);
        let contour_reflect = frame.far_edge(geometry);
        let top_contour = if geometry.height() <= 1.0 {
            [Point2::new(geometry.left(), contour_anchor); 5]
        } else {
            let contour_left = geometry.left() + geometry.radius();
            let contour_right = geometry.right() - geometry.radius();
            let contour_width = contour_right - contour_left;
            // The clamp interval is built in local space, where growth is toward
            // positive y in both directions.
            //
            // Expressed in world y it is *direction-dependent*, and that is the
            // whole difficulty: the two bounds are the near edge of the
            // available region and the body's far roundover, and which of the
            // two edges of a rectangle each of those is depends on the direction
            // of growth. Written as `available.top()` and `bottom - radius` the
            // interval is only meaningful downward - for an upward body
            // `bottom - radius` lands *above* the anchor, the interval inverts
            // silently, and the contour collapses onto the wrong bound and then
            // shoots outside the body it belongs to.
            //
            // So both bounds are taken in local space and mapped back:
            //   - the contour may not leave the available region
            //   - the contour may not indent past the body's far roundover
            let anchor_local = frame.local(contour_anchor);
            let near_available_local = frame.local(frame.near_available());
            let roundover_local = anchor_local + geometry.height() - geometry.radius();
            // Ordered rather than assumed: the two bounds can arrive inverted
            // when a body edge rises past the available region, and
            // `f32::clamp` panics on min > max.
            let (lo_local, hi_local) = if near_available_local <= roundover_local {
                (near_available_local, roundover_local)
            } else {
                (roundover_local, roundover_local)
            };
            let mut points = [Point2::new(contour_left, contour_anchor); 5];
            for (index, offset) in self.local_deformation.into_iter().enumerate() {
                let x = contour_left + contour_width * ((index + 1) as f32 / 4.0);
                let confinement_shape = if index == 1 {
                    confinement_strain
                } else {
                    -confinement_strain * 0.28
                };
                let y = frame
                    .world((anchor_local + offset + confinement_shape).clamp(lo_local, hi_local));
                points[index + 1] = Point2::new(x, y);
            }
            points[4] = Point2::new(contour_right, contour_anchor);
            points
        };
        let child_velocity = child_velocity_x;
        // CANDIDATE_DEFAULT: preserve the current resting neck widths while
        // introducing E11's independent response to stretch and velocity.
        let neck_input = NeckResponseInput {
            // CANDIDATE_DEFAULT: lateral shear lengthens the parent/child neck
            // without letting open-height alone pin the width to its minimum.
            gap: base_height * open + (center_x - parent_center_x).abs() * 0.5,
            axial_velocity: (velocity_y * base_height).hypot(child_velocity),
            velocity_lead: 0.035 * tuning.elasticity,
            rest_width: input.parent.width() * 0.30,
            minimum_width: input.parent.width() * 0.02,
            maximum_width: input.parent.width() * (19.0 / 60.0),
            stretch_gain: input.parent.width() * 0.265 / base_height.max(1.0),
        };
        let neck_target = neck_input.target()?;
        if input.reduced_motion {
            self.shoulder_center = center_x;
            self.shoulder_velocity = 0.0;
            self.neck_width = neck_target;
            self.neck_velocity = 0.0;
        } else {
            let dynamics = SecondOrderSpec::new(tuning.response, tuning.damping);
            step_shoulder_response(
                &mut self.shoulder_center,
                &mut self.shoulder_velocity,
                center_x,
                child_velocity,
                tuning.propagation,
                dynamics,
                dt,
            )?;
            step_neck_response(
                &mut self.neck_width,
                &mut self.neck_velocity,
                neck_input,
                dynamics,
                dt,
            )?;
            if child_velocity == 0.0
                && (self.shoulder_center - center_x).abs() < 0.001
                && self.shoulder_velocity.abs() < 0.02
            {
                self.shoulder_center = center_x;
                self.shoulder_velocity = 0.0;
            }
            if velocity_y == 0.0
                && (self.neck_width - neck_target).abs() < 0.05
                && self.neck_velocity.abs() < 0.10
            {
                self.neck_width = neck_target;
                self.neck_velocity = 0.0;
            }
        }
        // The connector runs from the parent joint to the body's facing edge.
        // Both are direction-aware, so the joint lies on the parent's bottom
        // edge when growing downward and on its top edge when growing upward.
        let start = Point2::new(
            self.shoulder_center
                .clamp(input.parent.left(), input.parent.right()),
            frame.joint_y,
        );
        let end = Point2::new(center_x, frame.facing_edge(geometry));
        // Topology is presentation policy derived from current geometry. The
        // solver owns no workspace identity or semantic attachment authority.
        // Strain is normalized by the parent width so the same candidate works
        // across output scales. Threshold hysteresis prevents state chatter.
        let lateral_separation = (center_x - parent_center_x).abs();
        let axial_separation = frame.local(frame.facing_edge(geometry)).max(0.0);
        let connection_strain = lateral_separation
            .hypot((axial_separation - rest_attachment_gap).max(0.0))
            / input.parent.width().max(1.0);
        // The current Bar and Envelope are axis-aligned. Their attachment-zone
        // surfaces are horizontal, so signed separation along the local +Y
        // attachment axis is the difference between these boundary positions.
        let attachment_gap = frame.local(frame.facing_edge(geometry)).max(0.0);
        let fusion_enter_gap = input.parent.radius() * 2.0;
        let fusion_exit_gap =
            fusion_enter_gap + input.parent.radius() * tuning.fusion_hysteresis_radius_ratio;
        let previous_attachment = self.attachment;
        let inferred_attachment = match previous_attachment {
            ReactiveAttachment::Detached if connection_strain > tuning.reattach_strain_ratio => {
                ReactiveAttachment::Detached
            }
            _ if open > 0.18 && connection_strain >= tuning.detach_strain_ratio => {
                ReactiveAttachment::Detached
            }
            ReactiveAttachment::Fused if attachment_gap < fusion_exit_gap => {
                ReactiveAttachment::Fused
            }
            _ if attachment_gap <= fusion_enter_gap => ReactiveAttachment::Fused,
            _ => ReactiveAttachment::Bridged,
        };
        self.attachment = match input.attachment_intent {
            ReactiveAttachmentIntent::Auto => inferred_attachment,
            ReactiveAttachmentIntent::Fused => ReactiveAttachment::Fused,
            ReactiveAttachmentIntent::Bridged => ReactiveAttachment::Bridged,
            ReactiveAttachmentIntent::Detached => ReactiveAttachment::Detached,
        };
        let attached_target = if self.attachment == ReactiveAttachment::Detached {
            0.0
        } else {
            1.0
        };
        if input.reduced_motion {
            self.attachment_phase = PhaseChannel::clamped(attached_target, 0.0)?;
        } else {
            self.attachment_phase.step(
                UnitInterval::try_new(attached_target)?,
                PhaseResponseProfile::new(
                    SecondOrderSpec::new(tuning.response, tuning.damping).scaled(0.72, 0.86)?,
                    SecondOrderSpec::new(tuning.response, tuning.damping).scaled(0.40, 0.70)?,
                    SettlePolicy::new(0.0005, 0.01, true),
                ),
                TemporalResponseMode::Full,
                dt,
            )?;
        }
        let attachment_phase = self.attachment_phase.phase().value();
        self.coupling_settled = self.shoulder_center == center_x
            && self.shoulder_velocity == 0.0
            && self.neck_width == neck_target
            && self.neck_velocity == 0.0
            && self.bilateral_confinement.position() == confinement_target
            && self.bilateral_confinement.velocity() == 0.0
            && self.neighbor_compression.position() == neighbor_compression_target
            && self.neighbor_compression.velocity() == 0.0
            && attachment_phase == attached_target
            && self.attachment_phase.velocity() == 0.0;
        let release_phase = 1.0 - attachment_phase;
        let reattach_phase = attachment_phase;
        let hull = ConnectiveHullGeometry::resolve(ConnectiveHullInput {
            parent: input.parent,
            child: geometry,
            // The hull is told the direction rather than inferring it from where
            // the two rectangles sit: the joint is a modelled fact about the
            // lineage, and the hull cannot see the lineage.
            growth: input.growth,
            attachment: self.attachment,
            topology_phase: ((attachment_gap - fusion_enter_gap)
                / (fusion_exit_gap - fusion_enter_gap).max(0.001))
            .clamp(0.0, 1.0),
            attachment_phase,
            release_phase,
            reattach_phase,
            neck_width: self.neck_width,
            mouth_width: input.parent.width() * 0.45,
            shoulder_run: 0.0,
            shoulder_center_x: self.shoulder_center,
            // Reflect the attachment contour onto the body's far edge. For
            // `Downward` this is the original expression unchanged; the
            // direction only decides which of the two edges is which.
            outer_contour: top_contour.map(|point| {
                // Keep valid inward deformation unchanged; outward deformation
                // must indent the far boundary rather than escape body bounds.
                let inward = (point.y() - contour_anchor)
                    .abs()
                    .min((geometry.height() - geometry.radius()).max(0.0));
                let old_y = contour_reflect + point.y() - contour_anchor;
                let y = if input.attachment_intent == ReactiveAttachmentIntent::Fused {
                    // A fused body keeps its interior while the far cap moves:
                    // constrain contour indentation to the far roundover tip.
                    contour_reflect - frame.sign * inward.min(geometry.radius() * 0.25)
                } else if old_y >= geometry.top()
                    && old_y <= geometry.bottom()
                    && (old_y - contour_anchor) * frame.sign >= geometry.radius()
                {
                    old_y
                } else {
                    contour_reflect - frame.sign * inward
                };
                Point2::new(point.x(), y)
            }),
            openness: open,
        })?;
        let paint_bounds = hull.bounds();
        let middle = start.lerp(end, 0.5);
        let direction = start.vector_to(end).normalized_or_zero();
        let run = start.distance_to(end) * 0.28;
        let connector = CubicJoinPair::from_controls(
            start,
            start.translated(direction.scaled(run)),
            middle.minus(direction.scaled(run)),
            middle,
            middle.translated(direction.scaled(run)),
            end.minus(direction.scaled(run)),
            end,
        );
        let neck_width = self.neck_width;
        let connector_presence = attachment_phase;
        let connector_width = neck_width * open * connector_presence;
        let connector_half = (connector_width * 0.5).max(0.0);
        // Normalized from the two joint coordinates rather than from a
        // difference that happens to be positive in one direction. Building the
        // extent as `body_bottom - parent_bottom` is what produced a negative
        // rect for an upward body; taking min/max of the two edges cannot.
        let joint_near = frame.joint_y.min(frame.facing_edge(geometry));
        let joint_far = frame.joint_y.max(frame.facing_edge(geometry));
        let connector_bounds = GeometryRect::try_new(
            (start.x().min(end.x()) - connector_half).max(input.available.left()),
            joint_near,
            (start.x().max(end.x()) + connector_half).min(input.available.right()),
            joint_far,
            0.0,
        )?;
        // Same-frame paint and input use the exact resolved derived geometry.
        self.last_parent = input.parent;
        Ok(ReactiveEnvelopeFrame {
            hull,
            geometry,
            paint_bounds,
            input_bounds: paint_bounds,
            top_contour,
            connector,
            connector_bounds,
            connector_width,
            connector_presence,
            attachment: self.attachment,
            attachment_phase,
            release_phase,
            reattach_phase,
            openness: open,
            velocity_x: child_velocity,
            velocity_y,
            neck_width,
            deformation,
            constraint_pressure: pressure,
            edge_response,
            neighbor_pressure,
            neighbor_compression,
            neighbor_compression_target,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::SurfaceViewport;

    /// Mirror a rectangle about the vertical centre of a viewport of `height`.
    fn mirror(rect: GeometryRect, height: f32) -> GeometryRect {
        GeometryRect::try_new(
            rect.left(),
            height - rect.bottom(),
            rect.right(),
            height - rect.top(),
            rect.radius(),
        )
        .expect("mirrored rect stays normalized")
    }

    /// Settle a solver over a fixed number of steps and return the last frame.
    fn settle(
        parent: GeometryRect,
        available: GeometryRect,
        viewport: SurfaceViewport,
        growth: ReactiveGrowthDirection,
        target_open: bool,
        reduced_motion: bool,
        steps: usize,
    ) -> ReactiveEnvelopeFrame {
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).expect("solver");
        let mut last = None;
        for _ in 0..steps {
            last = Some(
                solver
                    .step(
                        ReactiveEnvelopeInput {
                            attachment_intent:
                                crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                            parent,
                            available,
                            viewport,
                            target_open,
                            pointer: None,
                            neighbor: None,
                            reduced_motion,
                            growth,
                        },
                        1.0 / 60.0,
                        ReactiveEnvelopeTuning::candidate_default(),
                    )
                    .expect("step"),
            );
        }
        last.expect("at least one step")
    }

    /// A bottom-edge parent with the available region above it: the canonical
    /// AppStack case, and the case the old solver could not express at all.
    fn bottom_edge_case() -> (GeometryRect, GeometryRect, SurfaceViewport) {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let dock = GeometryRect::try_new(380.0, 736.0, 900.0, 792.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 0.0, 1280.0, 728.0, 0.0).unwrap();
        (dock, available, viewport)
    }

    /// A top-edge parent with the available region below it: the pre-existing
    /// downward shape.
    fn top_edge_case() -> (GeometryRect, GeometryRect, SurfaceViewport) {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(380.0, 8.0, 900.0, 64.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 792.0, 0.0).unwrap();
        (parent, available, viewport)
    }

    const STEPS: usize = 400;

    #[test]
    fn attachment_intent_defaults_to_auto_with_identical_frames() {
        let (parent, available, viewport) = top_edge_case();
        let mut a = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let mut b = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        for step in 0..800 {
            let input = ReactiveEnvelopeInput {
                parent,
                available,
                viewport,
                target_open: step < 500,
                pointer: (step > 200 && step < 400).then_some(Point2::new(1000.0, 170.0)),
                neighbor: None,
                reduced_motion: false,
                growth: ReactiveGrowthDirection::Downward,
                attachment_intent: ReactiveAttachmentIntent::default(),
            };
            assert_eq!(
                a.step(
                    input,
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default()
                )
                .unwrap(),
                b.step(
                    ReactiveEnvelopeInput {
                        attachment_intent: ReactiveAttachmentIntent::Auto,
                        ..input
                    },
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default()
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn explicit_fused_is_flush_simple_and_filled_during_growth_and_close() {
        use crate::grammar::connective_hull::hull_probe;
        for growth in [
            ReactiveGrowthDirection::Downward,
            ReactiveGrowthDirection::Upward,
        ] {
            let (parent, available, viewport) = if growth == ReactiveGrowthDirection::Downward {
                top_edge_case()
            } else {
                bottom_edge_parent()
            };
            for reduced_motion in [false, true] {
                let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
                for index in 0..600 {
                    let frame = solver
                        .step(
                            ReactiveEnvelopeInput {
                                parent,
                                available,
                                viewport,
                                target_open: index < 400,
                                pointer: None,
                                neighbor: None,
                                reduced_motion,
                                growth,
                                attachment_intent: ReactiveAttachmentIntent::Fused,
                            },
                            1.0 / 60.0,
                            ReactiveEnvelopeTuning::candidate_default(),
                        )
                        .unwrap();
                    assert_eq!(frame.attachment, ReactiveAttachment::Fused);
                    assert_eq!(
                        GrowthFrame::new(growth, parent, available).facing_edge(frame.geometry),
                        growth.joint(parent)
                    );
                    assert_eq!(frame.hull.contour_count(), 1);
                    let contours = hull_probe::contours(frame.hull);
                    assert_eq!(
                        hull_probe::self_intersections(&contours[0]),
                        0,
                        "{growth:?} frame {index}"
                    );
                    if frame.geometry.height() > 4.0 && frame.openness > 0.01 {
                        assert_child_body_is_enclosed(frame.hull, frame.geometry);
                    }
                }
            }
        }
    }

    #[test]
    fn fused_input_bands_are_inside_the_painted_outline() {
        use crate::grammar::connective_hull::hull_probe;
        let (parent, available, viewport) = bottom_edge_parent();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Upward,
                    attachment_intent: ReactiveAttachmentIntent::Fused,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        let contour = &hull_probe::contours(frame.hull)[0];
        let regions = frame.shape_input_regions();
        assert!(!regions.is_empty());
        for region in regions {
            // Probe strictly inside the band. A band edge can land exactly on a
            // contour vertex height, where the fill rule reports the boundary as
            // a pair of degenerate spans; the containment claim is about the
            // band's interior, not that artefact.
            let lo = region.top() + 0.001;
            let hi = region.bottom() - 0.001;
            for iy in 0..=8 {
                let y = lo + (hi - lo) * iy as f32 / 8.0;
                let intervals = hull_probe::filled_intervals_at(contour, y);
                assert!(
                    intervals
                        .iter()
                        .any(|&(a, b)| region.left() >= a - 0.001 && region.right() <= b + 0.001),
                    "region {region:?} y={y} intervals={intervals:?}"
                );
            }
        }
    }

    #[test]
    fn explicit_bridged_and_detached_use_existing_geometry() {
        let (parent, available, viewport) = top_edge_case();
        for (intent, attachment) in [
            (
                ReactiveAttachmentIntent::Bridged,
                ReactiveAttachment::Bridged,
            ),
            (
                ReactiveAttachmentIntent::Detached,
                ReactiveAttachment::Detached,
            ),
        ] {
            let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
            let result = solver
                .step(
                    ReactiveEnvelopeInput {
                        parent,
                        available,
                        viewport,
                        target_open: true,
                        pointer: None,
                        neighbor: None,
                        reduced_motion: true,
                        growth: ReactiveGrowthDirection::Downward,
                        attachment_intent: intent,
                    },
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .unwrap();
            assert_eq!(result.attachment, attachment);
            assert_eq!(
                result.geometry.top(),
                parent.bottom() + (parent.width() * 0.075).max(parent.radius() * 2.5)
            );
        }
    }

    // --- §3 / §4 direction is typed state, and defaults to the old behaviour

    // ---------------------------------------------------------------------
    // The connective hull, judged as a filled region rather than a number
    // stream. These drive the real solver in both directions; the upward case
    // is a bottom-edge parent with the body grown above it.

    /// A bottom-edge parent grown upward: the shape a Dock-anchored body takes.
    ///
    /// Named for the geometry, not for a product. The primitive does not know
    /// what the parent is, and a fixture named after one invites the reader to
    /// believe it does.
    fn bottom_edge_parent() -> (GeometryRect, GeometryRect, SurfaceViewport) {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        // A pill parent: height derived from the radius, so radius == height/2.
        // A pill is the shape that made the defect worst rather than mildest, so
        // it is the case worth regressing.
        let dock = GeometryRect::try_new(380.0, 652.0, 900.0, 746.0, 47.0).unwrap();
        let available = GeometryRect::try_new(0.0, 0.0, 1280.0, 644.0, 0.0).unwrap();
        (dock, available, viewport)
    }

    /// Assert the hull encloses the child's body: a simple outline, and a
    /// filled interior at every height inside the child.
    ///
    /// The width threshold is deliberately on the *narrowest* sampled row rather
    /// than an average or a single probe. A hull that fills its body everywhere
    /// but pinches to a stem somewhere inside it passes an average, and that
    /// pinch is exactly the failure this catches.
    fn assert_child_body_is_enclosed(
        hull: crate::grammar::ConnectiveHullGeometry,
        child: GeometryRect,
    ) {
        use crate::grammar::connective_hull::hull_probe;

        let contours = hull_probe::contours(hull);
        assert!(
            !contours.is_empty(),
            "a visible hull must carry at least one closed contour"
        );

        for contour in &contours {
            let crossings = hull_probe::self_intersections(contour);
            assert_eq!(
                crossings, 0,
                "the outline must be simple; {crossings} self-intersections means the \
                 fill rule, not the geometry, decides what is painted"
            );
        }

        let quarter = child.height() * 0.25;
        let lo = child.top() + quarter;
        let hi = child.bottom() - quarter;
        let narrowest = hull_probe::narrowest_filled_width(&contours[0], lo, hi, 24);
        let required = child.width() * 0.9;
        assert!(
            narrowest >= required,
            "the child body must be filled: narrowest painted span {narrowest:.1}px \
             over y {lo:.1}..{hi:.1} is below 90% of the body width {required:.1}px \
             (child {child:?}, hull bounds {:?})",
            hull.bounds()
        );
    }

    #[test]
    fn an_upward_hull_is_simple_and_fills_the_child_body() {
        // RED before the hull becomes direction-neutral. The upward case drew a
        // bare stem from the parent's bottom edge to the child's top, because the
        // joint gap was computed as `child.top() - parent.bottom()` and clamped
        // to zero. The body itself was then cancelled by the outline's own
        // self-intersection, so bounds described a large body while the interior
        // painted almost nothing.
        let (dock, available, viewport) = bottom_edge_parent();
        let frame = settle(
            dock,
            available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        assert_child_body_is_enclosed(frame.hull, frame.geometry);
    }

    #[test]
    fn a_downward_hull_is_simple_and_fills_the_child_body() {
        // The control. The downward path was already correct, and this must stay
        // true: if a direction-neutral rewrite breaks it, the rewrite is wrong.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(380.0, 8.0, 900.0, 64.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 792.0, 0.0).unwrap();
        let frame = settle(
            parent,
            available,
            viewport,
            ReactiveGrowthDirection::Downward,
            true,
            false,
            STEPS,
        );
        assert_child_body_is_enclosed(frame.hull, frame.geometry);
    }

    #[test]
    fn the_default_growth_direction_is_downward() {
        assert_eq!(
            ReactiveGrowthDirection::default(),
            ReactiveGrowthDirection::Downward
        );
    }

    // --- §16 the strongest evidence: upward is the same solver, not a second

    #[test]
    fn an_upward_body_mirrors_a_downward_body() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let (down_parent, down_available, _) = top_edge_case();
        let (up_parent, up_available, _) = bottom_edge_case();

        let down = settle(
            down_parent,
            down_available,
            viewport,
            ReactiveGrowthDirection::Downward,
            true,
            false,
            STEPS,
        );
        let up = settle(
            up_parent,
            up_available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );

        let down_body = down.geometry;
        let up_body = mirror(up.geometry, 800.0);

        // Body size is identical after reflection: the direction changes where
        // the body is, not how much of it there is.
        assert!(
            (down_body.width() - up_body.width()).abs() < 0.01,
            "width {} vs {}",
            down_body.width(),
            up_body.width()
        );
        assert!(
            (down_body.height() - up_body.height()).abs() < 0.5,
            "height {} vs {}",
            down_body.height(),
            up_body.height()
        );
        // Horizontal position is untouched by a vertical mirror.
        assert!(
            (down_body.left() - up_body.left()).abs() < 0.01,
            "left {} vs {}",
            down_body.left(),
            up_body.left()
        );
        // And the reflected body occupies the same absolute place.
        assert!(
            (down_body.top() - up_body.top()).abs() < 0.5,
            "top {} vs {}",
            down_body.top(),
            up_body.top()
        );
        assert!(
            (down_body.bottom() - up_body.bottom()).abs() < 0.5,
            "bottom {} vs {}",
            down_body.bottom(),
            up_body.bottom()
        );
    }

    #[test]
    fn the_dynamic_state_mirrors_between_directions() {
        // Direction transforms geometry. It must not fork the dynamics, so the
        // stateful quantities have to agree after reflection too.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let (down_parent, down_available, _) = top_edge_case();
        let (up_parent, up_available, _) = bottom_edge_case();
        let down = settle(
            down_parent,
            down_available,
            viewport,
            ReactiveGrowthDirection::Downward,
            true,
            false,
            STEPS,
        );
        let up = settle(
            up_parent,
            up_available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        assert_eq!(
            down.attachment, up.attachment,
            "attachment must be direction-neutral"
        );
        assert!((down.neck_width - up.neck_width).abs() < 0.5, "neck width");
        assert!(
            (down.constraint_pressure - up.constraint_pressure).abs() < 0.5,
            "constraint pressure {} vs {}",
            down.constraint_pressure,
            up.constraint_pressure
        );
        assert!((down.openness - up.openness).abs() < 1e-3, "openness");
        assert_eq!(
            down.neighbor_pressure, up.neighbor_pressure,
            "neighbour pressure"
        );
    }

    #[test]
    fn an_upward_body_produces_the_same_number_of_input_regions() {
        // Input geometry must follow the body in either direction, or the
        // mirrored surface would be painted where it cannot be clicked.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let (down_parent, down_available, _) = top_edge_case();
        let (up_parent, up_available, _) = bottom_edge_case();
        let down = settle(
            down_parent,
            down_available,
            viewport,
            ReactiveGrowthDirection::Downward,
            true,
            false,
            STEPS,
        );
        let up = settle(
            up_parent,
            up_available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        // Input geometry must follow the body in either direction, or the
        // mirrored surface would be painted where it cannot be clicked.
        //
        // The two fixtures are near-mirrors, not literal reflections: the solver
        // dynamics and the concave far edge differ slightly, so an exact region
        // count is not the right invariant - whole-pixel quantization can round
        // one strip away and a concave edge can split one strip in two. What must
        // hold is that the covered vertical extent mirrors about the viewport's
        // centre and that neither direction loses its input.
        let axis = viewport.height * 0.5;
        let extent = |regions: &[GeometryRect]| {
            regions.iter().fold(None, |acc: Option<(f32, f32)>, r| {
                Some(match acc {
                    None => (r.top(), r.bottom()),
                    Some((top, bottom)) => (top.min(r.top()), bottom.max(r.bottom())),
                })
            })
        };
        let down_regions = down.shape_input_regions();
        let up_regions = up.shape_input_regions();
        assert!(
            !down_regions.is_empty() && !up_regions.is_empty(),
            "both directions must produce input regions"
        );
        let (down_top, down_bottom) = extent(&down_regions).expect("down extent");
        let (up_top, up_bottom) = extent(&up_regions).expect("up extent");
        assert!(
            (down_top + up_bottom - 2.0 * axis).abs() <= 1.0
                && (down_bottom + up_top - 2.0 * axis).abs() <= 1.0,
            "input coverage must mirror: down {down_top}..{down_bottom} vs \
             up {up_top}..{up_bottom} about {axis}"
        );
        assert!(
            down_regions.len().abs_diff(up_regions.len()) <= 2,
            "input region counts must agree up to quantization and span splits: \
             {} vs {}",
            down_regions.len(),
            up_regions.len()
        );
    }

    #[test]
    fn every_hull_command_lies_within_the_hulls_own_reported_bounds() {
        // The invariant that caught a real defect in the directional work: the
        // attachment contour's clamp interval, written in world y, is only
        // meaningful downward. For an upward body the far-roundover bound lands
        // *above* the anchor, the interval inverts silently, and the contour
        // collapses onto the wrong bound and shoots outside the body it belongs
        // to - while `hull.bounds()` keeps reporting the body's own rectangle.
        //
        // So the hull is checked against itself: every point its command stream
        // names must lie within the bounds the hull reports. A mismatch means
        // the outline escapes the shape it claims to describe.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let (down_parent, down_available, _) = top_edge_case();
        let (up_parent, up_available, _) = bottom_edge_case();
        for (parent, available, direction) in [
            (
                down_parent,
                down_available,
                ReactiveGrowthDirection::Downward,
            ),
            (up_parent, up_available, ReactiveGrowthDirection::Upward),
        ] {
            let frame = settle(parent, available, viewport, direction, true, false, STEPS);
            let hull = frame.hull;
            let bounds = hull.bounds();
            let mut worst = 0.0_f32;
            for index in 0..hull.command_count() {
                let points: Vec<Point2> = match hull.command(index).expect("command") {
                    crate::grammar::HullCommand::MoveTo(p)
                    | crate::grammar::HullCommand::LineTo(p) => vec![p],
                    crate::grammar::HullCommand::CubicTo {
                        control1,
                        control2,
                        to,
                    } => vec![control1, control2, to],
                    crate::grammar::HullCommand::Close | crate::grammar::HullCommand::Empty => {
                        continue;
                    }
                };
                for p in points {
                    worst = worst
                        .max(bounds.left() - p.x())
                        .max(p.x() - bounds.right())
                        .max(bounds.top() - p.y())
                        .max(p.y() - bounds.bottom());
                }
            }
            assert!(
                worst < 0.5,
                "{direction:?}: a hull command escapes its own bounds by {worst}px; \
                 bounds={bounds:?} commands={}",
                hull.command_count()
            );
        }
    }

    // --- §17 Fused upward, the canonical AppStack case

    #[test]
    fn an_upward_body_meets_its_parent_with_no_gap() {
        let (dock, available, viewport) = bottom_edge_case();
        let frame = settle(
            dock,
            available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        assert!(frame.geometry.height() > 1.0, "the body must be visible");
        assert!(
            frame.geometry.top() < dock.top(),
            "an upward body must sit above its parent: {:?} vs {:?}",
            frame.geometry,
            dock
        );
        assert!(
            frame.geometry.bottom() < dock.top() + 1.0,
            "the body must not overlap through its parent: {:?}",
            frame.geometry
        );
        // Fused is the *initial* attachment. A fully open body settles to
        // Bridged in both directions, because attachment is an axis orthogonal
        // to growth: asserting Fused at rest would be asserting an asymmetry
        // that the solver does not have.
        let mut first_solver = ReactiveEnvelopeSolver::new(dock, viewport).expect("solver");
        let first = first_solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent: dock,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: None,
                    reduced_motion: false,
                    growth: ReactiveGrowthDirection::Upward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .expect("first step");
        assert_eq!(
            first.attachment,
            ReactiveAttachment::Fused,
            "the initial attachment is Fused"
        );
        assert!(
            first.connector_bounds.top() <= first.connector_bounds.bottom(),
            "connector bounds must stay normalized on the first step: {:?}",
            first.connector_bounds
        );
    }

    #[test]
    fn the_upward_body_never_penetrates_the_parents_top_edge() {
        // The available region is bounded by `available.top`. Overshoot in either
        // direction has to be refused.
        let (dock, available, viewport) = bottom_edge_case();
        let frame = settle(
            dock,
            available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        assert!(
            frame.geometry.top() >= available.top() - 1.0,
            "body top {} escaped available top {}",
            frame.geometry.top(),
            available.top()
        );
    }

    // --- §18 / §19 attachment states are orthogonal to direction

    #[test]
    fn every_attachment_state_resolves_upward_without_invalid_geometry() {
        // Attachment and growth direction are separate axes. Whatever the
        // attachment settles to, the geometry must stay normalized and above the
        // parent.
        let (dock, available, viewport) = bottom_edge_case();
        for reduced in [false, true] {
            let frame = settle(
                dock,
                available,
                viewport,
                ReactiveGrowthDirection::Upward,
                true,
                reduced,
                STEPS,
            );
            assert!(
                frame.geometry.width() >= 0.0 && frame.geometry.height() >= 0.0,
                "non-negative extents required: {:?}",
                frame.geometry
            );
            assert!(
                frame.geometry.left() <= frame.geometry.right(),
                "normalized width required"
            );
            assert!(
                frame.geometry.top() <= frame.geometry.bottom(),
                "normalized height required"
            );
            assert!(
                frame.connector_bounds.width() >= 0.0 && frame.connector_bounds.height() >= 0.0,
                "connector bounds must be normalized: {:?}",
                frame.connector_bounds
            );
            assert!(
                frame.geometry.bottom() <= dock.top() + 1.0,
                "the body must stay above its parent"
            );
        }
    }

    #[test]
    fn an_upward_connector_stays_within_its_parent() {
        // The connector bounds are built from min/max of the two joint
        // coordinates, so they cannot invert.
        let (dock, available, viewport) = bottom_edge_case();
        let frame = settle(
            dock,
            available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        assert!(
            frame.connector_bounds.left() <= frame.connector_bounds.right(),
            "connector width must be normalized: {:?}",
            frame.connector_bounds
        );
        assert!(
            frame.connector_bounds.top() <= frame.connector_bounds.bottom(),
            "connector height must be normalized: {:?}",
            frame.connector_bounds
        );
    }

    // --- §20 reversal, with retained velocity semantics

    #[test]
    fn an_upward_body_reverses_without_teleporting() {
        let (dock, available, viewport) = bottom_edge_case();
        let mut solver = ReactiveEnvelopeSolver::new(dock, viewport).expect("solver");
        // Frames are tracked here rather than read back from the solver: adding
        // an accessor to a capability-proven component purely so a test can
        // inspect it would put test convenience ahead of the component's shape.
        let mut run = |target_open: bool, steps: usize| -> GeometryRect {
            let mut geometry = GeometryRect::try_new(0.0, 0.0, 0.0, 0.0, 0.0).unwrap();
            for _ in 0..steps {
                geometry = solver
                    .step(
                        ReactiveEnvelopeInput {
                            attachment_intent:
                                crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                            parent: dock,
                            available,
                            viewport,
                            target_open,
                            pointer: None,
                            neighbor: None,
                            reduced_motion: false,
                            growth: ReactiveGrowthDirection::Upward,
                        },
                        1.0 / 60.0,
                        ReactiveEnvelopeTuning::candidate_default(),
                    )
                    .expect("step")
                    .geometry;
            }
            geometry
        };

        let open = run(true, STEPS);
        assert!(open.height() > 1.0, "must be open first: {open:?}");

        // Sampled a few frames in. The settle budget closes the body completely,
        // so a mid-budget sample would assert on an already-shut surface; the
        // point being made is that closing is a continuous collapse, so an early
        // partial sample has to be strictly between open and shut.
        let partial = run(false, 3);
        assert!(
            partial.height() < open.height(),
            "closing must reduce the body, not reset it: {partial:?}"
        );
        assert!(
            partial.height() > 1.0,
            "a partially closed body must still have extent: {partial:?}"
        );
        assert!(
            partial.bottom() <= dock.top() + 1.0,
            "a partially closed upward body stays above its parent: {partial:?}"
        );

        let closed = run(false, STEPS);
        assert!(
            closed.height() <= 1.0,
            "closing must reach the parent, got {closed:?}"
        );

        let reopened = run(true, STEPS);
        assert!(
            (reopened.height() - open.height()).abs() < 1.0,
            "reopening must return to the same body, {reopened:?} vs {open:?}"
        );
    }

    // --- §21 reduced motion is an accessibility policy, not a geometry one

    #[test]
    fn an_upward_reduced_motion_endpoint_matches_the_motion_endpoint() {
        let (dock, available, viewport) = bottom_edge_case();
        let animated = settle(
            dock,
            available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            false,
            STEPS,
        );
        let reduced = settle(
            dock,
            available,
            viewport,
            ReactiveGrowthDirection::Upward,
            true,
            true,
            1,
        );
        assert!(
            (animated.geometry.height() - reduced.geometry.height()).abs() < 1.0,
            "reduced motion must reach the same endpoint: {:?} vs {:?}",
            animated.geometry,
            reduced.geometry
        );
        assert_eq!(animated.attachment, reduced.attachment);
    }

    // --- §12 symmetric constraint behaviour

    #[test]
    fn an_upward_body_is_clamped_by_a_near_neighbour() {
        // The same neighbour constraint has to bite upward. A body blocked just
        // above the parent must not grow through it.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let dock = GeometryRect::try_new(380.0, 736.0, 900.0, 792.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 0.0, 1280.0, 728.0, 0.0).unwrap();
        let blocker = GeometryRect::try_new(0.0, 600.0, 1280.0, 660.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(dock, viewport).unwrap();
        let mut last = None;
        for _ in 0..STEPS {
            last = Some(
                solver
                    .step(
                        ReactiveEnvelopeInput {
                            attachment_intent:
                                crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                            parent: dock,
                            available,
                            viewport,
                            target_open: true,
                            pointer: None,
                            neighbor: Some(blocker),
                            reduced_motion: false,
                            growth: ReactiveGrowthDirection::Upward,
                        },
                        1.0 / 60.0,
                        ReactiveEnvelopeTuning::candidate_default(),
                    )
                    .expect("step"),
            );
        }
        let frame = last.expect("steps");
        assert!(
            frame.geometry.bottom() >= blocker.top() - 1.0,
            "the body must not grow through the blocker: {:?} vs {:?}",
            frame.geometry,
            blocker
        );
    }

    // --- §22 property sweep over valid geometry

    #[test]
    fn every_settled_frame_is_normalized_in_both_directions() {
        // A bounded, deterministic sweep. No randomness, so a failure is
        // reproducible from the printed case.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        for parent_top in [0.0_f32, 40.0, 300.0, 700.0, 736.0] {
            for parent_height in [40.0_f32, 56.0, 120.0] {
                let parent = GeometryRect::try_new(
                    380.0,
                    parent_top,
                    900.0,
                    parent_top + parent_height,
                    12.0,
                )
                .unwrap();
                for direction in [
                    ReactiveGrowthDirection::Downward,
                    ReactiveGrowthDirection::Upward,
                ] {
                    // The available region is the half of the viewport the body
                    // grows into. It has to contain the parent's far edge, or
                    // the region itself is inverted and the case is testing the
                    // fixture rather than the solver.
                    let available = match direction {
                        ReactiveGrowthDirection::Downward => GeometryRect::try_new(
                            0.0,
                            parent.bottom().min(780.0),
                            1280.0,
                            792.0,
                            0.0,
                        )
                        .unwrap(),
                        ReactiveGrowthDirection::Upward => {
                            GeometryRect::try_new(0.0, 0.0, 1280.0, parent.top().max(12.0), 0.0)
                                .unwrap()
                        }
                    };
                    let frame = settle(parent, available, viewport, direction, true, false, 200);
                    let g = frame.geometry;
                    let label =
                        format!("{direction:?} parent_top={parent_top} h={parent_height} -> {g:?}");
                    assert!(g.width().is_finite() && g.height().is_finite(), "{label}");
                    assert!(g.width() >= 0.0 && g.height() >= 0.0, "{label}");
                    assert!(g.left() <= g.right(), "{label}");
                    assert!(g.top() <= g.bottom(), "{label}");
                    assert!(
                        frame.connector_bounds.left() <= frame.connector_bounds.right(),
                        "{label}"
                    );
                    assert!(
                        frame.connector_bounds.top() <= frame.connector_bounds.bottom(),
                        "{label}"
                    );
                    match direction {
                        ReactiveGrowthDirection::Downward => assert!(
                            g.top() >= parent.bottom() - 60.0,
                            "a downward body must not cross above its joint: {label}"
                        ),
                        ReactiveGrowthDirection::Upward => assert!(
                            g.bottom() <= parent.top() + 60.0,
                            "an upward body must not cross below its joint: {label}"
                        ),
                    }
                }
            }
        }
    }

    #[test]
    fn local_field_changes_product_contour_and_input_bounds_together() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: Some(Point2::new(640.0, 120.0)),
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut deformed = None;
        for _ in 0..30 {
            let frame = solver
                .step(
                    input,
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .unwrap();
            if frame.deformation > 0.001 && frame.geometry.height() > 40.0 {
                deformed = Some(frame);
                break;
            }
        }
        let frame = deformed.expect("reactive field did not deform during opening");
        assert_ne!(
            frame.paint_bounds, frame.geometry,
            "deformation must alter the spatial contour bounds"
        );
        assert_eq!(frame.input_bounds, frame.paint_bounds);
        let reduced = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..input
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(reduced.paint_bounds, reduced.hull.bounds());
    }

    #[test]
    fn fusion_hysteresis_uses_distinct_enter_and_exit_gaps() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 40.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let tuning = ReactiveEnvelopeTuning::candidate_default();
        let enter_gap = parent.radius() * 2.0;
        let exit_gap = enter_gap + parent.radius() * 0.25;
        let mut saw_fused_in_opening_band = false;
        let mut opened = None;

        for _ in 0..240 {
            let frame = solver
                .step(input, 1.0 / 60.0, tuning)
                .expect("opening frame");
            let gap = frame.geometry.top() - parent.bottom();
            if gap >= enter_gap && gap < exit_gap {
                assert_eq!(frame.attachment, ReactiveAttachment::Fused);
                saw_fused_in_opening_band = true;
            }
            if gap >= exit_gap {
                opened = Some(frame);
                break;
            }
        }
        assert!(
            saw_fused_in_opening_band,
            "opening must cross the fusion band"
        );
        assert_eq!(
            opened.expect("bridged open endpoint").attachment,
            ReactiveAttachment::Bridged
        );

        let close_input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            target_open: false,
            ..input
        };
        let mut saw_bridged_in_closing_band = false;
        let mut closed = None;
        for _ in 0..240 {
            let frame = solver
                .step(close_input, 1.0 / 60.0, tuning)
                .expect("closing frame");
            let gap = frame.geometry.top() - parent.bottom();
            if gap >= enter_gap && gap < exit_gap {
                assert_eq!(frame.attachment, ReactiveAttachment::Bridged);
                saw_bridged_in_closing_band = true;
            }
            if gap < enter_gap {
                closed = Some(frame);
                break;
            }
        }
        assert!(
            saw_bridged_in_closing_band,
            "closing must cross the fusion band"
        );
        assert_eq!(
            closed.expect("fused close endpoint").attachment,
            ReactiveAttachment::Fused
        );
    }

    #[test]
    fn product_attachment_topology_tracks_geometry_and_closed_state() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: true,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let open = solver
            .step(
                input,
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(open.attachment, ReactiveAttachment::Bridged);
        assert!(open.connector_width > 1.0);
        assert_eq!(open.connector_width, open.neck_width * open.openness);
        assert!(((open.geometry.left() + open.geometry.right()) * 0.5 - 640.0).abs() < 0.001);

        let closed = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    target_open: false,
                    ..input
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(closed.attachment, ReactiveAttachment::Fused);
        assert_eq!(closed.connector_width, 0.0);
        assert_eq!(closed.attachment_phase, 1.0);
    }

    #[test]
    fn small_pointer_offsets_return_to_the_parent_center_rest_position() {
        let viewport = SurfaceViewport::try_new(1280.0, 320.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 228.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: Some(Point2::new(652.0, 40.0)),
            neighbor: Some(GeometryRect::try_new(380.0, 240.0, 900.0, 304.0, 20.0).unwrap()),
            reduced_motion: true,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let near_center = solver
            .step(
                input,
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(
            (near_center.geometry.left() + near_center.geometry.right()) * 0.5,
            640.0
        );

        let displaced = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    pointer: Some(Point2::new(260.0, 40.0)),
                    ..input
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert!(displaced.geometry.left() < near_center.geometry.left() - 20.0);

        let released = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    pointer: None,
                    ..input
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(
            (released.geometry.left() + released.geometry.right()) * 0.5,
            640.0
        );
        assert_eq!(released.attachment, ReactiveAttachment::Bridged);
        assert!(solver.is_settled_for(true));
    }

    #[test]
    fn reduced_motion_resolves_the_same_attachment_topology_endpoints() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let tuning = ReactiveEnvelopeTuning::candidate_default();
        let mut normal = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let mut reduced = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();

        let settle_normal = |solver: &mut ReactiveEnvelopeSolver, input: ReactiveEnvelopeInput| {
            let mut frame = None;
            for _ in 0..600 {
                frame = Some(solver.step(input, 1.0 / 60.0, tuning).unwrap());
                if solver.is_settled_for(input.target_open) {
                    break;
                }
            }
            frame.unwrap()
        };
        let assert_same_endpoint =
            |normal_frame: ReactiveEnvelopeFrame, reduced_frame: ReactiveEnvelopeFrame| {
                assert_eq!(reduced_frame.geometry, normal_frame.geometry);
                assert_eq!(reduced_frame.top_contour, normal_frame.top_contour);
                assert_eq!(reduced_frame.attachment, normal_frame.attachment);
                assert_eq!(reduced_frame.connector_width, normal_frame.connector_width);
                assert_eq!(
                    reduced_frame.attachment_phase,
                    normal_frame.attachment_phase
                );
                assert_eq!(reduced_frame.release_phase, normal_frame.release_phase);
                assert_eq!(reduced_frame.reattach_phase, normal_frame.reattach_phase);
            };

        let normal_open = settle_normal(&mut normal, input);
        let reduced_open = reduced
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..input
                },
                1.0 / 60.0,
                tuning,
            )
            .unwrap();
        assert_eq!(normal_open.attachment, ReactiveAttachment::Bridged);
        assert_same_endpoint(normal_open, reduced_open);
        assert!(reduced.is_settled_for(true));

        let displaced = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            pointer: Some(Point2::new(0.0, 100.0)),
            ..input
        };
        let normal_detached = settle_normal(&mut normal, displaced);
        let reduced_detached = reduced
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..displaced
                },
                1.0 / 60.0,
                tuning,
            )
            .unwrap();
        assert_eq!(normal_detached.attachment, ReactiveAttachment::Detached);
        assert_same_endpoint(normal_detached, reduced_detached);
        assert!(reduced.is_settled_for(true));

        let normal_reattached = settle_normal(&mut normal, input);
        let reduced_reattached = reduced
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..input
                },
                1.0 / 60.0,
                tuning,
            )
            .unwrap();
        assert_eq!(normal_reattached.attachment, ReactiveAttachment::Bridged);
        assert_same_endpoint(normal_reattached, reduced_reattached);
        assert!(reduced.is_settled_for(true));

        let closing = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            target_open: false,
            ..input
        };
        let normal_closed = settle_normal(&mut normal, closing);
        let reduced_closed = reduced
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..closing
                },
                1.0 / 60.0,
                tuning,
            )
            .unwrap();
        assert_eq!(normal_closed.attachment, ReactiveAttachment::Fused);
        assert_same_endpoint(normal_closed, reduced_closed);
        assert_eq!(reduced_closed.geometry.height(), 0.0);
        assert!(reduced.is_settled_for(false));
    }

    #[test]
    fn product_attachment_release_and_reattach_preserve_connector_geometry() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let tuning = ReactiveEnvelopeTuning::candidate_default();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        for _ in 0..180 {
            solver.step(input, 1.0 / 60.0, tuning).unwrap();
        }
        let attached = solver.step(input, 1.0 / 60.0, tuning).unwrap();
        assert_eq!(attached.attachment, ReactiveAttachment::Bridged);
        assert!(attached.connector_width > 1.0);

        let displaced_input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            pointer: Some(Point2::new(0.0, 100.0)),
            ..input
        };
        let mut previous_width = attached.connector_width;
        let displaced = (0..120)
            .map(|_| solver.step(displaced_input, 1.0 / 60.0, tuning).unwrap())
            .find(|frame| {
                let detached = frame.attachment == ReactiveAttachment::Detached;
                if !detached {
                    previous_width = frame.connector_width;
                }
                detached
            })
            .expect("lateral geometric strain should release the connector");
        assert!(displaced.release_phase > 0.0 && displaced.release_phase < 1.0);
        assert!(displaced.connector_width > 0.0);
        assert!(displaced.connector_width < previous_width);

        let returned = (0..120)
            .map(|_| solver.step(input, 1.0 / 60.0, tuning).unwrap())
            .find(|frame| frame.attachment == ReactiveAttachment::Bridged)
            .expect("returning inside the hysteresis band should reattach");
        assert_eq!(returned.attachment, ReactiveAttachment::Bridged);
        assert!(returned.reattach_phase > 0.0 && returned.reattach_phase < 1.0);
        let continuing = solver.step(input, 1.0 / 60.0, tuning).unwrap();
        assert!(
            (continuing.connector_width - returned.connector_width).abs() < 1.0,
            "the attachment channel should reverse continuously, without a connector jump"
        );

        let settled = (0..180)
            .map(|_| solver.step(input, 1.0 / 60.0, tuning).unwrap())
            .last()
            .unwrap();
        assert_eq!(settled.attachment, ReactiveAttachment::Bridged);
        assert!(settled.connector_width > returned.connector_width);
        assert_eq!(settled.connector_presence, 1.0);
        assert!(solver.is_settled_for(true));
    }

    #[test]
    fn parent_translation_rebases_shoulder_before_soft_edge_response() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: true,
            growth: ReactiveGrowthDirection::Downward,
        };
        solver
            .step(
                input,
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        let moved = GeometryRect::try_new(340.0, 16.0, 1100.0, 72.0, 18.0).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent: moved,
                    reduced_motion: false,
                    ..input
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert!((frame.connector.p0().x() - 720.0).abs() < 0.001);
        let body_center = (frame.geometry.left() + frame.geometry.right()) * 0.5;
        assert!(body_center < 720.0, "edge response should push inward");
        assert!(frame.velocity_x < 0.0, "inward edge response is continuous");
    }

    #[test]
    fn neck_response_lags_the_presentation_drive() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let first = solver
            .step(
                input,
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        let rest = parent.width() * 0.30;
        let algebraic = parent.width() * (0.30 * (1.0 - first.openness) + 0.035 * first.openness);
        assert!(
            (first.neck_width - rest).abs() < (algebraic - rest).abs() * 0.5,
            "neck needs an independent velocity channel, not a formula of openness"
        );
        let reduced = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..input
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert!((reduced.neck_width - parent.width() * 0.035).abs() < 0.001);
        assert!(solver.is_settled_for(true));
    }

    #[test]
    fn deformation_input_rejection_preserves_the_entire_field() {
        let mut positions = [1.0, 2.0, 3.0];
        let mut velocities = [4.0, 5.0, 6.0];
        let input = LocalDeformationInput {
            node_x: [0.0, 10.0, 20.0],
            pointer_x: 10.0,
            directional_drive: [1.0; 3],
            locality_sigma: 0.0,
            elasticity_gain: 1.0,
            propagation: 1.0,
            target_limit: 10.0,
            position_limit: 10.0,
        };
        assert!(
            step_local_deformation(
                &mut positions,
                &mut velocities,
                input,
                SecondOrderSpec::new(92.0, 13.0),
                1.0 / 60.0
            )
            .is_err()
        );
        assert_eq!(positions, [1.0, 2.0, 3.0]);
        assert_eq!(velocities, [4.0, 5.0, 6.0]);
        let invalid_last_node = LocalDeformationInput {
            locality_sigma: 10.0,
            directional_drive: [1.0, 1.0, f32::NAN],
            ..input
        };
        assert!(
            step_local_deformation(
                &mut positions,
                &mut velocities,
                invalid_last_node,
                SecondOrderSpec::new(92.0, 13.0),
                1.0 / 60.0
            )
            .is_err()
        );
        assert_eq!(positions, [1.0, 2.0, 3.0]);
        assert_eq!(velocities, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn local_deformation_outlives_drive_then_settles_without_late_geometry() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: Some(Point2::new(640.0, 100.0)),
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let mut residual_seen = false;
        for _ in 0..600 {
            let frame = solver
                .step(
                    input,
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .unwrap();
            if frame.velocity_y == 0.0 && frame.deformation > 0.0001 {
                residual_seen = true;
                assert!(!solver.is_settled_for(true));
            }
            if solver.is_settled_for(true) {
                break;
            }
        }
        assert!(
            residual_seen,
            "local deformation must retain independent velocity after the presentation drive settles"
        );
        assert!(solver.is_settled_for(true));
        let settled = solver
            .step(
                input,
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(settled.deformation, 0.0);
        assert_eq!(settled.geometry.height(), 156.0);
        assert_eq!(
            settled.geometry,
            solver
                .step(
                    input,
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default()
                )
                .unwrap()
                .geometry
        );
    }

    #[test]
    fn neighbor_bounds_the_workspace_without_changing_semantic_open_state() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap();
        let neighbor = GeometryRect::try_new(800.0, 180.0, 1120.0, 300.0, 12.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: Some(neighbor),
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        assert_eq!(frame.openness, 1.0);
        assert_eq!(frame.geometry.bottom(), neighbor.top());
        assert!(frame.constraint_pressure > 0.0);
    }

    #[test]
    fn nearby_dock_compresses_the_child_before_the_available_bound() {
        let viewport = SurfaceViewport::try_new(1280.0, 400.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 308.0, 0.0).unwrap();
        let dock = GeometryRect::try_new(380.0, 320.0, 900.0, 384.0, 20.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: Some(dock),
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();

        assert_eq!(frame.openness, 1.0);
        assert!(frame.geometry.height() < 156.0);
        assert!(frame.geometry.bottom() < available.bottom());
        assert!(frame.constraint_pressure > 0.0);
        assert!(frame.neighbor_pressure > 0.0);
        assert_eq!(
            (frame.geometry.left() + frame.geometry.right()) * 0.5,
            640.0
        );
    }

    #[test]
    fn laterally_separate_neighbor_does_not_compress_the_child() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap();
        let neighbor = GeometryRect::try_new(20.0, 100.0, 180.0, 220.0, 12.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: Some(neighbor),
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();

        assert_eq!(frame.geometry.height(), 156.0);
        assert_eq!(frame.constraint_pressure, 0.0);
        assert_eq!(frame.neighbor_pressure, 0.0);
    }

    #[test]
    fn reduced_motion_and_normal_motion_settle_to_the_same_neighbor_shape() {
        let viewport = SurfaceViewport::try_new(1280.0, 400.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 308.0, 0.0).unwrap();
        let dock = GeometryRect::try_new(380.0, 320.0, 900.0, 384.0, 20.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available,
            viewport,
            target_open: true,
            pointer: None,
            neighbor: Some(dock),
            reduced_motion: true,
            growth: ReactiveGrowthDirection::Downward,
        };
        let tuning = ReactiveEnvelopeTuning::candidate_default();
        let mut reduced_solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let reduced = reduced_solver.step(input, 1.0 / 60.0, tuning).unwrap();
        assert!(reduced.geometry.height() < 156.0);

        let mut normal_solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let mut normal = normal_solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: false,
                    ..input
                },
                1.0 / 60.0,
                tuning,
            )
            .unwrap();
        for _ in 0..240 {
            normal = normal_solver
                .step(
                    ReactiveEnvelopeInput {
                        attachment_intent:
                            crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                        reduced_motion: false,
                        ..input
                    },
                    1.0 / 60.0,
                    tuning,
                )
                .unwrap();
            if normal_solver.is_settled_for(true) {
                break;
            }
        }
        assert!(normal_solver.is_settled_for(true));
        assert_eq!(normal.geometry, reduced.geometry);
        assert_eq!(normal.paint_bounds, reduced.paint_bounds);
        assert_eq!(normal.input_bounds, reduced.input_bounds);
    }

    #[test]
    fn soft_available_edge_constraint_redirects_lateral_target_before_hard_bound() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: Some(Point2::new(1020.0, 100.0)),
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();

        let center_x = (frame.geometry.left() + frame.geometry.right()) * 0.5;
        let unbounded_target = 640.0 + (1020.0 - 640.0) * 0.12;
        assert!(
            center_x > 640.0 && center_x < unbounded_target - 8.0,
            "available-edge pressure should softly redirect the derived body before hard clamping; center={center_x:.1}, requested={unbounded_target:.1}"
        );
        assert!(frame.constraint_pressure > 0.0);
        assert!(frame.geometry.left() >= available.left());
        assert!(frame.geometry.right() <= available.right());
    }

    #[test]
    fn centered_envelope_deforms_under_bilateral_edge_confinement_without_translation() {
        let viewport = SurfaceViewport::try_new(1062.0, 943.0).unwrap();
        let parent = GeometryRect::try_new(151.0, 16.0, 911.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1062.0, 927.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();

        assert_eq!(
            (frame.geometry.left() + frame.geometry.right()) * 0.5,
            531.0,
            "symmetric edge pressure must not translate the centered envelope"
        );
        assert!(
            frame.constraint_pressure > 0.0,
            "symmetric proximity must remain observable as environmental pressure"
        );
        let expected_bilateral_pressure = 96.0 / 247.0;
        assert!((frame.edge_response.left_pressure - expected_bilateral_pressure).abs() < 0.001);
        assert!((frame.edge_response.right_pressure - expected_bilateral_pressure).abs() < 0.001);
        assert!(frame.edge_response.directional_bias.abs() < 0.001);
        assert!(
            (frame.edge_response.bilateral_confinement - expected_bilateral_pressure).abs() < 0.001
        );
        assert!(
            frame.deformation > 0.0,
            "bilateral confinement must resolve into a nonzero contour strain"
        );
        assert!(
            (frame.top_contour[2].y() - frame.geometry.top()).abs() > 1.0,
            "the center contour node must visibly depart from its unconstrained shape"
        );
    }

    #[test]
    fn centered_bilateral_confinement_is_monotonic_across_product_viewports() {
        let cases = [
            (1920.0, 0.0),
            (1280.0, 0.0),
            (1200.0, 27.0 / 247.0),
            (1062.0, 96.0 / 247.0),
            (1024.0, 115.0 / 247.0),
            (800.0, 227.0 / 247.0),
        ];
        let mut previous_pressure = -1.0_f32;
        let mut previous_strain = -1.0_f32;

        for (width, expected_pressure) in cases {
            let viewport = SurfaceViewport::try_new(width, 943.0).unwrap();
            let bar_width = (width - 32.0).min(760.0);
            let left = (width - bar_width) * 0.5;
            let parent = GeometryRect::try_new(left, 16.0, left + bar_width, 72.0, 18.0).unwrap();
            let available = GeometryRect::try_new(0.0, 72.0, width, 927.0, 0.0).unwrap();
            let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
            let frame = solver
                .step(
                    ReactiveEnvelopeInput {
                        attachment_intent:
                            crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                        parent,
                        available,
                        viewport,
                        target_open: true,
                        pointer: None,
                        neighbor: None,
                        reduced_motion: true,
                        growth: ReactiveGrowthDirection::Downward,
                    },
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .unwrap();

            assert!(
                (frame.edge_response.bilateral_confinement - expected_pressure).abs() < 0.002,
                "unexpected bilateral pressure at viewport width {width}: {}",
                frame.edge_response.bilateral_confinement
            );
            assert!(frame.edge_response.directional_bias.abs() < 0.001);
            assert!(
                ((frame.geometry.left() + frame.geometry.right()) * 0.5 - width * 0.5).abs()
                    < 0.001
            );
            assert!(frame.edge_response.bilateral_confinement >= previous_pressure);
            assert!(frame.deformation >= previous_strain);
            assert!(frame.geometry.left() >= available.left());
            assert!(frame.geometry.right() <= available.right());
            previous_pressure = frame.edge_response.bilateral_confinement;
            previous_strain = frame.deformation;
        }
    }

    #[test]
    fn unilateral_pointer_pressure_redirects_inward_and_mirrors() {
        let viewport = SurfaceViewport::try_new(1062.0, 943.0).unwrap();
        let parent = GeometryRect::try_new(151.0, 16.0, 911.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1062.0, 927.0, 0.0).unwrap();
        let mut right_solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let right = right_solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: Some(Point2::new(911.0, 40.0)),
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        let mut left_solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let left = left_solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    pointer: Some(Point2::new(151.0, 40.0)),
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        let center = (parent.left() + parent.right()) * 0.5;
        let right_center = (right.geometry.left() + right.geometry.right()) * 0.5;
        let left_center = (left.geometry.left() + left.geometry.right()) * 0.5;

        assert!(right.edge_response.directional_bias > 0.0);
        assert!(left.edge_response.directional_bias < 0.0);
        assert!(right.edge_response.bilateral_confinement > 0.0);
        assert!(left.edge_response.bilateral_confinement > 0.0);
        assert!(right_center > center && right_center < 911.0 - 0.12 * 760.0);
        assert!(left_center < center && left_center > 151.0 + 0.12 * 760.0);
        assert!((right_center + left_center - 2.0 * center).abs() < 0.01);
        assert!(right.geometry.right() < available.right());
        assert!(left.geometry.left() > available.left());
    }

    #[test]
    fn reduced_motion_resolves_the_same_bilateral_contour_endpoint() {
        let viewport = SurfaceViewport::try_new(1062.0, 943.0).unwrap();
        let parent = GeometryRect::try_new(151.0, 16.0, 911.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1062.0, 927.0, 0.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available,
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let tuning = ReactiveEnvelopeTuning::candidate_default();
        let mut normal_solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let mut normal_frame = None;
        for _ in 0..600 {
            normal_frame = Some(normal_solver.step(input, 1.0 / 60.0, tuning).unwrap());
            if normal_solver.is_settled_for(true) {
                break;
            }
        }
        let normal_frame = normal_frame.unwrap();
        assert!(normal_solver.is_settled_for(true));

        let mut reduced_solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let reduced_frame = reduced_solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    reduced_motion: true,
                    ..input
                },
                1.0 / 60.0,
                tuning,
            )
            .unwrap();

        assert_eq!(reduced_frame.geometry, normal_frame.geometry);
        assert_eq!(reduced_frame.top_contour, normal_frame.top_contour);
        assert_eq!(reduced_frame.deformation, normal_frame.deformation);
        assert_eq!(reduced_frame.edge_response, normal_frame.edge_response);
        assert!(reduced_solver.is_settled_for(true));
    }

    #[test]
    fn severe_lateral_confinement_keeps_hard_bounds_as_final_safety() {
        let viewport = SurfaceViewport::try_new(800.0, 943.0).unwrap();
        let parent = GeometryRect::try_new(16.0, 16.0, 784.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 800.0, 927.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: Some(Point2::new(784.0, 40.0)),
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();

        assert!(frame.edge_response.directional_bias > 0.0);
        assert!(frame.edge_response.bilateral_confinement > 0.0);
        assert!(frame.constraint_pressure.is_finite());
        assert!(frame.deformation.is_finite());
        assert!(frame.geometry.left() >= available.left());
        assert!(frame.geometry.right() <= available.right());
        assert!(frame.geometry.right() < available.right());
        assert!(frame.constraint_pressure > 0.0);
    }

    #[test]
    fn lateral_pointer_target_respects_available_edge_and_settles_at_clamp() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1024.0, 704.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: Some(Point2::new(1020.0, 100.0)),
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();

        assert!(frame.geometry.right() <= available.right());
        assert!(frame.geometry.right() < available.right());
        assert!(frame.constraint_pressure > 0.0);
        assert!(solver.is_settled_for(true));
    }

    #[test]
    fn rejects_parent_wider_than_available_region() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(300.0, 72.0, 900.0, 704.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        assert!(matches!(
            solver.step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: None,
                    reduced_motion: false,
                    growth: ReactiveGrowthDirection::Downward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default()
            ),
            Err(GeometryError::InvalidClamp { .. })
        ));
    }

    #[test]
    fn open_close_keeps_velocity_and_resolves_one_shared_derived_geometry() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let available = GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap();
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available,
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let tuning = ReactiveEnvelopeTuning::candidate_default();
        let first = solver.step(input, 1.0 / 60.0, tuning).unwrap();
        assert!(first.geometry.height() > 0.0 && first.geometry.height() < 156.0);
        let mut frame = first;
        for _ in 0..120 {
            frame = solver.step(input, 1.0 / 60.0, tuning).unwrap();
        }
        assert!((frame.geometry.height() - 156.0).abs() < 1.0);
        assert!(frame.neck_width > 0.0);
        assert_eq!(frame.paint_bounds, frame.hull.bounds());
        assert_eq!(frame.input_bounds, frame.hull.bounds());
        let closing = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            target_open: false,
            ..input
        };
        let closing_frame = solver.step(closing, 1.0 / 60.0, tuning).unwrap();
        assert!(closing_frame.velocity_y < 0.0);
        let reduced = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            reduced_motion: true,
            ..input
        };
        let reduced_frame = solver.step(reduced, 1.0 / 60.0, tuning).unwrap();
        assert!((reduced_frame.geometry.height() - 156.0).abs() < 0.01);
        assert_eq!(reduced_frame.velocity_y, 0.0);
        assert_eq!(reduced_frame.deformation, 0.0);
        assert!(solver.is_settled_for(true));
    }

    #[test]
    fn fusion_begins_when_local_attachment_gap_reaches_two_corner_radii() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 40.0).unwrap();
        let input = ReactiveEnvelopeInput {
            attachment_intent: crate::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
            parent,
            available: GeometryRect::try_new(0.0, 72.0, 1280.0, 704.0, 0.0).unwrap(),
            viewport,
            target_open: true,
            pointer: None,
            neighbor: None,
            reduced_motion: false,
            growth: ReactiveGrowthDirection::Downward,
        };
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let tuning = ReactiveEnvelopeTuning::candidate_default();

        for _ in 0..120 {
            let frame = solver
                .step(input, 1.0 / 60.0, tuning)
                .expect("reactive frame");
            let local_gap = frame.geometry.top() - parent.bottom();
            if local_gap > parent.width() * 0.004 && local_gap <= parent.radius() * 2.0 {
                assert_eq!(
                    frame.attachment,
                    ReactiveAttachment::Fused,
                    "fusion must begin at two shared corner radii, regardless of neck width"
                );
                return;
            }
        }
        panic!("opening should pass through the two-radius fusion threshold");
    }
}
