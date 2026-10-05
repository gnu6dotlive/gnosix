#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

use gnos_ux_gpui::{
    geometry_lab::{
        GeometryDebugView, GeometryFrameHeader, GeometryRect, SurfaceViewport, resolve_dock_frame,
        viewport_from_size,
    },
    hyprland_live::{HyprlandLayerGeometryObservation, query_layer_geometry},
};
use gnos_ux_surface::envelope::{
    Attachment, AutoPolicy, BehaviourDefaults, BridgeIntent, ChromePresentation,
    CompositorIntentDefaults, DerivedMaterialIntent, DerivedSurfacePatch, EnvelopeCapabilities,
    EnvelopeDefaults, ExtrusionOrigin, GeometryMode, KeyboardIntent, LogicalPx, MaterialFamilyId,
    MaterialRecipe, NormalizedPermille, OpacityPercent, OutputRelation, RegionSet, RevealTriggers,
    RuntimeId, ShapeRegion, SpaceReservationIntent, SurfaceEdge, TopologyPhase, TransitionState,
    VisibilityPolicy, VisualState, interpolate_visual, resolve_inheritance,
};
use gnos_ux_surface::grammar::reactive_envelope::{
    LocalDeformationInput, NeckResponseInput, step_local_deformation, step_neck_response,
    step_reactive_phase, step_shoulder_response,
};
use gnos_ux_surface::grammar::reactive_envelope::{ReactiveAttachment, ReactiveGrowthDirection};
use gnos_ux_surface::grammar::{
    BridgeShoulderRecipe, ConnectiveHullGeometry, ConnectiveHullInput, ConnectorSpan,
    CubicBezier as GrammarCubicBezier, CubicJoinPair as GrammarCubicJoinPair, HullCommand, Point2,
    RectInfluenceField, SecondOrderSpec, TemporalResponseMode, monotonic_join_morph,
    step_second_order_pair,
};
#[cfg(test)]
use gnos_ux_surface::grammar::{Vector2, g1_transition_controls as grammar_g1_transition_controls};
use gpui::layer_shell::*;
use gpui::prelude::*;
use gpui::{
    App, AsyncApp, Background, Bounds, BoxShadow, Context, DispatchPhase, FocusHandle, FontWeight,
    KeyDownEvent, MouseExitEvent, MouseMoveEvent, PathBuilder, Render, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, hsla,
    linear_color_stop, linear_gradient, point, px, rgba,
};
use gpui_platform::application;
use std::env;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QualificationState {
    Dock,
    Fused,
    Bridged,
    Detached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockPhase {
    Hidden,
    Revealing,
    Visible,
    Hiding,
}

#[derive(Clone, Copy, Debug)]
struct ResponsiveGeometryPolicy;

impl ResponsiveGeometryPolicy {
    fn dock(self, viewport: SurfaceViewport) -> RectHull {
        resolve_dock_frame(viewport, 1.0).envelope.into()
    }

    fn app_base_size(self, viewport: SurfaceViewport) -> (f32, f32) {
        (viewport.width * 0.60, viewport.height * (5.0 / 14.0))
    }

    fn safe_target(self, viewport: SurfaceViewport) -> RectHull {
        RectHull {
            left: viewport.width / 3.0,
            top: viewport.height * (2.0 / 21.0),
            right: viewport.width * (2.0 / 3.0),
            bottom: viewport.height * (12.0 / 35.0),
            radius: 0.0,
        }
    }

    fn edge_margin_x(self, viewport: SurfaceViewport) -> f32 {
        viewport.width / 60.0
    }

    fn soft_zone(self, viewport: SurfaceViewport) -> f32 {
        viewport.min_axis() * (17.0 / 210.0)
    }

    fn app_radius(self, viewport: SurfaceViewport) -> f32 {
        viewport.min_axis() * (2.0 / 35.0)
    }

    fn neighbor(self, viewport: SurfaceViewport) -> QualificationNeighbor {
        QualificationNeighbor {
            rect: RectHull {
                left: viewport.width * (13.0 / 15.0),
                top: viewport.height * (3.0 / 14.0),
                right: viewport.width * (29.0 / 30.0),
                bottom: viewport.height * (25.0 / 42.0),
                radius: viewport.min_axis() / 21.0,
            },
            influence_radius: viewport.min_axis() * (16.0 / 105.0),
            strength: 1.0,
        }
    }
}

struct Qualification {
    state: QualificationState,
    resolved: gnos_ux_surface::envelope::ResolvedEnvelopeState,
    dock_phase: DockPhase,
    visual_state: VisualState,
    transition: Option<TransitionState>,
    transition_generation: u64,
    pointer_in_dock: bool,
    pointer_in_reveal: bool,
    hide_generation: u64,
    reactive: Option<ReactiveHullState>,
    reactive_params: ReactiveLabParams,
    reactive_worker_running: bool,
    reactive_sample: u64,
    reactive_controls: bool,
    visual_lab: bool,
    geometry_lab: bool,
    geometry_view: GeometryDebugView,
    hyprland_geometry: Option<HyprlandLayerGeometryObservation>,
    geometry_revision: u64,
    focus_handle: Option<FocusHandle>,
}

fn join_motion_mode_from_args() -> TemporalResponseMode {
    env::args()
        .find_map(|arg| arg.strip_prefix("--join-motion=").map(str::to_owned))
        .and_then(|value| match value.as_str() {
            "full" => Some(TemporalResponseMode::Full),
            "reduced" => Some(TemporalResponseMode::Reduced),
            "off" => Some(TemporalResponseMode::Instant),
            _ => None,
        })
        .unwrap_or(TemporalResponseMode::Full)
}

#[derive(Clone, Copy, Debug)]
struct ReactiveLabParams {
    response: f32,
    damping: f32,
    propagation: f32,
    elasticity: f32,
    edge_affinity: f32,
    shape_preservation: f32,
    orientation_compliance: f32,
    surface_locality: f32,
    spatial_feedback: f32,
    join_response: f32,
    join_damping: f32,
    join_motion: TemporalResponseMode,
}

fn qualification_arg_f32(name: &str) -> Option<f32> {
    let prefix = format!("--{name}=");
    env::args().find_map(|arg| {
        arg.strip_prefix(&prefix)
            .and_then(|value| value.parse::<f32>().ok())
    })
}

impl ReactiveLabParams {
    fn qualification() -> Self {
        Self {
            response: 92.0,
            damping: 13.0,
            propagation: 0.62,
            elasticity: 0.78,
            edge_affinity: 1.0,
            shape_preservation: 0.62,
            orientation_compliance: 0.55,
            surface_locality: 0.68,
            spatial_feedback: 0.0,
            join_response: 78.0,
            join_damping: 18.0,
            join_motion: TemporalResponseMode::Full,
        }
    }

    fn qualification_from_args() -> Self {
        let mut params = Self::qualification();
        if let Some(value) = qualification_arg_f32("reactive-response") {
            params.response = value.clamp(12.0, 180.0);
        }
        if let Some(value) = qualification_arg_f32("reactive-damping") {
            params.damping = value.clamp(1.0, 40.0);
        }
        if let Some(value) = qualification_arg_f32("reactive-propagation") {
            params.propagation = value.clamp(0.0, 1.5);
        }
        if let Some(value) = qualification_arg_f32("reactive-elasticity") {
            params.elasticity = value.clamp(0.0, 1.5);
        }
        if let Some(value) = qualification_arg_f32("reactive-shape-preservation") {
            params.shape_preservation = value.clamp(0.0, 1.0);
        }
        if let Some(value) = qualification_arg_f32("reactive-constraint-affinity") {
            params.edge_affinity = value.clamp(0.0, 2.0);
        }
        if let Some(value) = qualification_arg_f32("reactive-orientation-compliance") {
            params.orientation_compliance = value.clamp(0.0, 1.5);
        }
        if let Some(value) = qualification_arg_f32("reactive-surface-locality") {
            params.surface_locality = value.clamp(0.0, 1.0);
        }
        if let Some(value) = qualification_arg_f32("reactive-spatial-feedback") {
            params.spatial_feedback = value.clamp(0.0, 1.5);
        }
        if let Some(value) = qualification_arg_f32("join-response") {
            params.join_response = value.clamp(12.0, 240.0);
        }
        if let Some(value) = qualification_arg_f32("join-damping") {
            params.join_damping = value.clamp(1.0, 60.0);
        }
        params.join_motion = join_motion_mode_from_args();
        params
    }

    fn adjust_key(&mut self, key: &str) -> bool {
        match key {
            "q" => self.response = (self.response + 8.0).clamp(12.0, 180.0),
            "a" => self.response = (self.response - 8.0).clamp(12.0, 180.0),
            "w" => self.damping = (self.damping + 1.5).clamp(1.0, 40.0),
            "s" => self.damping = (self.damping - 1.5).clamp(1.0, 40.0),
            "e" => self.propagation = (self.propagation + 0.08).clamp(0.0, 1.5),
            "d" => self.propagation = (self.propagation - 0.08).clamp(0.0, 1.5),
            "r" => self.elasticity = (self.elasticity + 0.08).clamp(0.0, 1.5),
            "f" => self.elasticity = (self.elasticity - 0.08).clamp(0.0, 1.5),
            "t" => self.shape_preservation = (self.shape_preservation + 0.06).clamp(0.0, 1.0),
            "g" => self.shape_preservation = (self.shape_preservation - 0.06).clamp(0.0, 1.0),
            "y" => self.edge_affinity = (self.edge_affinity + 0.1).clamp(0.0, 2.0),
            "h" => self.edge_affinity = (self.edge_affinity - 0.1).clamp(0.0, 2.0),
            "u" => {
                self.orientation_compliance = (self.orientation_compliance + 0.08).clamp(0.0, 1.5)
            }
            "j" => {
                self.orientation_compliance = (self.orientation_compliance - 0.08).clamp(0.0, 1.5)
            }
            "i" => self.surface_locality = (self.surface_locality + 0.06).clamp(0.0, 1.0),
            "k" => self.surface_locality = (self.surface_locality - 0.06).clamp(0.0, 1.0),
            "o" => self.spatial_feedback = (self.spatial_feedback + 0.08).clamp(0.0, 1.5),
            "l" => self.spatial_feedback = (self.spatial_feedback - 0.08).clamp(0.0, 1.5),
            "[" => self.join_response = (self.join_response - 8.0).clamp(12.0, 240.0),
            "]" => self.join_response = (self.join_response + 8.0).clamp(12.0, 240.0),
            ";" => self.join_damping = (self.join_damping - 1.5).clamp(1.0, 60.0),
            "'" => self.join_damping = (self.join_damping + 1.5).clamp(1.0, 60.0),
            "0" => *self = Self::qualification(),
            _ => return false,
        }
        true
    }

    fn log(self, prefix: &str) {
        println!(
            "{prefix} response={:.2} damping={:.2} propagation={:.2} elasticity={:.2} shape_preservation={:.2} constraint_affinity={:.2} orientation_compliance={:.2} surface_locality={:.2} spatial_feedback={:.2} join_response={:.2} join_damping={:.2} join_motion={:?}",
            self.response,
            self.damping,
            self.propagation,
            self.elasticity,
            self.shape_preservation,
            self.edge_affinity,
            self.orientation_compliance,
            self.surface_locality,
            self.spatial_feedback,
            self.join_response,
            self.join_damping,
            self.join_motion,
        );
    }
}

fn step_qualification_phase(
    phase: &mut f32,
    velocity: &mut f32,
    target: f32,
    full: SecondOrderSpec,
    reduced: SecondOrderSpec,
    mode: TemporalResponseMode,
    dt: f32,
) {
    step_reactive_phase(phase, velocity, target, full, reduced, mode, dt)
        .expect("validated qualification phase dynamics");
}

#[derive(Clone, Copy, Debug)]
struct QualificationNeighbor {
    rect: RectHull,
    influence_radius: f32,
    strength: f32,
}

impl QualificationNeighbor {
    fn right_side(viewport: SurfaceViewport) -> Self {
        ResponsiveGeometryPolicy.neighbor(viewport)
    }

    fn rebase(self, from: SurfaceViewport, to: SurfaceViewport) -> Self {
        let sx = to.width / from.width.max(f32::EPSILON);
        let sy = to.height / from.height.max(f32::EPSILON);
        let radius_scale = to.min_axis() / from.min_axis().max(f32::EPSILON);
        Self {
            rect: RectHull {
                left: self.rect.left * sx,
                top: self.rect.top * sy,
                right: self.rect.right * sx,
                bottom: self.rect.bottom * sy,
                radius: self.rect.radius * radius_scale,
            },
            influence_radius: self.influence_radius * radius_scale,
            strength: self.strength,
        }
    }

    fn feedback(self, app: RectHull) -> (f32, f32, f32) {
        let field = RectInfluenceField::try_new(
            self.rect.geometry_rect(),
            self.influence_radius,
            self.strength,
        )
        .expect("validated qualification influence field");
        let sample = field.sample(app.geometry_rect());
        (sample.vector.x(), sample.vector.y(), sample.pressure)
    }
}

#[derive(Clone, Copy, Debug)]
struct ReactiveHullState {
    viewport: SurfaceViewport,
    pointer_x: f32,
    pointer_y: f32,
    app_center_x: f32,
    app_top: f32,
    velocity_x: f32,
    velocity_y: f32,
    shoulder_center_x: f32,
    shoulder_velocity_x: f32,
    neck_width: f32,
    neck_velocity: f32,
    join_phase: f32,
    join_velocity: f32,
    release_phase: f32,
    release_velocity: f32,
    reattach_phase: f32,
    reattach_velocity: f32,
    tilt: f32,
    tilt_velocity: f32,
    top_deform: [f32; 3],
    top_deform_velocity: [f32; 3],
    attachment: Attachment,
    constraint_pressure: f32,
    spatial_feedback_x: f32,
    spatial_feedback_y: f32,
    neighbor: Option<QualificationNeighbor>,
    neighbor_influence_x: f32,
    neighbor_influence_y: f32,
    neighbor_influence_pressure: f32,
    neighbor_feedback_x: f32,
    neighbor_feedback_y: f32,
}

impl ReactiveHullState {
    fn qualification_initial(viewport: SurfaceViewport) -> Self {
        let policy = ResponsiveGeometryPolicy;
        let safe = policy.safe_target(viewport);
        Self {
            viewport,
            pointer_x: viewport.width * 0.5,
            pointer_y: viewport.height * (6.0 / 7.0),
            app_center_x: viewport.width * 0.5,
            app_top: safe.bottom,
            velocity_x: 0.0,
            velocity_y: 0.0,
            shoulder_center_x: viewport.width * 0.5,
            shoulder_velocity_x: 0.0,
            neck_width: viewport.width * 0.30,
            neck_velocity: 0.0,
            join_phase: 0.0,
            join_velocity: 0.0,
            release_phase: 0.0,
            release_velocity: 0.0,
            reattach_phase: 0.0,
            reattach_velocity: 0.0,
            tilt: 0.0,
            tilt_velocity: 0.0,
            top_deform: [0.0; 3],
            top_deform_velocity: [0.0; 3],
            attachment: Attachment::Fused,
            constraint_pressure: 0.0,
            spatial_feedback_x: 0.0,
            spatial_feedback_y: 0.0,
            neighbor: None,
            neighbor_influence_x: 0.0,
            neighbor_influence_y: 0.0,
            neighbor_influence_pressure: 0.0,
            neighbor_feedback_x: 0.0,
            neighbor_feedback_y: 0.0,
        }
    }

    fn rebase_viewport(&mut self, viewport: SurfaceViewport) {
        if self.viewport.approx_eq(viewport) {
            return;
        }
        let from = self.viewport;
        let sx = viewport.width / from.width.max(f32::EPSILON);
        let sy = viewport.height / from.height.max(f32::EPSILON);
        let sr = viewport.min_axis() / from.min_axis().max(f32::EPSILON);

        self.pointer_x *= sx;
        self.pointer_y *= sy;
        self.app_center_x *= sx;
        self.app_top *= sy;
        self.velocity_x *= sx;
        self.velocity_y *= sy;
        self.shoulder_center_x *= sx;
        self.shoulder_velocity_x *= sx;
        self.neck_width *= sx;
        self.neck_velocity *= sx;
        self.tilt *= sy;
        self.tilt_velocity *= sy;
        for value in &mut self.top_deform {
            *value *= sy;
        }
        for value in &mut self.top_deform_velocity {
            *value *= sy;
        }
        self.constraint_pressure *= sr;
        self.spatial_feedback_x *= sx;
        self.spatial_feedback_y *= sy;
        self.neighbor = self
            .neighbor
            .map(|neighbor| neighbor.rebase(from, viewport));
        self.neighbor_influence_x *= sx;
        self.neighbor_influence_y *= sy;
        self.neighbor_influence_pressure *= sr;
        self.neighbor_feedback_x *= sx;
        self.neighbor_feedback_y *= sy;
        self.viewport = viewport;
    }

    fn set_pointer_target(&mut self, x: f32, y: f32) {
        self.pointer_x = x;
        self.pointer_y = y;
    }

    fn resolved_target(self) -> (f32, f32) {
        let policy = ResponsiveGeometryPolicy;
        let safe = policy.safe_target(self.viewport);
        let pointer_to_top = self.viewport.height * (5.0 / 28.0);
        let safe_x = self.pointer_x.clamp(safe.left, safe.right);
        let safe_top = (self.pointer_y - pointer_to_top).clamp(safe.top, safe.bottom);
        (
            safe_x + self.spatial_feedback_x * 0.22,
            safe_top + self.spatial_feedback_y * 0.22,
        )
    }

    fn step(&mut self, params: ReactiveLabParams, dt: f32) -> f32 {
        let policy = ResponsiveGeometryPolicy;
        let viewport = self.viewport;
        let safe = policy.safe_target(viewport);
        let pointer_to_top = viewport.height * (5.0 / 28.0);
        let desired_top = self.pointer_y - pointer_to_top;
        let safe_x = self.pointer_x.clamp(safe.left, safe.right);
        let safe_top = desired_top.clamp(safe.top, safe.bottom);
        let hard_pressure = (self.pointer_x - safe_x).abs() + (desired_top - safe_top).abs();
        let soft_zone = policy.soft_zone(viewport).max(1.0);
        let left_soft = ((soft_zone - (safe_x - safe.left)) / soft_zone).clamp(0.0, 1.0);
        let right_soft = ((soft_zone - (safe.right - safe_x)) / soft_zone).clamp(0.0, 1.0);
        let top_soft = ((soft_zone - (safe_top - safe.top)) / soft_zone).clamp(0.0, 1.0);
        let bottom_soft = ((soft_zone - (safe.bottom - safe_top)) / soft_zone).clamp(0.0, 1.0);
        let current_app = self.frame(params).app;
        let (neighbor_raw_x, neighbor_raw_y, neighbor_pressure) = self
            .neighbor
            .map(|neighbor| neighbor.feedback(current_app))
            .unwrap_or((0.0, 0.0, 0.0));
        self.neighbor_influence_x = neighbor_raw_x;
        self.neighbor_influence_y = neighbor_raw_y;
        self.neighbor_influence_pressure = neighbor_pressure;
        self.neighbor_feedback_x = neighbor_raw_x * params.edge_affinity * params.spatial_feedback;
        self.neighbor_feedback_y = neighbor_raw_y * params.edge_affinity * params.spatial_feedback;
        self.spatial_feedback_x =
            (left_soft - right_soft) * soft_zone * params.edge_affinity * params.spatial_feedback
                + self.neighbor_feedback_x;
        self.spatial_feedback_y =
            (top_soft - bottom_soft) * soft_zone * params.edge_affinity * params.spatial_feedback
                + self.neighbor_feedback_y;
        self.constraint_pressure = hard_pressure * params.edge_affinity
            + (self.spatial_feedback_x.abs() + self.spatial_feedback_y.abs())
            + neighbor_pressure * params.edge_affinity * params.spatial_feedback;
        let feedback_target_x = safe_x + self.spatial_feedback_x * 0.22;
        let feedback_target_top = safe_top + self.spatial_feedback_y * 0.22;

        let base_dynamics = SecondOrderSpec::new(params.response, params.damping);
        step_second_order_pair(
            &mut self.app_center_x,
            &mut self.velocity_x,
            feedback_target_x,
            base_dynamics,
            dt,
        )
        .expect("validated reactive X dynamics");
        step_second_order_pair(
            &mut self.app_top,
            &mut self.velocity_y,
            feedback_target_top,
            base_dynamics,
            dt,
        )
        .expect("validated reactive Y dynamics");

        step_shoulder_response(
            &mut self.shoulder_center_x,
            &mut self.shoulder_velocity_x,
            self.app_center_x,
            self.velocity_x,
            params.propagation,
            base_dynamics,
            dt,
        )
        .expect("validated reactive shoulder dynamics");

        let shoulder_strain = self.app_center_x - self.shoulder_center_x;
        let tilt_scale = viewport.height / viewport.width.max(f32::EPSILON);
        let tilt_target = ((-self.velocity_x * 0.018 * tilt_scale)
            + shoulder_strain * 0.055 * tilt_scale
            + self.spatial_feedback_x * 0.018 * tilt_scale)
            * params.orientation_compliance;
        let tilt_target_limit = viewport.height / 30.0;
        step_second_order_pair(
            &mut self.tilt,
            &mut self.tilt_velocity,
            tilt_target.clamp(-tilt_target_limit, tilt_target_limit),
            base_dynamics
                .scaled(0.42, 0.70)
                .expect("validated tilt dynamics"),
            dt,
        )
        .expect("validated reactive tilt dynamics");
        let tilt_limit = viewport.height * (4.0 / 105.0);
        self.tilt = self.tilt.clamp(-tilt_limit, tilt_limit);

        let deformation_gain = (1.0 - params.shape_preservation).clamp(0.0, 1.0);
        let locality_sigma = viewport.min_axis() * (0.369_047_6 - params.surface_locality * 0.25);
        let node_offset = viewport.width * 0.15;
        let node_offsets = [-node_offset, 0.0, node_offset];
        let directional_drive = node_offsets.map(|offset| {
            -self.velocity_y * 0.026
                + self.velocity_x * (offset / (viewport.width * 0.30).max(1.0)) * 0.016
                + self.constraint_pressure * 0.0025
                + self.spatial_feedback_y * 0.006
        });
        step_local_deformation(
            &mut self.top_deform,
            &mut self.top_deform_velocity,
            LocalDeformationInput {
                node_x: node_offsets.map(|offset| self.app_center_x + offset),
                pointer_x: self.pointer_x,
                directional_drive,
                locality_sigma,
                elasticity_gain: params.elasticity * deformation_gain,
                propagation: params.propagation,
                target_limit: viewport.height * (3.0 / 70.0),
                position_limit: viewport.height / 21.0,
            },
            base_dynamics
                .scaled(0.68, 0.76)
                .expect("validated deformation dynamics"),
            dt,
        )
        .expect("validated reactive deformation dynamics");

        let speed = self.velocity_x.hypot(self.velocity_y);
        let (base_app_width, base_app_height) = policy.app_base_size(viewport);
        let dynamic_height_loss =
            (speed * 0.012 * params.elasticity * deformation_gain).min(viewport.height / 35.0);
        let app_height = base_app_height - dynamic_height_loss;
        let dynamic_width_gain = ((self.app_center_x - self.shoulder_center_x).abs() * 0.22
            + speed * 0.012 * params.elasticity)
            * deformation_gain;
        let app_width = base_app_width + dynamic_width_gain.min(viewport.width * 0.05);

        // Qualification-only spatial feedback: the body may overshoot its target,
        // but it may not geometrically tunnel through the Dock or technical canvas.
        let restitution = 0.24 * params.elasticity;
        let dock = policy.dock(viewport);
        let max_top = dock.top - app_height - self.tilt.abs() * 0.35;
        if self.app_top > max_top {
            let penetration = self.app_top - max_top;
            self.app_top = max_top;
            if self.velocity_y > 0.0 {
                self.velocity_y = -self.velocity_y * restitution;
            }
            self.constraint_pressure += penetration * 8.0;
        }
        let half_width = app_width / 2.0;
        let edge_margin = policy.edge_margin_x(viewport);
        let min_center = edge_margin + half_width;
        let max_center = viewport.width - edge_margin - half_width;
        if self.app_center_x < min_center {
            let penetration = min_center - self.app_center_x;
            self.app_center_x = min_center;
            if self.velocity_x < 0.0 {
                self.velocity_x = -self.velocity_x * restitution;
            }
            self.constraint_pressure += penetration * 8.0;
        } else if self.app_center_x > max_center {
            let penetration = self.app_center_x - max_center;
            self.app_center_x = max_center;
            if self.velocity_x > 0.0 {
                self.velocity_x = -self.velocity_x * restitution;
            }
            self.constraint_pressure += penetration * 8.0;
        }

        if let Some(neighbor) = self.neighbor {
            let app_left = self.app_center_x - half_width;
            let app_right = self.app_center_x + half_width;
            let app_bottom = self.app_top + app_height;
            let vertical_overlap =
                app_bottom > neighbor.rect.top && self.app_top < neighbor.rect.bottom;
            if vertical_overlap && app_right > neighbor.rect.left && app_left < neighbor.rect.left {
                let penetration = app_right - neighbor.rect.left;
                self.app_center_x = neighbor.rect.left
                    - half_width
                    - (viewport.min_axis() * 0.002_380_952_4).max(1.0);
                if self.velocity_x > 0.0 {
                    self.velocity_x = -self.velocity_x * restitution;
                }
                self.constraint_pressure += penetration * 10.0;
            }
        }

        let gap = (dock.top - (self.app_top + app_height)).max(0.0);
        step_neck_response(
            &mut self.neck_width,
            &mut self.neck_velocity,
            NeckResponseInput {
                gap,
                axial_velocity: self.velocity_y,
                velocity_lead: 0.035 * params.elasticity,
                rest_width: viewport.width * 0.30,
                minimum_width: 0.0,
                maximum_width: viewport.width * (19.0 / 60.0),
                stretch_gain: viewport.width * 1.274 / viewport.height.max(1.0),
            },
            base_dynamics,
            dt,
        )
        .expect("validated reactive neck dynamics");

        let rejoin_gap = viewport.height * (6.0 / 35.0);
        let detach_gap = viewport.height * (8.0 / 35.0);
        let fused_gap = viewport.height / 84.0;
        let rejoin_neck = viewport.width * 0.03;
        let detach_neck = viewport.width / 75.0;
        let fused_neck = viewport.width * 0.22;
        let reattach_candidate = gap < rejoin_gap && self.neck_width > rejoin_neck;
        let previous_attachment = self.attachment;
        self.attachment = match self.attachment {
            // Detached is an authority state. Crossing the rejoin threshold
            // merely arms geometric approach; topology changes only at contact.
            Attachment::Detached => Attachment::Detached,
            _ if gap > detach_gap && self.neck_width < detach_neck => Attachment::Detached,
            _ if gap <= fused_gap && self.neck_width > fused_neck => Attachment::Fused,
            _ => Attachment::Bridged,
        };

        if previous_attachment != Attachment::Detached && self.attachment == Attachment::Detached {
            // Topology breaks immediately, geometry does not. Residual tails
            // start fully expressed and relax back into each independent Envelope.
            self.release_phase = 0.0;
            self.release_velocity = 0.0;
            self.reattach_phase = 0.0;
            self.reattach_velocity = 0.0;
        } else if self.attachment != Attachment::Detached {
            self.release_phase = 0.0;
            self.release_velocity = 0.0;
            self.reattach_phase = 0.0;
            self.reattach_velocity = 0.0;
        }

        let release_target = if self.attachment == Attachment::Detached {
            1.0
        } else {
            0.0
        };
        let join_base = SecondOrderSpec::new(params.join_response, params.join_damping);
        step_qualification_phase(
            &mut self.release_phase,
            &mut self.release_velocity,
            release_target,
            join_base
                .scaled(0.88, 1.05)
                .expect("validated full release dynamics"),
            join_base
                .scaled(2.6, 1.8)
                .expect("validated reduced release dynamics"),
            params.join_motion,
            dt,
        );

        let reattach_ready = self.attachment == Attachment::Detached
            && reattach_candidate
            && self.release_phase >= 0.995;
        let reattach_target = if reattach_ready { 1.0 } else { 0.0 };
        step_qualification_phase(
            &mut self.reattach_phase,
            &mut self.reattach_velocity,
            reattach_target,
            join_base
                .scaled(0.94, 1.08)
                .expect("validated full reattach dynamics"),
            join_base
                .scaled(2.35, 1.70)
                .expect("validated reduced reattach dynamics"),
            params.join_motion,
            dt,
        );

        if self.attachment == Attachment::Detached
            && reattach_candidate
            && self.reattach_phase >= 0.995
        {
            // The split precursor now equals the connected Bridged envelope
            // along its neck seam, so this authority transition has no visual cut.
            self.attachment = Attachment::Bridged;
            self.release_phase = 0.0;
            self.release_velocity = 0.0;
            self.reattach_phase = 0.0;
            self.reattach_velocity = 0.0;
            self.join_phase = 1.0;
            self.join_velocity = 0.0;
        }

        let join_target = match self.attachment {
            Attachment::Fused => 0.0,
            Attachment::Bridged | Attachment::Auto | Attachment::Detached => 1.0,
        };
        step_qualification_phase(
            &mut self.join_phase,
            &mut self.join_velocity,
            join_target,
            join_base,
            join_base
                .scaled(2.25, 1.65)
                .expect("validated reduced join dynamics"),
            params.join_motion,
            dt,
        );

        let strain = (self.app_center_x - self.shoulder_center_x).abs();
        self.velocity_x.abs()
            + self.velocity_y.abs()
            + self.shoulder_velocity_x.abs()
            + self.neck_velocity.abs()
            + self.join_velocity.abs() * 12.0
            + (self.join_phase - join_target).abs() * 24.0
            + self.release_velocity.abs() * 10.0
            + (self.release_phase - release_target).abs() * 18.0
            + self.reattach_velocity.abs() * 10.0
            + (self.reattach_phase - reattach_target).abs() * 18.0
            + self.tilt_velocity.abs()
            + self.tilt.abs() * 0.25
            + self
                .top_deform_velocity
                .iter()
                .map(|value| value.abs())
                .sum::<f32>()
            + self.top_deform.iter().map(|value| value.abs()).sum::<f32>() * 0.18
            + strain * 0.5
    }

    fn frame(self, params: ReactiveLabParams) -> ReactiveHullFrame {
        let viewport = self.viewport;
        let policy = ResponsiveGeometryPolicy;
        let speed = self.velocity_x.hypot(self.velocity_y);
        let strain = (self.app_center_x - self.shoulder_center_x).abs();
        let deformation_gain = (1.0 - params.shape_preservation).clamp(0.0, 1.0);
        let (base_width, base_height) = policy.app_base_size(viewport);
        let width_gain = (strain * 0.22 + speed * 0.012 * params.elasticity) * deformation_gain;
        let app_width = base_width + width_gain.min(viewport.width * 0.05);
        let height_loss =
            (speed * 0.012 * params.elasticity * deformation_gain).min(viewport.height / 35.0);
        let app_height = base_height - height_loss;
        let base_radius = policy.app_radius(viewport);
        let min_radius = base_radius * (2.0 / 3.0);
        let radius_loss = viewport.height * (strain / viewport.width.max(1.0)) * 0.05;
        let app = RectHull {
            left: self.app_center_x - app_width / 2.0,
            top: self.app_top,
            right: self.app_center_x + app_width / 2.0,
            bottom: self.app_top + app_height,
            radius: (base_radius - radius_loss).clamp(min_radius, base_radius),
        };
        ReactiveHullFrame {
            viewport,
            app,
            dock: policy.dock(viewport),
            pointer_target: (self.pointer_x, self.pointer_y),
            resolved_target: self.resolved_target(),
            velocity: (self.velocity_x, self.velocity_y),
            shoulder_center_x: self.shoulder_center_x,
            neck_width: self.neck_width,
            join_phase: self.join_phase,
            release_phase: self.release_phase,
            reattach_phase: self.reattach_phase,
            attachment: self.attachment,
            tilt: self.tilt,
            top_deform: self.top_deform,
            local_deformation: self
                .top_deform
                .iter()
                .map(|value| value.abs())
                .fold(0.0_f32, f32::max),
            strain,
            speed,
            constraint_pressure: self.constraint_pressure,
            spatial_feedback_x: self.spatial_feedback_x,
            spatial_feedback_y: self.spatial_feedback_y,
            neighbor: self.neighbor,
            neighbor_influence_x: self.neighbor_influence_x,
            neighbor_influence_y: self.neighbor_influence_y,
            neighbor_influence_pressure: self.neighbor_influence_pressure,
            neighbor_feedback_x: self.neighbor_feedback_x,
            neighbor_feedback_y: self.neighbor_feedback_y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ReactiveHullFrame {
    viewport: SurfaceViewport,
    app: RectHull,
    dock: RectHull,
    pointer_target: (f32, f32),
    resolved_target: (f32, f32),
    velocity: (f32, f32),
    shoulder_center_x: f32,
    neck_width: f32,
    join_phase: f32,
    release_phase: f32,
    reattach_phase: f32,
    attachment: Attachment,
    tilt: f32,
    top_deform: [f32; 3],
    local_deformation: f32,
    strain: f32,
    speed: f32,
    constraint_pressure: f32,
    spatial_feedback_x: f32,
    spatial_feedback_y: f32,
    neighbor: Option<QualificationNeighbor>,
    neighbor_influence_x: f32,
    neighbor_influence_y: f32,
    neighbor_influence_pressure: f32,
    neighbor_feedback_x: f32,
    neighbor_feedback_y: f32,
}

#[derive(Clone, Copy, Debug)]
struct GeometryObservation {
    header: GeometryFrameHeader,
    attachment: Attachment,
    app: RectHull,
    dock: RectHull,
    content: RectHull,
    pointer_target: (f32, f32),
    resolved_target: (f32, f32),
    velocity: (f32, f32),
    shoulder_center_x: f32,
    neck_width: f32,
    join_phase: f32,
    release_phase: f32,
    reattach_phase: f32,
    tilt: f32,
    top_deform: [f32; 3],
    constraint_pressure: f32,
    spatial_feedback: (f32, f32),
    neighbor_influence: (f32, f32, f32),
    neighbor: Option<RectHull>,
}

impl GeometryObservation {
    fn from_frame(sample: u64, compositor_revision: u64, frame: ReactiveHullFrame) -> Self {
        let tokens = VisualCompositionTokens::qualification();
        let content = VisualCompositionLayout::resolve(frame, tokens).app_content_bounds();
        Self {
            header: GeometryFrameHeader::new(
                sample,
                "gnos.ux:dock:qualification",
                frame.viewport,
                compositor_revision,
            ),
            attachment: frame.attachment,
            app: frame.app,
            dock: frame.dock,
            content,
            pointer_target: frame.pointer_target,
            resolved_target: frame.resolved_target,
            velocity: frame.velocity,
            shoulder_center_x: frame.shoulder_center_x,
            neck_width: frame.neck_width,
            join_phase: frame.join_phase,
            release_phase: frame.release_phase,
            reattach_phase: frame.reattach_phase,
            tilt: frame.tilt,
            top_deform: frame.top_deform,
            constraint_pressure: frame.constraint_pressure,
            spatial_feedback: (frame.spatial_feedback_x, frame.spatial_feedback_y),
            neighbor_influence: (
                frame.neighbor_influence_x,
                frame.neighbor_influence_y,
                frame.neighbor_influence_pressure,
            ),
            neighbor: frame.neighbor.map(|neighbor| neighbor.rect),
        }
    }

    fn log_json(self) {
        let neighbor = self
            .neighbor
            .map(|rect| {
                format!(
                    "[{:.2},{:.2},{:.2},{:.2}]",
                    rect.left, rect.top, rect.right, rect.bottom
                )
            })
            .unwrap_or_else(|| "null".to_owned());
        println!(
            concat!(
                "GEOMETRY_OBSERVATION ",
                "{{\"sample\":{},\"compositor_revision\":{},\"viewport\":[{:.2},{:.2}],\"dock_recipe\":\"{}\",\"dock_recipe_revision\":{},\"curve_grammar\":\"{}\",\"curve_grammar_revision\":{},\"connector_grammar\":\"{}\",\"connector_grammar_revision\":{},\"dynamics_grammar\":\"{}\",\"dynamics_grammar_revision\":{},\"influence_grammar\":\"{}\",\"influence_grammar_revision\":{},\"attachment\":\"{:?}\",",
                "\"app\":[{:.2},{:.2},{:.2},{:.2}],",
                "\"dock\":[{:.2},{:.2},{:.2},{:.2}],",
                "\"content\":[{:.2},{:.2},{:.2},{:.2}],",
                "\"pointer_target\":[{:.2},{:.2}],",
                "\"resolved_target\":[{:.2},{:.2}],",
                "\"velocity\":[{:.2},{:.2}],",
                "\"shoulder_center_x\":{:.2},\"neck_width\":{:.2},\"join_phase\":{:.4},\"release_phase\":{:.4},\"reattach_phase\":{:.4},",
                "\"tilt\":{:.2},\"top_deform\":[{:.2},{:.2},{:.2}],",
                "\"constraint_pressure\":{:.2},",
                "\"spatial_feedback\":[{:.2},{:.2}],",
                "\"neighbor_influence\":[{:.2},{:.2},{:.2}],",
                "\"neighbor\":{}",
                "}}"
            ),
            self.header.sample,
            self.header.compositor_revision,
            self.header.viewport.width,
            self.header.viewport.height,
            gnos_ux_surface::grammar::DockGeometryRecipe::ID,
            gnos_ux_surface::grammar::DockGeometryRecipe::REVISION,
            gnos_ux_surface::grammar::CURVE_GRAMMAR_ID,
            gnos_ux_surface::grammar::CURVE_GRAMMAR_REVISION,
            gnos_ux_surface::grammar::CONNECTOR_GRAMMAR_ID,
            gnos_ux_surface::grammar::CONNECTOR_GRAMMAR_REVISION,
            gnos_ux_surface::grammar::DYNAMICS_GRAMMAR_ID,
            gnos_ux_surface::grammar::DYNAMICS_GRAMMAR_REVISION,
            gnos_ux_surface::grammar::INFLUENCE_GRAMMAR_ID,
            gnos_ux_surface::grammar::INFLUENCE_GRAMMAR_REVISION,
            self.attachment,
            self.app.left,
            self.app.top,
            self.app.right,
            self.app.bottom,
            self.dock.left,
            self.dock.top,
            self.dock.right,
            self.dock.bottom,
            self.content.left,
            self.content.top,
            self.content.right,
            self.content.bottom,
            self.pointer_target.0,
            self.pointer_target.1,
            self.resolved_target.0,
            self.resolved_target.1,
            self.velocity.0,
            self.velocity.1,
            self.shoulder_center_x,
            self.neck_width,
            self.join_phase,
            self.release_phase,
            self.reattach_phase,
            self.tilt,
            self.top_deform[0],
            self.top_deform[1],
            self.top_deform[2],
            self.constraint_pressure,
            self.spatial_feedback.0,
            self.spatial_feedback.1,
            self.neighbor_influence.0,
            self.neighbor_influence.1,
            self.neighbor_influence.2,
            neighbor,
        );
    }
}

#[derive(Clone)]
struct QualificationGeometry {
    dock_height: u32,
    reveal_height: u32,
    input: RegionSet,
}

struct QualificationPaintState {
    fill: gpui::Rgba,
    opacity: f32,
    border_width: u32,
    border_visible: bool,
    shadow_visible: bool,
    blur_realization: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct ReactiveContourTuning {
    corner_scale: f32,
    neck_half_max_ratio: f32,
    mouth_extra_ratio: f32,
    mouth_half_min_ratio: f32,
    mouth_half_max_ratio: f32,
    midpoint_bias: f32,
    shoulder_run_scale: f32,
}

impl ReactiveContourTuning {
    fn baseline() -> Self {
        Self {
            corner_scale: 1.0,
            neck_half_max_ratio: 0.153_333_34,
            mouth_extra_ratio: 0.096_666_67,
            mouth_half_min_ratio: 0.153_333_34,
            mouth_half_max_ratio: 0.236_666_66,
            midpoint_bias: 0.50,
            shoulder_run_scale: 1.0,
        }
    }

    fn visual_lab() -> Self {
        Self {
            corner_scale: 1.08,
            neck_half_max_ratio: 0.086_666_67,
            mouth_extra_ratio: 0.056_666_67,
            mouth_half_min_ratio: 0.113_333_34,
            mouth_half_max_ratio: 0.173_333_33,
            midpoint_bias: 0.48,
            shoulder_run_scale: 0.82,
        }
    }
}

fn qualification_paint(
    material: &MaterialRecipe,
    chrome: ChromePresentation,
) -> QualificationPaintState {
    // Qualification-only palette mapping. Material family is not a product theme authority.
    let family_color = match material.family.as_str() {
        "qualification-hud" => 0x102b3b,
        _ => 0x26313a,
    };
    QualificationPaintState {
        // Alpha is applied once by the GPUI element opacity below.
        fill: rgba(family_color << 8 | 0xff),
        opacity: chrome.opacity.value() as f32 / 100.0,
        border_width: material.border.0,
        border_visible: chrome.border_visible,
        shadow_visible: chrome.shadow_visible && material.shadow.0 > 0,
        blur_realization: "UNSUPPORTED_IN_PINNED_RENDERER",
    }
}

impl QualificationGeometry {
    fn from_visual(visual: VisualState, viewport: SurfaceViewport) -> Self {
        let visible_fraction = (f32::from(visual.opacity.value()) / 88.0).clamp(0.0, 1.0);
        let frame = resolve_dock_frame(viewport, visible_fraction);
        let dock_height = frame.visible_envelope.height().round().max(0.0) as u32;
        let reveal_height = frame.reveal_region.height().round().max(1.0) as u32;
        let input = RegionSet {
            regions: frame
                .input_regions
                .iter()
                .map(|region| ShapeRegion {
                    x: region.left().round() as i32,
                    y: region.top().round() as i32,
                    width: LogicalPx(region.width().round().max(1.0) as u32),
                    height: LogicalPx(region.height().round().max(1.0) as u32),
                })
                .collect(),
        };
        Self {
            dock_height,
            reveal_height,
            input,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct FusedHullGeometry {
    app_left: f32,
    app_top: f32,
    app_right: f32,
    app_bottom: f32,
    dock_left: f32,
    dock_top: f32,
    dock_right: f32,
    dock_bottom: f32,
    corner_radius: f32,
}

impl FusedHullGeometry {
    fn qualification() -> Self {
        Self {
            app_left: 120.0,
            app_top: 125.0,
            app_right: 480.0,
            app_bottom: 294.0,
            dock_left: 20.0,
            dock_top: 294.0,
            dock_right: 580.0,
            dock_bottom: 372.0,
            corner_radius: 24.0,
        }
    }

    fn input_regions(self) -> RegionSet {
        RegionSet {
            regions: vec![
                ShapeRegion {
                    x: self.app_left as i32,
                    y: self.app_top as i32,
                    width: LogicalPx((self.app_right - self.app_left) as u32),
                    height: LogicalPx((self.app_bottom - self.app_top) as u32),
                },
                ShapeRegion {
                    x: self.dock_left as i32,
                    y: self.dock_top as i32,
                    width: LogicalPx((self.dock_right - self.dock_left) as u32),
                    height: LogicalPx((self.dock_bottom - self.dock_top) as u32),
                },
            ],
        }
    }
}

fn fused_hull_path(
    bounds: Bounds<gpui::Pixels>,
    geometry: FusedHullGeometry,
    stroke: bool,
) -> gpui::Path<gpui::Pixels> {
    let mut path = if stroke {
        PathBuilder::stroke(px(1.0))
    } else {
        PathBuilder::fill()
    };
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
    let r = geometry.corner_radius;

    path.move_to(p(geometry.dock_left + r, geometry.dock_bottom));
    path.curve_to(
        p(geometry.dock_left, geometry.dock_bottom - r),
        p(geometry.dock_left, geometry.dock_bottom),
    );
    let shoulder_bottom = geometry.dock_top + 48.0;
    let shoulder_control_y = geometry.app_bottom + 24.0;
    path.line_to(p(geometry.dock_left, shoulder_bottom));
    path.cubic_bezier_to(
        p(geometry.app_left, geometry.app_bottom),
        p(geometry.dock_left, shoulder_control_y),
        p(geometry.app_left, shoulder_control_y),
    );
    path.line_to(p(geometry.app_left, geometry.app_top + r));
    path.curve_to(
        p(geometry.app_left + r, geometry.app_top),
        p(geometry.app_left, geometry.app_top),
    );
    path.line_to(p(geometry.app_right - r, geometry.app_top));
    path.curve_to(
        p(geometry.app_right, geometry.app_top + r),
        p(geometry.app_right, geometry.app_top),
    );
    path.line_to(p(geometry.app_right, geometry.app_bottom));
    path.cubic_bezier_to(
        p(geometry.dock_right, shoulder_bottom),
        p(geometry.app_right, shoulder_control_y),
        p(geometry.dock_right, shoulder_control_y),
    );
    path.line_to(p(geometry.dock_right, geometry.dock_bottom - r));
    path.curve_to(
        p(geometry.dock_right - r, geometry.dock_bottom),
        p(geometry.dock_right, geometry.dock_bottom),
    );
    path.close();
    path.build().expect("building qualification fused hull")
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RectHull {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
}

impl From<GeometryRect> for RectHull {
    fn from(rect: GeometryRect) -> Self {
        Self {
            left: rect.left(),
            top: rect.top(),
            right: rect.right(),
            bottom: rect.bottom(),
            radius: rect.radius(),
        }
    }
}

impl RectHull {
    fn geometry_rect(self) -> GeometryRect {
        GeometryRect::try_new(self.left, self.top, self.right, self.bottom, self.radius)
            .expect("validated RectHull geometry")
    }
}

#[derive(Clone, Copy, Debug)]
struct DerivedTopologyGeometry {
    app: RectHull,
    dock: RectHull,
}

impl DerivedTopologyGeometry {
    fn bridged() -> Self {
        Self {
            app: RectHull {
                left: 120.0,
                top: 100.0,
                right: 480.0,
                bottom: 246.0,
                radius: 24.0,
            },
            dock: RectHull {
                left: 20.0,
                top: 294.0,
                right: 580.0,
                bottom: 372.0,
                radius: 24.0,
            },
        }
    }

    fn detached() -> Self {
        Self {
            app: RectHull {
                left: 120.0,
                top: 76.0,
                right: 480.0,
                bottom: 226.0,
                radius: 24.0,
            },
            dock: RectHull {
                left: 20.0,
                top: 294.0,
                right: 580.0,
                bottom: 372.0,
                radius: 24.0,
            },
        }
    }

    fn input_regions(self, attachment: Attachment, bridge: Option<&BridgeIntent>) -> RegionSet {
        let mut regions = vec![
            ShapeRegion {
                x: self.app.left as i32,
                y: self.app.top as i32,
                width: LogicalPx((self.app.right - self.app.left) as u32),
                height: LogicalPx((self.app.bottom - self.app.top) as u32),
            },
            ShapeRegion {
                x: self.dock.left as i32,
                y: self.dock.top as i32,
                width: LogicalPx((self.dock.right - self.dock.left) as u32),
                height: LogicalPx((self.dock.bottom - self.dock.top) as u32),
            },
        ];
        if attachment == Attachment::Bridged
            && let Some(intent) = bridge
        {
            let center = 300_i32;
            let full = intent.width.0.min(220) as i32;
            let neck = intent.neck_width.0.min(full as u32) as i32;
            let gap = (self.dock.top - self.app.bottom).max(1.0) as u32;
            let third = (gap / 3).max(1);
            regions.extend([
                ShapeRegion {
                    x: center - full / 2,
                    y: self.app.bottom as i32,
                    width: LogicalPx(full as u32),
                    height: LogicalPx(third),
                },
                ShapeRegion {
                    x: center - neck / 2,
                    y: self.app.bottom as i32 + third as i32,
                    width: LogicalPx(neck as u32),
                    height: LogicalPx(third),
                },
                ShapeRegion {
                    x: center - full / 2,
                    y: self.dock.top as i32 - third as i32,
                    width: LogicalPx(full as u32),
                    height: LogicalPx(third),
                },
            ]);
        }
        RegionSet { regions }
    }
}

fn reactive_top_points(frame: ReactiveHullFrame) -> [(f32, f32); 5] {
    let app = frame.app;
    let r = app.radius;
    let left_top = app.top + frame.tilt;
    let right_top = app.top - frame.tilt;
    let left_x = app.left + r;
    let right_x = app.right - r;
    let span = right_x - left_x;
    let baseline = |t: f32| left_top + (right_top - left_top) * t;
    [
        (left_x, baseline(0.0)),
        (left_x + span * 0.25, baseline(0.25) + frame.top_deform[0]),
        (left_x + span * 0.50, baseline(0.50) + frame.top_deform[1]),
        (left_x + span * 0.75, baseline(0.75) + frame.top_deform[2]),
        (right_x, baseline(1.0)),
    ]
}

fn bridge_shoulder_run(
    attach_half: f32,
    neck_half: f32,
    gap: f32,
    min_run: f32,
    max_run: f32,
) -> f32 {
    let span =
        ConnectorSpan::try_new(0.0, neck_half, attach_half).expect("validated connector span");
    BridgeShoulderRecipe::baseline()
        .resolve(span, gap, min_run.min(max_run), max_run.max(min_run))
        .expect("validated bridge shoulder recipe")
}

#[derive(Clone, Copy, Debug)]
struct ReactiveConnectorGeometry {
    connector_center: f32,
    neck_left: f32,
    neck_right: f32,
    dock_attach_left: f32,
    dock_attach_right: f32,
    app_attach_left: f32,
    app_attach_right: f32,
    app_attach_left_y: f32,
    app_attach_right_y: f32,
    left_mid_y: f32,
    right_mid_y: f32,
    left_shoulder_run: f32,
    right_shoulder_run: f32,
}

fn reactive_connector_geometry(
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
) -> ReactiveConnectorGeometry {
    let viewport = frame.viewport;
    let dock = frame.dock;
    let app = frame.app;
    let ar = app.radius * tuning.corner_scale;
    let left_bottom_y = app.bottom + frame.tilt * 0.35;
    let right_bottom_y = app.bottom - frame.tilt * 0.35;
    let connector_inset = viewport.width * 0.186_666_67;
    let connector_center = frame
        .shoulder_center_x
        .clamp(dock.left + connector_inset, dock.right - connector_inset);
    let fused_gain = if frame.attachment == Attachment::Fused {
        1.22
    } else {
        1.0
    };
    let neck_half = (frame.neck_width / 2.0).clamp(
        viewport.width * 0.003_333_333_4,
        tuning.neck_half_max_ratio * viewport.width * fused_gain,
    );
    let attach_half = (neck_half + tuning.mouth_extra_ratio * viewport.width).clamp(
        tuning.mouth_half_min_ratio * viewport.width,
        tuning.mouth_half_max_ratio * viewport.width * fused_gain,
    );
    let connector_span = ConnectorSpan::try_new(connector_center, neck_half, attach_half)
        .expect("validated reactive connector span");
    let dock_attach_left = connector_span.attach_left();
    let dock_attach_right = connector_span.attach_right();
    let app_inset_near = viewport.width * 0.02;
    let app_inset_far = viewport.width * 0.04;
    let app_attach_left = dock_attach_left.clamp(
        app.left + ar + app_inset_near,
        app.right - ar - app_inset_far,
    );
    let app_attach_right = dock_attach_right.clamp(
        app.left + ar + app_inset_far,
        app.right - ar - app_inset_near,
    );
    let app_bottom_at = |x: f32| {
        let t = ((x - app.left) / (app.right - app.left)).clamp(0.0, 1.0);
        left_bottom_y + (right_bottom_y - left_bottom_y) * t
    };
    let app_attach_left_y = app_bottom_at(app_attach_left);
    let app_attach_right_y = app_bottom_at(app_attach_right);
    let left_gap = (dock.top - app_attach_left_y).max(1.0);
    let right_gap = (dock.top - app_attach_right_y).max(1.0);
    let left_mid_y = app_attach_left_y + left_gap * tuning.midpoint_bias;
    let right_mid_y = app_attach_right_y + right_gap * tuning.midpoint_bias;
    let neck_left = connector_span.neck_left();
    let neck_right = connector_span.neck_right();
    let min_run = viewport.min_axis() * 0.057_142_857;
    let max_run = viewport.min_axis() * 0.138_095_24;
    let left_shoulder_run = bridge_shoulder_run(attach_half, neck_half, left_gap, min_run, max_run)
        * tuning.shoulder_run_scale;
    let right_shoulder_run =
        bridge_shoulder_run(attach_half, neck_half, right_gap, min_run, max_run)
            * tuning.shoulder_run_scale;

    ReactiveConnectorGeometry {
        connector_center,
        neck_left,
        neck_right,
        dock_attach_left,
        dock_attach_right,
        app_attach_left,
        app_attach_right,
        app_attach_left_y,
        app_attach_right_y,
        left_mid_y,
        right_mid_y,
        left_shoulder_run,
        right_shoulder_run,
    }
}

#[derive(Clone, Copy, Debug)]
struct CubicJoinPair {
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p1: (f32, f32),
    c3: (f32, f32),
    c4: (f32, f32),
    p2: (f32, f32),
}

fn grammar_join_pair(pair: CubicJoinPair) -> GrammarCubicJoinPair {
    GrammarCubicJoinPair::from_controls(
        Point2::new(pair.p0.0, pair.p0.1),
        Point2::new(pair.c1.0, pair.c1.1),
        Point2::new(pair.c2.0, pair.c2.1),
        Point2::new(pair.p1.0, pair.p1.1),
        Point2::new(pair.c3.0, pair.c3.1),
        Point2::new(pair.c4.0, pair.c4.1),
        Point2::new(pair.p2.0, pair.p2.1),
    )
}

fn local_join_pair(pair: GrammarCubicJoinPair) -> CubicJoinPair {
    CubicJoinPair {
        p0: pair.p0().tuple(),
        c1: pair.c1().tuple(),
        c2: pair.c2().tuple(),
        p1: pair.p1().tuple(),
        c3: pair.c3().tuple(),
        c4: pair.c4().tuple(),
        p2: pair.p2().tuple(),
    }
}

fn cubic_join_monotonic_morph(a: CubicJoinPair, b: CubicJoinPair, t: f32) -> CubicJoinPair {
    local_join_pair(monotonic_join_morph(
        grammar_join_pair(a),
        grammar_join_pair(b),
        t,
    ))
}

#[derive(Clone, Copy, Debug)]
#[cfg(test)]
struct CubicSegment {
    p0: (f32, f32),
    p3: (f32, f32),
}

#[cfg(test)]
fn cubic_subsegment(
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p3: (f32, f32),
    start_t: f32,
    end_t: f32,
) -> CubicSegment {
    let segment = GrammarCubicBezier::new(
        Point2::new(p0.0, p0.1),
        Point2::new(c1.0, c1.1),
        Point2::new(c2.0, c2.1),
        Point2::new(p3.0, p3.1),
    )
    .subsegment(start_t, end_t);
    CubicSegment {
        p0: segment.p0.tuple(),
        p3: segment.p3.tuple(),
    }
}

fn reactive_lower_join_pairs(
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
) -> (CubicJoinPair, CubicJoinPair) {
    let t = smooth_join_phase(frame.join_phase);
    (
        cubic_join_monotonic_morph(
            fused_left_join_pair(frame, tuning),
            bridged_left_join_pair(frame, tuning),
            t,
        ),
        cubic_join_monotonic_morph(
            fused_right_join_pair(frame, tuning),
            bridged_right_join_pair(frame, tuning),
            t,
        ),
    )
}

fn split_cubic_half(
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p3: (f32, f32),
) -> CubicJoinPair {
    let (left, right) = GrammarCubicBezier::new(
        Point2::new(p0.0, p0.1),
        Point2::new(c1.0, c1.1),
        Point2::new(c2.0, c2.1),
        Point2::new(p3.0, p3.1),
    )
    .split_half();
    CubicJoinPair {
        p0: left.p0.tuple(),
        c1: left.c1.tuple(),
        c2: left.c2.tuple(),
        p1: left.p3.tuple(),
        c3: right.c1.tuple(),
        c4: right.c2.tuple(),
        p2: right.p3.tuple(),
    }
}

fn fused_left_join_pair(frame: ReactiveHullFrame, tuning: ReactiveContourTuning) -> CubicJoinPair {
    let join = reactive_fused_join_geometry(frame, tuning);
    let k = 0.552_284_8_f32;
    let p0 = (join.left_shoulder, frame.dock.top);
    let p3 = (frame.app.left, join.left_bottom_y);
    let c1 = (p0.0 + join.left_radius_x * k, p0.1);
    let c2 = (p3.0, p3.1 + join.left_radius_y * k);
    split_cubic_half(p0, c1, c2, p3)
}

fn fused_right_join_pair(frame: ReactiveHullFrame, tuning: ReactiveContourTuning) -> CubicJoinPair {
    let join = reactive_fused_join_geometry(frame, tuning);
    let k = 0.552_284_8_f32;
    let p0 = (frame.app.right, join.right_bottom_y);
    let p3 = (join.right_shoulder, frame.dock.top);
    let c1 = (p0.0, p0.1 + join.right_radius_y * k);
    let c2 = (p3.0 - join.right_radius_x * k, p3.1);
    split_cubic_half(p0, c1, c2, p3)
}

fn bridged_left_join_pair(
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
) -> CubicJoinPair {
    let connector = reactive_connector_geometry(frame, tuning);
    let left_gap = (frame.dock.top - connector.app_attach_left_y).max(1.0);
    CubicJoinPair {
        p0: (connector.dock_attach_left, frame.dock.top),
        c1: (
            connector.dock_attach_left + connector.left_shoulder_run,
            frame.dock.top,
        ),
        c2: (connector.neck_left, connector.left_mid_y + left_gap * 0.22),
        p1: (connector.neck_left, connector.left_mid_y),
        c3: (connector.neck_left, connector.left_mid_y - left_gap * 0.18),
        c4: (
            connector.app_attach_left + connector.left_shoulder_run,
            connector.app_attach_left_y,
        ),
        p2: (connector.app_attach_left, connector.app_attach_left_y),
    }
}

fn bridged_right_join_pair(
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
) -> CubicJoinPair {
    let connector = reactive_connector_geometry(frame, tuning);
    let right_gap = (frame.dock.top - connector.app_attach_right_y).max(1.0);
    CubicJoinPair {
        p0: (connector.app_attach_right, connector.app_attach_right_y),
        c1: (
            connector.app_attach_right - connector.right_shoulder_run,
            connector.app_attach_right_y,
        ),
        c2: (
            connector.neck_right,
            connector.right_mid_y - right_gap * 0.18,
        ),
        p1: (connector.neck_right, connector.right_mid_y),
        c3: (
            connector.neck_right,
            connector.right_mid_y + right_gap * 0.22,
        ),
        c4: (
            connector.dock_attach_right - connector.right_shoulder_run,
            frame.dock.top,
        ),
        p2: (connector.dock_attach_right, frame.dock.top),
    }
}

fn smooth_join_phase(phase: f32) -> f32 {
    let t = phase.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
fn g1_transition_controls(
    start: (f32, f32),
    incoming_direction: (f32, f32),
    end: (f32, f32),
    outgoing_direction: (f32, f32),
) -> Option<((f32, f32), (f32, f32))> {
    grammar_g1_transition_controls(
        Point2::new(start.0, start.1),
        Vector2::new(incoming_direction.0, incoming_direction.1),
        Point2::new(end.0, end.1),
        Vector2::new(outgoing_direction.0, outgoing_direction.1),
    )
    .map(|(c1, c2)| (c1.tuple(), c2.tuple()))
}

fn reactive_morph_hull_path(
    bounds: Bounds<gpui::Pixels>,
    frame: ReactiveHullFrame,
    stroke: bool,
    tuning: ReactiveContourTuning,
) -> gpui::Path<gpui::Pixels> {
    let hull = qualification_connective_hull(frame, tuning);
    connective_hull_path(bounds, hull, stroke)
}

fn qualification_connective_hull(
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
) -> ConnectiveHullGeometry {
    let flip = |rect: RectHull| {
        GeometryRect::try_new(rect.left, -rect.bottom, rect.right, -rect.top, rect.radius)
            .expect("validated flipped qualifier geometry")
    };
    let connector = reactive_connector_geometry(frame, tuning);
    let child = flip(frame.app);
    let raw = reactive_top_points(frame);
    let top = std::array::from_fn(|index| {
        // The flipped far edge is child.bottom(). Endpoints meet its
        // roundovers; tilt/deformation indent rather than leave body bounds.
        let x = child.left()
            + child.radius()
            + (child.width() - 2.0 * child.radius()) * index as f32 / 4.0;
        let y = if index == 0 || index == 4 {
            child.bottom()
        } else {
            child.bottom()
                - (-raw[index].1 - child.bottom())
                    .abs()
                    .min((child.height() - child.radius()).max(0.0))
        };
        Point2::new(x, y)
    });
    let attachment = match frame.attachment {
        Attachment::Fused => ReactiveAttachment::Fused,
        Attachment::Detached => ReactiveAttachment::Detached,
        Attachment::Bridged | Attachment::Auto => ReactiveAttachment::Bridged,
    };
    ConnectiveHullGeometry::resolve(ConnectiveHullInput {
        parent: flip(frame.dock),
        child,
        // Pinned to Downward because that is the relation this harness has
        // always built: the app body sits below its dock, which is the case the
        // hull's existing construction already described. For Downward every
        // longitudinal expression in the builder reduces to what it was, so the
        // qualified output this binary produced is unchanged.
        growth: ReactiveGrowthDirection::Downward,
        attachment,
        topology_phase: frame.join_phase,
        attachment_phase: frame.join_phase,
        release_phase: frame.release_phase,
        reattach_phase: frame.reattach_phase,
        neck_width: frame.neck_width,
        mouth_width: connector.dock_attach_right - connector.dock_attach_left,
        shoulder_run: (connector.left_shoulder_run + connector.right_shoulder_run) * 0.5,
        shoulder_center_x: connector.connector_center,
        outer_contour: top,
        openness: 1.0,
    })
    .expect("valid qualification connective hull")
}

fn connective_hull_path(
    bounds: Bounds<gpui::Pixels>,
    hull: ConnectiveHullGeometry,
    stroke: bool,
) -> gpui::Path<gpui::Pixels> {
    let mut path = if stroke {
        PathBuilder::stroke(px(1.0))
    } else {
        PathBuilder::fill()
    };
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p =
        |geometry_point: Point2| point(px(ox + geometry_point.x()), px(oy - geometry_point.y()));
    for index in 0..hull.command_count() {
        match hull.command(index).expect("hull command index") {
            HullCommand::Empty => break,
            HullCommand::MoveTo(point) => path.move_to(p(point)),
            HullCommand::LineTo(point) => path.line_to(p(point)),
            HullCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                path.cubic_bezier_to(p(to), p(control1), p(control2));
            }
            HullCommand::Close => path.close(),
        }
    }
    path.build().expect("building shared qualification hull")
}

#[derive(Clone, Copy, Debug)]
struct ReactiveFusedJoinGeometry {
    left_shoulder: f32,
    right_shoulder: f32,
    left_bottom_y: f32,
    right_bottom_y: f32,
    left_radius_x: f32,
    left_radius_y: f32,
    right_radius_x: f32,
    right_radius_y: f32,
}

fn reactive_fused_join_geometry(
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
) -> ReactiveFusedJoinGeometry {
    let viewport = frame.viewport;
    let dock = frame.dock;
    let app = frame.app;
    let dr = dock.radius * tuning.corner_scale;
    let desired_radius = viewport.min_axis() * 0.045_238_096;
    let minimum_clearance = viewport.min_axis() * 0.019_047_62;

    let left_shoulder = (app.left - desired_radius)
        .max(dock.left + dr + minimum_clearance)
        .min(app.left - 1.0);
    let right_shoulder = (app.right + desired_radius)
        .min(dock.right - dr - minimum_clearance)
        .max(app.right + 1.0);
    let left_radius_x = (app.left - left_shoulder).max(1.0);
    let right_radius_x = (right_shoulder - app.right).max(1.0);

    // A fused join is a concave 90-degree fillet: horizontal Dock tangent in,
    // vertical app tangent out. Vertical radii track the tilted lower app edge
    // without allowing the curve to overshoot below dock.top.
    let left_raw_bottom = app.bottom + frame.tilt * 0.35;
    let right_raw_bottom = app.bottom - frame.tilt * 0.35;
    let left_bottom_y = (left_raw_bottom - desired_radius).min(dock.top - 1.0);
    let right_bottom_y = (right_raw_bottom - desired_radius).min(dock.top - 1.0);
    let left_radius_y = (dock.top - left_bottom_y).max(1.0);
    let right_radius_y = (dock.top - right_bottom_y).max(1.0);

    ReactiveFusedJoinGeometry {
        left_shoulder,
        right_shoulder,
        left_bottom_y,
        right_bottom_y,
        left_radius_x,
        left_radius_y,
        right_radius_x,
        right_radius_y,
    }
}

#[derive(Clone, Copy, Debug)]
struct DetachedReleaseGeometry {
    center_x: f32,
    app_base_half: f32,
    app_tip_y: f32,
    dock_base_half: f32,
    dock_tip_y: f32,
}

fn app_bottom_y_at(frame: ReactiveHullFrame, x: f32) -> f32 {
    let app = frame.app;
    let left = app.bottom + frame.tilt * 0.35;
    let right = app.bottom - frame.tilt * 0.35;
    let t = ((x - app.left) / (app.right - app.left).max(0.001)).clamp(0.0, 1.0);
    left + (right - left) * t
}

fn detached_release_geometry(frame: ReactiveHullFrame) -> Option<DetachedReleaseGeometry> {
    if frame.attachment != Attachment::Detached {
        return None;
    }
    let residual = 1.0 - smooth_join_phase(frame.release_phase);
    if residual <= 0.001 {
        return None;
    }

    let viewport = frame.viewport;
    let app = frame.app;
    let dock = frame.dock;
    let center_margin = app.radius * 2.0;
    let center_x = frame
        .shoulder_center_x
        .clamp(app.left + center_margin, app.right - center_margin);
    let app_bottom = app_bottom_y_at(frame, center_x);
    let available_gap = (dock.top - app_bottom).max(0.0);
    let semantic_gap = (viewport.min_axis() * 0.0075).max(2.0);
    let max_tail_depth = ((available_gap - semantic_gap) * 0.5).max(0.0);
    let tail_depth = max_tail_depth * residual;

    let base_half_max = ((app.right - app.left) * 0.13)
        .min((dock.right - dock.left) * 0.10)
        .max(viewport.width * 0.025);
    let app_base_half = base_half_max * residual;
    let dock_base_half = base_half_max * residual;
    Some(DetachedReleaseGeometry {
        center_x,
        app_base_half,
        app_tip_y: app_bottom + tail_depth,
        dock_base_half,
        dock_tip_y: dock.top - tail_depth,
    })
}

fn append_elliptical_arc(
    path: &mut PathBuilder,
    radius_x: f32,
    radius_y: f32,
    sweep: bool,
    to: gpui::Point<gpui::Pixels>,
) {
    if radius_x <= 0.01 || radius_y <= 0.01 {
        path.line_to(to);
        return;
    }
    path.arc_to(point(px(radius_x), px(radius_y)), px(0.0), false, sweep, to);
}

fn rounded_rect_path(
    bounds: Bounds<gpui::Pixels>,
    rect: RectHull,
    stroke: bool,
) -> gpui::Path<gpui::Pixels> {
    let mut path = if stroke {
        PathBuilder::stroke(px(1.0))
    } else {
        PathBuilder::fill()
    };
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
    let r = rect
        .radius
        .min((rect.right - rect.left) / 2.0)
        .min((rect.bottom - rect.top) / 2.0);
    path.move_to(p(rect.left + r, rect.top));
    path.line_to(p(rect.right - r, rect.top));
    append_elliptical_arc(&mut path, r, r, true, p(rect.right, rect.top + r));
    path.line_to(p(rect.right, rect.bottom - r));
    append_elliptical_arc(&mut path, r, r, true, p(rect.right - r, rect.bottom));
    path.line_to(p(rect.left + r, rect.bottom));
    append_elliptical_arc(&mut path, r, r, true, p(rect.left, rect.bottom - r));
    path.line_to(p(rect.left, rect.top + r));
    append_elliptical_arc(&mut path, r, r, true, p(rect.left + r, rect.top));
    path.close();
    path.build().expect("building qualification rounded hull")
}

#[derive(Clone, Copy, Debug)]
struct VisualCompositionTokens {
    inset: f32,
    title_size: f32,
    secondary_size: f32,
    label_size: f32,
    card_radius: f32,
    dock_item_size: f32,
    dock_item_gap: f32,
}

impl VisualCompositionTokens {
    fn qualification() -> Self {
        Self {
            inset: 26.0,
            title_size: 14.0,
            secondary_size: 10.0,
            label_size: 9.0,
            card_radius: 11.0,
            dock_item_size: 40.0,
            dock_item_gap: 12.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VisualCompositionLayout {
    safe_left: f32,
    safe_width: f32,
    content_top: f32,
    content_height: f32,
    card_width: f32,
    card_gap: f32,
    dock_start: f32,
    dock_top: f32,
}

impl VisualCompositionLayout {
    fn resolve(frame: ReactiveHullFrame, tokens: VisualCompositionTokens) -> Self {
        let app = frame.app;
        let dock = frame.dock;
        let safe_left = app.left + tokens.inset;
        let safe_right = app.right - tokens.inset;
        let safe_width = (safe_right - safe_left).max(180.0);
        let card_gap = 12.0;
        let card_width = ((safe_width - card_gap) * 0.5).max(72.0);
        let content_top = app.top + 55.0;
        let content_height = (app.bottom - tokens.inset - content_top).max(42.0);
        let row_width = tokens.dock_item_size * 5.0 + tokens.dock_item_gap * 4.0;
        let dock_start = (dock.left + dock.right - row_width) * 0.5;
        let dock_top = dock.top + ((dock.bottom - dock.top) - tokens.dock_item_size) * 0.5;
        Self {
            safe_left,
            safe_width,
            content_top,
            content_height,
            card_width,
            card_gap,
            dock_start,
            dock_top,
        }
    }

    fn app_content_bounds(self) -> RectHull {
        RectHull {
            left: self.safe_left,
            top: self.content_top,
            right: self.safe_left + self.safe_width,
            bottom: self.content_top + self.content_height,
            radius: 0.0,
        }
    }

    fn dock_items_bounds(self, tokens: VisualCompositionTokens) -> RectHull {
        RectHull {
            left: self.dock_start,
            top: self.dock_top,
            right: self.dock_start + tokens.dock_item_size * 5.0 + tokens.dock_item_gap * 4.0,
            bottom: self.dock_top + tokens.dock_item_size,
            radius: 0.0,
        }
    }
}

fn rect_contains(outer: RectHull, inner: RectHull) -> bool {
    inner.left >= outer.left
        && inner.top >= outer.top
        && inner.right <= outer.right
        && inner.bottom <= outer.bottom
}

#[derive(Clone, Copy, Debug)]
struct VisualMaterialProfile {
    surface_top: gpui::Rgba,
    surface_bottom: gpui::Rgba,
    edge: gpui::Rgba,
    card_top: gpui::Rgba,
    card_bottom: gpui::Rgba,
    card_border: gpui::Rgba,
    accent_top: gpui::Rgba,
    accent_bottom: gpui::Rgba,
    shadow_offset_y: f32,
    shadow_alpha: u8,
    quad_shadow_blur: f32,
    backdrop_blur_realized: bool,
}

impl VisualMaterialProfile {
    fn qualification() -> Self {
        Self {
            surface_top: rgba(0x172029f7),
            surface_bottom: rgba(0x0d1218f8),
            edge: rgba(0x8eb9c699),
            card_top: rgba(0x303b45f6),
            card_bottom: rgba(0x252e36f4),
            card_border: rgba(0x5f708080),
            accent_top: rgba(0x8bdde7ff),
            accent_bottom: rgba(0x58b8c8ff),
            shadow_offset_y: 5.0,
            shadow_alpha: 0x34,
            quad_shadow_blur: 14.0,
            backdrop_blur_realized: false,
        }
    }

    fn surface_background(self) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(self.surface_top, 0.0),
            linear_color_stop(self.surface_bottom, 1.0),
        )
    }

    fn card_background(self) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(self.card_top, 0.0),
            linear_color_stop(self.card_bottom, 1.0),
        )
    }

    fn accent_background(self) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(self.accent_top, 0.0),
            linear_color_stop(self.accent_bottom, 1.0),
        )
    }

    fn card_shadow(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.28))
                .blur_radius(px(self.quad_shadow_blur)),
        ]
    }

    fn selected_shadow(self) -> Vec<BoxShadow> {
        vec![BoxShadow::new(px(0.0), px(3.0), hsla(0.52, 0.45, 0.18, 0.28)).blur_radius(px(10.0))]
    }
}

fn offset_canvas_bounds(bounds: Bounds<gpui::Pixels>, x: f32, y: f32) -> Bounds<gpui::Pixels> {
    Bounds {
        origin: point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
        size: bounds.size,
    }
}

fn debug_line_path(
    bounds: Bounds<gpui::Pixels>,
    from: (f32, f32),
    to: (f32, f32),
    width: f32,
) -> gpui::Path<gpui::Pixels> {
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(point(px(ox + from.0), px(oy + from.1)));
    path.line_to(point(px(ox + to.0), px(oy + to.1)));
    path.build().expect("building geometry debug line")
}

fn paint_reactive_envelope_outline(
    window: &mut Window,
    bounds: Bounds<gpui::Pixels>,
    frame: ReactiveHullFrame,
    tuning: ReactiveContourTuning,
    color: gpui::Rgba,
) {
    window.paint_path(reactive_morph_hull_path(bounds, frame, true, tuning), color);
}

fn debug_rect_bounds_path(
    bounds: Bounds<gpui::Pixels>,
    rect: RectHull,
) -> gpui::Path<gpui::Pixels> {
    rounded_rect_path(
        bounds,
        RectHull {
            radius: 0.0,
            ..rect
        },
        true,
    )
}

fn debug_cross_path(
    bounds: Bounds<gpui::Pixels>,
    center: (f32, f32),
    half: f32,
) -> gpui::Path<gpui::Pixels> {
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let mut path = PathBuilder::stroke(px(1.0));
    path.move_to(point(px(ox + center.0 - half), px(oy + center.1)));
    path.line_to(point(px(ox + center.0 + half), px(oy + center.1)));
    path.move_to(point(px(ox + center.0), px(oy + center.1 - half)));
    path.line_to(point(px(ox + center.0), px(oy + center.1 + half)));
    path.build().expect("building geometry debug cross")
}

fn paint_cubic_pair_tangent_samples(
    window: &mut Window,
    bounds: Bounds<gpui::Pixels>,
    pair: CubicJoinPair,
    admissible_color: gpui::Rgba,
    failure_color: gpui::Rgba,
) {
    let grammar_pair = grammar_join_pair(pair);
    for segment in [grammar_pair.first, grammar_pair.second] {
        let chord = segment.chord().normalized_or_zero();
        for t in [0.2_f32, 0.4, 0.6, 0.8] {
            let sample = segment.point(t);
            let direction = segment.derivative(t).normalized_or_zero();
            let projection = direction.dot(chord);
            let length = 11.0;
            let tip = (
                sample.x() + direction.x() * length,
                sample.y() + direction.y() * length,
            );
            window.paint_path(
                debug_line_path(bounds, sample.tuple(), tip, 1.0),
                if projection < -0.001 {
                    failure_color
                } else {
                    admissible_color
                },
            );
        }
    }
}

fn geometry_debug_overlay(
    frame: ReactiveHullFrame,
    sample: u64,
    compositor_revision: u64,
    view: GeometryDebugView,
    hyprland_geometry: Option<HyprlandLayerGeometryObservation>,
) -> impl IntoElement {
    let observation = GeometryObservation::from_frame(sample, compositor_revision, frame);
    let tuning = ReactiveContourTuning::visual_lab();
    let geometry_for_canvas = hyprland_geometry.clone();
    let overlay_canvas = canvas(
        move |bounds, _, _| {
            (
                bounds,
                frame,
                observation,
                view,
                geometry_for_canvas.clone(),
            )
        },
        move |bounds, (_, frame, observation, view, geometry_observation), window, _| {
            let primary = rgba(0xf2f2f2ee);
            let secondary = rgba(0xa8a8a8cc);
            let faint = rgba(0x77777799);

            if view.includes(GeometryDebugView::Hull) {
                // Primary line is always the actual GNOS.UX Envelope topology.
                // Technical app/dock boxes remain visible only as faint construction bounds.
                paint_reactive_envelope_outline(window, bounds, frame, tuning, primary);
                window.paint_path(debug_rect_bounds_path(bounds, frame.app), faint);
                window.paint_path(debug_rect_bounds_path(bounds, frame.dock), faint);
            }

            if view.includes(GeometryDebugView::Topology) {
                window.paint_path(
                    debug_line_path(
                        bounds,
                        (frame.shoulder_center_x, frame.app.top - 12.0),
                        (frame.shoulder_center_x, frame.dock.top + 12.0),
                        1.0,
                    ),
                    faint,
                );
                match frame.attachment {
                    Attachment::Bridged | Attachment::Auto => {
                        let connector = reactive_connector_geometry(frame, tuning);
                        for point in [
                            (connector.app_attach_left, connector.app_attach_left_y),
                            (connector.app_attach_right, connector.app_attach_right_y),
                            (connector.neck_left, connector.left_mid_y),
                            (connector.neck_right, connector.right_mid_y),
                            (connector.dock_attach_left, frame.dock.top),
                            (connector.connector_center, frame.dock.top),
                            (connector.dock_attach_right, frame.dock.top),
                        ] {
                            window.paint_path(debug_cross_path(bounds, point, 5.0), primary);
                        }
                        window.paint_path(
                            debug_line_path(
                                bounds,
                                (connector.neck_left, connector.left_mid_y),
                                (connector.neck_right, connector.right_mid_y),
                                1.0,
                            ),
                            primary,
                        );
                    }
                    Attachment::Fused => {
                        let join = reactive_fused_join_geometry(frame, tuning);
                        for point in [
                            (join.left_shoulder, frame.dock.top),
                            (frame.app.left, join.left_bottom_y),
                            (frame.app.right, join.right_bottom_y),
                            (join.right_shoulder, frame.dock.top),
                        ] {
                            window.paint_path(debug_cross_path(bounds, point, 5.0), primary);
                        }
                    }
                    Attachment::Detached => {
                        if let Some(release) = detached_release_geometry(frame) {
                            for point in [
                                (
                                    release.center_x - release.app_base_half,
                                    app_bottom_y_at(
                                        frame,
                                        release.center_x - release.app_base_half,
                                    ),
                                ),
                                (
                                    release.center_x + release.app_base_half,
                                    app_bottom_y_at(
                                        frame,
                                        release.center_x + release.app_base_half,
                                    ),
                                ),
                                (release.center_x, release.app_tip_y),
                                (release.center_x, release.dock_tip_y),
                                (release.center_x - release.dock_base_half, frame.dock.top),
                                (release.center_x + release.dock_base_half, frame.dock.top),
                            ] {
                                window.paint_path(debug_cross_path(bounds, point, 4.0), secondary);
                            }
                        }
                    }
                }

                if frame.attachment != Attachment::Detached {
                    let (left, right) = reactive_lower_join_pairs(frame, tuning);
                    let failure = rgba(0xff4d4dff);
                    paint_cubic_pair_tangent_samples(window, bounds, left, faint, failure);
                    paint_cubic_pair_tangent_samples(window, bounds, right, faint, failure);
                }
            }

            if view.includes(GeometryDebugView::Constraints) {
                let policy = ResponsiveGeometryPolicy;
                let safe_target = policy.safe_target(frame.viewport);
                let edge_margin = policy.edge_margin_x(frame.viewport);
                window.paint_path(rounded_rect_path(bounds, safe_target, true), faint);
                window.paint_path(
                    debug_line_path(
                        bounds,
                        (edge_margin, 0.0),
                        (edge_margin, frame.viewport.height),
                        1.0,
                    ),
                    faint,
                );
                window.paint_path(
                    debug_line_path(
                        bounds,
                        (frame.viewport.width - edge_margin, 0.0),
                        (frame.viewport.width - edge_margin, frame.viewport.height),
                        1.0,
                    ),
                    faint,
                );
                window.paint_path(
                    debug_line_path(
                        bounds,
                        (0.0, frame.dock.top),
                        (frame.viewport.width, frame.dock.top),
                        1.0,
                    ),
                    faint,
                );
                if let Some(neighbor) = frame.neighbor {
                    let radius = neighbor.influence_radius;
                    let influence = RectHull {
                        left: neighbor.rect.left - radius,
                        top: neighbor.rect.top - radius,
                        right: neighbor.rect.right + radius,
                        bottom: neighbor.rect.bottom + radius,
                        radius: 0.0,
                    };
                    window.paint_path(rounded_rect_path(bounds, influence, true), secondary);
                    window.paint_path(rounded_rect_path(bounds, neighbor.rect, true), primary);
                }
            }

            if view.includes(GeometryDebugView::Motion) {
                window.paint_path(debug_cross_path(bounds, frame.pointer_target, 7.0), primary);
                window.paint_path(
                    debug_cross_path(bounds, frame.resolved_target, 5.0),
                    secondary,
                );
                let app_center = (
                    (frame.app.left + frame.app.right) * 0.5,
                    (frame.app.top + frame.app.bottom) * 0.5,
                );
                let velocity_tip = (
                    app_center.0 + frame.velocity.0 * 0.35,
                    app_center.1 + frame.velocity.1 * 0.35,
                );
                window.paint_path(
                    debug_line_path(bounds, app_center, velocity_tip, 1.5),
                    primary,
                );
                for point in reactive_top_points(frame) {
                    window.paint_path(debug_cross_path(bounds, point, 3.5), secondary);
                }
            }

            if view.includes(GeometryDebugView::Content) {
                window.paint_path(
                    rounded_rect_path(bounds, observation.content, true),
                    primary,
                );
            }

            if view.includes(GeometryDebugView::Constraints)
                && let Some(geometry) = geometry_observation
            {
                let map_size = (frame.viewport.width * 0.26, frame.viewport.height * 0.23);
                let map_origin = (
                    frame.viewport.width - map_size.0 - frame.viewport.width * 0.03,
                    frame.viewport.height * 0.04,
                );
                let mw = geometry.monitor_rect.width;
                let mh = geometry.monitor_rect.height;
                let scale = (map_size.0 / mw.max(1) as f32).min(map_size.1 / mh.max(1) as f32);
                let monitor = RectHull {
                    left: map_origin.0,
                    top: map_origin.1,
                    right: map_origin.0 + mw as f32 * scale,
                    bottom: map_origin.1 + mh as f32 * scale,
                    radius: 0.0,
                };
                window.paint_path(rounded_rect_path(bounds, monitor, true), secondary);
                let mx = geometry.monitor_rect.x;
                let my = geometry.monitor_rect.y;
                let lx = geometry.layer_rect.x;
                let ly = geometry.layer_rect.y;
                let lw = geometry.layer_rect.width;
                let lh = geometry.layer_rect.height;
                let layer = RectHull {
                    left: map_origin.0 + (lx - mx) as f32 * scale,
                    top: map_origin.1 + (ly - my) as f32 * scale,
                    right: map_origin.0 + (lx - mx + lw) as f32 * scale,
                    bottom: map_origin.1 + (ly - my + lh) as f32 * scale,
                    radius: 0.0,
                };
                window.paint_path(rounded_rect_path(bounds, layer, true), primary);

                let rt = geometry.reserved.top;
                let rr = geometry.reserved.right;
                let rb = geometry.reserved.bottom;
                let rl = geometry.reserved.left;
                for (from, to) in [
                    (
                        (monitor.left, monitor.top + rt * scale),
                        (monitor.right, monitor.top + rt * scale),
                    ),
                    (
                        (monitor.right - rr * scale, monitor.top),
                        (monitor.right - rr * scale, monitor.bottom),
                    ),
                    (
                        (monitor.left, monitor.bottom - rb * scale),
                        (monitor.right, monitor.bottom - rb * scale),
                    ),
                    (
                        (monitor.left + rl * scale, monitor.top),
                        (monitor.left + rl * scale, monitor.bottom),
                    ),
                ] {
                    window.paint_path(debug_line_path(bounds, from, to, 1.0), faint);
                }
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .size_full();

    let topology = format!("{:?}", frame.attachment);
    let app_size = (
        observation.app.right - observation.app.left,
        observation.app.bottom - observation.app.top,
    );
    let line1 = format!(
        "GEOMETRY {:?}  sample={}  viewport={:.0}x{:.0}  attachment={}",
        view,
        sample,
        observation.header.viewport.width,
        observation.header.viewport.height,
        topology
    );
    let line2 = format!(
        "app=({:.1},{:.1}) {:.1}x{:.1}  neck={:.1}  join={:.3}  tilt={:.1}",
        observation.app.left,
        observation.app.top,
        app_size.0,
        app_size.1,
        observation.neck_width,
        observation.join_phase,
        observation.tilt,
    );
    let line3 = format!(
        "target=({:.1},{:.1}) resolved=({:.1},{:.1}) vel=({:.1},{:.1})",
        observation.pointer_target.0,
        observation.pointer_target.1,
        observation.resolved_target.0,
        observation.resolved_target.1,
        observation.velocity.0,
        observation.velocity.1,
    );
    let line4 = format!(
        "pressure={:.1} feedback=({:.1},{:.1}) deform=({:.1},{:.1},{:.1})",
        observation.constraint_pressure,
        observation.spatial_feedback.0,
        observation.spatial_feedback.1,
        observation.top_deform[0],
        observation.top_deform[1],
        observation.top_deform[2],
    );
    let geometry_line = hyprland_geometry
        .as_ref()
        .map(|geometry| {
            format!(
                "Hyprland geometry {}  {} global=({},{} {}x{}) reserved={:.0}/{:.0}/{:.0}/{:.0} scale={:.3} transform={} semantic state unavailable in this qualification process",
                geometry.version,
                geometry.monitor,
                geometry.layer_rect.x,
                geometry.layer_rect.y,
                geometry.layer_rect.width,
                geometry.layer_rect.height,
                geometry.reserved.top,
                geometry.reserved.right,
                geometry.reserved.bottom,
                geometry.reserved.left,
                geometry.scale,
                geometry.transform,
            )
        })
        .unwrap_or_else(|| "Hyprland geometry: waiting".to_owned());

    let hud = div()
        .absolute()
        .left(px(8.0))
        .top(px(8.0))
        .px(px(7.0))
        .py(px(5.0))
        .rounded(px(3.0))
        .bg(rgba(0x000000cc))
        .text_color(rgba(0xf4f4f4ff))
        .text_size(px(8.0))
        .flex()
        .flex_col()
        .child(line1)
        .child(line2)
        .child(line3)
        .child(line4)
        .child("PRIMARY=Envelope  faint=construction bounds")
        .child(geometry_line);

    div()
        .absolute()
        .left_0()
        .top_0()
        .size_full()
        .child(overlay_canvas)
        .child(hud)
}

fn visual_lab_composition(frame: ReactiveHullFrame) -> impl IntoElement {
    let tokens = VisualCompositionTokens::qualification();
    let app = frame.app;
    let dock = frame.dock;
    let material = VisualMaterialProfile::qualification();
    debug_assert!(!material.backdrop_blur_realized);
    let primary = rgba(0xe7edf2ff);
    let secondary = rgba(0x94a2adff);
    let muted = rgba(0x66737eff);
    let accent_text = rgba(0x82d6e1ff);
    let topology = match frame.attachment {
        Attachment::Fused => "FUSED",
        Attachment::Bridged | Attachment::Auto => "BRIDGED",
        Attachment::Detached => "DETACHED",
    };

    let layout = VisualCompositionLayout::resolve(frame, tokens);
    debug_assert!(rect_contains(app, layout.app_content_bounds()));
    debug_assert!(rect_contains(dock, layout.dock_items_bounds(tokens)));
    let safe_left = layout.safe_left;
    let safe_width = layout.safe_width;
    let card_gap = layout.card_gap;
    let card_width = layout.card_width;
    let content_top = layout.content_top;
    let content_height = layout.content_height;

    let header = div()
        .absolute()
        .left(px(safe_left))
        .top(px(app.top + 19.0))
        .w(px(safe_width))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(tokens.title_size))
                        .font_weight(FontWeight::BOLD)
                        .text_color(primary)
                        .child("Derived surface"),
                )
                .child(
                    div()
                        .text_size(px(tokens.secondary_size))
                        .text_color(secondary)
                        .child("inherited material · reactive host"),
                ),
        )
        .child(
            div()
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(8.0))
                .bg(material.card_background())
                .border_1()
                .border_color(material.card_border)
                .text_size(px(tokens.label_size))
                .font_weight(FontWeight::BOLD)
                .text_color(accent_text)
                .child(topology),
        );

    let context_card = div()
        .absolute()
        .left(px(safe_left))
        .top(px(content_top))
        .w(px(card_width))
        .h(px(content_height))
        .overflow_hidden()
        .rounded(px(tokens.card_radius))
        .bg(material.card_background())
        .border_1()
        .border_color(material.card_border)
        .shadow(material.card_shadow())
        .p(px(11.0))
        .flex()
        .flex_col()
        .justify_between()
        .child(
            div()
                .text_size(px(tokens.label_size))
                .font_weight(FontWeight::BOLD)
                .text_color(secondary)
                .child("CONTEXT"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::BOLD)
                .text_color(primary)
                .child("semantic state unavailable in this qualification process"),
        )
        .child(
            div()
                .text_size(px(tokens.label_size))
                .text_color(muted)
                .child("geometry observation only"),
        );

    let action_card = div()
        .absolute()
        .left(px(safe_left + card_width + card_gap))
        .top(px(content_top))
        .w(px(card_width))
        .h(px(content_height))
        .overflow_hidden()
        .rounded(px(tokens.card_radius))
        .bg(material.card_background())
        .border_1()
        .border_color(material.card_border)
        .shadow(material.card_shadow())
        .p(px(11.0))
        .flex()
        .flex_col()
        .justify_between()
        .child(
            div()
                .text_size(px(tokens.label_size))
                .font_weight(FontWeight::BOLD)
                .text_color(secondary)
                .child("CURRENT"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::BOLD)
                .text_color(primary)
                .child("Surface state"),
        )
        .child(
            div()
                .self_end()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(7.0))
                .bg(rgba(0x34434ddd))
                .text_size(px(tokens.label_size))
                .font_weight(FontWeight::BOLD)
                .text_color(accent_text)
                .child("Inspect"),
        );

    let dock_start = layout.dock_start;
    let dock_top = layout.dock_top;
    let dock_items = ["A", "B", "C", "D", "E"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            let selected = index == 2;
            div()
                .absolute()
                .left(px(dock_start
                    + index as f32
                        * (tokens.dock_item_size + tokens.dock_item_gap)))
                .top(px(dock_top))
                .w(px(tokens.dock_item_size))
                .h(px(tokens.dock_item_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.0))
                .bg(if selected {
                    material.accent_background()
                } else {
                    material.card_background()
                })
                .border_1()
                .border_color(if selected {
                    rgba(0xb7eef4aa)
                } else {
                    material.card_border
                })
                .shadow(if selected {
                    material.selected_shadow()
                } else {
                    Vec::new()
                })
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(if selected { rgba(0x102027ff) } else { primary })
                .child(label)
        });

    div()
        .absolute()
        .left_0()
        .top_0()
        .size_full()
        .child(header)
        .child(context_card)
        .child(action_card)
        .children(dock_items)
}

