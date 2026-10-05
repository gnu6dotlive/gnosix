#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! Internal GNOS.UX SDK façade.
//!
//! This crate consolidates the currently proven surface model, GPUI runtime,
//! geometry/debug instrumentation and development scenarios. It is not yet a
//! stable public ABI.

pub mod grammar;
pub mod lab;
pub mod surface;

pub mod model {
    pub use gnos_ux_surface::SurfaceRole;
    pub use gnos_ux_surface::envelope::*;
}

pub mod geometry {
    pub use gnos_ux_gpui::geometry_lab::{
        AtlasCoverageLevel, GeometryDebugView, GeometryFrameHeader, surface_atlas_coverage,
        surface_atlas_ledger,
    };
    pub use gnos_ux_surface::grammar::{
        BarGeometryFrame, BarGeometryRecipe, BridgeShoulderRecipe, ConnectorSpan, CubicBezier,
        CubicJoinPair, CurveInvariantViolation, CurveValidation, DockGeometryFrame,
        DockGeometryRecipe, GeometryError, GeometryRect, LengthBasis, Point2, RectRecipe,
        ResponsiveLength, SurfaceGeometryRecipe, SurfaceViewport, UnitInterval, Vector2,
        g1_transition_controls, monotonic_join_morph,
    };
}

pub mod dynamics {
    pub use gnos_ux_surface::grammar::{
        DYNAMICS_GRAMMAR_ID, DYNAMICS_GRAMMAR_REVISION, DynamicsDiagnostics, PhaseChannel,
        PhaseResponseProfile, PhaseStep, SecondOrderChannel, SecondOrderSpec, SecondOrderStep,
        SettlePolicy, TemporalResponseMode, simulate_step_response, step_second_order_pair,
    };
}

pub mod influences {
    pub use gnos_ux_surface::grammar::{
        INFLUENCE_GRAMMAR_ID, INFLUENCE_GRAMMAR_REVISION, InfluenceSample, RectInfluenceField,
    };
}

pub mod scene {
    pub use gnos_ux_gpui::scene_runtime::{SCENE_IDENTITY, SCENE_NAMESPACE};
    pub use gnos_ux_surface::scene::*;
}

pub mod runtime {
    pub use gnos_ux_gpui::hyprland_live::{HyprlandLayerGeometryObservation, query_layer_geometry};
    pub use gnos_ux_gpui::{ProductSurfacePlan, run_surface, surface_plan, try_surface_plan};
}
