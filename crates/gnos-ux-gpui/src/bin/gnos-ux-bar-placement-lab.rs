//! Development-only source-bound cardinal Bar fixture. No live providers/execution.
use gnos_ux_gpui::bar_placement_lab::{BarPlacementLab, FixtureState};
use gnos_ux_surface::envelope::SurfaceEdge;
use gpui::{
    App, AppContext, Bounds, WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size,
};
fn main() {
    let mut edge = SurfaceEdge::Top;
    let mut state = FixtureState::Closed;
    let mut width = 1280.;
    let mut height = 800.;
    let mut ack = None;
    let mut transition_source = None;
    let mut transition_progress = None;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--edge=") {
            edge = match v {
                "top" => SurfaceEdge::Top,
                "right" => SurfaceEdge::Right,
                "bottom" => SurfaceEdge::Bottom,
                "left" => SurfaceEdge::Left,
                _ => {
                    eprintln!("invalid edge");
                    std::process::exit(2)
                }
            };
        } else if let Some(v) = arg.strip_prefix("--state=") {
            state = match v {
                "closed" => FixtureState::Closed,
                "workspace" => FixtureState::Workspace,
                "appstack" => FixtureState::AppStack,
                _ => {
                    eprintln!("invalid state");
                    std::process::exit(2)
                }
            };
        } else if let Some(v) = arg.strip_prefix("--width=") {
            width = v.parse::<f32>().expect("width");
        } else if let Some(v) = arg.strip_prefix("--height=") {
            height = v.parse::<f32>().expect("height");
        } else if let Some(v) = arg.strip_prefix("--ack=") {
            ack = Some(std::path::PathBuf::from(v));
        } else if let Some(v) = arg.strip_prefix("--transition-source=") {
            transition_source = Some(parse_edge(v));
        } else if let Some(v) = arg.strip_prefix("--progress=") {
            transition_progress = Some(v.parse::<f32>().expect("finite transition progress"));
        } else {
            eprintln!("invalid argument {arg}");
            std::process::exit(2);
        }
    }
    gnos_ux_surface::grammar::SurfaceViewport::try_new(width, height)
        .expect("positive finite viewport");
    let transition = match (transition_source, transition_progress) {
        (None, None) => None,
        (Some(source), Some(progress)) => Some((
            source,
            gnos_ux_surface::grammar::UnitInterval::try_new(progress).expect("progress in 0..=1"),
        )),
        _ => {
            eprintln!("transition-source and progress must be supplied together");
            std::process::exit(2);
        }
    };
    gpui_platform::application().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(width), px(height)),
                    cx,
                ))),
                app_id: Some("gnos.ux.bar.placement.lab".into()),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|cx| {
                    if let Some((source, progress)) = transition {
                        BarPlacementLab::new_transition(source, edge, progress, state, ack, cx)
                    } else {
                        BarPlacementLab::new(edge, state, ack, cx)
                    }
                })
            },
        )
        .expect("isolated fixture window");
    });
}

fn parse_edge(value: &str) -> SurfaceEdge {
    match value {
        "top" => SurfaceEdge::Top,
        "right" => SurfaceEdge::Right,
        "bottom" => SurfaceEdge::Bottom,
        "left" => SurfaceEdge::Left,
        _ => {
            eprintln!("invalid edge {value}");
            std::process::exit(2);
        }
    }
}