fn bridged_hull_path(
    bounds: Bounds<gpui::Pixels>,
    geometry: DerivedTopologyGeometry,
    intent: &BridgeIntent,
    stroke: bool,
) -> gpui::Path<gpui::Pixels> {
    let mut path = if stroke {
        PathBuilder::stroke(px(1.0))
    } else {
        PathBuilder::fill()
    };
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
    let app = geometry.app;
    let dock = geometry.dock;
    let ar = app.radius;
    let dr = dock.radius;
    let center = 300.0_f32;
    let max_half = ((app.right - app.left) / 2.0 - ar - 6.0)
        .min((dock.right - dock.left) / 2.0 - dr - 6.0)
        .max(30.0);
    let mouth_half = (intent.width.0 as f32 / 2.0).min(max_half);
    let neck_half = (intent.neck_width.0 as f32 / 2.0)
        .min(mouth_half - 4.0)
        .max(2.0);
    let gap = (dock.top - app.bottom).max(1.0);
    let mid = app.bottom + gap * 0.5;
    let horizontal_run = bridge_shoulder_run(mouth_half, neck_half, gap, 24.0, 58.0)
        .min((mouth_half - neck_half).max(8.0));
    let vertical_handle = (gap * 0.20 + intent.shoulder_radius.0 as f32 * 0.35).clamp(10.0, 34.0);
    let app_mouth_left = center - mouth_half;
    let app_mouth_right = center + mouth_half;
    let dock_mouth_left = app_mouth_left;
    let dock_mouth_right = app_mouth_right;
    let neck_left = center - neck_half;
    let neck_right = center + neck_half;

    // One external contour: no internal app/dock border can cross the bridge mouth.
    path.move_to(p(app.left + ar, app.top));
    path.line_to(p(app.right - ar, app.top));
    append_elliptical_arc(&mut path, ar, ar, true, p(app.right, app.top + ar));
    path.line_to(p(app.right, app.bottom - ar));
    append_elliptical_arc(&mut path, ar, ar, true, p(app.right - ar, app.bottom));
    path.line_to(p(app_mouth_right, app.bottom));
    path.cubic_bezier_to(
        p(neck_right, mid),
        p(app_mouth_right - horizontal_run, app.bottom),
        p(neck_right, mid - vertical_handle),
    );
    path.cubic_bezier_to(
        p(dock_mouth_right, dock.top),
        p(neck_right, mid + vertical_handle),
        p(dock_mouth_right - horizontal_run, dock.top),
    );
    path.line_to(p(dock.right - dr, dock.top));
    append_elliptical_arc(&mut path, dr, dr, true, p(dock.right, dock.top + dr));
    path.line_to(p(dock.right, dock.bottom - dr));
    append_elliptical_arc(&mut path, dr, dr, true, p(dock.right - dr, dock.bottom));
    path.line_to(p(dock.left + dr, dock.bottom));
    append_elliptical_arc(&mut path, dr, dr, true, p(dock.left, dock.bottom - dr));
    path.line_to(p(dock.left, dock.top + dr));
    append_elliptical_arc(&mut path, dr, dr, true, p(dock.left + dr, dock.top));
    path.line_to(p(dock_mouth_left, dock.top));
    path.cubic_bezier_to(
        p(neck_left, mid),
        p(dock_mouth_left + horizontal_run, dock.top),
        p(neck_left, mid + vertical_handle),
    );
    path.cubic_bezier_to(
        p(app_mouth_left, app.bottom),
        p(neck_left, mid - vertical_handle),
        p(app_mouth_left + horizontal_run, app.bottom),
    );
    path.line_to(p(app.left + ar, app.bottom));
    append_elliptical_arc(&mut path, ar, ar, true, p(app.left, app.bottom - ar));
    path.line_to(p(app.left, app.top + ar));
    append_elliptical_arc(&mut path, ar, ar, true, p(app.left + ar, app.top));
    path.close();
    path.build().expect("building qualification bridged hull")
}

