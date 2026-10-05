//! Scenario catalogue and launch planning for the GNOS.UX development lab.
//!
//! Scenarios intentionally orchestrate the already-proven qualification binaries.
//! They do not duplicate solver, renderer, geometry or input logic.

use std::{
    env,
    path::{Path, PathBuf},
};

use gnos_ux_gpui::geometry_lab::{AtlasCoverageLevel, surface_atlas_coverage};
use gnos_ux_surface::SurfaceRole;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LabTarget {
    Scene,
    Envelope,
    Bar,
    Dock,
}

impl LabTarget {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "scene" => Some(Self::Scene),
            "envelope" => Some(Self::Envelope),
            "bar" => Some(Self::Bar),
            "dock" => Some(Self::Dock),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Envelope => "envelope",
            Self::Bar => "bar",
            Self::Dock => "dock",
        }
    }

    pub const fn qualifier_binary(self) -> &'static str {
        match self {
            Self::Scene => "gnos-ux-scene",
            Self::Envelope => "gnos-ux-envelope-qualify",
            Self::Bar => "gnos-ux-bar-envelope-qualify",
            Self::Dock => "gnos-ux-dock-envelope-qualify",
        }
    }

    pub const fn namespace(self) -> &'static str {
        match self {
            Self::Scene => "gnos.ux:scene",
            Self::Envelope => "gnos.ux:dock:qualification",
            Self::Bar => "gnos.ux:bar:qualification",
            Self::Dock => "gnos.ux:dock:atlas-qualification",
        }
    }

    pub fn coverage(self) -> AtlasCoverageLevel {
        match self {
            Self::Scene => AtlasCoverageLevel::ProductRuntimeOnly,
            Self::Envelope => AtlasCoverageLevel::AtlasRich,
            Self::Bar => surface_atlas_coverage(SurfaceRole::Bar),
            Self::Dock => surface_atlas_coverage(SurfaceRole::Dock),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScenarioCapability {
    GeometryDebug,
    StructuredTelemetry,
    CompositorResize,
    ShapeAwareInput,
    ReactiveTopology,
    ReadOnlyHyprlandContext,
    AutoHide,
    GeometryMorph,
    TemporalDynamics,
    SoftSpatialInfluence,
    UnifiedScene,
    PersistentHost,
    SceneNegotiation,
    ReservationIntent,
    CompositorProjection,
    HyprlandRuntimeMutation,
}

impl ScenarioCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GeometryDebug => "geometry-debug",
            Self::StructuredTelemetry => "structured-telemetry",
            Self::CompositorResize => "compositor-resize",
            Self::ShapeAwareInput => "shape-aware-input",
            Self::ReactiveTopology => "reactive-topology",
            Self::ReadOnlyHyprlandContext => "hyprland-read-only-context",
            Self::AutoHide => "autohide",
            Self::GeometryMorph => "geometry-morph",
            Self::TemporalDynamics => "temporal-dynamics",
            Self::SoftSpatialInfluence => "soft-spatial-influence",
            Self::UnifiedScene => "unified-scene",
            Self::PersistentHost => "persistent-host",
            Self::SceneNegotiation => "scene-negotiation",
            Self::ReservationIntent => "reservation-intent",
            Self::CompositorProjection => "compositor-projection",
            Self::HyprlandRuntimeMutation => "hyprland-runtime-mutation",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenarioDescriptor {
    pub id: &'static str,
    pub target: LabTarget,
    pub name: &'static str,
    pub purpose: &'static str,
    pub args: &'static [&'static str],
    pub capabilities: &'static [ScenarioCapability],
    pub expected_markers: &'static [&'static str],
    pub completion_marker: Option<&'static str>,
    pub nested_required: bool,
}

impl ScenarioDescriptor {
    pub fn coverage(self) -> AtlasCoverageLevel {
        self.target.coverage()
    }

    pub const fn binary(self) -> &'static str {
        self.target.qualifier_binary()
    }

    pub const fn namespace(self) -> &'static str {
        self.target.namespace()
    }
}

