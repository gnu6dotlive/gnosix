//! Backend-agnostic GNOS.UX surface grammar.
//!
//! The grammar is intentionally below GPUI so runtime paint/input/debug paths
//! can consume the same resolved geometry instead of reimplementing it.

pub mod atoms;
pub mod connective_hull;
pub mod connectors;
pub mod curves;
pub mod dynamics;
pub mod frames;
pub mod influences;
pub mod molecules;
pub mod placement;
pub mod placement_transition;
pub mod reactive_envelope;
pub mod recipes;
pub mod shared_envelope;
mod transposition;

pub use atoms::{
    GeometryError, GeometryRect, LengthBasis, ResponsiveLength, ResponsiveRadiusToken,
    SurfaceViewport, UnitInterval,
};
pub use connective_hull::{ConnectiveHullGeometry, ConnectiveHullInput, HullCommand};
pub use connectors::{
    BridgeShoulderRecipe, CONNECTOR_GRAMMAR_ID, CONNECTOR_GRAMMAR_REVISION, ConnectorSpan,
};
pub use curves::{
    CURVE_GRAMMAR_ID, CURVE_GRAMMAR_REVISION, CubicBezier, CubicJoinPair, CurveInvariantViolation,
    CurveValidation, Point2, Vector2, admissible_direction, g1_transition_controls,
    monotonic_join_morph, shared_mid_direction,
};
pub use dynamics::{
    DYNAMICS_GRAMMAR_ID, DYNAMICS_GRAMMAR_REVISION, DynamicsDiagnostics, PhaseChannel,
    PhaseResponseProfile, PhaseStep, SecondOrderChannel, SecondOrderSpec, SecondOrderStep,
    SettlePolicy, TemporalResponseMode, simulate_step_response, step_second_order_pair,
};
pub use frames::{
    BarGeometryFrame, DockGeometryFrame, FrameInvariantViolation, FrameValidation, RecipeIdentity,
};
pub use influences::{
    INFLUENCE_GRAMMAR_ID, INFLUENCE_GRAMMAR_REVISION, InfluenceSample, RectInfluenceField,
};
pub use placement::{
    AlongEdgeAlignment, BarOrientation, BarPlacement, BarReservationPolicy, BarSensingPolicy,
    EdgeBasis, ResolvedBarPlacement,
};
pub use placement_transition::{
    BarPlacementTransition, BarPlacementTransitionPhase, BarTransitionArm, BarTransitionCorner,
    ResolvedBarTransitionFrame,
};
pub use recipes::{BarGeometryRecipe, DockGeometryRecipe, RectRecipe, SurfaceGeometryRecipe};