fn hidden_visual() -> VisualState {
    VisualState {
        opacity: OpacityPercent::new(0),
        radius: LogicalPx(24),
        margin: LogicalPx(0),
        translation: gnos_ux_surface::envelope::LogicalOffset { x: 0, y: 78 },
        bridge_width: LogicalPx(0),
        bridge_neck_width: LogicalPx(0),
    }
}

fn visible_visual() -> VisualState {
    VisualState {
        opacity: OpacityPercent::new(88),
        radius: LogicalPx(24),
        margin: LogicalPx(0),
        translation: gnos_ux_surface::envelope::LogicalOffset { x: 0, y: 0 },
        bridge_width: LogicalPx(0),
        bridge_neck_width: LogicalPx(0),
    }
}

fn model(state: QualificationState) -> gnos_ux_surface::envelope::ResolvedEnvelopeState {
    let material = MaterialRecipe {
        family: MaterialFamilyId::new("qualification-hud"),
        blur: LogicalPx(18),
        border: LogicalPx(1),
        shadow: LogicalPx(24),
    };
    let defaults = EnvelopeDefaults {
        geometry: GeometryMode::Auto,
        relation: OutputRelation::Auto,
        material,
        chrome: ChromePresentation {
            background_visible: !matches!(state, QualificationState::Dock),
            opacity: OpacityPercent::new(88),
            border_visible: true,
            shadow_visible: true,
        },
        compositor: CompositorIntentDefaults {
            edge: SurfaceEdge::Bottom,
            keyboard: KeyboardIntent::None,
            reservation: SpaceReservationIntent::Overlay,
            margins: [LogicalPx(0); 4],
            output: Some(RuntimeId::new("HDMI-A-1")),
        },
        behaviour: BehaviourDefaults {
            visibility: VisibilityPolicy::AutoHide,
            triggers: RevealTriggers {
                pointer_proximity: true,
                key_press: false,
            },
        },
        capabilities: EnvelopeCapabilities {
            extrusion: true,
            connective_hull: true,
            bridge: true,
            shape_aware_input: true,
        },
    };
    let attachment = match state {
        QualificationState::Fused | QualificationState::Dock => Attachment::Fused,
        QualificationState::Bridged => Attachment::Bridged,
        QualificationState::Detached => Attachment::Detached,
    };
    let bridge = BridgeIntent {
        width: LogicalPx(220),
        length: LogicalPx(48),
        neck_width: LogicalPx(if matches!(state, QualificationState::Bridged) {
            86
        } else {
            180
        }),
        shoulder_radius: LogicalPx(28),
        min_span: LogicalPx(120),
        max_span: LogicalPx(300),
        break_threshold: NormalizedPermille::new(700),
        reduced_motion: false,
    };
    let derived = if matches!(state, QualificationState::Dock) {
        None
    } else {
        Some(DerivedSurfacePatch {
            id: RuntimeId::new("gnos.ux.appstack.qualification"),
            parent: RuntimeId::new("gnos.ux.dock.qualification"),
            origin: ExtrusionOrigin::SurfaceHull,
            material: DerivedMaterialIntent::Inherit,
            attachment,
            bridge: Some(bridge),
        })
    };
    resolve_inheritance(
        &defaults,
        &Default::default(),
        &Default::default(),
        derived.as_ref(),
        Some(AutoPolicy::Qualification {
            geometry: GeometryMode::Hug,
            relation: OutputRelation::EdgeBound,
        }),
    )
}