const GEOMETRY_BASE: &[&str] = &["--geometry-lab", "--geometry-view=all"];

const SCENE_DESKTOP_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::ShapeAwareInput,
    ScenarioCapability::ReactiveTopology,
    ScenarioCapability::TemporalDynamics,
    ScenarioCapability::SoftSpatialInfluence,
    ScenarioCapability::UnifiedScene,
    ScenarioCapability::PersistentHost,
    ScenarioCapability::SceneNegotiation,
    ScenarioCapability::ReservationIntent,
];

const SCENE_PROJECTION_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::UnifiedScene,
    ScenarioCapability::PersistentHost,
    ScenarioCapability::ReservationIntent,
    ScenarioCapability::CompositorProjection,
    ScenarioCapability::HyprlandRuntimeMutation,
];

const ENVELOPE_GEOMETRY_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::CompositorResize,
    ScenarioCapability::ShapeAwareInput,
    ScenarioCapability::ReactiveTopology,
    ScenarioCapability::ReadOnlyHyprlandContext,
];

const ENVELOPE_DETACH_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::ReactiveTopology,
    ScenarioCapability::ShapeAwareInput,
    ScenarioCapability::TemporalDynamics,
];

const ENVELOPE_INFLUENCE_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::ReactiveTopology,
    ScenarioCapability::TemporalDynamics,
    ScenarioCapability::SoftSpatialInfluence,
];

const BAR_RESPONSIVE_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::CompositorResize,
];

const BAR_MORPH_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::GeometryMorph,
];

const DOCK_RESPONSIVE_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::CompositorResize,
    ScenarioCapability::ShapeAwareInput,
];

const DOCK_AUTOHIDE_CAPS: &[ScenarioCapability] = &[
    ScenarioCapability::GeometryDebug,
    ScenarioCapability::StructuredTelemetry,
    ScenarioCapability::ShapeAwareInput,
    ScenarioCapability::AutoHide,
    ScenarioCapability::GeometryMorph,
];

