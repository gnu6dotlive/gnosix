//! Backend-independent GNOS.UX desktop scene kernel.
//!
//! A scene node is a visual component, not a Wayland surface. The scene
//! resolves Bar, Dock, reactive body and their relations into one SceneFrame.

use crate::{
    envelope::{
        LogicalOffset, LogicalPx, NormalizedPermille, OpacityPercent, VisualState,
        interpolate_visual,
    },
    grammar::{
        BarGeometryRecipe, CubicJoinPair, DockGeometryRecipe, GeometryError, GeometryRect,
        PhaseChannel, PhaseResponseProfile, Point2, RectInfluenceField, SecondOrderChannel,
        SecondOrderSpec, SettlePolicy, SurfaceGeometryRecipe, SurfaceViewport,
        TemporalResponseMode, UnitInterval, Vector2,
    },
};

pub const SCENE_GRAMMAR_ID: &str = "gnos.ux.scene";
pub const SCENE_GRAMMAR_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MonitorSceneId(u64);

impl MonitorSceneId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneNodeId(u64);

impl SceneNodeId {
    pub const BAR: Self = Self(1);
    pub const BODY: Self = Self(2);
    pub const DOCK: Self = Self(3);
    /// The AppStack's derived body. A distinct node from `BODY` so the scene
    /// graph names the *product* surface, not just a composition slot. The two
    /// are mutually exclusive product body modes, so they never coexist; giving
    /// them distinct ids keeps `Body` from being mistaken for product identity.
    pub const APPSTACK: Self = Self(4);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SceneEpoch(u64);

impl SceneEpoch {
    pub const INITIAL: Self = Self(0);
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn value(self) -> u64 {
        self.0
    }