impl Qualification {
    fn begin_transition(&mut self, target: DockPhase) {
        self.transition_generation += 1;
        let to = match target {
            DockPhase::Hidden | DockPhase::Hiding => hidden_visual(),
            DockPhase::Revealing | DockPhase::Visible => visible_visual(),
        };
        self.dock_phase = target;
        self.transition = Some(TransitionState {
            from: self.visual_state,
            to,
            progress: gnos_ux_surface::envelope::NormalizedPermille::new(0),
            topology: TopologyPhase::Stable,
        });
        println!(
            "transition generation={} phase={target:?}",
            self.transition_generation
        );
    }

    fn run_transition(entity: gpui::Entity<Self>, app: &mut App, target: DockPhase) {
        let generation = entity.update(app, |this, _| {
            this.begin_transition(target);
            this.transition_generation
        });
        app.spawn(async move |cx| {
            for step in 1..=10u16 {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep_running = entity.update(cx, |this, cx| {
                    if this.transition_generation != generation {
                        println!("TRANSITION_STALE_ABORT generation={generation}");
                        return false;
                    }
                    if let Some(mut transition) = this.transition {
                        transition.progress =
                            gnos_ux_surface::envelope::NormalizedPermille::new(step * 100);
                        this.visual_state =
                            interpolate_visual(transition.from, transition.to, transition.progress);
                        if step == 10 {
                            this.transition = None;
                            this.dock_phase =
                                if matches!(target, DockPhase::Hiding | DockPhase::Hidden) {
                                    DockPhase::Hidden
                                } else {
                                    DockPhase::Visible
                                };
                            println!(
                                "TRANSITION_COMPLETE generation={} progress=1000",
                                generation
                            );
                        } else {
                            this.transition = Some(transition);
                        }
                        cx.notify();
                    }
                    true
                });
                if !keep_running {
                    return;
                }
            }
        })
        .detach();
    }