pub const SCENARIOS: &[ScenarioDescriptor] = &[
    ScenarioDescriptor {
        id: "scene.desktop",
        target: LabTarget::Scene,
        name: "desktop",
        purpose: "Persistent single-window desktop scene cycle integrating Bar, Body, Dock, relations, dynamics and influence.",
        args: &["--scene-cycle"],
        capabilities: SCENE_DESKTOP_CAPS,
        expected_markers: &[
            "GNOS_UX_SCENE_START",
            "GNOS_UX_SCENE_FRAME",
            "step=connected",
            "step=detached",
            "step=influence",
            "step=reattach",
            "GNOS_UX_SCENE_VIEWPORT_REBASE",
            "GNOS_UX_SCENE_REBASE_CONTINUITY",
            "GNOS_UX_SCENE_CONTINUITY_PASS",
            "GNOS_UX_SCENE_IDENTITY_PASS",
            "GNOS_UX_SCENE_CYCLE_COMPLETE",
        ],
        completion_marker: Some("GNOS_UX_SCENE_CYCLE_COMPLETE"),
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "scene.projection",
        target: LabTarget::Scene,
        name: "projection",
        purpose: "SDK6 compositor-intent projection: persistent scene reservation plus bounded reversible Hyprland runtime Lua mutation.",
        args: &["--scene-cycle", "--sdk6-projection-qualification"],
        capabilities: SCENE_PROJECTION_CAPS,
        expected_markers: &[
            "GNOS_UX_SCENE_RESERVATION_APPLIED",
            "GNOS_UX_SCENE_RESERVATION_RETURNED",
            "GNOSIX_RUNTIME_OUTPUT_TARGET_PASS",
            "GNOSIX_HYPRLAND_GLOBAL_PROJECTION_PASS",
            "GNOS_UX_SCENE_IDENTITY_PASS",
            "GNOS_UX_SCENE_CYCLE_COMPLETE",
        ],
        completion_marker: Some("GNOS_UX_SCENE_CYCLE_COMPLETE"),
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "envelope.geometry",
        target: LabTarget::Envelope,
        name: "geometry",
        purpose: "Interactive Reactive Envelope geometry/topology inspection.",
        args: &[
            "--geometry-lab",
            "--geometry-view=all",
            "--reactive-controls",
        ],
        capabilities: ENVELOPE_GEOMETRY_CAPS,
        expected_markers: &["GEOMETRY_LAB_START", "GEOMETRY_OBSERVATION"],
        completion_marker: None,
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "envelope.detach-relax",
        target: LabTarget::Envelope,
        name: "detach-relax",
        purpose: "Deterministic Fused/Bridged/Detached cycle including release-tail and reattach relaxation.",
        args: &[
            "--geometry-lab",
            "--geometry-view=all",
            "--reactive-controls",
            "--reactive-script",
        ],
        capabilities: ENVELOPE_DETACH_CAPS,
        expected_markers: &[
            "REACTIVE_ATTACHMENT_CHANGE",
            "REACTIVE_SAMPLE",
            "gnos.ux.recipe.dock.atlas-baseline",
            "gnos.ux.grammar.curves",
            "gnos.ux.grammar.connectors",
            "gnos.ux.grammar.dynamics",
            "REACTIVE_SCRIPT_COMPLETE",
        ],
        completion_marker: Some("REACTIVE_SCRIPT_COMPLETE"),
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "envelope.influence",
        target: LabTarget::Envelope,
        name: "influence",
        purpose: "Deterministic neighbor-influence cycle exercising shared temporal dynamics and soft spatial influence grammar.",
        args: &[
            "--geometry-lab",
            "--geometry-view=all",
            "--reactive-neighbor",
            "--reactive-neighbor-script",
            "--reactive-spatial-feedback=0.30",
        ],
        capabilities: ENVELOPE_INFLUENCE_CAPS,
        expected_markers: &[
            "REACTIVE_NEIGHBOR_ENABLED",
            "REACTIVE_INFLUENCE_ACTIVE",
            "gnos.ux.grammar.dynamics",
            "gnos.ux.grammar.influences",
            "REACTIVE_SCRIPT_COMPLETE",
        ],
        completion_marker: Some("REACTIVE_SCRIPT_COMPLETE"),
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "bar.responsive",
        target: LabTarget::Bar,
        name: "responsive",
        purpose: "Bar Envelope/frame telemetry under compositor-sized viewport changes.",
        args: GEOMETRY_BASE,
        capabilities: BAR_RESPONSIVE_CAPS,
        expected_markers: &["BAR_GEOMETRY_OBSERVATION", "BAR_GEOMETRY_VIEWPORT_REBASE"],
        completion_marker: None,
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "bar.morph",
        target: LabTarget::Bar,
        name: "morph",
        purpose: "Deterministic Bar Standard/Hug geometry morph cycle.",
        args: &["--geometry-lab", "--geometry-view=all", "--morph-cycle"],
        capabilities: BAR_MORPH_CAPS,
        expected_markers: &[
            "BAR_MORPH_SAMPLE",
            "gnos.ux.recipe.bar.qualifier-baseline",
            "BAR_MORPH_SEQUENCE_COMPLETE",
        ],
        completion_marker: Some("BAR_MORPH_SEQUENCE_COMPLETE"),
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "dock.responsive",
        target: LabTarget::Dock,
        name: "responsive",
        purpose: "Dock Envelope/input telemetry under compositor-sized viewport changes.",
        args: GEOMETRY_BASE,
        capabilities: DOCK_RESPONSIVE_CAPS,
        expected_markers: &["DOCK_GEOMETRY_OBSERVATION", "DOCK_GEOMETRY_VIEWPORT_REBASE"],
        completion_marker: None,
        nested_required: true,
    },
    ScenarioDescriptor {
        id: "dock.autohide",
        target: LabTarget::Dock,
        name: "autohide",
        purpose: "Deterministic visible/hidden Dock cycle with reveal/input parity.",
        args: &[
            "--geometry-lab",
            "--geometry-view=all",
            "--dock-atlas-cycle",
        ],
        capabilities: DOCK_AUTOHIDE_CAPS,
        expected_markers: &[
            "DOCK_ATLAS_MORPH_SAMPLE",
            "gnos.ux.recipe.dock.atlas-baseline",
            "DOCK_ATLAS_MORPH_SEQUENCE_COMPLETE",
        ],
        completion_marker: Some("DOCK_ATLAS_MORPH_SEQUENCE_COMPLETE"),
        nested_required: true,
    },
];

