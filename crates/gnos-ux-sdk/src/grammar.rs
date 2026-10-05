//! SDK-facing catalogue and re-exports for the foundational GNOS.UX surface grammar.

pub use gnos_ux_surface::grammar::*;
use gnos_ux_surface::scene::{SCENE_GRAMMAR_ID, SCENE_GRAMMAR_REVISION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrammarKind {
    Atom,
    Molecule,
    Recipe,
    Frame,
}

impl GrammarKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Atom => "ATOM",
            Self::Molecule => "MOLECULE",
            Self::Recipe => "RECIPE",
            Self::Frame => "FRAME",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrammarEntry {
    pub kind: GrammarKind,
    pub id: &'static str,
    pub purpose: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeSdkEntry {
    pub id: &'static str,
    pub revision: u32,
    pub target: &'static str,
    pub state: &'static str,
    pub frame: &'static str,
    pub molecules: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrammarSubsystemEntry {
    pub id: &'static str,
    pub revision: u32,
    pub purpose: &'static str,
}

pub const SUBGRAMMARS: &[GrammarSubsystemEntry] = &[
    GrammarSubsystemEntry {
        id: CURVE_GRAMMAR_ID,
        revision: CURVE_GRAMMAR_REVISION,
        purpose: "Cubic curve, G1 continuity and monotonic connector geometry.",
    },
    GrammarSubsystemEntry {
        id: CONNECTOR_GRAMMAR_ID,
        revision: CONNECTOR_GRAMMAR_REVISION,
        purpose: "Connector span and bridge-shoulder geometry.",
    },
    GrammarSubsystemEntry {
        id: DYNAMICS_GRAMMAR_ID,
        revision: DYNAMICS_GRAMMAR_REVISION,
        purpose: "Second-order temporal response, normalized phases and settling.",
    },
    GrammarSubsystemEntry {
        id: INFLUENCE_GRAMMAR_ID,
        revision: INFLUENCE_GRAMMAR_REVISION,
        purpose: "Soft spatial influence fields and pressure sampling.",
    },
    GrammarSubsystemEntry {
        id: SCENE_GRAMMAR_ID,
        revision: SCENE_GRAMMAR_REVISION,
        purpose: "Persistent per-monitor visual scene kernel and unified SceneFrame resolution.",
    },
];

pub const ATOMS: &[GrammarEntry] = &[
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "SurfaceViewport",
        purpose: "Validated live logical viewport dimensions.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "UnitInterval",
        purpose: "Finite normalized scalar constrained to 0..=1.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "ResponsiveLength",
        purpose: "Length resolved from viewport axes with optional bounds.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "LengthBasis",
        purpose: "Semantic basis for responsive length resolution.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "GeometryRect",
        purpose: "Finite non-inverted rectangle with bounded corner radius.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "Point2",
        purpose: "Finite 2D point used by curved surface geometry.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "Vector2",
        purpose: "Finite 2D vector with normalization, dot and scale operations.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "SecondOrderSpec",
        purpose: "Validated response/damping parameters for scalar temporal dynamics.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "SettlePolicy",
        purpose: "Explicit position/velocity convergence thresholds and snap semantics.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "TemporalResponseMode",
        purpose: "Full, Reduced or Instant response semantics without product topology meaning.",
    },
    GrammarEntry {
        kind: GrammarKind::Atom,
        id: "InfluenceSample",
        purpose: "Soft advisory Vector2 plus non-negative geometric pressure.",
    },
];

pub const MOLECULES: &[GrammarEntry] = &[
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "visible_from_bottom",
        purpose: "Bottom-preserving visibility crop with radius projection.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "reveal_strip_below",
        purpose: "Responsive reveal strip derived from an Envelope and viewport.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "visible_plus_reveal_input",
        purpose: "Shape-aware input composition for visible + reveal geometry.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "envelope_only_input",
        purpose: "Single-envelope input composition.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "RectRecipe",
        purpose: "Composes responsive length atoms into a validated GeometryRect.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "CubicBezier",
        purpose: "Validated cubic curve evaluation, derivative, split and subsegment primitive.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "CubicJoinPair",
        purpose: "Two connected cubic segments with continuity and monotonicity validation.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "g1_transition_controls",
        purpose: "Constructs endpoint controls from explicit incoming/outgoing tangent directions.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "monotonic_join_morph",
        purpose: "Morphs join pairs while constraining tangents against negative extrusion.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "ConnectorSpan",
        purpose: "Validated symmetric connector neck/attachment span.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "BridgeShoulderRecipe",
        purpose: "Reusable connector shoulder-run resolver from span/gap constraints.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "CurveValidation",
        purpose: "Shared G1, directional-backtracking and chord-slab invariant report.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "SecondOrderChannel",
        purpose: "Finite scalar position/velocity channel using the shared second-order integrator.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "PhaseChannel",
        purpose: "Normalized 0..=1 temporal channel with Full/Reduced/Instant response modes.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "DynamicsDiagnostics",
        purpose: "Step-response diagnostics for overshoot, settling and finite-state qualification.",
    },
    GrammarEntry {
        kind: GrammarKind::Molecule,
        id: "RectInfluenceField",
        purpose: "Soft rectangular spatial field producing advisory vector plus geometric pressure.",
    },
];

pub const FRAMES: &[GrammarEntry] = &[
    GrammarEntry {
        kind: GrammarKind::Frame,
        id: "DockGeometryFrame",
        purpose: "Authoritative Dock paint/input/debug/telemetry geometry payload.",
    },
    GrammarEntry {
        kind: GrammarKind::Frame,
        id: "BarGeometryFrame",
        purpose: "Authoritative Bar paint/input/debug/telemetry geometry payload.",
    },
];

pub const RECIPES: &[RecipeSdkEntry] = &[
    RecipeSdkEntry {
        id: DockGeometryRecipe::ID,
        revision: DockGeometryRecipe::REVISION,
        target: "dock",
        state: "UnitInterval visible_fraction",
        frame: "DockGeometryFrame",
        molecules: &[
            "visible_from_bottom",
            "reveal_strip_below",
            "visible_plus_reveal_input",
        ],
    },
    RecipeSdkEntry {
        id: BarGeometryRecipe::ID,
        revision: BarGeometryRecipe::REVISION,
        target: "bar",
        state: "VisualState",
        frame: "BarGeometryFrame",
        molecules: &["envelope_only_input"],
    },
];

pub fn grammar_catalog() -> impl Iterator<Item = GrammarEntry> {
    ATOMS.iter().chain(MOLECULES).chain(FRAMES).copied()
}

pub fn recipe_catalog() -> &'static [RecipeSdkEntry] {
    RECIPES
}

