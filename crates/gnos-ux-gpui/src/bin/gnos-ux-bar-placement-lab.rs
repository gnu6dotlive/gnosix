//! Development-only source-bound cardinal Bar fixture. No execution; live
//! providers only behind `--reference-status-live` (read-only observers).
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
    let mut reference_idle = false;
    let mut active_window_case = gnos_ux_gpui::reference_bar::ActiveWindowCase::Live;
    let mut tray_fixture_root = None;
    let mut reference_status = gnos_ux_gpui::bar_placement_lab::ReferenceStatusRequest::Frozen;
    let mut native_shell = false;
    let mut transparency = gnos_ux_gpui::product_shell_material::Transparency::OBSERVED;
    let mut wallpaper_luminance = 0.0_f32;
    let mut shell_light = false;
    let mut shell_locator = false;
    let mut shell_bar_present = true;
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
        } else if arg == "--reference-bar-idle" {
            reference_idle = true;
        } else if let Some(v) = arg.strip_prefix("--active-window-case=") {
            active_window_case = gnos_ux_gpui::reference_bar::ActiveWindowCase::parse(v)
                .unwrap_or_else(|| {
                    eprintln!("invalid active-window case");
                    std::process::exit(2)
                });
        } else if let Some(v) = arg.strip_prefix("--reference-status-case=") {
            reference_status = gnos_ux_gpui::bar_placement_lab::ReferenceStatusRequest::Case(
                gnos_ux_gpui::reference_status_fixture::StatusCase::parse(v).unwrap_or_else(|| {
                    eprintln!("invalid status case");
                    std::process::exit(2)
                }),
            );
        } else if arg == "--shell-material=native" {
            native_shell = true;
        } else if let Some(v) = arg.strip_prefix("--shell-wallpaper-luminance=") {
            wallpaper_luminance = v.parse::<f32>().expect("finite luminance");
        } else if let Some(v) = arg.strip_prefix("--shell-transparency=") {
            transparency.enabled = match v {
                "on" => true,
                "off" => false,
                _ => {
                    eprintln!("invalid shell transparency");
                    std::process::exit(2)
                }
            };
        } else if let Some(v) = arg.strip_prefix("--shell-transparency-base=") {
            transparency.base = v.parse::<f32>().expect("finite alpha");
        } else if arg == "--shell-light" {
            shell_light = true;
        } else if arg == "--shell-bar=none" {
            shell_bar_present = false;
        } else if arg == "--shell-locator" {
            shell_locator = true;
        } else if arg == "--reference-status-live" {
            reference_status = gnos_ux_gpui::bar_placement_lab::ReferenceStatusRequest::Live;
        } else if let Some(v) = arg.strip_prefix("--reference-tray-fixture-root=") {
            tray_fixture_root = Some(std::path::PathBuf::from(v));
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
    if reference_idle && transition.is_some() {
        eprintln!("reference-bar-idle is a frozen endpoint and accepts no transition");
        std::process::exit(2);
    }
    if tray_fixture_root.is_some() && !reference_idle {
        eprintln!("reference-tray-fixture-root requires --reference-bar-idle");
        std::process::exit(2);
    }
    if native_shell && !reference_idle {
        eprintln!("shell-material=native requires --reference-bar-idle");
        std::process::exit(2);
    }
    let shell_material = if native_shell {
        gnos_ux_gpui::bar_placement_lab::ShellMaterialRequest::Native(
            gnos_ux_gpui::bar_placement_lab::NativeShell {
                transparency,
                wallpaper_luminance,
                light: shell_light,
                locator: shell_locator,
                bar_present: shell_bar_present,
            },
        )
    } else {
        gnos_ux_gpui::bar_placement_lab::ShellMaterialRequest::OpaqueComposite
    };
    if reference_idle {
        edge = SurfaceEdge::Left;
        state = FixtureState::Closed;
    }
    gpui_platform::application().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(width), px(height)),
                    cx,
                ))),
                app_id: Some("gnos.ux.bar.placement.lab".into()),
                window_background: if native_shell {
                    WindowBackgroundAppearance::Transparent
                } else {
                    WindowBackgroundAppearance::Opaque
                },
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|cx| {
                    if reference_idle {
                        BarPlacementLab::new_reference_idle(
                            ack,
                            active_window_case,
                            tray_fixture_root,
                            reference_status,
                            shell_material,
                            cx,
                        )
                    } else if let Some((source, progress)) = transition {
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