    fn next(self) -> Self {
        Self(self.0.checked_add(1).expect("scene epoch exhausted"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneNodeRole {
    Bar,
    /// An internal composition slot. NOT a product identity: the AppStack's
    /// body is `SceneNodeRole::AppStack`, not this.
    Body,
    Dock,
    /// The canonical AppStack product surface, a derived body of the Dock.
    AppStack,
    Notification,
    Overlay,
    Plugin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneRelationState {
    Connected,
    Bridged,
    Detached,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneNodeFrame {
    pub id: SceneNodeId,
    pub role: SceneNodeRole,
    pub envelope: GeometryRect,
    pub velocity: Vector2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneRelationFrame {
    pub from: SceneNodeId,
    pub to: SceneNodeId,
    pub state: SceneRelationState,
    pub presence: UnitInterval,
    pub connector: CubicJoinPair,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneReservationIntent {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SceneReservationIntent {
    pub const NONE: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneCycleStep {
    Rest,
    DockReveal,
    BarMorph,
    BodyAttraction,
    Connected,
    Bridged,
    Detached,
    Relax,
    Influence,
    Reattach,
    ReturnRest,
    Complete,
}

impl SceneCycleStep {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::DockReveal => "dock-reveal",
            Self::BarMorph => "bar-morph",
            Self::BodyAttraction => "body-attraction",
            Self::Connected => "connected",
            Self::Bridged => "bridged",
            Self::Detached => "detached",
            Self::Relax => "relax",
            Self::Influence => "influence",
            Self::Reattach => "reattach",
            Self::ReturnRest => "return-rest",
            Self::Complete => "complete",
        }
    }

    fn duration(self) -> f32 {
        match self {
            Self::Rest => 0.7,
            Self::DockReveal => 0.9,
            Self::BarMorph => 0.9,
            Self::BodyAttraction => 1.1,
            Self::Connected => 1.0,
            Self::Bridged => 1.0,
            Self::Detached => 0.9,
            Self::Relax => 1.0,
            Self::Influence => 1.1,
            Self::Reattach => 1.2,
            Self::ReturnRest => 1.0,
            Self::Complete => f32::INFINITY,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Rest => Self::DockReveal,
            Self::DockReveal => Self::BarMorph,
            Self::BarMorph => Self::BodyAttraction,
            Self::BodyAttraction => Self::Connected,
            Self::Connected => Self::Bridged,
            Self::Bridged => Self::Detached,
            Self::Detached => Self::Relax,
            Self::Relax => Self::Influence,
            Self::Influence => Self::Reattach,
            Self::Reattach => Self::ReturnRest,
            Self::ReturnRest | Self::Complete => Self::Complete,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneFrame {
    pub scene_id: MonitorSceneId,
    pub epoch: SceneEpoch,
    pub viewport: SurfaceViewport,
    pub cycle_step: SceneCycleStep,
    pub nodes: Vec<SceneNodeFrame>,
    pub relations: Vec<SceneRelationFrame>,
    pub input_regions: Vec<GeometryRect>,
    pub reservation: SceneReservationIntent,
    pub influence_pressure: f32,
}

impl SceneFrame {
    pub fn node(&self, id: SceneNodeId) -> Option<&SceneNodeFrame> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn validate(&self) -> SceneValidation {
        let mut validation = SceneValidation::default();

        for (index, node) in self.nodes.iter().enumerate() {
            if self.nodes[index + 1..]
                .iter()
                .any(|other| other.id == node.id)
            {
                validation.push(SceneInvariantViolation::DuplicateNode);
            }
            if !rect_inside_viewport(node.envelope, self.viewport) {
                validation.push(SceneInvariantViolation::NodeOutsideViewport);
            }
            if !node.velocity.x().is_finite() || !node.velocity.y().is_finite() {
                validation.push(SceneInvariantViolation::NonFiniteNodeVelocity);
            }
        }

        for relation in &self.relations {
            let from_exists = self.nodes.iter().any(|node| node.id == relation.from);
            let to_exists = self.nodes.iter().any(|node| node.id == relation.to);
            if !from_exists || !to_exists {
                validation.push(SceneInvariantViolation::DanglingRelation);
            }
            if !relation.connector.validate(32, 0.02).is_valid() {
                validation.push(SceneInvariantViolation::InvalidConnector);
            }
        }

        for region in &self.input_regions {
            if !rect_inside_viewport(*region, self.viewport) {
                validation.push(SceneInvariantViolation::InputOutsideViewport);
            }
        }

        for value in [
            self.reservation.top,
            self.reservation.right,
            self.reservation.bottom,
            self.reservation.left,
            self.influence_pressure,
        ] {
            if !value.is_finite() || value < 0.0 {
                validation.push(SceneInvariantViolation::InvalidSceneScalar);
            }
        }

        validation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneInvariantViolation {
    DuplicateNode,
    NodeOutsideViewport,
    NonFiniteNodeVelocity,
    DanglingRelation,
    InvalidConnector,
    InputOutsideViewport,
    InvalidSceneScalar,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneValidation {
    pub violations: Vec<SceneInvariantViolation>,
}

impl SceneValidation {
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    fn push(&mut self, violation: SceneInvariantViolation) {
        if !self.violations.contains(&violation) {
            self.violations.push(violation);
        }
    }
}

#[derive(Clone, Debug)]
pub struct MonitorScene {
    id: MonitorSceneId,
    viewport: SurfaceViewport,
    epoch: SceneEpoch,
    cycle_step: SceneCycleStep,
    step_elapsed: f32,
    scene_elapsed: f32,
    bar_phase: PhaseChannel,
    dock_phase: PhaseChannel,
    connector_presence: PhaseChannel,
    body_x: SecondOrderChannel,
    body_y: SecondOrderChannel,
    previous_frame: Option<SceneFrame>,
    max_connector_endpoint_delta: f32,
}

impl MonitorScene {
    pub fn new(id: MonitorSceneId, viewport: SurfaceViewport) -> Result<Self, GeometryError> {
        viewport.validate()?;
        Ok(Self {
            id,
            viewport,
            epoch: SceneEpoch::INITIAL,
            cycle_step: SceneCycleStep::Rest,
            step_elapsed: 0.0,
            scene_elapsed: 0.0,
            bar_phase: PhaseChannel::try_new(0.0, 0.0)?,
            dock_phase: PhaseChannel::try_new(0.0, 0.0)?,
            connector_presence: PhaseChannel::try_new(0.0, 0.0)?,
            body_x: SecondOrderChannel::new(viewport.width * 0.5, 0.0),
            body_y: SecondOrderChannel::new(viewport.height * 0.28, 0.0),
            previous_frame: None,
            max_connector_endpoint_delta: 0.0,
        })
    }

    pub const fn id(&self) -> MonitorSceneId {
        self.id
    }
    pub const fn epoch(&self) -> SceneEpoch {
        self.epoch
    }
    pub const fn cycle_step(&self) -> SceneCycleStep {
        self.cycle_step
    }

    pub const fn script_complete(&self) -> bool {
        matches!(self.cycle_step, SceneCycleStep::Complete)
    }

    pub const fn max_connector_endpoint_delta(&self) -> f32 {
        self.max_connector_endpoint_delta
    }

    pub fn rebase_viewport(&mut self, viewport: SurfaceViewport) -> Result<(), GeometryError> {
        viewport.validate()?;
        let sx = viewport.width / self.viewport.width;
        let sy = viewport.height / self.viewport.height;
        self.body_x
            .set_state(self.body_x.position() * sx, self.body_x.velocity() * sx)?;
        self.body_y
            .set_state(self.body_y.position() * sy, self.body_y.velocity() * sy)?;
        self.viewport = viewport;
        // Absolute scene coordinates changed. Do not treat compositor rebase
        // scaling as a topology-induced connector discontinuity.
        self.previous_frame = None;
        Ok(())
    }

    pub fn tick(&mut self, dt: f32) -> Result<SceneFrame, GeometryError> {
        if !dt.is_finite() {
            return Err(GeometryError::NonFinite("scene.dt"));
        }
        if dt <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "scene.dt",
                value: dt,
                requirement: "> 0",
            });
        }

        self.advance_cycle(dt);
        let phase_profile = scene_phase_profile();

        self.bar_phase.step(
            UnitInterval::clamped(self.bar_target()),
            phase_profile,
            TemporalResponseMode::Full,
            dt,
        )?;
        self.dock_phase.step(
            UnitInterval::clamped(self.dock_target()),
            phase_profile,
            TemporalResponseMode::Full,
            dt,
        )?;
        self.connector_presence.step(
            UnitInterval::clamped(self.connector_presence_target()),
            connector_profile(),
            TemporalResponseMode::Full,
            dt,
        )?;

        let dock =
            DockGeometryRecipe::atlas_baseline().resolve(self.viewport, self.dock_phase.phase())?;
        let body_size = body_size(self.viewport);
        let mut target = self.body_target(dock.envelope, body_size);

        let provisional_body = body_rect(
            self.body_x.position(),
            self.body_y.position(),
            body_size.0,
            body_size.1,
            self.viewport,
        )?;
        let influence = self
            .influence_field()
            .map(|field| field.sample(provisional_body))
            .unwrap_or_else(crate::grammar::InfluenceSample::zero);
        target.0 += influence.vector.x() * 0.20;
        target.1 += influence.vector.y() * 0.20;

        self.body_x.step(target.0, scene_body_dynamics(), dt)?;
        self.body_y.step(target.1, scene_body_dynamics(), dt)?;

        let body = body_rect(
            self.body_x.position(),
            self.body_y.position(),
            body_size.0,
            body_size.1,
            self.viewport,
        )?;

        let bar_visual = interpolate_visual(
            standard_bar_visual(),
            hug_bar_visual(),
            NormalizedPermille::new((self.bar_phase.phase().value() * 1000.0).round() as u16),
        );
        let bar = BarGeometryRecipe::qualifier_baseline().resolve(self.viewport, bar_visual)?;

        let connector = connector_between(body, dock.visible_envelope.max_height_floor(1.0));
        let relation = SceneRelationFrame {
            from: SceneNodeId::BODY,
            to: SceneNodeId::DOCK,
            state: self.relation_state(),
            presence: self.connector_presence.phase(),
            connector,
        };

        let nodes = vec![
            SceneNodeFrame {
                id: SceneNodeId::BAR,
                role: SceneNodeRole::Bar,
                envelope: bar.envelope,
                velocity: Vector2::new(0.0, 0.0),
            },
            SceneNodeFrame {
                id: SceneNodeId::BODY,
                role: SceneNodeRole::Body,
                envelope: body,
                velocity: Vector2::new(self.body_x.velocity(), self.body_y.velocity()),
            },
            SceneNodeFrame {
                id: SceneNodeId::DOCK,
                role: SceneNodeRole::Dock,
                envelope: dock.visible_envelope,
                velocity: Vector2::new(0.0, 0.0),
            },
        ];

        let mut input_regions = Vec::new();
        input_regions.extend(bar.input_regions.iter().copied());
        input_regions.push(body);
        input_regions.extend(dock.input_regions.iter().copied());

        self.epoch = self.epoch.next();
        let frame = SceneFrame {
            scene_id: self.id,
            epoch: self.epoch,
            viewport: self.viewport,
            cycle_step: self.cycle_step,
            nodes,
            relations: vec![relation],
            input_regions,
            reservation: bar.placement.reservation(),
            influence_pressure: influence.pressure,
        };

        self.observe_continuity(&frame);
        debug_assert!(frame.validate().is_valid());
        self.previous_frame = Some(frame.clone());
        Ok(frame)
    }

    fn advance_cycle(&mut self, dt: f32) {
        self.scene_elapsed += dt;
        self.step_elapsed += dt;
        while self.step_elapsed >= self.cycle_step.duration() {
            let duration = self.cycle_step.duration();
            if !duration.is_finite() {
                break;
            }
            self.step_elapsed -= duration;
            self.cycle_step = self.cycle_step.next();
        }
    }

    fn bar_target(&self) -> f32 {
        match self.cycle_step {
            SceneCycleStep::Rest | SceneCycleStep::DockReveal => 0.0,
            SceneCycleStep::ReturnRest | SceneCycleStep::Complete => 0.0,
            _ => 1.0,
        }
    }

    fn dock_target(&self) -> f32 {
        match self.cycle_step {
            SceneCycleStep::Rest | SceneCycleStep::Complete => 0.0,
            SceneCycleStep::ReturnRest => 0.25,
            _ => 1.0,
        }
    }

    fn connector_presence_target(&self) -> f32 {
        match self.cycle_step {
            SceneCycleStep::Rest
            | SceneCycleStep::DockReveal
            | SceneCycleStep::BarMorph
            | SceneCycleStep::BodyAttraction
            | SceneCycleStep::Complete => 0.0,
            SceneCycleStep::Connected | SceneCycleStep::Bridged | SceneCycleStep::Reattach => 1.0,
            SceneCycleStep::Detached => 0.70,
            SceneCycleStep::Relax => 0.16,
            SceneCycleStep::Influence | SceneCycleStep::ReturnRest => 0.0,
        }
    }

    fn relation_state(&self) -> SceneRelationState {
        match self.cycle_step {
            SceneCycleStep::Connected => SceneRelationState::Connected,
            SceneCycleStep::Bridged | SceneCycleStep::Reattach => SceneRelationState::Bridged,
            _ => SceneRelationState::Detached,
        }
    }

    fn body_target(&self, dock: GeometryRect, size: (f32, f32)) -> (f32, f32) {
        let rest = (self.viewport.width * 0.5, self.viewport.height * 0.28);
        let dock_center = (dock.left() + dock.right()) * 0.5;
        let connected_top = (dock.top() - size.1 * 1.08).max(self.viewport.height * 0.18);

        match self.cycle_step {
            SceneCycleStep::Rest | SceneCycleStep::DockReveal | SceneCycleStep::BarMorph => rest,
            SceneCycleStep::BodyAttraction
            | SceneCycleStep::Connected
            | SceneCycleStep::Bridged
            | SceneCycleStep::Reattach => (dock_center, connected_top),
            SceneCycleStep::Detached | SceneCycleStep::Relax => {
                (self.viewport.width * 0.38, self.viewport.height * 0.34)
            }
            SceneCycleStep::Influence => (self.viewport.width * 0.54, self.viewport.height * 0.33),
            SceneCycleStep::ReturnRest | SceneCycleStep::Complete => rest,
        }
    }

    fn influence_field(&self) -> Option<RectInfluenceField> {
        if !matches!(self.cycle_step, SceneCycleStep::Influence) {
            return None;
        }
        let width = self.viewport.width * 0.16;
        let height = self.viewport.height * 0.24;
        let center_x = self.viewport.width * 0.67;
        let center_y = self.viewport.height * 0.34;
        let region = GeometryRect::try_new(
            center_x - width * 0.5,
            center_y - height * 0.5,
            center_x + width * 0.5,
            center_y + height * 0.5,
            self.viewport.min_axis() * 0.025,
        )
        .expect("scene influence geometry");
        Some(
            RectInfluenceField::try_new(region, self.viewport.min_axis() * 0.12, 0.85)
                .expect("scene influence field"),
        )
    }

    fn observe_continuity(&mut self, frame: &SceneFrame) {
        let Some(previous) = &self.previous_frame else {
            return;
        };
        let Some(current_relation) = frame.relations.first() else {
            return;
        };
        let Some(previous_relation) = previous.relations.first() else {
            return;
        };

        let delta = current_relation
            .connector
            .p0()
            .distance_to(previous_relation.connector.p0())
            .max(
                current_relation
                    .connector
                    .p2()
                    .distance_to(previous_relation.connector.p2()),
            );
        self.max_connector_endpoint_delta = self.max_connector_endpoint_delta.max(delta);
    }
}

trait GeometryRectSceneExt {
    fn max_height_floor(self, minimum: f32) -> Self;
}

impl GeometryRectSceneExt for GeometryRect {
    fn max_height_floor(self, minimum: f32) -> Self {
        if self.height() >= minimum {
            return self;
        }
        GeometryRect::try_new(
            self.left(),
            self.bottom() - minimum,
            self.right(),
            self.bottom(),
            self.radius().min(minimum * 0.5),
        )
        .expect("height-floored geometry")
    }
}

fn rect_inside_viewport(rect: GeometryRect, viewport: SurfaceViewport) -> bool {
    let epsilon = 0.05;
    rect.left() >= -epsilon
        && rect.top() >= -epsilon
        && rect.right() <= viewport.width + epsilon
        && rect.bottom() <= viewport.height + epsilon
}

fn body_size(viewport: SurfaceViewport) -> (f32, f32) {
    (
        (viewport.width * 0.28).clamp(180.0, 430.0),
        (viewport.height * 0.16).clamp(96.0, 190.0),
    )
}

fn body_rect(
    center_x: f32,
    top: f32,
    width: f32,
    height: f32,
    viewport: SurfaceViewport,
) -> Result<GeometryRect, GeometryError> {
    let half = width * 0.5;
    let left = (center_x - half).clamp(0.0, (viewport.width - width).max(0.0));
    let top = top.clamp(0.0, (viewport.height - height).max(0.0));
    GeometryRect::try_new(
        left,
        top,
        (left + width).min(viewport.width),
        (top + height).min(viewport.height),
        viewport.min_axis() * 0.035,
    )
}

fn connector_between(body: GeometryRect, dock: GeometryRect) -> CubicJoinPair {
    let p0 = Point2::new((body.left() + body.right()) * 0.5, body.bottom());
    let p2 = Point2::new((dock.left() + dock.right()) * 0.5, dock.top());
    let p1 = p0.lerp(p2, 0.52);

    let chord1 = p0.vector_to(p1);
    let chord2 = p1.vector_to(p2);
    let mid_direction = chord1
        .normalized_or_zero()
        .plus(chord2.normalized_or_zero())
        .normalized_or_zero();
    let start_direction = chord1.normalized_or_zero();
    let end_direction = chord2.normalized_or_zero();
    let h1 = chord1.length() * 0.28;
    let h2 = chord2.length() * 0.28;

    CubicJoinPair::from_controls(
        p0,
        p0.translated(start_direction.scaled(h1)),
        p1.minus(mid_direction.scaled(h1)),
        p1,
        p1.translated(mid_direction.scaled(h2)),
        p2.minus(end_direction.scaled(h2)),
        p2,
    )
}

fn scene_body_dynamics() -> SecondOrderSpec {
    SecondOrderSpec::new(68.0, 14.0)
}

fn scene_phase_profile() -> PhaseResponseProfile {
    PhaseResponseProfile::new(
        SecondOrderSpec::new(74.0, 16.0),
        SecondOrderSpec::new(120.0, 22.0),
        SettlePolicy::new(0.001, 0.01, true),
    )
}

fn connector_profile() -> PhaseResponseProfile {
    PhaseResponseProfile::new(
        SecondOrderSpec::new(52.0, 13.0),
        SecondOrderSpec::new(96.0, 19.0),
        SettlePolicy::new(0.001, 0.01, true),
    )
}

fn standard_bar_visual() -> VisualState {
    VisualState {
        opacity: OpacityPercent::new(92),
        radius: LogicalPx(0),
        margin: LogicalPx(0),
        translation: LogicalOffset { x: 0, y: 0 },
        bridge_width: LogicalPx(0),
        bridge_neck_width: LogicalPx(0),
    }
}

fn hug_bar_visual() -> VisualState {
    VisualState {
        opacity: OpacityPercent::new(92),
        radius: LogicalPx(24),
        margin: LogicalPx(48),
        translation: LogicalOffset { x: 0, y: 18 },
        bridge_width: LogicalPx(0),
        bridge_neck_width: LogicalPx(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> SurfaceViewport {
        SurfaceViewport::try_new(1200.0, 720.0).expect("viewport")
    }

    #[test]
    fn scene_frame_integrates_bar_body_and_dock_in_one_coordinate_space() {
        let mut scene = MonitorScene::new(MonitorSceneId::new(7), viewport()).expect("scene");
        let frame = scene.tick(1.0 / 60.0).expect("frame");

        assert_eq!(frame.nodes.len(), 3);
        assert!(frame.node(SceneNodeId::BAR).is_some());
        assert!(frame.node(SceneNodeId::BODY).is_some());
        assert!(frame.node(SceneNodeId::DOCK).is_some());
        assert_eq!(frame.scene_id, MonitorSceneId::new(7));
        assert!(frame.validate().is_valid());
        assert!(frame.reservation.top > 0.0);
    }

    #[test]
    fn epoch_is_monotonic_while_scene_identity_remains_stable() {
        let mut scene = MonitorScene::new(MonitorSceneId::new(9), viewport()).expect("scene");
        let first = scene.tick(1.0 / 60.0).expect("first");
        let second = scene.tick(1.0 / 60.0).expect("second");

        assert_eq!(first.scene_id, second.scene_id);
        assert!(second.epoch > first.epoch);
    }

    #[test]
    fn semantic_detach_keeps_residual_connector_presence() {
        let mut scene = MonitorScene::new(MonitorSceneId::new(11), viewport()).expect("scene");
        let mut observed = None;
        for _ in 0..900 {
            let frame = scene.tick(1.0 / 60.0).expect("frame");
            if frame.cycle_step == SceneCycleStep::Detached {
                observed = frame.relations.first().copied();
                break;
            }
        }

        let relation = observed.expect("detached relation");
        assert_eq!(relation.state, SceneRelationState::Detached);
        assert!(relation.presence.value() > 0.05);
    }

    #[test]
    fn scene_rebase_preserves_identity_and_resolves_inside_new_viewport() {
        let mut scene = MonitorScene::new(MonitorSceneId::new(13), viewport()).expect("scene");
        for _ in 0..180 {
            scene.tick(1.0 / 60.0).expect("frame");
        }
        scene
            .rebase_viewport(SurfaceViewport::try_new(900.0, 560.0).expect("viewport"))
            .expect("rebase");
        let frame = scene.tick(1.0 / 60.0).expect("frame");

        assert_eq!(frame.scene_id, MonitorSceneId::new(13));
        assert_eq!(frame.viewport.width, 900.0);
        assert_eq!(frame.viewport.height, 560.0);
        assert!(frame.validate().is_valid());
    }

    #[test]
    fn full_scene_cycle_completes_without_connector_cutoff_spike() {
        let mut scene = MonitorScene::new(MonitorSceneId::new(17), viewport()).expect("scene");
        let mut frames = 0usize;
        while !scene.script_complete() && frames < 1200 {
            let frame = scene.tick(1.0 / 60.0).expect("frame");
            assert!(frame.validate().is_valid());
            frames += 1;
        }

        assert!(scene.script_complete());
        assert!(frames > 300);
        assert!(scene.max_connector_endpoint_delta() < 24.0);
    }

    #[test]
    fn influence_step_produces_non_zero_scene_pressure() {
        let mut scene = MonitorScene::new(MonitorSceneId::new(19), viewport()).expect("scene");
        let mut peak = 0.0_f32;
        for _ in 0..1000 {
            let frame = scene.tick(1.0 / 60.0).expect("frame");
            peak = peak.max(frame.influence_pressure);
            if frame.cycle_step == SceneCycleStep::Reattach {
                break;
            }
        }
        assert!(peak > 0.0);
    }
}