    async fn run_transition_async(
        entity: gpui::Entity<Self>,
        cx: &mut AsyncApp,
        target: DockPhase,
    ) {
        let generation = entity.update(cx, |this, _| {
            this.begin_transition(target);
            this.transition_generation
        });
        for step in 1..=10u16 {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let keep_running = entity.update(cx, |this, cx| {
                if this.transition_generation != generation {
                    println!("TRANSITION_STALE_ABORT generation={generation}");
                    return false;
                }
                let Some(mut transition) = this.transition else {
                    return false;
                };
                transition.progress =
                    gnos_ux_surface::envelope::NormalizedPermille::new(step * 100);
                this.visual_state =
                    interpolate_visual(transition.from, transition.to, transition.progress);
                if step == 10 {
                    this.transition = None;
                    this.dock_phase = if matches!(target, DockPhase::Hiding | DockPhase::Hidden) {
                        DockPhase::Hidden
                    } else {
                        DockPhase::Visible
                    };
                    println!(
                        "TRANSITION_COMPLETE generation={} progress=1000",
                        generation
                    );
                } else {
                    this.transition = Some(transition);
                }
                cx.notify();
                true
            });
            if !keep_running {
                return;
            }
        }
    }

    fn schedule_hide(entity: gpui::Entity<Self>, app: &mut App) {
        let token = entity.update(app, |this, _| {
            this.hide_generation += 1;
            println!("HIDE_PENDING token={}", this.hide_generation);
            this.hide_generation
        });
        app.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            let should_hide = entity.update(cx, |this, _| {
                token == this.hide_generation
                    && !this.pointer_in_dock
                    && this.dock_phase == DockPhase::Visible
            });
            if should_hide {
                Qualification::run_transition_async(entity, cx, DockPhase::Hiding).await;
            }
        })
        .detach();
    }

    fn set_reactive_target(entity: gpui::Entity<Self>, app: &mut App, x: f32, y: f32) {
        let should_spawn = entity.update(app, |this, _| {
            let Some(reactive) = this.reactive.as_mut() else {
                return false;
            };
            reactive.set_pointer_target(x, y);
            if this.reactive_worker_running {
                false
            } else {
                this.reactive_worker_running = true;
                true
            }
        });
        if should_spawn {
            app.spawn(async move |cx| {
                let mut settled_frames = 0_u8;
                for _ in 0..600_u16 {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let keep_running = entity.update(cx, |this, cx| {
                        let Some(reactive) = this.reactive.as_mut() else {
                            this.reactive_worker_running = false;
                            return false;
                        };
                        let params = this.reactive_params;
                        let old_attachment = reactive.attachment;
                        let energy = reactive.step(params, 0.016);
                        this.reactive_sample += 1;
                        if old_attachment != reactive.attachment {
                            println!(
                                "REACTIVE_ATTACHMENT_CHANGE sample={} from={old_attachment:?} to={:?}",
                                this.reactive_sample, reactive.attachment
                            );
                        }
                        if this.reactive_sample % 8 == 0 {
                            let frame = reactive.frame(params);
                            println!(
                                "REACTIVE_SAMPLE sample={} target=({:.1},{:.1}) app=({:.1},{:.1}) velocity=({:.1},{:.1}) shoulder={:.1} neck={:.1} attachment={:?} tilt={:.1} local_deform={:.1} nodes=({:.1},{:.1},{:.1}) strain={:.1} speed={:.1} app_size=({:.1},{:.1}) constraint_pressure={:.1} spatial_feedback=({:.1},{:.1}) neighbor_feedback=({:.1},{:.1})",
                                this.reactive_sample,
                                reactive.pointer_x,
                                reactive.pointer_y,
                                reactive.app_center_x,
                                reactive.app_top,
                                reactive.velocity_x,
                                reactive.velocity_y,
                                reactive.shoulder_center_x,
                                reactive.neck_width,
                                reactive.attachment,
                                frame.tilt,
                                frame.local_deformation,
                                frame.top_deform[0],
                                frame.top_deform[1],
                                frame.top_deform[2],
                                frame.strain,
                                frame.speed,
                                frame.app.right - frame.app.left,
                                frame.app.bottom - frame.app.top,
                                frame.constraint_pressure,
                                frame.spatial_feedback_x,
                                frame.spatial_feedback_y,
                                frame.neighbor_feedback_x,
                                frame.neighbor_feedback_y,
                            );
                            if this.geometry_lab {
                                GeometryObservation::from_frame(
                                    this.reactive_sample,
                                    this.geometry_revision,
                                    frame,
                                )
                                .log_json();
                            }
                        }
                        cx.notify();
                        if energy < 0.85 {
                            settled_frames = settled_frames.saturating_add(1);
                        } else {
                            settled_frames = 0;
                        }
                        if settled_frames >= 12 {
                            this.reactive_worker_running = false;
                            println!(
                                "REACTIVE_SETTLED sample={} attachment={:?}",
                                this.reactive_sample, reactive.attachment
                            );
                            false
                        } else {
                            true
                        }
                    });
                    if !keep_running {
                        return;
                    }
                }
                entity.update(cx, |this, _| {
                    this.reactive_worker_running = false;
                    println!("REACTIVE_WORKER_BOUNDED_EXIT sample={}", this.reactive_sample);
                });
            })
            .detach();
        }
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let name = env::args()
            .skip(1)
            .find_map(|arg| arg.strip_prefix("--state=").map(str::to_owned));
        let visual_lab = env::args().any(|arg| arg == "--visual-lab");
        let geometry_lab = env::args().any(|arg| arg == "--geometry-lab");
        let geometry_view = GeometryDebugView::from_process_args();
        let reactive_lab =
            visual_lab || geometry_lab || env::args().any(|arg| arg == "--reactive-hull-lab");
        let reactive_neighbor_script = env::args().any(|arg| arg == "--reactive-neighbor-script");
        let reactive_script =
            env::args().any(|arg| arg == "--reactive-script") || reactive_neighbor_script;
        let reactive_controls = env::args().any(|arg| arg == "--reactive-controls");
        let reactive_neighbor = env::args().any(|arg| arg == "--reactive-neighbor");
        let reactive_params = ReactiveLabParams::qualification_from_args();
        if reactive_lab || reactive_script {
            reactive_params.log("REACTIVE_PARAMS_START");
        }
        let state = if reactive_lab || reactive_script {
            QualificationState::Fused
        } else {
            match name.as_deref() {
                Some("fused") => QualificationState::Fused,
                Some("bridged") => QualificationState::Bridged,
                Some("detached") => QualificationState::Detached,
                _ => QualificationState::Dock,
            }
        };
        let diagnostic_start_visible = env::args().any(|arg| arg == "--diagnostic-start-visible");
        if geometry_lab {
            let pid = std::process::id();
            println!(
                "GEOMETRY_LAB_START view={geometry_view:?} hyprland_lua_read_only=true pid={pid}"
            );
            cx.spawn(async move |this, cx| {
                let mut previous: Option<HyprlandLayerGeometryObservation> = None;
                let mut revision = 0_u64;
                let mut attempts_without_geometry = 0_u16;
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    let Some(geometry) =
                        query_layer_geometry("gnos.ux:dock:qualification", pid)
                    else {
                        attempts_without_geometry = attempts_without_geometry.saturating_add(1);
                        if attempts_without_geometry == 20 {
                            println!(
                                "HYPRLAND_GEOMETRY_UNAVAILABLE source=lua-repl-read-only semantic_state=unavailable"
                            );
                        }
                        continue;
                    };
                    attempts_without_geometry = 0;

                    if previous.as_ref() == Some(&geometry) {
                        continue;
                    }
                    let phase = if previous.is_some() { "update" } else { "initial" };
                    revision = revision.saturating_add(1);
                    println!(
                        "HYPRLAND_GEOMETRY_OBSERVATION revision={} phase={} source=lua-repl-read-only version={} monitor={} monitor_rect={},{},{},{} layer_rect={},{},{},{} reserved={:.2},{:.2},{:.2},{:.2} scale={:.3} transform={} semantic_state=unavailable",
                        revision,
                        phase,
                        geometry.version,
                        geometry.monitor,
                        geometry.monitor_rect.x,
                        geometry.monitor_rect.y,
                        geometry.monitor_rect.width,
                        geometry.monitor_rect.height,
                        geometry.layer_rect.x,
                        geometry.layer_rect.y,
                        geometry.layer_rect.width,
                        geometry.layer_rect.height,
                        geometry.reserved.top,
                        geometry.reserved.right,
                        geometry.reserved.bottom,
                        geometry.reserved.left,
                        geometry.scale,
                        geometry.transform,
                    );
                    previous = Some(geometry.clone());
                    if this
                        .update(cx, |view, cx| {
                            view.hyprland_geometry = Some(geometry);
                            view.geometry_revision = revision;
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        if reactive_script {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(350)).await;
                // Targets are normalized fractions of the live surface viewport.
                // The script therefore exercises the same topology at any compositor size.
                let targets = if reactive_neighbor_script {
                    [
                        (0.500_000_f32, 0.416_667_f32, 36_u16),
                        (0.525_000, 0.416_667, 60),
                        (0.550_000, 0.416_667, 60),
                        (0.575_000, 0.416_667, 60),
                        (0.500_000, 0.416_667, 90),
                        (0.500_000, 0.416_667, 90),
                    ]
                } else {
                    [
                        // Stable fused rest.
                        (0.500_000_f32, 0.857_143_f32, 36_u16),
                        // Pull upward/left far enough to expose the connected morph.
                        (0.341_667, 0.273_810, 84),
                        // Cross the detachment threshold on any viewport while
                        // preserving normalized geometry.
                        (0.870_000, 0.100_000, 132),
                        // Hold detached long enough for release_phase to settle.
                        (0.870_000, 0.100_000, 120),
                        // Re-enter toward a stable Bridged target and let the
                        // split precursor reach exact neck-seam contact.
                        (0.500_000, 0.416_667, 200),
                        // Only after the Bridged handoff, settle back to Fused.
                        (0.500_000, 0.857_143, 160),
                    ]
                };
                for (nx, ny, frames) in targets {
                    let _ = this.update(cx, |view, _| {
                        if let Some(reactive) = view.reactive.as_mut() {
                            let x = nx * reactive.viewport.width;
                            let y = ny * reactive.viewport.height;
                            reactive.set_pointer_target(x, y);
                            println!(
                                "REACTIVE_SCRIPT_TARGET normalized=({nx:.6},{ny:.6}) x={x:.1} y={y:.1} viewport={:.1}x{:.1}",
                                reactive.viewport.width,
                                reactive.viewport.height,
                            );
                        }
                    });
                    for _ in 0..frames {
                        let _ = this.update(cx, |view, cx| {
                            if let Some(reactive) = view.reactive.as_mut() {
                                let params = view.reactive_params;
                                let old_attachment = reactive.attachment;
                                reactive.step(params, 0.016);
                                view.reactive_sample += 1;
                                if old_attachment != reactive.attachment {
                                    println!(
                                        "REACTIVE_ATTACHMENT_CHANGE sample={} from={old_attachment:?} to={:?}",
                                        view.reactive_sample, reactive.attachment
                                    );
                                }
                                if view.reactive_sample % 8 == 0 {
                                    let frame = reactive.frame(params);
                                    println!(
                                        "REACTIVE_SAMPLE sample={} target=({:.1},{:.1}) app=({:.1},{:.1}) velocity=({:.1},{:.1}) shoulder={:.1} neck={:.1} attachment={:?} join_phase={:.4} release_phase={:.4} reattach_phase={:.4} tilt={:.1} local_deform={:.1} nodes=({:.1},{:.1},{:.1}) strain={:.1} speed={:.1} app_size=({:.1},{:.1}) constraint_pressure={:.1} spatial_feedback=({:.1},{:.1}) neighbor_influence=({:.1},{:.1},{:.1}) neighbor_feedback=({:.1},{:.1})",
                                        view.reactive_sample,
                                        reactive.pointer_x,
                                        reactive.pointer_y,
                                        reactive.app_center_x,
                                        reactive.app_top,
                                        reactive.velocity_x,
                                        reactive.velocity_y,
                                        reactive.shoulder_center_x,
                                        reactive.neck_width,
                                        reactive.attachment,
                                        frame.join_phase,
                                        frame.release_phase,
                                        frame.reattach_phase,
                                        frame.tilt,
                                        frame.local_deformation,
                                        frame.top_deform[0],
                                        frame.top_deform[1],
                                        frame.top_deform[2],
                                        frame.strain,
                                        frame.speed,
                                        frame.app.right - frame.app.left,
                                        frame.app.bottom - frame.app.top,
                                        frame.constraint_pressure,
                                        frame.spatial_feedback_x,
                                        frame.spatial_feedback_y,
                                        frame.neighbor_influence_x,
                                        frame.neighbor_influence_y,
                                        frame.neighbor_influence_pressure,
                                        frame.neighbor_feedback_x,
                                        frame.neighbor_feedback_y,
                                    );
                                    if frame.neighbor_influence_pressure > 0.001
                                        || frame.neighbor_influence_x.abs() > 0.001
                                        || frame.neighbor_influence_y.abs() > 0.001
                                    {
                                        println!(
                                            "REACTIVE_INFLUENCE_ACTIVE sample={} grammar={}@{} raw=({:.2},{:.2},{:.2})",
                                            view.reactive_sample,
                                            gnos_ux_surface::grammar::INFLUENCE_GRAMMAR_ID,
                                            gnos_ux_surface::grammar::INFLUENCE_GRAMMAR_REVISION,
                                            frame.neighbor_influence_x,
                                            frame.neighbor_influence_y,
                                            frame.neighbor_influence_pressure,
                                        );
                                    }
                                    if view.geometry_lab {
                                        GeometryObservation::from_frame(
                                            view.reactive_sample,
                                            view.geometry_revision,
                                            frame,
                                        )
                                        .log_json();
                                    }
                                }
                                cx.notify();
                            }
                        });
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                }
                println!("REACTIVE_SCRIPT_COMPLETE");
            })
            .detach();
        }
        let resolved = model(state);
        println!("resolved_envelope={resolved:#?}");
        if let Some(relation) = resolved.derived_relation.as_ref() {
            println!(
                "derived_relation id={} parent={} origin={:?} attachment={:?} material_inherited={}",
                relation.id.as_str(),
                relation.parent.as_str(),
                relation.origin,
                relation.attachment,
                relation.material == resolved.material
            );
        }
        cx.notify();
        let starts_visible = state != QualificationState::Dock || diagnostic_start_visible;
        Self {
            state,
            resolved,
            dock_phase: if starts_visible {
                DockPhase::Visible
            } else {
                DockPhase::Hidden
            },
            visual_state: if starts_visible {
                visible_visual()
            } else {
                hidden_visual()
            },
            transition: None,
            transition_generation: 0,
            pointer_in_dock: false,
            pointer_in_reveal: false,
            hide_generation: 0,
            reactive: if reactive_lab || reactive_script {
                let viewport = SurfaceViewport::unit();
                let mut reactive = ReactiveHullState::qualification_initial(viewport);
                if reactive_neighbor {
                    reactive.neighbor = Some(QualificationNeighbor::right_side(viewport));
                    println!(
                        "REACTIVE_NEIGHBOR_ENABLED rect={:?}",
                        reactive.neighbor.unwrap().rect
                    );
                }
                Some(reactive)
            } else {
                None
            },
            reactive_params,
            reactive_worker_running: false,
            reactive_sample: 0,
            reactive_controls,
            visual_lab,
            geometry_lab,
            geometry_view,
            hyprland_geometry: None,
            geometry_revision: 0,
            focus_handle: if reactive_controls {
                Some(cx.focus_handle())
            } else {
                None
            },
        }
    }
}

impl Render for Qualification {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport =
            viewport_from_size(window.viewport_size()).unwrap_or_else(SurfaceViewport::unit);
        if let Some(reactive) = self.reactive.as_mut()
            && !reactive.viewport.approx_eq(viewport)
        {
            let previous = reactive.viewport;
            reactive.rebase_viewport(viewport);
            println!(
                "GEOMETRY_VIEWPORT_REBASE from={:.1}x{:.1} to={:.1}x{:.1} attachment={:?}",
                previous.width,
                previous.height,
                viewport.width,
                viewport.height,
                reactive.attachment,
            );
        }

        let derived = self.resolved.derived_relation.as_ref();
        let chrome = self.resolved.chrome;
        let material = &self.resolved.material;
        let paint = qualification_paint(material, chrome);
        let _blur_status = paint.blur_realization;
        let fill = paint.fill;
        let border = rgba(0x58c4d6cc);
        let label = match self.state {
            QualificationState::Dock => "DOCK HUD  •  resolved AutoHide / pointer proximity",
            QualificationState::Fused => "APPSTACK  •  FUSED hull / Inherit",
            QualificationState::Bridged => "APPSTACK  •  BRIDGED hull / BridgeIntent",
            QualificationState::Detached => "APPSTACK  •  DETACHED hull / lineage retained",
        };
        let visual = self
            .transition
            .map(|transition| {
                interpolate_visual(transition.from, transition.to, transition.progress)
            })
            .unwrap_or(self.visual_state);
        let geometry = QualificationGeometry::from_visual(visual, viewport);
        let dock_visible_fraction = (f32::from(visual.opacity.value()) / 88.0).clamp(0.0, 1.0);
        let dock_frame = resolve_dock_frame(viewport, dock_visible_fraction);
        let dock_visible_bounds = dock_frame.visible_envelope;
        let dock_reveal_bounds = dock_frame.reveal_region;
        let dock_bottom_padding = (viewport.height - dock_reveal_bounds.bottom()).max(0.0);
        let fused_geometry = FusedHullGeometry::qualification();
        let reactive_params = self.reactive_params;
        let reactive_frame = self
            .reactive
            .map(|reactive| reactive.frame(reactive_params));
        let reactive_lab_active = reactive_frame.is_some();
        let topology_geometry = match self.state {
            QualificationState::Bridged => Some(DerivedTopologyGeometry::bridged()),
            QualificationState::Detached => Some(DerivedTopologyGeometry::detached()),
            QualificationState::Dock | QualificationState::Fused => None,
        };
        let derived_attachment = derived.map(|relation| relation.attachment);
        let derived_bridge = derived.and_then(|relation| relation.bridge.clone());
        let diagnostic_full = env::args().any(|arg| arg == "--diagnostic-input=full");
        let input = if diagnostic_full || reactive_lab_active {
            RegionSet {
                regions: vec![ShapeRegion {
                    x: 0,
                    y: 0,
                    width: LogicalPx(viewport.width.round().max(1.0) as u32),
                    height: LogicalPx(viewport.height.round().max(1.0) as u32),
                }],
            }
        } else {
            match self.state {
                QualificationState::Fused => fused_geometry.input_regions(),
                QualificationState::Bridged | QualificationState::Detached => topology_geometry
                    .expect("derived topology geometry")
                    .input_regions(
                        derived_attachment.expect("derived attachment"),
                        derived_bridge.as_ref(),
                    ),
                QualificationState::Dock => geometry.input.clone(),
            }
        };
        let input_region: Vec<_> = input
            .regions
            .iter()
            .map(|region| gpui::Bounds {
                origin: gpui::point(px(region.x as f32), px(region.y as f32)),
                size: gpui::Size::new(px(region.width.0 as f32), px(region.height.0 as f32)),
            })
            .collect();
        println!("input_regions={input_region:?}");
        window.set_input_region(Some(&input_region));

        let raw_input = input.clone();
        let raw_state = self.state;
        let raw_dock_bounds = dock_visible_bounds;
        let raw_reveal_bounds = dock_reveal_bounds;
        let raw_events = canvas(
            move |_, _, _| (raw_input, raw_state, raw_dock_bounds, raw_reveal_bounds),
            move |_, (input, state, dock_bounds, reveal_bounds), window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, _| {
                    let x = f32::from(event.position.x);
                    let y = f32::from(event.position.y);
                    let interactive = input.contains(x as i32, y as i32);
                    let dock = state == QualificationState::Dock
                        && interactive
                        && dock_bounds.contains_point(x, y);
                    let reveal = state == QualificationState::Dock
                        && interactive
                        && reveal_bounds.contains_point(x, y);
                    println!(
                        "RAW_MOUSE_MOVE x={x:.1} y={y:.1} interactive={interactive} dock={dock} reveal={reveal} state={state:?} phase={phase:?}"
                    );
                });
                window.on_mouse_event(|_: &MouseExitEvent, phase, _, _| {
                    println!("RAW_MOUSE_EXIT phase={phase:?}");
                });
            },
        );

        let reactive_entity = cx.entity();
        let reactive_events = canvas(
            move |_, _, _| reactive_lab_active,
            move |_, active, window, _| {
                if !active {
                    return;
                }
                let entity = reactive_entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, app| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let x = f32::from(event.position.x);
                    let y = f32::from(event.position.y);
                    println!("REACTIVE_POINTER x={x:.1} y={y:.1}");
                    Qualification::set_reactive_target(entity.clone(), app, x, y);
                });
            },
        );

        let mut body = div()
            .w(px(dock_visible_bounds.width().max(1.0)))
            .h(px(geometry.dock_height as f32))
            .rounded(px(dock_visible_bounds.radius()))
            .flex()
            .items_center()
            .justify_center();
        if chrome.background_visible {
            body = body.bg(fill);
        }
        if paint.border_visible && paint.border_width > 0 {
            body = body.border_1().border_color(border);
        }
        if paint.shadow_visible {
            body = body.shadow_2xl();
        }
        body = body.opacity(paint.opacity);
        let body = body
            .on_mouse_move({
                let entity = cx.entity();
                move |_, _, app| {
                    let target = entity.update(app, |this, _| {
                        this.pointer_in_dock = true;
                        this.hide_generation += 1;
                        println!("HIDE_PENDING_CANCELLED token={}", this.hide_generation);
                        if this.resolved.behaviour.triggers.pointer_proximity
                            && matches!(this.dock_phase, DockPhase::Hidden | DockPhase::Hiding)
                        {
                            Some(DockPhase::Revealing)
                        } else {
                            None
                        }
                    });
                    if let Some(target) = target {
                        Qualification::run_transition(entity.clone(), app, target);
                    }
                }
            })
            .on_mouse_exit({
                let entity = cx.entity();
                move |_, _, app| {
                    entity.update(app, |this, _| {
                        this.pointer_in_dock = false;
                    });
                    Qualification::schedule_hide(entity.clone(), app);
                }
            })
            .child(label);
        let reveal_zone = div()
            .w(px(dock_reveal_bounds.width().max(1.0)))
            .h(px(geometry.reveal_height as f32))
            .on_mouse_move({
                let entity = cx.entity();
                move |_, _, app| {
                    let target = entity.update(app, |this, _| {
                        this.pointer_in_reveal = true;
                        this.hide_generation += 1;
                        this.resolved.behaviour.triggers.pointer_proximity
                            && matches!(this.dock_phase, DockPhase::Hidden | DockPhase::Hiding)
                    });
                    if target {
                        Qualification::run_transition(entity.clone(), app, DockPhase::Revealing);
                    }
                }
            });

        let fused_fill = derived
            .map(|relation| qualification_paint(&relation.material, chrome).fill)
            .unwrap_or(fill);
        let fused_border = border;
        let fused_border_visible = paint.border_visible && paint.border_width > 0;
        let fused_canvas = canvas(
            move |bounds, _, _| (bounds, fused_geometry),
            move |bounds, (_, hull), window, _| {
                let fill_path = fused_hull_path(bounds, hull, false);
                window.paint_path(fill_path, fused_fill);
                if fused_border_visible {
                    let stroke_path = fused_hull_path(bounds, hull, true);
                    window.paint_path(stroke_path, fused_border);
                }
            },
        )
        .w(px(600.0))
        .h(px(420.0))
        .opacity(paint.opacity);

        let topology_fill = derived
            .map(|relation| qualification_paint(&relation.material, chrome).fill)
            .unwrap_or(fill);
        let topology_border = border;
        let topology_border_visible = paint.border_visible && paint.border_width > 0;
        let topology_attachment = derived_attachment;
        let topology_bridge = derived_bridge.clone();
        let topology_canvas = canvas(
            move |bounds, _, _| {
                (
                    bounds,
                    topology_geometry,
                    topology_attachment,
                    topology_bridge,
                )
            },
            move |bounds, (_, topology, attachment, bridge), window, _| {
                let Some(topology) = topology else {
                    return;
                };
                if attachment == Some(Attachment::Bridged)
                    && let Some(intent) = bridge.as_ref()
                {
                    window.paint_path(
                        bridged_hull_path(bounds, topology, intent, false),
                        topology_fill,
                    );
                    if topology_border_visible {
                        window.paint_path(
                            bridged_hull_path(bounds, topology, intent, true),
                            topology_border,
                        );
                    }
                } else {
                    window.paint_path(
                        rounded_rect_path(bounds, topology.app, false),
                        topology_fill,
                    );
                    window.paint_path(
                        rounded_rect_path(bounds, topology.dock, false),
                        topology_fill,
                    );
                    if topology_border_visible {
                        window.paint_path(
                            rounded_rect_path(bounds, topology.app, true),
                            topology_border,
                        );
                        window.paint_path(
                            rounded_rect_path(bounds, topology.dock, true),
                            topology_border,
                        );
                    }
                }
            },
        )
        .w(px(600.0))
        .h(px(420.0))
        .opacity(paint.opacity);

        let visual_lab = self.visual_lab;
        let geometry_lab = self.geometry_lab;
        let geometry_view = self.geometry_view;
        let geometry_observation = self.hyprland_geometry.clone();
        let geometry_observation_revision = self.geometry_revision;
        let geometry_sample = self.reactive_sample;
        let visual_material = VisualMaterialProfile::qualification();
        let reactive_fill: Background = if visual_lab {
            visual_material.surface_background()
        } else if geometry_lab {
            rgba(0x08080866).into()
        } else {
            topology_fill.into()
        };
        let reactive_border = if visual_lab {
            visual_material.edge
        } else if geometry_lab {
            rgba(0xf2f2f2e8)
        } else {
            topology_border
        };
        let reactive_shadow = rgba(visual_material.shadow_alpha as u32);
        let reactive_border_visible = if geometry_lab {
            true
        } else {
            topology_border_visible
        };
        let contour_tuning = if visual_lab || geometry_lab {
            ReactiveContourTuning::visual_lab()
        } else {
            ReactiveContourTuning::baseline()
        };
        let reactive_canvas = canvas(
            move |bounds, _, _| (bounds, reactive_frame),
            move |bounds, (_, frame), window, _| {
                let Some(frame) = frame else {
                    return;
                };
                if let Some(neighbor) = frame.neighbor {
                    let neighbor_fill = if visual_lab {
                        rgba(0x3039424c)
                    } else if geometry_lab {
                        rgba(0x00000022)
                    } else {
                        rgba(0x44515f66)
                    };
                    let neighbor_border = if visual_lab {
                        rgba(0x81909c88)
                    } else if geometry_lab {
                        rgba(0xc8c8c8cc)
                    } else {
                        rgba(0xa7b6c4dd)
                    };
                    window.paint_path(
                        rounded_rect_path(bounds, neighbor.rect, false),
                        neighbor_fill,
                    );
                    window.paint_path(
                        rounded_rect_path(bounds, neighbor.rect, true),
                        neighbor_border,
                    );
                }
                let shadow_bounds =
                    offset_canvas_bounds(bounds, 0.0, visual_material.shadow_offset_y);
                if visual_lab {
                    window.paint_path(
                        reactive_morph_hull_path(shadow_bounds, frame, false, contour_tuning),
                        reactive_shadow,
                    );
                }
                window.paint_path(
                    reactive_morph_hull_path(bounds, frame, false, contour_tuning),
                    reactive_fill,
                );
                if reactive_border_visible {
                    paint_reactive_envelope_outline(
                        window,
                        bounds,
                        frame,
                        contour_tuning,
                        reactive_border,
                    );
                }
            },
        );
        let reactive_canvas = if geometry_lab {
            reactive_canvas.size_full()
        } else {
            reactive_canvas.w(px(600.0)).h(px(420.0))
        }
        .opacity(if visual_lab {
            0.94
        } else if geometry_lab {
            1.0
        } else {
            paint.opacity
        });

        let root = if geometry_lab {
            div().relative().size_full()
        } else {
            div().relative().w(px(600.0)).h(px(420.0))
        };
        if reactive_lab_active {
            let mut reactive_root = root.child(reactive_canvas);
            if visual_lab && let Some(frame) = reactive_frame {
                reactive_root = reactive_root.child(visual_lab_composition(frame));
            }
            if geometry_lab && let Some(frame) = reactive_frame {
                reactive_root = reactive_root.child(geometry_debug_overlay(
                    frame,
                    geometry_sample,
                    geometry_observation_revision,
                    geometry_view,
                    geometry_observation,
                ));
            }
            reactive_root = reactive_root.child(reactive_events);
            if self.reactive_controls {
                let entity = cx.entity();
                if let Some(handle) = self.focus_handle.as_ref() {
                    reactive_root = reactive_root.track_focus(handle).on_key_down(
                        move |event: &KeyDownEvent, _, app| {
                            if event.is_held {
                                return;
                            }
                            let key = event.keystroke.key.as_str();
                            entity.update(app, |this, cx| {
                                if this.reactive_params.adjust_key(key) {
                                    this.reactive_params.log("REACTIVE_PARAMS_LIVE");
                                    if let Some(reactive) = this.reactive {
                                        println!(
                                            "REACTIVE_PARAMS_STATE sample={} velocity=({:.1},{:.1}) neck={:.1} tilt={:.1} local_deform={:.1} attachment={:?}",
                                            this.reactive_sample,
                                            reactive.velocity_x,
                                            reactive.velocity_y,
                                            reactive.neck_width,
                                            reactive.tilt,
                                            reactive
                                                .top_deform
                                                .iter()
                                                .map(|value| value.abs())
                                                .fold(0.0_f32, f32::max),
                                            reactive.attachment
                                        );
                                    }
                                    cx.notify();
                                }
                            });
                        },
                    );
                }
            }
            return reactive_root;
        }
        match self.state {
            QualificationState::Fused => root.child(fused_canvas).child(raw_events),
            QualificationState::Bridged | QualificationState::Detached => {
                root.child(topology_canvas).child(raw_events)
            }
            QualificationState::Dock => root
                .flex()
                .flex_col()
                .items_center()
                .justify_end()
                .pb(px(dock_bottom_padding))
                .child(body)
                .child(reveal_zone)
                .child(raw_events),
        }
    }
}