pub fn scenarios() -> &'static [ScenarioDescriptor] {
    SCENARIOS
}

pub fn scenarios_for(target: LabTarget) -> impl Iterator<Item = &'static ScenarioDescriptor> {
    SCENARIOS
        .iter()
        .filter(move |scenario| scenario.target == target)
}

pub fn find_scenario(target: LabTarget, name: &str) -> Option<&'static ScenarioDescriptor> {
    scenarios_for(target).find(|scenario| scenario.name == name)
}

pub fn resolve_repo_root() -> Option<PathBuf> {
    if let Some(root) = env::var_os("GNOS_UX_REPO_ROOT") {
        let path = PathBuf::from(root);
        if is_repo_root(&path) {
            return Some(path);
        }
    }

    if let Ok(current) = env::current_dir() {
        for ancestor in current.ancestors() {
            if is_repo_root(ancestor) {
                return Some(ancestor.to_path_buf());
            }
        }
    }

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .find(|path| is_repo_root(path))
        .map(Path::to_path_buf)
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("crates/gnos-ux-sdk/Cargo.toml").is_file()
        && path.join("tools/gnos-ux-nested-lab.sh").is_file()
}

pub fn current_binary_dir() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let mut parent = exe.parent()?.to_path_buf();
    if parent.file_name().is_some_and(|name| name == "deps") {
        parent = parent.parent()?.to_path_buf();
    }
    Some(parent)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabLaunchPlan {
    pub scenario_id: &'static str,
    pub harness: PathBuf,
    pub candidate: PathBuf,
    pub namespace: &'static str,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub completion_marker: Option<&'static str>,
}

impl LabLaunchPlan {
    pub fn new(
        repo_root: &Path,
        binary_dir: &Path,
        scenario: &'static ScenarioDescriptor,
        extra_args: &[String],
    ) -> Self {
        let candidate = binary_dir.join(scenario.binary());
        let harness = repo_root.join("tools/gnos-ux-nested-lab.sh");
        let mut args = scenario
            .args
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        args.extend(extra_args.iter().cloned());
        let env = vec![
            (
                "GNOS_UX_CANDIDATE".to_owned(),
                candidate.display().to_string(),
            ),
            (
                "GNOS_UX_CANDIDATE_NAMESPACE".to_owned(),
                scenario.namespace().to_owned(),
            ),
        ];

        Self {
            scenario_id: scenario.id,
            harness,
            candidate,
            namespace: scenario.namespace(),
            args,
            env,
            completion_marker: None,
        }
    }

    pub fn with_completion_marker(mut self, marker: &'static str) -> Self {
        self.env.push((
            "GNOS_UX_CANDIDATE_EXIT_MARKER".to_owned(),
            marker.to_owned(),
        ));
        self.completion_marker = Some(marker);
        self
    }