pub fn subgrammar_catalog() -> &'static [GrammarSubsystemEntry] {
    SUBGRAMMARS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_catalog_has_foundational_layers() {
        assert!(ATOMS.len() >= 11);
        assert!(MOLECULES.len() >= 16);
        assert_eq!(FRAMES.len(), 2);
        assert_eq!(RECIPES.len(), 2);
        assert_eq!(SUBGRAMMARS.len(), 5);
    }

    #[test]
    fn recipe_catalog_tracks_runtime_recipe_identity() {
        assert_eq!(RECIPES[0].id, DockGeometryRecipe::ID);
        assert_eq!(RECIPES[0].revision, DockGeometryRecipe::REVISION);
        assert_eq!(RECIPES[1].id, BarGeometryRecipe::ID);
        assert_eq!(RECIPES[1].revision, BarGeometryRecipe::REVISION);
    }

    #[test]
    fn curve_and_connector_primitives_are_discoverable_from_sdk() {
        assert!(ATOMS.iter().any(|entry| entry.id == "Point2"));
        assert!(ATOMS.iter().any(|entry| entry.id == "Vector2"));
        assert!(MOLECULES.iter().any(|entry| entry.id == "CubicBezier"));
        assert!(MOLECULES.iter().any(|entry| entry.id == "CubicJoinPair"));
        assert!(MOLECULES.iter().any(|entry| entry.id == "ConnectorSpan"));
        assert!(
            MOLECULES
                .iter()
                .any(|entry| entry.id == "BridgeShoulderRecipe")
        );

        let curve = CubicBezier::new(
            Point2::new(0.0, 0.0),
            Point2::new(-10.0, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(30.0, 0.0),
        );
        assert!(!curve.is_directionally_monotone(24, 0.001));
    }

    #[test]
    fn dynamics_and_influence_primitives_are_discoverable_from_sdk() {
        assert!(ATOMS.iter().any(|entry| entry.id == "SecondOrderSpec"));
        assert!(ATOMS.iter().any(|entry| entry.id == "SettlePolicy"));
        assert!(ATOMS.iter().any(|entry| entry.id == "TemporalResponseMode"));
        assert!(ATOMS.iter().any(|entry| entry.id == "InfluenceSample"));
        assert!(
            MOLECULES
                .iter()
                .any(|entry| entry.id == "SecondOrderChannel")
        );
        assert!(MOLECULES.iter().any(|entry| entry.id == "PhaseChannel"));
        assert!(
            MOLECULES
                .iter()
                .any(|entry| entry.id == "RectInfluenceField")
        );
        assert!(
            SUBGRAMMARS
                .iter()
                .any(|entry| entry.id == DYNAMICS_GRAMMAR_ID)
        );
        assert!(
            SUBGRAMMARS
                .iter()
                .any(|entry| entry.id == INFLUENCE_GRAMMAR_ID)
        );
    }

    #[test]
    fn recipes_resolve_valid_frames_from_public_sdk_types() {
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");
        let dock = DockGeometryRecipe::atlas_baseline()
            .resolve(viewport, UnitInterval::ONE)
            .expect("dock");
        assert!(dock.validate().is_valid());
    }
}