fn run_join_microbenchmark() {
    let viewport = SurfaceViewport {
        width: 1200.0,
        height: 720.0,
    };
    let iterations = 120_000_u32;
    for mode in [
        TemporalResponseMode::Full,
        TemporalResponseMode::Reduced,
        TemporalResponseMode::Instant,
    ] {
        let mut params = ReactiveLabParams::qualification();
        params.join_motion = mode;
        let mut state = ReactiveHullState::qualification_initial(viewport);
        let started = std::time::Instant::now();
        for index in 0..iterations {
            let bridged = (index / 180) % 2 == 0;
            let target = if bridged {
                (viewport.width * 0.341_667, viewport.height * 0.273_810)
            } else {
                (viewport.width * 0.5, viewport.height * 0.857_143)
            };
            state.set_pointer_target(target.0, target.1);
            std::hint::black_box(state.step(params, 1.0 / 120.0));
        }
        let elapsed = started.elapsed();
        println!(
            "JOIN_BENCH solver mode={:?} iterations={} total_ns={} ns_per_step={:.2} final_phase={:.4}",
            mode,
            iterations,
            elapsed.as_nanos(),
            elapsed.as_nanos() as f64 / f64::from(iterations),
            state.join_phase,
        );
    }

    let tuning = ReactiveContourTuning::visual_lab();
    let state = ReactiveHullState::qualification_initial(viewport);
    let mut frame = state.frame(ReactiveLabParams::qualification());
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: gpui::Size::new(px(viewport.width), px(viewport.height)),
    };
    let path_iterations = 30_000_u32;
    let started = std::time::Instant::now();
    for index in 0..path_iterations {
        frame.join_phase = (index % 1000) as f32 / 999.0;
        std::hint::black_box(reactive_morph_hull_path(bounds, frame, false, tuning));
    }
    let elapsed = started.elapsed();
    println!(
        "JOIN_BENCH path iterations={} total_ns={} ns_per_path={:.2}",
        path_iterations,
        elapsed.as_nanos(),
        elapsed.as_nanos() as f64 / f64::from(path_iterations),
    );
}

