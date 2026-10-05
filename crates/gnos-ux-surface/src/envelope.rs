use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuntimeId(String);

impl RuntimeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalPx(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NormalizedPermille(u16);

impl NormalizedPermille {
    pub fn new(value: u16) -> Self {
        Self(value.min(1000))
    }
    pub fn value(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpacityPercent(u8);

impl OpacityPercent {
    pub fn new(value: u8) -> Self {
        Self(value.min(100))
    }
    pub fn value(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryMode {
    Standard,
    Hug,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRelation {
    EdgeBound,
    Inset,
    Detached,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceEdge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialFamilyId(String);

impl MaterialFamilyId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DerivedMaterialIntent {
    Inherit,
    InheritWithOverride(MaterialPatch),
    Independent(MaterialRecipe),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attachment {
    Fused,
    Bridged,
    Detached,
    Auto,
}

// OutputRelation is surface-to-edge/output. Attachment is derived-to-parent.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtrusionOrigin {
    SurfaceHull,
    ScreenEdge,
    ControlAnchor,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityPolicy {
    AlwaysVisible,
    AutoHide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RevealTriggers {
    pub pointer_proximity: bool,
    pub key_press: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviourDefaults {
    pub visibility: VisibilityPolicy,
    pub triggers: RevealTriggers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BehaviourPatch {
    pub visibility: Option<VisibilityPolicy>,
    pub triggers: Option<RevealTriggers>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardIntent {
    None,
    OnDemand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceReservationIntent {
    Overlay,
    Reserve(LogicalPx),
    Auto,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositorIntentDefaults {
    pub edge: SurfaceEdge,
    pub keyboard: KeyboardIntent,
    pub reservation: SpaceReservationIntent,
    pub margins: [LogicalPx; 4],
    pub output: Option<RuntimeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CompositorIntentPatch {
    pub edge: Option<SurfaceEdge>,
    pub keyboard: Option<KeyboardIntent>,
    pub reservation: Option<SpaceReservationIntent>,
    pub margins: Option<[LogicalPx; 4]>,
    pub output: Option<Option<RuntimeId>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialRecipe {
    pub family: MaterialFamilyId,
    pub blur: LogicalPx,
    pub border: LogicalPx,
    pub shadow: LogicalPx,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MaterialPatch {
    pub family: Option<MaterialFamilyId>,
    pub blur: Option<LogicalPx>,
    pub border: Option<LogicalPx>,
    pub shadow: Option<LogicalPx>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromePresentation {
    pub background_visible: bool,
    pub opacity: OpacityPercent,
    pub border_visible: bool,
    pub shadow_visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ChromePatch {
    pub background_visible: Option<bool>,
    pub opacity: Option<OpacityPercent>,
    pub border_visible: Option<bool>,
    pub shadow_visible: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeIntent {
    pub width: LogicalPx,
    pub length: LogicalPx,
    pub neck_width: LogicalPx,
    pub shoulder_radius: LogicalPx,
    pub min_span: LogicalPx,
    pub max_span: LogicalPx,
    pub break_threshold: NormalizedPermille,
    pub reduced_motion: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeDefaults {
    pub geometry: GeometryMode,
    pub relation: OutputRelation,
    pub material: MaterialRecipe,
    pub chrome: ChromePresentation,
    pub compositor: CompositorIntentDefaults,
    pub behaviour: BehaviourDefaults,
    pub capabilities: EnvelopeCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EnvelopePatch {
    pub geometry: Option<GeometryMode>,
    pub relation: Option<OutputRelation>,
    pub material: MaterialPatch,
    pub chrome: ChromePatch,
    pub compositor: CompositorIntentPatch,
    pub behaviour: BehaviourPatch,
    pub capabilities: Option<EnvelopeCapabilities>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceDefaults {
    pub envelope: EnvelopeDefaults,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SurfacePatch {
    pub envelope: EnvelopePatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceProfile {
    pub monitor: RuntimeId,
    pub defaults: SurfaceDefaults,
    pub patch: SurfacePatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedSurfacePatch {
    pub id: RuntimeId,
    pub parent: RuntimeId,
    pub origin: ExtrusionOrigin,
    pub material: DerivedMaterialIntent,
    pub attachment: Attachment,
    pub bridge: Option<BridgeIntent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEnvelopeState {
    pub geometry: GeometryMode,
    pub relation: OutputRelation,
    pub material: MaterialRecipe,
    pub chrome: ChromePresentation,
    pub compositor: CompositorIntentDefaults,
    pub behaviour: BehaviourDefaults,
    pub capabilities: EnvelopeCapabilities,
    pub derived_relation: Option<ResolvedDerivedRelation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPolicy {
    Qualification {
        geometry: GeometryMode,
        relation: OutputRelation,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeCapabilities {
    pub extrusion: bool,
    pub connective_hull: bool,
    pub bridge: bool,
    pub shape_aware_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDerivedRelation {
    pub id: RuntimeId,
    pub parent: RuntimeId,
    pub origin: ExtrusionOrigin,
    pub material: MaterialRecipe,
    pub attachment: Attachment,
    pub bridge: Option<BridgeIntent>,
}

pub fn resolve_geometry(
    mode: GeometryMode,
    relation: OutputRelation,
    policy: Option<AutoPolicy>,
) -> (GeometryMode, OutputRelation) {
    match policy {
        Some(AutoPolicy::Qualification {
            geometry,
            relation: fallback_relation,
        }) => (
            if mode == GeometryMode::Auto {
                geometry
            } else {
                mode
            },
            if relation == OutputRelation::Auto {
                fallback_relation
            } else {
                relation
            },
        ),
        None => (mode, relation),
    }
}

pub fn resolve_inheritance(
    global: &EnvelopeDefaults,
    monitor: &EnvelopePatch,
    surface: &EnvelopePatch,
    derived: Option<&DerivedSurfacePatch>,
    policy: Option<AutoPolicy>,
) -> ResolvedEnvelopeState {
    let mut state = resolve_patch(global.clone(), monitor);
    state = resolve_patch(state, surface);
    let derived_relation = derived.map(|patch| {
        let material = match &patch.material {
            DerivedMaterialIntent::Inherit => state.material.clone(),
            DerivedMaterialIntent::InheritWithOverride(material_patch) => {
                apply_material_patch(state.material.clone(), material_patch)
            }
            DerivedMaterialIntent::Independent(recipe) => recipe.clone(),
        };
        ResolvedDerivedRelation {
            id: patch.id.clone(),
            parent: patch.parent.clone(),
            origin: patch.origin,
            material,
            attachment: patch.attachment,
            bridge: patch.bridge.clone(),
        }
    });
    let (geometry, relation) = resolve_geometry(state.geometry, state.relation, policy);
    state.geometry = geometry;
    state.relation = relation;
    ResolvedEnvelopeState {
        geometry: state.geometry,
        relation: state.relation,
        material: state.material,
        chrome: state.chrome,
        compositor: state.compositor,
        behaviour: state.behaviour,
        capabilities: state.capabilities,
        derived_relation,
    }
}

fn resolve_patch(mut state: EnvelopeDefaults, patch: &EnvelopePatch) -> EnvelopeDefaults {
    if let Some(value) = patch.geometry {
        state.geometry = value;
    }
    if let Some(value) = patch.relation {
        state.relation = value;
    }
    state.material = apply_material_patch(state.material, &patch.material);
    if let Some(value) = patch.chrome.background_visible {
        state.chrome.background_visible = value;
    }
    if let Some(value) = patch.chrome.opacity {
        state.chrome.opacity = value;
    }
    if let Some(value) = patch.chrome.border_visible {
        state.chrome.border_visible = value;
    }
    if let Some(value) = patch.chrome.shadow_visible {
        state.chrome.shadow_visible = value;
    }
    if let Some(value) = patch.compositor.edge {
        state.compositor.edge = value;
    }
    if let Some(value) = patch.compositor.keyboard {
        state.compositor.keyboard = value;
    }
    if let Some(value) = patch.compositor.reservation {
        state.compositor.reservation = value;
    }
    if let Some(value) = &patch.compositor.margins {
        state.compositor.margins = *value;
    }
    if let Some(value) = &patch.compositor.output {
        state.compositor.output = value.clone();
    }
    if let Some(value) = patch.behaviour.visibility {
        state.behaviour.visibility = value;
    }
    if let Some(value) = patch.behaviour.triggers {
        state.behaviour.triggers = value;
    }
    if let Some(value) = patch.capabilities {
        state.capabilities = value;
    }
    state
}

fn apply_material_patch(mut material: MaterialRecipe, patch: &MaterialPatch) -> MaterialRecipe {
    if let Some(value) = &patch.family {
        material.family = value.clone();
    }
    if let Some(value) = patch.blur {
        material.blur = value;
    }
    if let Some(value) = patch.border {
        material.border = value;
    }
    if let Some(value) = patch.shadow {
        material.shadow = value;
    }
    material
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyPhase {
    Stable,
    Fusing,
    Bridging,
    Detaching,
    Collapsing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualState {
    pub opacity: OpacityPercent,
    pub radius: LogicalPx,
    pub margin: LogicalPx,
    pub translation: LogicalOffset,
    pub bridge_width: LogicalPx,
    pub bridge_neck_width: LogicalPx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionState {
    pub from: VisualState,
    pub to: VisualState,
    pub progress: NormalizedPermille,
    pub topology: TopologyPhase,
}

pub fn interpolate_visual(
    from: VisualState,
    to: VisualState,
    progress: NormalizedPermille,
) -> VisualState {
    fn lerp(a: u32, b: u32, p: u16) -> u32 {
        if b >= a {
            a + (b - a) * u32::from(p.min(1000)) / 1000
        } else {
            a - (a - b) * u32::from(p.min(1000)) / 1000
        }
    }
    fn lerp_signed(a: i32, b: i32, p: u16) -> i32 {
        a + ((i64::from(b) - i64::from(a)) * i64::from(p.min(1000)) / 1000) as i32
    }
    VisualState {
        opacity: OpacityPercent::new(lerp(
            u32::from(from.opacity.value()),
            u32::from(to.opacity.value()),
            progress.0,
        ) as u8),
        radius: LogicalPx(lerp(from.radius.0, to.radius.0, progress.0)),
        margin: LogicalPx(lerp(from.margin.0, to.margin.0, progress.0)),
        translation: LogicalOffset {
            x: lerp_signed(from.translation.x, to.translation.x, progress.value()),
            y: lerp_signed(from.translation.y, to.translation.y, progress.value()),
        },
        bridge_width: LogicalPx(lerp(from.bridge_width.0, to.bridge_width.0, progress.0)),
        bridge_neck_width: LogicalPx(lerp(
            from.bridge_neck_width.0,
            to.bridge_neck_width.0,
            progress.0,
        )),
    }
}

pub fn transition_topology(from: Attachment, to: Attachment) -> TopologyPhase {
    match (from, to) {
        (Attachment::Fused, Attachment::Bridged) => TopologyPhase::Bridging,
        (Attachment::Bridged, Attachment::Detached) => TopologyPhase::Detaching,
        (Attachment::Detached, Attachment::Bridged) => TopologyPhase::Bridging,
        (Attachment::Bridged, Attachment::Fused) => TopologyPhase::Fusing,
        (Attachment::Detached, Attachment::Fused) => TopologyPhase::Collapsing,
        _ => TopologyPhase::Stable,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeRegion {
    pub x: i32,
    pub y: i32,
    pub width: LogicalPx,
    pub height: LogicalPx,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionSet {
    pub regions: Vec<ShapeRegion>,
}

impl RegionSet {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.regions.iter().any(|region| {
            x >= region.x
                && y >= region.y
                && x < region.x + region.width.0 as i32
                && y < region.y + region.height.0 as i32
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeAwareRegions {
    pub allocation: RegionSet,
    pub paint_hull: RegionSet,
    pub hit: RegionSet,
    pub input: RegionSet,
}

pub fn regions_with_transparent_gap(
    allocation: RegionSet,
    paint_hull: RegionSet,
    hit: RegionSet,
    input: RegionSet,
) -> ShapeAwareRegions {
    ShapeAwareRegions {
        allocation,
        paint_hull,
        hit,
        input,
    }
}

pub fn resolve_monitor_profiles(
    global: &EnvelopeDefaults,
    monitors: &BTreeMap<RuntimeId, EnvelopePatch>,
) -> BTreeMap<RuntimeId, ResolvedEnvelopeState> {
    monitors
        .iter()
        .map(|(id, patch)| {
            let state = resolve_inheritance(global, patch, &EnvelopePatch::default(), None, None);
            (id.clone(), state)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> EnvelopeDefaults {
        EnvelopeDefaults {
            geometry: GeometryMode::Standard,
            relation: OutputRelation::EdgeBound,
            material: MaterialRecipe {
                family: MaterialFamilyId::new("calestia"),
                blur: LogicalPx(8),
                border: LogicalPx(1),
                shadow: LogicalPx(12),
            },
            chrome: ChromePresentation {
                background_visible: true,
                opacity: OpacityPercent::new(100),
                border_visible: true,
                shadow_visible: true,
            },
            compositor: CompositorIntentDefaults {
                edge: SurfaceEdge::Bottom,
                keyboard: KeyboardIntent::None,
                reservation: SpaceReservationIntent::Overlay,
                margins: [LogicalPx(0); 4],
                output: None,
            },
            behaviour: BehaviourDefaults {
                visibility: VisibilityPolicy::AlwaysVisible,
                triggers: RevealTriggers::default(),
            },
            capabilities: EnvelopeCapabilities {
                extrusion: true,
                connective_hull: true,
                bridge: true,
                shape_aware_input: true,
            },
        }
    }

    #[test]
    fn partial_inheritance_only_overrides_selected_fields() {
        let base = defaults();
        let monitor = EnvelopePatch {
            geometry: Some(GeometryMode::Hug),
            ..Default::default()
        };
        let surface = EnvelopePatch {
            compositor: CompositorIntentPatch {
                edge: Some(SurfaceEdge::Left),
                ..Default::default()
            },
            ..Default::default()
        };
        let state = resolve_inheritance(&base, &monitor, &surface, None, None);
        assert_eq!(state.geometry, GeometryMode::Hug);
        assert_eq!(state.compositor.edge, SurfaceEdge::Left);
        assert_eq!(state.material.family.as_str(), "calestia");
        assert_eq!(state.compositor.keyboard, KeyboardIntent::None);
    }

    #[test]
    fn material_policies_are_distinct() {
        let base = defaults();
        let inherited = resolve_inheritance(
            &base,
            &EnvelopePatch::default(),
            &EnvelopePatch::default(),
            Some(&DerivedSurfacePatch {
                id: RuntimeId::new("a"),
                parent: RuntimeId::new("dock"),
                origin: ExtrusionOrigin::ControlAnchor,
                material: DerivedMaterialIntent::Inherit,
                attachment: Attachment::Detached,
                bridge: None,
            }),
            None,
        );
        assert_eq!(inherited.material, base.material);
        let override_state = resolve_inheritance(
            &base,
            &EnvelopePatch::default(),
            &EnvelopePatch::default(),
            Some(&DerivedSurfacePatch {
                id: RuntimeId::new("b"),
                parent: RuntimeId::new("dock"),
                origin: ExtrusionOrigin::ControlAnchor,
                material: DerivedMaterialIntent::InheritWithOverride(MaterialPatch {
                    blur: Some(LogicalPx(20)),
                    ..Default::default()
                }),
                attachment: Attachment::Bridged,
                bridge: None,
            }),
            None,
        );
        assert_eq!(
            override_state
                .derived_relation
                .as_ref()
                .unwrap()
                .material
                .blur,
            LogicalPx(20)
        );
        assert_eq!(
            override_state
                .derived_relation
                .as_ref()
                .unwrap()
                .material
                .border,
            base.material.border
        );
        let independent = resolve_inheritance(
            &base,
            &EnvelopePatch::default(),
            &EnvelopePatch::default(),
            Some(&DerivedSurfacePatch {
                id: RuntimeId::new("c"),
                parent: RuntimeId::new("dock"),
                origin: ExtrusionOrigin::SurfaceHull,
                material: DerivedMaterialIntent::Independent(MaterialRecipe {
                    family: MaterialFamilyId::new("ink"),
                    blur: LogicalPx(4),
                    border: LogicalPx(2),
                    shadow: LogicalPx(6),
                }),
                attachment: Attachment::Detached,
                bridge: None,
            }),
            None,
        );
        assert_eq!(
            independent
                .derived_relation
                .as_ref()
                .unwrap()
                .material
                .family
                .as_str(),
            "ink"
        );
        assert_eq!(
            inherited
                .derived_relation
                .as_ref()
                .map(|r| r.parent.clone()),
            Some(RuntimeId::new("dock"))
        );
    }

    #[test]
    fn behaviour_can_combine_autohide_and_reveal_triggers() {
        let behaviour = BehaviourDefaults {
            visibility: VisibilityPolicy::AutoHide,
            triggers: RevealTriggers {
                pointer_proximity: true,
                key_press: true,
            },
        };
        assert!(behaviour.triggers.pointer_proximity && behaviour.triggers.key_press);
    }

    #[test]
    fn auto_requires_injected_policy() {
        let base = defaults();
        let unresolved = resolve_inheritance(
            &EnvelopeDefaults {
                geometry: GeometryMode::Auto,
                relation: OutputRelation::Auto,
                ..base.clone()
            },
            &EnvelopePatch::default(),
            &EnvelopePatch::default(),
            None,
            None,
        );
        assert_eq!(unresolved.geometry, GeometryMode::Auto);
        let qualified = resolve_inheritance(
            &EnvelopeDefaults {
                geometry: GeometryMode::Auto,
                relation: OutputRelation::Auto,
                ..base
            },
            &EnvelopePatch::default(),
            &EnvelopePatch::default(),
            None,
            Some(AutoPolicy::Qualification {
                geometry: GeometryMode::Standard,
                relation: OutputRelation::Inset,
            }),
        );
        assert_eq!(
            (qualified.geometry, qualified.relation),
            (GeometryMode::Standard, OutputRelation::Inset)
        );
    }

    #[test]
    fn runtime_ids_are_owned() {
        let id = RuntimeId::new(String::from("DP-2"));
        assert_eq!(id.as_str(), "DP-2");
    }

    #[test]
    fn reservation_intent_is_not_visual_geometry() {
        let base = defaults();
        let patch = EnvelopePatch {
            compositor: CompositorIntentPatch {
                reservation: Some(SpaceReservationIntent::Reserve(LogicalPx(36))),
                ..Default::default()
            },
            ..Default::default()
        };
        let state = resolve_inheritance(&base, &patch, &EnvelopePatch::default(), None, None);
        assert_eq!(
            state.compositor.reservation,
            SpaceReservationIntent::Reserve(LogicalPx(36))
        );
        assert_eq!(state.geometry, GeometryMode::Standard);
    }

    #[test]
    fn visual_interpolation_is_continuous_at_multiple_samples() {
        let a = VisualState {
            opacity: OpacityPercent::new(0),
            radius: LogicalPx(0),
            margin: LogicalPx(0),
            translation: LogicalOffset { x: -20, y: 0 },
            bridge_width: LogicalPx(100),
            bridge_neck_width: LogicalPx(40),
        };
        let b = VisualState {
            opacity: OpacityPercent::new(100),
            radius: LogicalPx(28),
            margin: LogicalPx(24),
            translation: LogicalOffset { x: 20, y: 72 },
            bridge_width: LogicalPx(0),
            bridge_neck_width: LogicalPx(0),
        };
        assert_eq!(interpolate_visual(a, b, NormalizedPermille::new(0)), a);
        assert_eq!(interpolate_visual(a, b, NormalizedPermille::new(1000)), b);
        assert!(
            interpolate_visual(a, b, NormalizedPermille::new(500))
                .radius
                .0
                > 0
        );
    }

    #[test]
    fn topology_and_regions_remain_separate() {
        assert_eq!(
            transition_topology(Attachment::Fused, Attachment::Bridged),
            TopologyPhase::Bridging
        );
        let allocation = RegionSet {
            regions: vec![ShapeRegion {
                x: 0,
                y: 0,
                width: LogicalPx(1000),
                height: LogicalPx(400),
            }],
        };
        let paint = RegionSet {
            regions: vec![
                ShapeRegion {
                    x: 100,
                    y: 100,
                    width: LogicalPx(200),
                    height: LogicalPx(100),
                },
                ShapeRegion {
                    x: 700,
                    y: 100,
                    width: LogicalPx(200),
                    height: LogicalPx(100),
                },
            ],
        };
        let regions =
            regions_with_transparent_gap(allocation, paint.clone(), paint.clone(), paint.clone());
        assert_ne!(regions.allocation, regions.input);
        assert_eq!(regions.paint_hull.regions.len(), 2);
        assert!(regions.input.contains(150, 150));
        assert!(regions.input.contains(750, 150));
        assert!(!regions.input.contains(500, 150));
    }
}