    pub fn with_required_markers(mut self, markers: &'static [&'static str]) -> Self {
        self.env.push((
            "GNOS_UX_CANDIDATE_REQUIRED_MARKERS".to_owned(),
            markers.join("|"),
        ));
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalogue_ids_are_unique_and_current_targets_are_atlas_rich() {
        let mut ids = HashSet::new();
        for scenario in scenarios() {
            assert!(
                ids.insert(scenario.id),
                "duplicate scenario id {}",
                scenario.id
            );
            match scenario.target {
                LabTarget::Scene => {
                    assert_eq!(scenario.coverage(), AtlasCoverageLevel::ProductRuntimeOnly)
                }
                LabTarget::Envelope | LabTarget::Bar | LabTarget::Dock => {
                    assert_eq!(scenario.coverage(), AtlasCoverageLevel::AtlasRich)
                }
            }
            assert!(scenario.nested_required);
            assert!(!scenario.expected_markers.is_empty());
        }
    }

    #[test]
    fn every_proven_target_has_at_least_one_scenario() {
        for target in [
            LabTarget::Scene,
            LabTarget::Envelope,
            LabTarget::Bar,
            LabTarget::Dock,
        ] {
            assert!(scenarios_for(target).next().is_some());
        }
    }

    #[test]
    fn rail_and_overlay_are_not_lab_product_targets() {
        assert_eq!(LabTarget::parse("scene"), Some(LabTarget::Scene));
        assert_eq!(LabTarget::parse("rail"), None);
        assert_eq!(LabTarget::parse("overlay"), None);
        assert_eq!(
            surface_atlas_coverage(SurfaceRole::Rail),
            AtlasCoverageLevel::ProbeOnly
        );
        assert_eq!(
            surface_atlas_coverage(SurfaceRole::Overlay),
            AtlasCoverageLevel::ProbeOnly
        );
    }

    #[test]
    fn scenario_resolution_is_target_scoped() {
        assert_eq!(
            find_scenario(LabTarget::Dock, "autohide").map(|value| value.id),
            Some("dock.autohide")
        );
        assert!(find_scenario(LabTarget::Bar, "autohide").is_none());
    }

    #[test]
    fn launch_plan_reuses_existing_qualifier_and_nested_harness() {
        let root = Path::new("/repo");
        let bin = Path::new("/target/debug");
        let scenario = find_scenario(LabTarget::Dock, "autohide").expect("Dock scenario");
        let plan = LabLaunchPlan::new(root, bin, scenario, &["--state=hidden".to_owned()]);

        assert_eq!(
            plan.harness,
            PathBuf::from("/repo/tools/gnos-ux-nested-lab.sh")
        );
        assert_eq!(
            plan.candidate,
            PathBuf::from("/target/debug/gnos-ux-dock-envelope-qualify")
        );
        assert_eq!(plan.namespace, "gnos.ux:dock:atlas-qualification");
        assert!(plan.args.contains(&"--dock-atlas-cycle".to_owned()));
        assert!(plan.args.contains(&"--state=hidden".to_owned()));
        assert_eq!(
            plan.env
                .iter()
                .find(|(key, _)| key == "GNOS_UX_CANDIDATE_NAMESPACE")
                .map(|(_, value)| value.as_str()),
            Some("gnos.ux:dock:atlas-qualification")
        );
        assert_eq!(plan.completion_marker, None);

        let finite = plan
            .with_completion_marker(
                scenario
                    .completion_marker
                    .expect("autohide has a completion marker"),
            )
            .with_required_markers(scenario.expected_markers);
        assert_eq!(
            finite.completion_marker,
            Some("DOCK_ATLAS_MORPH_SEQUENCE_COMPLETE")
        );
        assert_eq!(
            finite
                .env
                .iter()
                .find(|(key, _)| key == "GNOS_UX_CANDIDATE_EXIT_MARKER")
                .map(|(_, value)| value.as_str()),
            Some("DOCK_ATLAS_MORPH_SEQUENCE_COMPLETE")
        );
        assert_eq!(
            finite
                .env
                .iter()
                .find(|(key, _)| key == "GNOS_UX_CANDIDATE_REQUIRED_MARKERS")
                .map(|(_, value)| value.as_str()),
            Some(
                "DOCK_ATLAS_MORPH_SAMPLE|gnos.ux.recipe.dock.atlas-baseline|DOCK_ATLAS_MORPH_SEQUENCE_COMPLETE"
            )
        );
    }
}