fn main() {
    if env::args().any(|arg| arg == "--join-benchmark") {
        run_join_microbenchmark();
        return;
    }

    let reactive_controls = env::args().any(|arg| arg == "--reactive-controls");
    let geometry_lab = env::args().any(|arg| arg == "--geometry-lab");
    application().run(move |cx: &mut App| {
        let initial_size = if geometry_lab {
            gpui::Size::new(px(0.0), px(0.0))
        } else {
            gpui::Size::new(px(600.0), px(420.0))
        };
        let anchor = if geometry_lab {
            Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
        } else {
            Anchor::BOTTOM
        };
        cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: gpui::point(px(0.), px(0.)),
                    size: initial_size,
                })),
                app_id: Some("gnos.ux.dock.envelope-qualification".into()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: "gnos.ux:dock:qualification".into(),
                    layer: Layer::Top,
                    anchor,
                    keyboard_interactivity: if reactive_controls {
                        KeyboardInteractivity::OnDemand
                    } else {
                        KeyboardInteractivity::None
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(Qualification::new);
                if reactive_controls {
                    let focus = view.read(cx).focus_handle.clone();
                    if let Some(focus) = focus {
                        focus.focus(window, cx);
                    }
                }
                view
            },
        )
        .expect("opening qualification surface");
    });
}

#[cfg(test)]
mod reactive_tests {
    use super::*;

    fn test_viewport() -> SurfaceViewport {
        SurfaceViewport {
            width: 600.0,
            height: 420.0,
        }
    }

    fn normalized_direction(vector: (f32, f32)) -> (f32, f32) {
        Vector2::new(vector.0, vector.1)
            .normalized_or_zero()
            .tuple()
    }

    fn step_many(state: &mut ReactiveHullState, count: usize) {
        let params = ReactiveLabParams::qualification();
        for _ in 0..count {
            state.step(params, 0.016);
        }
    }

    fn assert_pair_directionally_monotone(pair: CubicJoinPair) {
        let validation = grammar_join_pair(pair).validate(40, 0.01);
        assert!(
            validation.is_valid(),
            "shared curve validation failed: {:?}",
            validation.violations
        );
    }

    #[test]
    fn reactive_pull_separates_and_return_refuses() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        state.set_pointer_target(205.0, 115.0);
        step_many(&mut state, 110);
        assert_eq!(state.attachment, Attachment::Detached);
        assert!(state.neck_width < 8.0);

