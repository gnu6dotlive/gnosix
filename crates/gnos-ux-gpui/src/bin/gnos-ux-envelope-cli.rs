#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

use std::{collections::BTreeMap, env, fmt};

use gnos_ux_surface::envelope::{
    Attachment, BehaviourDefaults, BehaviourPatch, BridgeIntent, ChromePresentation,
    CompositorIntentDefaults, DerivedMaterialIntent, DerivedSurfacePatch, EnvelopeCapabilities,
    EnvelopeDefaults, EnvelopePatch, ExtrusionOrigin, GeometryMode, KeyboardIntent, LogicalOffset,
    LogicalPx, MaterialFamilyId, MaterialRecipe, NormalizedPermille, OpacityPercent,
    OutputRelation, RevealTriggers, RuntimeId, SpaceReservationIntent, SurfaceEdge,
    VisibilityPolicy, VisualState, interpolate_visual, resolve_inheritance,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceTarget {
    Dock,
    Bar,
    AppStack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionCommandKind {
    Inspect,
    Apply,
    Attachment,
    Transition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionCommand {
    kind: SessionCommandKind,
    surface: SurfaceTarget,
    output: RuntimeId,
    options: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn parse_surface(value: &str) -> Result<SurfaceTarget, CliError> {
    match value {
        "dock" => Ok(SurfaceTarget::Dock),
        "bar" => Ok(SurfaceTarget::Bar),
        "appstack" => Ok(SurfaceTarget::AppStack),
        _ => Err(CliError(format!("invalid surface: {value}"))),
    }
}

fn parse_geometry(value: &str) -> Result<GeometryMode, CliError> {
    match value {
        "standard" => Ok(GeometryMode::Standard),
        "hug" => Ok(GeometryMode::Hug),
        "auto" => Ok(GeometryMode::Auto),
        _ => Err(CliError(format!("invalid geometry: {value}"))),
    }
}

fn parse_relation(value: &str) -> Result<OutputRelation, CliError> {
    match value {
        "edge" => Ok(OutputRelation::EdgeBound),
        "inset" => Ok(OutputRelation::Inset),
        "detached" => Ok(OutputRelation::Detached),
        "auto" => Ok(OutputRelation::Auto),
        _ => Err(CliError(format!("invalid output relation: {value}"))),
    }
}

fn parse_visibility(value: &str) -> Result<VisibilityPolicy, CliError> {
    match value {
        "always" => Ok(VisibilityPolicy::AlwaysVisible),
        "autohide" => Ok(VisibilityPolicy::AutoHide),
        _ => Err(CliError(format!("invalid visibility: {value}"))),
    }
}

fn parse_attachment(value: &str) -> Result<Attachment, CliError> {
    match value {
        "fused" => Ok(Attachment::Fused),
        "bridged" => Ok(Attachment::Bridged),
        "detached" => Ok(Attachment::Detached),
        _ => Err(CliError(format!("invalid attachment: {value}"))),
    }
}

fn parse_args<I>(args: I) -> Result<SessionCommand, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let kind = match args.next().as_deref() {
        Some("inspect") => SessionCommandKind::Inspect,
        Some("apply") => SessionCommandKind::Apply,
        Some("attachment") => SessionCommandKind::Attachment,
        Some("transition") => SessionCommandKind::Transition,
        Some(other) => return Err(CliError(format!("unknown command: {other}"))),
        None => return Err(CliError("missing command".into())),
    };
    let mut options = BTreeMap::new();
    while let Some(key) = args.next() {
        if !key.starts_with("--") {
            return Err(CliError(format!("expected --option, got: {key}")));
        }
        let value = args
            .next()
            .ok_or_else(|| CliError(format!("missing value for {key}")))?;
        if value.starts_with("--") {
            return Err(CliError(format!("missing value for {key}")));
        }
        let key = key.trim_start_matches("--").to_string();
        if options.insert(key.clone(), value).is_some() {
            return Err(CliError(format!("duplicate option: --{key}")));
        }
    }
    let surface = parse_surface(
        options
            .get("surface")
            .ok_or_else(|| CliError("--surface is required".into()))?,
    )?;
    let output = RuntimeId::new(
        options
            .get("output")
            .ok_or_else(|| CliError("--output is required".into()))?,
    );
    Ok(SessionCommand {
        kind,
        surface,
        output,
        options,
    })
}

fn qualification_defaults(surface: SurfaceTarget, output: RuntimeId) -> EnvelopeDefaults {
    let (geometry, relation, edge, visibility, capabilities) = match surface {
        SurfaceTarget::Dock => (
            GeometryMode::Hug,
            OutputRelation::EdgeBound,
            SurfaceEdge::Bottom,
            VisibilityPolicy::AutoHide,
            EnvelopeCapabilities {
                extrusion: true,
                connective_hull: true,
                bridge: true,
                shape_aware_input: true,
            },
        ),
        SurfaceTarget::Bar => (
            GeometryMode::Standard,
            OutputRelation::EdgeBound,
            SurfaceEdge::Top,
            VisibilityPolicy::AlwaysVisible,
            EnvelopeCapabilities {
                extrusion: false,
                connective_hull: false,
                bridge: false,
                shape_aware_input: true,
            },
        ),
        SurfaceTarget::AppStack => (
            GeometryMode::Hug,
            OutputRelation::EdgeBound,
            SurfaceEdge::Bottom,
            VisibilityPolicy::AlwaysVisible,
            EnvelopeCapabilities {
                extrusion: true,
                connective_hull: true,
                bridge: true,
                shape_aware_input: true,
            },
        ),
    };
    EnvelopeDefaults {
        geometry,
        relation,
        material: MaterialRecipe {
            family: MaterialFamilyId::new("qualification-cli"),
            blur: LogicalPx(18),
            border: LogicalPx(1),
            shadow: LogicalPx(18),
        },
        chrome: ChromePresentation {
            background_visible: true,
            opacity: OpacityPercent::new(92),
            border_visible: true,
            shadow_visible: true,
        },
        compositor: CompositorIntentDefaults {
            edge,
            keyboard: KeyboardIntent::None,
            reservation: SpaceReservationIntent::Overlay,
            margins: [LogicalPx(0); 4],
            output: Some(output),
        },
        behaviour: BehaviourDefaults {
            visibility,
            triggers: RevealTriggers {
                pointer_proximity: visibility == VisibilityPolicy::AutoHide,
                key_press: false,
            },
        },
        capabilities,
    }
}

fn qualification_bridge(attachment: Attachment) -> BridgeIntent {
    BridgeIntent {
        width: LogicalPx(220),
        length: LogicalPx(48),
        neck_width: LogicalPx(match attachment {
            Attachment::Fused => 180,
            Attachment::Bridged => 86,
            Attachment::Detached | Attachment::Auto => 0,
        }),
        shoulder_radius: LogicalPx(28),
        min_span: LogicalPx(120),
        max_span: LogicalPx(300),
        break_threshold: NormalizedPermille::new(700),
        reduced_motion: false,
    }
}

fn derived_patch(attachment: Attachment) -> DerivedSurfacePatch {
    DerivedSurfacePatch {
        id: RuntimeId::new("gnos.ux.appstack.qualification"),
        parent: RuntimeId::new("gnos.ux.dock.qualification"),
        origin: ExtrusionOrigin::SurfaceHull,
        material: DerivedMaterialIntent::Inherit,
        attachment,
        bridge: Some(qualification_bridge(attachment)),
    }
}

fn patch_from_options(options: &BTreeMap<String, String>) -> Result<EnvelopePatch, CliError> {
    let mut patch = EnvelopePatch::default();
    if let Some(value) = options.get("geometry") {
        patch.geometry = Some(parse_geometry(value)?);
    }
    if let Some(value) = options.get("relation") {
        patch.relation = Some(parse_relation(value)?);
    }
    if let Some(value) = options.get("visibility") {
        patch.behaviour = BehaviourPatch {
            visibility: Some(parse_visibility(value)?),
            ..Default::default()
        };
    }
    if let Some(value) = options.get("background") {
        patch.chrome.background_visible = Some(match value.as_str() {
            "visible" => true,
            "hidden" => false,
            _ => return Err(CliError(format!("invalid background: {value}"))),
        });
    }
    if let Some(value) = options.get("opacity") {
        let opacity: u8 = value
            .parse()
            .map_err(|_| CliError(format!("invalid opacity: {value}")))?;
        if opacity > 100 {
            return Err(CliError("opacity must be 0..=100".into()));
        }
        patch.chrome.opacity = Some(OpacityPercent::new(opacity));
    }
    Ok(patch)
}

fn resolve_session(
    command: &SessionCommand,
) -> Result<gnos_ux_surface::envelope::ResolvedEnvelopeState, CliError> {
    let defaults = qualification_defaults(command.surface, command.output.clone());
    let patch = patch_from_options(&command.options)?;
    let derived = if command.surface == SurfaceTarget::AppStack {
        let attachment = command
            .options
            .get("attachment")
            .map(|value| parse_attachment(value))
            .transpose()?
            .unwrap_or(Attachment::Fused);
        Some(derived_patch(attachment))
    } else {
        None
    };
    Ok(resolve_inheritance(
        &defaults,
        &Default::default(),
        &patch,
        derived.as_ref(),
        None,
    ))
}

fn visual_for_geometry(mode: GeometryMode) -> Result<VisualState, CliError> {
    match mode {
        GeometryMode::Standard => Ok(VisualState {
            opacity: OpacityPercent::new(92),
            radius: LogicalPx(0),
            margin: LogicalPx(0),
            translation: LogicalOffset { x: 0, y: 0 },
            bridge_width: LogicalPx(0),
            bridge_neck_width: LogicalPx(0),
        }),
        GeometryMode::Hug => Ok(VisualState {
            opacity: OpacityPercent::new(92),
            radius: LogicalPx(24),
            margin: LogicalPx(48),
            translation: LogicalOffset { x: 0, y: 18 },
            bridge_width: LogicalPx(0),
            bridge_neck_width: LogicalPx(0),
        }),
        GeometryMode::Auto => Err(CliError(
            "Auto transition endpoint requires an injected resolver policy; not implicit in CLI"
                .into(),
        )),
    }
}

fn print_header(command: &SessionCommand) {
    println!("SESSION_ONLY=true");
    println!("PERSISTENCE_SIDE_EFFECTS=none");
    println!("RUNTIME_MUTATION=none");
    println!("TRANSPORT=none");
    println!("surface={:?}", command.surface);
    println!("output={}", command.output.as_str());
}

fn execute(command: &SessionCommand) -> Result<(), CliError> {
    print_header(command);
    match command.kind {
        SessionCommandKind::Inspect | SessionCommandKind::Apply => {
            let resolved = resolve_session(command)?;
            println!("resolved={resolved:#?}");
        }
        SessionCommandKind::Attachment => {
            if command.surface != SurfaceTarget::AppStack {
                return Err(CliError(
                    "attachment command requires --surface appstack".into(),
                ));
            }
            let attachment = parse_attachment(
                command
                    .options
                    .get("attachment")
                    .ok_or_else(|| CliError("--attachment is required".into()))?,
            )?;
            let mut applied = command.clone();
            applied.options.insert(
                "attachment".into(),
                (match attachment {
                    Attachment::Fused => "fused",
                    Attachment::Bridged => "bridged",
                    Attachment::Detached => "detached",
                    Attachment::Auto => "auto",
                })
                .to_string(),
            );
            let resolved = resolve_session(&applied)?;
            println!("resolved={resolved:#?}");
        }
        SessionCommandKind::Transition => execute_transition(command)?,
    }
    Ok(())
}

fn execute_transition(command: &SessionCommand) -> Result<(), CliError> {
    let from = command
        .options
        .get("from")
        .ok_or_else(|| CliError("--from is required".into()))?;
    let to = command
        .options
        .get("to")
        .ok_or_else(|| CliError("--to is required".into()))?;
    let samples = command
        .options
        .get("samples")
        .map(|value| {
            value
                .parse::<u16>()
                .map_err(|_| CliError("invalid --samples".into()))
        })
        .transpose()?
        .unwrap_or(11);
    if !(2..=21).contains(&samples) {
        return Err(CliError("--samples must be 2..=21".into()));
    }
    match command.surface {
        SurfaceTarget::Dock | SurfaceTarget::Bar => {
            let from_mode = parse_geometry(from)?;
            let to_mode = parse_geometry(to)?;
            let from_visual = visual_for_geometry(from_mode)?;
            let to_visual = visual_for_geometry(to_mode)?;
            for index in 0..samples {
                let progress = NormalizedPermille::new(
                    ((u32::from(index) * 1000) / u32::from(samples - 1)) as u16,
                );
                let visual = interpolate_visual(from_visual, to_visual, progress);
                println!(
                    "TRANSITION_SAMPLE progress={} radius={} margin={} offset=({}, {})",
                    progress.value(),
                    visual.radius.0,
                    visual.margin.0,
                    visual.translation.x,
                    visual.translation.y
                );
            }
        }
        SurfaceTarget::AppStack => {
            let from_attachment = parse_attachment(from)?;
            let to_attachment = parse_attachment(to)?;
            let adjacent = matches!(
                (from_attachment, to_attachment),
                (Attachment::Fused, Attachment::Bridged)
                    | (Attachment::Bridged, Attachment::Fused)
                    | (Attachment::Bridged, Attachment::Detached)
                    | (Attachment::Detached, Attachment::Bridged)
            );
            if !adjacent {
                return Err(CliError(
                    "AppStack topology transition must use adjacent Fused↔Bridged↔Detached states"
                        .into(),
                ));
            }
            let from_neck = qualification_bridge(from_attachment).neck_width.0;
            let to_neck = qualification_bridge(to_attachment).neck_width.0;
            for index in 0..samples {
                let progress = NormalizedPermille::new(
                    ((u32::from(index) * 1000) / u32::from(samples - 1)) as u16,
                );
                let p = progress.value() as f32 / 1000.0;
                let neck = (from_neck as f32 + (to_neck as f32 - from_neck as f32) * p)
                    .round()
                    .max(0.0) as u32;
                println!(
                    "TOPOLOGY_SAMPLE progress={} from={from_attachment:?} to={to_attachment:?} neck_width={neck}",
                    progress.value()
                );
            }
            println!("FINAL_BRIDGE_ALGORITHM_AUTHORITY=OPEN");
        }
    }
    Ok(())
}

fn usage() {
    eprintln!(
        "usage: gnos-ux-envelope-cli <inspect|apply|attachment|transition> --surface <dock|bar|appstack> --output <qualification-id> [options]"
    );
}

fn main() {
    match parse_args(env::args().skip(1)).and_then(|command| execute(&command)) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("error: {error}");
            usage();
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn inspect_requires_typed_surface_and_output() {
        let parsed = parse_args(args(&[
            "inspect",
            "--surface",
            "dock",
            "--output",
            "HDMI-A-1",
        ]))
        .unwrap();
        assert_eq!(parsed.kind, SessionCommandKind::Inspect);
        assert_eq!(parsed.surface, SurfaceTarget::Dock);
        assert_eq!(parsed.output, RuntimeId::new("HDMI-A-1"));
    }

    #[test]
    fn missing_output_is_rejected() {
        let error = parse_args(args(&["inspect", "--surface", "bar"])).unwrap_err();
        assert!(error.0.contains("--output is required"));
    }

    #[test]
    fn attachment_is_typed_and_resolves_lineage() {
        let command = parse_args(args(&[
            "attachment",
            "--surface",
            "appstack",
            "--output",
            "DP-2",
            "--attachment",
            "bridged",
        ]))
        .unwrap();
        let resolved = resolve_session(&command).unwrap();
        let relation = resolved.derived_relation.unwrap();
        assert_eq!(relation.attachment, Attachment::Bridged);
        assert_eq!(
            relation.parent,
            RuntimeId::new("gnos.ux.dock.qualification")
        );
    }

    #[test]
    fn opacity_out_of_range_is_rejected() {
        let command = parse_args(args(&[
            "apply",
            "--surface",
            "dock",
            "--output",
            "HDMI-A-1",
            "--opacity",
            "101",
        ]))
        .unwrap();
        assert!(resolve_session(&command).is_err());
    }

    #[test]
    fn auto_endpoint_is_not_resolved_implicitly() {
        assert!(visual_for_geometry(GeometryMode::Auto).is_err());
    }
}