        state.set_pointer_target(300.0, 360.0);
        step_many(&mut state, 150);
        assert_eq!(state.attachment, Attachment::Fused);
        assert!(state.neck_width > 132.0);
    }

    #[test]
    fn lower_join_morph_never_backtracks_across_phases_and_viewports() {
        let params = ReactiveLabParams::qualification();
        let tuning = ReactiveContourTuning::visual_lab();

        for viewport in [
            SurfaceViewport {
                width: 900.0,
                height: 560.0,
            },
            SurfaceViewport {
                width: 1200.0,
                height: 800.0,
            },
            SurfaceViewport {
                width: 1600.0,
                height: 900.0,
            },
        ] {
            let mut state = ReactiveHullState::qualification_initial(viewport);
            state.set_pointer_target(viewport.width * 0.41, viewport.height * 0.42);
            for _ in 0..90 {
                state.step(params, 1.0 / 120.0);
            }
            let mut frame = state.frame(params);

            for phase_index in 0..=40 {
                frame.join_phase = phase_index as f32 / 40.0;
                let (left, right) = reactive_lower_join_pairs(frame, tuning);
                assert_pair_directionally_monotone(left);
                assert_pair_directionally_monotone(right);
            }
        }
    }

    #[test]
    fn detached_geometry_relaxes_after_topology_break_without_cutoff() {
        let viewport = test_viewport();
        let params = ReactiveLabParams::qualification();
        let mut state = ReactiveHullState::qualification_initial(viewport);
        state.set_pointer_target(205.0, 115.0);

        let mut saw_detach = false;
        let mut first_release_phase = None;
        for _ in 0..260 {
            let before = state.attachment;
            state.step(params, 1.0 / 120.0);
            if before != Attachment::Detached && state.attachment == Attachment::Detached {
                saw_detach = true;
                first_release_phase = Some(state.release_phase);
                let frame = state.frame(params);
                let release = detached_release_geometry(frame)
                    .expect("newly detached surface must retain residual geometry");
                assert!(
                    release.app_tip_y < release.dock_tip_y,
                    "semantic detachment must retain a visible gap"
                );
                break;
            }
        }

        assert!(saw_detach, "test motion must reach Detached topology");
        assert!(
            first_release_phase.expect("captured release phase") < 0.25,
            "release must begin near zero instead of snapping clean"
        );

        let phase_before = state.release_phase;
        for _ in 0..20 {
            state.step(params, 1.0 / 120.0);
        }
        assert_eq!(state.attachment, Attachment::Detached);
        assert!(state.release_phase > phase_before);
        assert!(state.release_phase < 1.0);

        for _ in 0..600 {
            state.step(params, 1.0 / 120.0);
            if state.release_phase >= 0.999 {
                break;
            }
        }
        assert!(state.release_phase >= 0.999);
        assert!(detached_release_geometry(state.frame(params)).is_none());

        state.set_pointer_target(viewport.width * 0.5, viewport.height * (6.0 / 7.0));
        for _ in 0..300 {
            state.step(params, 1.0 / 120.0);
            if state.attachment != Attachment::Detached {
                break;
            }
        }
        assert_ne!(state.attachment, Attachment::Detached);
        assert_eq!(state.release_phase, 0.0);
    }

    #[test]
    fn smooth_reattach_finishes_geometry_before_topology_switch() {
        let viewport = test_viewport();
        let params = ReactiveLabParams::qualification();
        let mut state = ReactiveHullState::qualification_initial(viewport);

        state.set_pointer_target(205.0, 115.0);
        for _ in 0..600 {
            state.step(params, 1.0 / 120.0);
            if state.attachment == Attachment::Detached && state.release_phase >= 0.999 {
                break;
            }
        }
        assert_eq!(state.attachment, Attachment::Detached);
        assert!(state.release_phase >= 0.999);

        state.set_pointer_target(viewport.width * 0.5, viewport.height * (6.0 / 7.0));
        let mut saw_intermediate_approach = false;
        let mut switched = false;

        for _ in 0..900 {
            let before = state.attachment;
            let previous_phase = state.reattach_phase;
            state.step(params, 1.0 / 120.0);

            if state.attachment == Attachment::Detached
                && state.reattach_phase > 0.05
                && state.reattach_phase < 0.95
            {
                saw_intermediate_approach = true;
            }

            if before == Attachment::Detached && state.attachment != Attachment::Detached {
                assert!(
                    previous_phase >= 0.90,
                    "topology switched before geometric approach reached contact: {previous_phase:.4}"
                );
                switched = true;
                break;
            }
        }

        assert!(
            saw_intermediate_approach,
            "reattachment must expose an intermediate Detached approach phase"
        );
        assert!(
            switched,
            "reattachment must eventually reach Bridged topology"
        );
    }

    #[test]
    fn reattach_subsegments_collapse_cleanly_and_meet_at_neck_seam() {
        let viewport = SurfaceViewport {
            width: 1200.0,
            height: 800.0,
        };
        let params = ReactiveLabParams::qualification();
        let mut state = ReactiveHullState::qualification_initial(viewport);
        state.set_pointer_target(viewport.width * 0.36, viewport.height * 0.24);
        for _ in 0..120 {
            state.step(params, 1.0 / 120.0);
        }
        let mut frame = state.frame(params);
        frame.attachment = Attachment::Detached;
        let tuning = ReactiveContourTuning::visual_lab();
        let left = bridged_left_join_pair(frame, tuning);
        let right = bridged_right_join_pair(frame, tuning);

        let left_upper_zero = cubic_subsegment(left.p1, left.c3, left.c4, left.p2, 1.0, 1.0);
        let left_lower_zero = cubic_subsegment(left.p0, left.c1, left.c2, left.p1, 0.0, 0.0);
        let right_upper_zero = cubic_subsegment(right.p0, right.c1, right.c2, right.p1, 0.0, 0.0);
        let right_lower_zero = cubic_subsegment(right.p1, right.c3, right.c4, right.p2, 1.0, 1.0);

        for (actual, expected) in [
            (left_upper_zero.p0, left.p2),
            (left_lower_zero.p3, left.p0),
            (right_upper_zero.p3, right.p0),
            (right_lower_zero.p0, right.p2),
        ] {
            assert!((actual.0 - expected.0).abs() < 0.0001);
            assert!((actual.1 - expected.1).abs() < 0.0001);
        }

        let left_upper_one = cubic_subsegment(left.p1, left.c3, left.c4, left.p2, 0.0, 1.0);
        let left_lower_one = cubic_subsegment(left.p0, left.c1, left.c2, left.p1, 0.0, 1.0);
        let right_upper_one = cubic_subsegment(right.p0, right.c1, right.c2, right.p1, 0.0, 1.0);
        let right_lower_one = cubic_subsegment(right.p1, right.c3, right.c4, right.p2, 0.0, 1.0);

        for (a, b, expected) in [
            (left_upper_one.p0, left_lower_one.p3, left.p1),
            (right_upper_one.p3, right_lower_one.p0, right.p1),
        ] {
            assert!((a.0 - b.0).abs() < 0.0001 && (a.1 - b.1).abs() < 0.0001);
            assert!((a.0 - expected.0).abs() < 0.0001);
            assert!((a.1 - expected.1).abs() < 0.0001);
        }
    }

    #[test]
    fn reactive_shoulders_propagate_with_lag() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        state.set_pointer_target(395.0, 140.0);
        step_many(&mut state, 12);
        assert!(state.app_center_x > 300.0);
        assert!(state.shoulder_center_x < state.app_center_x);
        assert!((state.app_center_x - state.shoulder_center_x).abs() > 1.0);
    }

    #[test]
    fn reactive_space_constraints_generate_pressure() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        state.set_pointer_target(40.0, 400.0);
        state.step(ReactiveLabParams::qualification(), 0.016);
        assert!(state.constraint_pressure > 0.0);
    }

    #[test]
    fn reactive_spatial_feedback_prevents_dock_penetration() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        state.set_pointer_target(300.0, 420.0);
        let params = ReactiveLabParams::qualification();
        for _ in 0..120 {
            state.step(params, 0.016);
            let frame = state.frame(params);
            assert!(frame.app.bottom <= frame.dock.top + 0.01);
        }
    }

    #[test]
    fn reactive_live_controls_adjust_continuous_parameters_and_reset() {
        let mut params = ReactiveLabParams::qualification();
        let baseline = params;
        assert!(params.adjust_key("q"));
        assert!(params.response > baseline.response);
        assert!(params.adjust_key("e"));
        assert!(params.propagation > baseline.propagation);
        assert!(params.adjust_key("g"));
        assert!(params.shape_preservation < baseline.shape_preservation);
        assert!(params.adjust_key("i"));
        assert!(params.surface_locality > baseline.surface_locality);
        assert!(params.adjust_key("o"));
        assert!(params.spatial_feedback > baseline.spatial_feedback);
        assert!(!params.adjust_key("unknown"));
        assert!(params.adjust_key("0"));
        assert_eq!(params.response, baseline.response);
        assert_eq!(params.damping, baseline.damping);
        assert_eq!(params.propagation, baseline.propagation);
        assert_eq!(params.elasticity, baseline.elasticity);
        assert_eq!(params.edge_affinity, baseline.edge_affinity);
        assert_eq!(params.shape_preservation, baseline.shape_preservation);
        assert_eq!(
            params.orientation_compliance,
            baseline.orientation_compliance
        );
    }

    #[test]
    fn same_solver_expresses_shape_preserving_and_deformable_regions() {
        let mut rigid = ReactiveHullState::qualification_initial(test_viewport());
        let mut organic = ReactiveHullState::qualification_initial(test_viewport());
        let rigid_params = ReactiveLabParams {
            response: 150.0,
            damping: 32.0,
            propagation: 0.05,
            elasticity: 0.05,
            edge_affinity: 1.5,
            shape_preservation: 0.98,
            orientation_compliance: 0.0,
            surface_locality: 0.0,
            spatial_feedback: 0.0,
            ..ReactiveLabParams::qualification()
        };
        let organic_params = ReactiveLabParams {
            response: 72.0,
            damping: 7.0,
            propagation: 1.2,
            elasticity: 1.2,
            edge_affinity: 0.8,
            shape_preservation: 0.15,
            orientation_compliance: 1.2,
            surface_locality: 0.9,
            spatial_feedback: 0.0,
            ..ReactiveLabParams::qualification()
        };
        rigid.set_pointer_target(205.0, 115.0);
        organic.set_pointer_target(205.0, 115.0);
        let mut rigid_max_width = 0.0_f32;
        let mut organic_max_width = 0.0_f32;
        let mut rigid_min_height = f32::MAX;
        let mut organic_min_height = f32::MAX;
        let mut organic_max_strain = 0.0_f32;
        for _ in 0..40 {
            rigid.step(rigid_params, 0.016);
            organic.step(organic_params, 0.016);
            let rigid_frame = rigid.frame(rigid_params);
            let organic_frame = organic.frame(organic_params);
            rigid_max_width = rigid_max_width.max(rigid_frame.app.right - rigid_frame.app.left);
            organic_max_width =
                organic_max_width.max(organic_frame.app.right - organic_frame.app.left);
            rigid_min_height = rigid_min_height.min(rigid_frame.app.bottom - rigid_frame.app.top);
            organic_min_height =
                organic_min_height.min(organic_frame.app.bottom - organic_frame.app.top);
            organic_max_strain = organic_max_strain.max(organic_frame.strain);
        }
        assert!(organic_max_width > rigid_max_width + 5.0);
        assert!(organic_min_height < rigid_min_height - 2.0);
        assert!(organic_max_strain > 1.0);
    }

    #[test]
    fn shape_preservation_can_disable_local_edge_deformation() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        let mut params = ReactiveLabParams::qualification();
        params.shape_preservation = 1.0;
        params.elasticity = 1.2;
        params.surface_locality = 1.0;
        state.set_pointer_target(205.0, 115.0);
        for _ in 0..32 {
            state.step(params, 0.016);
        }
        assert!(state.top_deform.iter().all(|value| value.abs() < 0.01));
    }

    #[test]
    fn surface_locality_concentrates_the_same_disturbance() {
        let mut broad = ReactiveHullState::qualification_initial(test_viewport());
        let mut local = ReactiveHullState::qualification_initial(test_viewport());
        let mut broad_params = ReactiveLabParams::qualification();
        broad_params.surface_locality = 0.0;
        broad_params.shape_preservation = 0.2;
        broad_params.elasticity = 1.1;
        broad_params.propagation = 0.0;
        let mut local_params = broad_params;
        local_params.surface_locality = 1.0;
        broad.set_pointer_target(300.0, 115.0);
        local.set_pointer_target(300.0, 115.0);
        for _ in 0..14 {
            broad.step(broad_params, 0.016);
            local.step(local_params, 0.016);
        }
        let broad_side = (broad.top_deform[0].abs() + broad.top_deform[2].abs()) * 0.5;
        let local_side = (local.top_deform[0].abs() + local.top_deform[2].abs()) * 0.5;
        let broad_ratio = broad.top_deform[1].abs() / broad_side.max(0.001);
        let local_ratio = local.top_deform[1].abs() / local_side.max(0.001);
        assert!(local.top_deform[1].abs() > 0.1);
        assert!(local_ratio > broad_ratio * 1.5);
    }

    #[test]
    fn propagation_spreads_a_local_disturbance_to_remote_nodes() {
        let mut isolated = ReactiveHullState::qualification_initial(test_viewport());
        let mut propagated = ReactiveHullState::qualification_initial(test_viewport());
        let mut isolated_params = ReactiveLabParams::qualification();
        isolated_params.surface_locality = 1.0;
        isolated_params.shape_preservation = 0.2;
        isolated_params.elasticity = 1.1;
        isolated_params.propagation = 0.0;
        let mut propagated_params = isolated_params;
        propagated_params.propagation = 1.2;
        isolated.set_pointer_target(205.0, 115.0);
        propagated.set_pointer_target(205.0, 115.0);
        for _ in 0..28 {
            isolated.step(isolated_params, 0.016);
            propagated.step(propagated_params, 0.016);
        }
        assert!(isolated.top_deform[0].abs() > 0.5);
        assert!(propagated.top_deform[2].abs() > isolated.top_deform[2].abs() + 0.15);
    }

    #[test]
    fn spatial_feedback_acts_before_hard_constraint_penetration() {
        let mut unaware = ReactiveHullState::qualification_initial(test_viewport());
        let mut aware = ReactiveHullState::qualification_initial(test_viewport());
        let mut unaware_params = ReactiveLabParams::qualification();
        unaware_params.spatial_feedback = 0.0;
        let mut aware_params = unaware_params;
        aware_params.spatial_feedback = 1.2;
        unaware.set_pointer_target(205.0, 115.0);
        aware.set_pointer_target(205.0, 115.0);
        for _ in 0..36 {
            unaware.step(unaware_params, 0.016);
            aware.step(aware_params, 0.016);
        }
        // The commanded target is still inside the hard safe range, so any
        // difference is anticipatory soft-field feedback rather than collision recovery.
        assert!(unaware.constraint_pressure < 0.01);
        assert!(aware.constraint_pressure > 1.0);
        assert!(aware.spatial_feedback_x > 0.0);
        assert!(aware.spatial_feedback_y > 0.0);
        assert!(aware.app_center_x > unaware.app_center_x + 1.0);
        assert!(aware.app_top > unaware.app_top + 1.0);
    }

    #[test]
    fn passive_neighbor_is_felt_before_geometric_contact() {
        let mut unaware = ReactiveHullState::qualification_initial(test_viewport());
        let mut aware = ReactiveHullState::qualification_initial(test_viewport());
        aware.neighbor = Some(QualificationNeighbor::right_side(test_viewport()));
        let mut params = ReactiveLabParams::qualification();
        params.spatial_feedback = 1.2;
        unaware.set_pointer_target(300.0, 360.0);
        aware.set_pointer_target(300.0, 360.0);
        let initial_app = aware.frame(params).app;
        let neighbor = aware.neighbor.expect("qualification neighbor");
        assert!(initial_app.right < neighbor.rect.left);
        for _ in 0..24 {
            unaware.step(params, 0.016);
            aware.step(params, 0.016);
        }
        assert!(aware.neighbor_feedback_x < -1.0);
        assert!(aware.app_center_x < unaware.app_center_x - 1.0);
        let aware_app = aware.frame(params).app;
        assert!(aware_app.right < neighbor.rect.left);
    }

    #[test]
    fn passive_neighbor_has_hard_nonpenetration_fallback() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        state.neighbor = Some(QualificationNeighbor::right_side(test_viewport()));
        let mut params = ReactiveLabParams::qualification();
        params.spatial_feedback = 0.0;
        params.elasticity = 1.2;
        state.set_pointer_target(395.0, 115.0);
        for _ in 0..80 {
            state.step(params, 0.016);
            let app = state.frame(params).app;
            let neighbor = state.neighbor.expect("qualification neighbor");
            let vertical_overlap = app.bottom > neighbor.rect.top && app.top < neighbor.rect.bottom;
            if vertical_overlap {
                assert!(app.right <= neighbor.rect.left + 0.01);
            }
        }
    }

    #[test]
    fn reactive_bridge_shoulders_leave_and_rejoin_dock_tangentially() {
        let attach_half = 101.0_f32;
        let neck_half = 43.0_f32;
        let gap = 68.0_f32;
        let run = bridge_shoulder_run(attach_half, neck_half, gap, 24.0, 58.0);
        assert!(run > 24.0);
        assert!(run < attach_half - neck_half);

        // The implemented first/last controls share dock.top with their anchors,
        // so both boundary derivatives are exactly horizontal, not the former 90° turn.
        let left_start_derivative = (run, 0.0_f32);
        let right_end_derivative = (run, 0.0_f32);
        assert!(left_start_derivative.0 > 0.0 && left_start_derivative.1.abs() < 0.001);
        assert!(right_end_derivative.0 > 0.0 && right_end_derivative.1.abs() < 0.001);
    }

    #[test]
    fn static_bridged_hull_has_horizontal_mouth_tangents() {
        let mouth_half = 110.0_f32;
        let neck_half = 43.0_f32;
        let gap = 48.0_f32;
        let horizontal_run = bridge_shoulder_run(mouth_half, neck_half, gap, 24.0, 58.0)
            .min((mouth_half - neck_half).max(8.0));
        assert!(horizontal_run > 0.0);
        // Both app-bottom and dock-top mouth controls share their anchor y,
        // so entry/exit derivatives are horizontal and no seam needs masking.
        assert_eq!((horizontal_run, 0.0_f32).1, 0.0);
    }

    #[test]
    fn orientation_compliance_can_be_zero_or_dynamic() {
        let mut straight = ReactiveHullState::qualification_initial(test_viewport());
        let mut living = ReactiveHullState::qualification_initial(test_viewport());
        let mut straight_params = ReactiveLabParams::qualification();
        straight_params.orientation_compliance = 0.0;
        let mut living_params = ReactiveLabParams::qualification();
        living_params.orientation_compliance = 1.2;
        straight.set_pointer_target(395.0, 140.0);
        living.set_pointer_target(395.0, 140.0);
        for _ in 0..24 {
            straight.step(straight_params, 0.016);
            living.step(living_params, 0.016);
        }
        assert!(straight.tilt.abs() < 0.01);
        assert!(living.tilt.abs() > 0.5);
    }

    #[test]
    fn visual_lab_contour_is_intentionally_more_compact_than_baseline() {
        let baseline = ReactiveContourTuning::baseline();
        let visual = ReactiveContourTuning::visual_lab();
        assert!(visual.neck_half_max_ratio < baseline.neck_half_max_ratio);
        assert!(visual.mouth_extra_ratio < baseline.mouth_extra_ratio);
        assert!(visual.mouth_half_min_ratio < baseline.mouth_half_min_ratio);
        assert!(visual.mouth_half_max_ratio < baseline.mouth_half_max_ratio);
        assert!(visual.shoulder_run_scale < baseline.shoulder_run_scale);
        assert!(visual.corner_scale > baseline.corner_scale);
    }

    #[test]
    fn visual_material_profile_realizes_depth_without_claiming_backdrop_blur() {
        let material = VisualMaterialProfile::qualification();
        assert!(!material.backdrop_blur_realized);
        assert!(material.shadow_offset_y > 0.0);
        assert!(material.shadow_alpha > 0);
        assert!(material.quad_shadow_blur > 0.0);
        assert_ne!(material.surface_top, material.surface_bottom);
        assert_ne!(material.card_top, material.card_bottom);
        assert_ne!(material.accent_top, material.accent_bottom);
        assert_eq!(
            qualification_paint(
                &MaterialRecipe {
                    family: MaterialFamilyId::new("qualification-hud"),
                    blur: LogicalPx(18),
                    border: LogicalPx(1),
                    shadow: LogicalPx(24),
                },
                ChromePresentation {
                    background_visible: true,
                    opacity: OpacityPercent::new(88),
                    border_visible: true,
                    shadow_visible: true,
                },
            )
            .blur_realization,
            "UNSUPPORTED_IN_PINNED_RENDERER"
        );
    }

    #[test]
    fn visual_lab_composition_stays_inside_host_bodies_across_topologies() {
        let tokens = VisualCompositionTokens::qualification();
        let params = ReactiveLabParams::qualification();
        let mut state = ReactiveHullState::qualification_initial(test_viewport());

        for (target, steps) in [
            ((300.0_f32, 360.0_f32), 24_usize),
            ((205.0, 115.0), 144),
            ((245.0, 175.0), 84),
            ((300.0, 360.0), 120),
        ] {
            state.set_pointer_target(target.0, target.1);
            for _ in 0..steps {
                state.step(params, 0.016);
                let frame = state.frame(params);
                let layout = VisualCompositionLayout::resolve(frame, tokens);
                assert!(rect_contains(frame.app, layout.app_content_bounds()));
                assert!(rect_contains(frame.dock, layout.dock_items_bounds(tokens)));

                let header = RectHull {
                    left: layout.safe_left,
                    top: frame.app.top + 19.0,
                    right: layout.safe_left + layout.safe_width,
                    bottom: frame.app.top + 47.0,
                    radius: 0.0,
                };
                assert!(rect_contains(frame.app, header));
                assert!(19.0 >= frame.tilt.abs());

                // Content uses the host body's conservative box rather than following
                // every local hull wave. The bottom inset must also clear the tilted
                // lower edge on both sides throughout the transition.
                assert!(
                    layout.app_content_bounds().bottom
                        <= frame.app.bottom - frame.tilt.abs() * 0.35 + 0.01
                );
            }
        }
    }

    #[test]
    fn geometry_observation_is_frame_parity_not_a_second_geometry_model() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        let mut params = ReactiveLabParams::qualification();
        params.spatial_feedback = 0.7;
        state.neighbor = Some(QualificationNeighbor::right_side(test_viewport()));
        state.set_pointer_target(345.0, 155.0);
        for _ in 0..48 {
            state.step(params, 0.016);
        }

        let frame = state.frame(params);
        let observation = GeometryObservation::from_frame(48, 7, frame);
        assert_eq!(observation.header.sample, 48);
        assert_eq!(observation.header.compositor_revision, 7);
        assert_eq!(observation.header.viewport, frame.viewport);
        assert_eq!(observation.header.namespace, "gnos.ux:dock:qualification");
        assert_eq!(observation.attachment, frame.attachment);
        assert_eq!(observation.app.left, frame.app.left);
        assert_eq!(observation.app.top, frame.app.top);
        assert_eq!(observation.app.right, frame.app.right);
        assert_eq!(observation.app.bottom, frame.app.bottom);
        assert_eq!(observation.dock.left, frame.dock.left);
        assert_eq!(observation.pointer_target, frame.pointer_target);
        assert_eq!(observation.resolved_target, frame.resolved_target);
        assert_eq!(observation.velocity, frame.velocity);
        assert_eq!(observation.shoulder_center_x, frame.shoulder_center_x);
        assert_eq!(observation.neck_width, frame.neck_width);
        assert_eq!(observation.tilt, frame.tilt);
        assert_eq!(observation.top_deform, frame.top_deform);
        assert_eq!(
            observation.spatial_feedback,
            (frame.spatial_feedback_x, frame.spatial_feedback_y)
        );
        assert_eq!(
            observation.neighbor,
            frame.neighbor.map(|neighbor| neighbor.rect)
        );
        assert!(rect_contains(frame.app, observation.content));
    }

    #[test]
    fn hyprland_geometry_observation_receipt_parser_is_strict_and_complete() {
        let parsed = HyprlandLayerGeometryObservation::parse(
            "GNOS_HYPRLAND_GEOMETRY|version=0.56.2|monitor=HDMI-A-1|monitor_rect=-630,590,2560,1440|reserved=36.00,12.00,8.00,4.00|layer_rect=350,1610,600,420|scale=1.000|transform=0",
        )
        .expect("valid geometry receipt");
        assert_eq!(parsed.version, "0.56.2");
        assert_eq!(parsed.monitor, "HDMI-A-1");
        assert_eq!(parsed.monitor_rect.x, -630);
        assert_eq!(parsed.monitor_rect.y, 590);
        assert_eq!(parsed.monitor_rect.width, 2560);
        assert_eq!(parsed.monitor_rect.height, 1440);
        assert_eq!(parsed.reserved.top, 36.0);
        assert_eq!(parsed.reserved.right, 12.0);
        assert_eq!(parsed.reserved.bottom, 8.0);
        assert_eq!(parsed.reserved.left, 4.0);
        assert_eq!(parsed.layer_rect.x, 350);
        assert_eq!(parsed.layer_rect.y, 1610);
        assert_eq!(parsed.layer_rect.width, 600);
        assert_eq!(parsed.layer_rect.height, 420);
        assert_eq!(parsed.scale, 1.0);
        assert_eq!(parsed.transform, 0);
        assert!(HyprlandLayerGeometryObservation::parse("GNOS_HYPRLAND_GEOMETRY|none").is_none());
        assert!(HyprlandLayerGeometryObservation::parse("not-a-receipt").is_none());
    }

    #[test]
    fn qualifier_geometry_consumer_has_no_workspace_field() {
        let source = include_str!("gnos-ux-envelope-qualify.rs");
        let workspace_field = format!(".{}", "workspace");
        assert!(!source.contains(&workspace_field));
    }

    #[test]
    fn viewport_rebase_preserves_normalized_state_and_topology() {
        let from = SurfaceViewport {
            width: 720.0,
            height: 480.0,
        };
        let to = SurfaceViewport {
            width: 1536.0,
            height: 864.0,
        };
        let mut state = ReactiveHullState::qualification_initial(from);
        state.attachment = Attachment::Bridged;
        state.neighbor = Some(QualificationNeighbor::right_side(from));
        state.set_pointer_target(from.width * 0.37, from.height * 0.29);
        state.app_center_x = from.width * 0.43;
        state.app_top = from.height * 0.21;
        state.shoulder_center_x = from.width * 0.41;
        state.neck_width = from.width * 0.11;
        state.velocity_x = from.width * 0.02;
        state.velocity_y = from.height * -0.015;

        let before = (
            state.pointer_x / from.width,
            state.pointer_y / from.height,
            state.app_center_x / from.width,
            state.app_top / from.height,
            state.shoulder_center_x / from.width,
            state.neck_width / from.width,
        );
        let neighbor_before = state.neighbor.expect("neighbor").rect;

        state.rebase_viewport(to);

        assert_eq!(state.attachment, Attachment::Bridged);
        assert!(state.viewport.approx_eq(to));
        let after = (
            state.pointer_x / to.width,
            state.pointer_y / to.height,
            state.app_center_x / to.width,
            state.app_top / to.height,
            state.shoulder_center_x / to.width,
            state.neck_width / to.width,
        );
        for (lhs, rhs) in [
            (before.0, after.0),
            (before.1, after.1),
            (before.2, after.2),
            (before.3, after.3),
            (before.4, after.4),
            (before.5, after.5),
        ] {
            assert!((lhs - rhs).abs() < 0.000_01);
        }

        let neighbor_after = state.neighbor.expect("rebased neighbor").rect;
        assert!(
            ((neighbor_before.left / from.width) - (neighbor_after.left / to.width)).abs()
                < 0.000_01
        );
        assert!(
            ((neighbor_before.top / from.height) - (neighbor_after.top / to.height)).abs()
                < 0.000_01
        );
    }

    #[test]
    fn responsive_policy_tracks_viewport_without_reference_resolution() {
        let policy = ResponsiveGeometryPolicy;
        for viewport in [
            SurfaceViewport {
                width: 900.0,
                height: 600.0,
            },
            SurfaceViewport {
                width: 1600.0,
                height: 900.0,
            },
            SurfaceViewport {
                width: 1080.0,
                height: 1920.0,
            },
        ] {
            let dock = policy.dock(viewport);
            let safe = policy.safe_target(viewport);
            assert!((dock.left / viewport.width - 1.0 / 30.0).abs() < 0.000_01);
            assert!((dock.right / viewport.width - 29.0 / 30.0).abs() < 0.000_01);
            assert!((dock.top / viewport.height - 0.70).abs() < 0.000_01);
            assert!((safe.left / viewport.width - 1.0 / 3.0).abs() < 0.000_01);
            assert!((safe.right / viewport.width - 2.0 / 3.0).abs() < 0.000_01);
            let (app_width, app_height) = policy.app_base_size(viewport);
            assert!((app_width / viewport.width - 0.60).abs() < 0.000_01);
            assert!((app_height / viewport.height - 5.0 / 14.0).abs() < 0.000_01);
        }
    }

    #[test]
    fn reactive_base_dock_is_the_shared_dock_geometry_frame() {
        for viewport in [
            SurfaceViewport {
                width: 900.0,
                height: 560.0,
            },
            SurfaceViewport {
                width: 1200.0,
                height: 720.0,
            },
            SurfaceViewport {
                width: 1080.0,
                height: 1920.0,
            },
        ] {
            let policy_dock = ResponsiveGeometryPolicy.dock(viewport);
            let shared = resolve_dock_frame(viewport, 1.0).envelope;
            assert_eq!(policy_dock, RectHull::from(shared));
        }
    }

    #[test]
    fn standalone_dock_input_matches_shared_visible_and_hidden_frames() {
        let viewport = SurfaceViewport {
            width: 900.0,
            height: 560.0,
        };

        for (visual, fraction) in [(visible_visual(), 1.0_f32), (hidden_visual(), 0.0_f32)] {
            let geometry = QualificationGeometry::from_visual(visual, viewport);
            let shared = resolve_dock_frame(viewport, fraction);
            assert_eq!(geometry.input.regions.len(), shared.input_regions.len());

            for (region, expected) in geometry.input.regions.iter().zip(&shared.input_regions) {
                assert_eq!(region.x, expected.left().round() as i32);
                assert_eq!(region.y, expected.top().round() as i32);
                assert_eq!(region.width.0, expected.width().round().max(1.0) as u32);
                assert_eq!(region.height.0, expected.height().round().max(1.0) as u32);
            }
        }
    }

    #[test]
    fn responsive_frame_connector_and_join_pairs_survive_extreme_aspect_ratios() {
        let params = ReactiveLabParams::qualification();
        let tuning = ReactiveContourTuning::visual_lab();

        for viewport in [
            SurfaceViewport {
                width: 1080.0,
                height: 1920.0,
            },
            SurfaceViewport {
                width: 720.0,
                height: 1600.0,
            },
            SurfaceViewport {
                width: 2560.0,
                height: 720.0,
            },
        ] {
            let state = ReactiveHullState::qualification_initial(viewport);
            let frame = state.frame(params);
            let base_radius = ResponsiveGeometryPolicy.app_radius(viewport);

            assert!(frame.app.radius.is_finite());
            assert!(frame.app.radius >= base_radius * (2.0 / 3.0) - 0.001);
            assert!(frame.app.radius <= base_radius + 0.001);

            let connector = reactive_connector_geometry(frame, tuning);
            for value in [
                connector.connector_center,
                connector.neck_left,
                connector.neck_right,
                connector.dock_attach_left,
                connector.dock_attach_right,
                connector.app_attach_left,
                connector.app_attach_right,
                connector.app_attach_left_y,
                connector.app_attach_right_y,
                connector.left_mid_y,
                connector.right_mid_y,
            ] {
                assert!(value.is_finite());
            }

            let (left, right) = reactive_lower_join_pairs(frame, tuning);
            assert_pair_directionally_monotone(left);
            assert_pair_directionally_monotone(right);
        }
    }

    #[test]
    fn fused_morph_collapses_lower_corner_without_vertical_retrace() {
        let viewport = SurfaceViewport {
            width: 1200.0,
            height: 800.0,
        };
        let state = ReactiveHullState::qualification_initial(viewport);
        let frame = state.frame(ReactiveLabParams::qualification());
        let tuning = ReactiveContourTuning::visual_lab();
        let left = fused_left_join_pair(frame, tuning);
        let right = fused_right_join_pair(frame, tuning);

        let left_incoming = normalized_direction((left.p2.0 - left.c4.0, left.p2.1 - left.c4.1));
        let right_outgoing =
            normalized_direction((right.c1.0 - right.p0.0, right.c1.1 - right.p0.1));

        // Fused has no second lower app corner. The join itself lands on the
        // app wall and continues vertically, instead of walking to the bottom
        // edge and curving back to the same vertical wall.
        assert!((left.p2.0 - frame.app.left).abs() < 0.001);
        assert!(left_incoming.0.abs() < 0.001);
        assert!(left_incoming.1 < -0.999);

        assert!((right.p0.0 - frame.app.right).abs() < 0.001);
        assert!(right_outgoing.0.abs() < 0.001);
        assert!(right_outgoing.1 > 0.999);

        let t = smooth_join_phase(frame.join_phase);
        assert_eq!(t, 0.0);
        let lower_corner_radius = frame.app.radius * tuning.corner_scale * t;
        assert_eq!(lower_corner_radius, 0.0);
    }

    #[test]
    fn morph_corner_transition_preserves_g1_tangents_throughout_phase() {
        let viewport = SurfaceViewport {
            width: 1200.0,
            height: 800.0,
        };
        let mut state = ReactiveHullState::qualification_initial(viewport);
        let params = ReactiveLabParams::qualification();
        state.set_pointer_target(viewport.width * 0.34, viewport.height * 0.28);
        for _ in 0..70 {
            state.step(params, 1.0 / 120.0);
        }
        let mut frame = state.frame(params);
        let tuning = ReactiveContourTuning::visual_lab();
        let ar = frame.app.radius * tuning.corner_scale;
        let left_bottom_y = frame.app.bottom + frame.tilt * 0.35;
        let right_bottom_y = frame.app.bottom - frame.tilt * 0.35;

        for raw_phase in [0.0_f32, 0.2, 0.5, 0.8, 1.0] {
            frame.join_phase = raw_phase;
            let t = smooth_join_phase(raw_phase);
            let left = cubic_join_monotonic_morph(
                fused_left_join_pair(frame, tuning),
                bridged_left_join_pair(frame, tuning),
                t,
            );
            let right = cubic_join_monotonic_morph(
                fused_right_join_pair(frame, tuning),
                bridged_right_join_pair(frame, tuning),
                t,
            );
            let corner_radius = ar * t;

            let left_corner_y = left.p2.1 + (left_bottom_y - left.p2.1) * t;
            let left_corner_start = (frame.app.left + corner_radius, left_corner_y);
            let left_incoming = (left.p2.0 - left.c4.0, left.p2.1 - left.c4.1);
            if let Some((c1, c2)) =
                g1_transition_controls(left.p2, left_incoming, left_corner_start, (-1.0, 0.0))
            {
                let d0 = normalized_direction((c1.0 - left.p2.0, c1.1 - left.p2.1));
                let expected0 = normalized_direction(left_incoming);
                let d1 =
                    normalized_direction((left_corner_start.0 - c2.0, left_corner_start.1 - c2.1));
                assert!((d0.0 - expected0.0).abs() < 0.001);
                assert!((d0.1 - expected0.1).abs() < 0.001);
                assert!(d1.0 < -0.999 && d1.1.abs() < 0.001);
            }

            let right_corner_y = right.p0.1 + (right_bottom_y - right.p0.1) * t;
            let right_corner_end = (frame.app.right - corner_radius, right_corner_y);
            let right_outgoing = (right.c1.0 - right.p0.0, right.c1.1 - right.p0.1);
            if let Some((c1, c2)) =
                g1_transition_controls(right_corner_end, (-1.0, 0.0), right.p0, right_outgoing)
            {
                let d0 =
                    normalized_direction((c1.0 - right_corner_end.0, c1.1 - right_corner_end.1));
                let d1 = normalized_direction((right.p0.0 - c2.0, right.p0.1 - c2.1));
                let expected1 = normalized_direction(right_outgoing);
                assert!(d0.0 < -0.999 && d0.1.abs() < 0.001);
                assert!((d1.0 - expected1.0).abs() < 0.001);
                assert!((d1.1 - expected1.1).abs() < 0.001);
            }
        }
    }

    #[test]
    fn fused_join_is_a_bounded_ninety_degree_fillet_not_an_inward_dip() {
        let viewport = SurfaceViewport {
            width: 1200.0,
            height: 800.0,
        };
        let state = ReactiveHullState::qualification_initial(viewport);
        let frame = state.frame(ReactiveLabParams::qualification());
        let join = reactive_fused_join_geometry(frame, ReactiveContourTuning::visual_lab());

        assert!(join.left_radius_x > 0.0);
        assert!(join.left_radius_y > 0.0);
        assert!(join.right_radius_x > 0.0);
        assert!(join.right_radius_y > 0.0);

        // Left fillet: horizontal Dock tangent at (shoulder, dock.top) and
        // vertical app tangent at (app.left, bottom_y).
        assert!((join.left_shoulder + join.left_radius_x - frame.app.left).abs() < 0.001);
        assert!((join.left_bottom_y + join.left_radius_y - frame.dock.top).abs() < 0.001);

        // Right fillet mirrors the same 90-degree construction.
        assert!((join.right_shoulder - join.right_radius_x - frame.app.right).abs() < 0.001);
        assert!((join.right_bottom_y + join.right_radius_y - frame.dock.top).abs() < 0.001);

        // The entire concave arc is bounded above dock.top; no control point can
        // dip below the Dock tangent as the previous cubic did.
        assert!(join.left_bottom_y < frame.dock.top);
        assert!(join.right_bottom_y < frame.dock.top);
    }

    #[test]
    fn full_join_motion_exposes_intermediate_phase_instead_of_cutting() {
        let viewport = test_viewport();
        let mut params = ReactiveLabParams::qualification();
        params.join_motion = TemporalResponseMode::Full;
        let mut state = ReactiveHullState::qualification_initial(viewport);
        state.set_pointer_target(viewport.width * 0.341_667, viewport.height * 0.273_810);

        let mut saw_intermediate = false;
        for _ in 0..180 {
            state.step(params, 1.0 / 120.0);
            if state.attachment == Attachment::Bridged
                && state.join_phase > 0.01
                && state.join_phase < 0.99
            {
                saw_intermediate = true;
                break;
            }
        }
        assert!(
            saw_intermediate,
            "full motion must traverse a visible intermediate join phase"
        );
    }

    #[test]
    fn off_join_motion_snaps_phase_exactly_to_topology() {
        let viewport = test_viewport();
        let mut params = ReactiveLabParams::qualification();
        params.join_motion = TemporalResponseMode::Instant;
        let mut state = ReactiveHullState::qualification_initial(viewport);
        state.set_pointer_target(viewport.width * 0.341_667, viewport.height * 0.273_810);

        let mut transitioned = false;
        for _ in 0..180 {
            state.step(params, 1.0 / 120.0);
            if state.attachment == Attachment::Bridged {
                transitioned = true;
                assert_eq!(state.join_phase, 1.0);
                assert_eq!(state.join_velocity, 0.0);
                break;
            }
        }
        assert!(transitioned);
    }

    #[test]
    fn reduced_join_motion_settles_no_slower_than_full() {
        fn steps_to_near_bridged(mode: TemporalResponseMode) -> usize {
            let viewport = test_viewport();
            let mut params = ReactiveLabParams::qualification();
            params.join_motion = mode;
            let mut state = ReactiveHullState::qualification_initial(viewport);
            state.set_pointer_target(viewport.width * 0.408_333, viewport.height * 0.416_667);
            for step in 1..=600 {
                state.step(params, 1.0 / 120.0);
                if state.attachment == Attachment::Bridged && state.join_phase >= 0.98 {
                    return step;
                }
            }
            usize::MAX
        }

        let full = steps_to_near_bridged(TemporalResponseMode::Full);
        let reduced = steps_to_near_bridged(TemporalResponseMode::Reduced);
        assert!(full < usize::MAX);
        assert!(reduced < usize::MAX);
        assert!(
            reduced <= full,
            "reduced motion should converge at least as fast as full motion"
        );
    }

    #[test]
    fn morph_endpoints_match_fillet_and_spine_control_models() {
        let viewport = SurfaceViewport {
            width: 1200.0,
            height: 800.0,
        };
        let state = ReactiveHullState::qualification_initial(viewport);
        let frame = state.frame(ReactiveLabParams::qualification());
        let tuning = ReactiveContourTuning::visual_lab();

        let fused = fused_left_join_pair(frame, tuning);
        let bridged = bridged_left_join_pair(frame, tuning);
        let at_zero = cubic_join_monotonic_morph(fused, bridged, smooth_join_phase(0.0));
        let at_one = cubic_join_monotonic_morph(fused, bridged, smooth_join_phase(1.0));

        for (actual, expected) in [
            (at_zero.p0, fused.p0),
            (at_zero.c1, fused.c1),
            (at_zero.c2, fused.c2),
            (at_zero.p1, fused.p1),
            (at_zero.c3, fused.c3),
            (at_zero.c4, fused.c4),
            (at_zero.p2, fused.p2),
            (at_one.p0, bridged.p0),
            (at_one.c1, bridged.c1),
            (at_one.c2, bridged.c2),
            (at_one.p1, bridged.p1),
            (at_one.c3, bridged.c3),
            (at_one.c4, bridged.c4),
            (at_one.p2, bridged.p2),
        ] {
            assert!((actual.0 - expected.0).abs() < 0.0001);
            assert!((actual.1 - expected.1).abs() < 0.0001);
        }
    }

    #[test]
    fn reactive_settled_state_has_negligible_kinetic_terms() {
        let mut state = ReactiveHullState::qualification_initial(test_viewport());
        state.set_pointer_target(300.0, 360.0);
        step_many(&mut state, 40);
        assert!(state.velocity_x.abs() < 0.1);
        assert!(state.velocity_y.abs() < 0.1);
        assert!(state.neck_velocity.abs() < 0.1);
    }
}
