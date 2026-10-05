#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

use std::{env, time::Duration};

use gnos_ux_gpui::geometry_lab::{
    BarGeometryFrame, BarGeometryRecipe, GeometryDebugView, GeometryFrameHeader,
    SurfaceGeometryRecipe, SurfaceViewport, pointer_region_diagnostic, viewport_from_size,
};
use gnos_ux_surface::envelope::{
    BehaviourDefaults, ChromePresentation, CompositorIntentDefaults, EnvelopeCapabilities,
    EnvelopeDefaults, GeometryMode, KeyboardIntent, LogicalOffset, LogicalPx, MaterialFamilyId,
    MaterialRecipe, NormalizedPermille, OpacityPercent, OutputRelation, RevealTriggers, RuntimeId,
    SpaceReservationIntent, SurfaceEdge, TopologyPhase, TransitionState, VisibilityPolicy,
    VisualState, interpolate_visual, resolve_inheritance,
};
use gnos_ux_surface::pointer_regions::project_bar_pointer_regions;
use gpui::layer_shell::*;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, FontWeight, PathBuilder, Render, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, canvas, div, point, px, rgba,
};
use gpui_platform::application;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarState {
    Standard,
    Hug,
}

struct BarQualification {
    state: BarState,
    resolved: gnos_ux_surface::envelope::ResolvedEnvelopeState,
    transition: Option<TransitionState>,
    geometry_lab: bool,
    geometry_view: GeometryDebugView,
    sample: u64,
    viewport: SurfaceViewport,
    compositor_revision: u64,
}

fn resolve_bar_frame(viewport: SurfaceViewport, visual: VisualState) -> BarGeometryFrame {
    BarGeometryRecipe::qualifier_baseline()
        .resolve(viewport, visual)
        .expect("validated Bar geometry recipe")
}

fn standard_visual() -> VisualState {
    VisualState {
        opacity: OpacityPercent::new(92),
        radius: LogicalPx(0),
        margin: LogicalPx(0),
        translation: LogicalOffset { x: 0, y: 0 },
        bridge_width: LogicalPx(0),
        bridge_neck_width: LogicalPx(0),
    }
}

fn hug_visual() -> VisualState {
    VisualState {
        opacity: OpacityPercent::new(92),
        radius: LogicalPx(24),
        margin: LogicalPx(48),
        translation: LogicalOffset { x: 0, y: 18 },
        bridge_width: LogicalPx(0),
        bridge_neck_width: LogicalPx(0),
    }
}

fn bar_model(
    state: BarState,
    reservation: SpaceReservationIntent,
) -> gnos_ux_surface::envelope::ResolvedEnvelopeState {
    let defaults = EnvelopeDefaults {
        geometry: match state {
            BarState::Standard => GeometryMode::Standard,
            BarState::Hug => GeometryMode::Hug,
        },
        relation: match state {
            BarState::Standard => OutputRelation::EdgeBound,
            BarState::Hug => OutputRelation::Inset,
        },
        material: MaterialRecipe {
            family: MaterialFamilyId::new("qualification-bar"),
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
            edge: SurfaceEdge::Top,
            keyboard: KeyboardIntent::None,
            reservation,
            margins: [LogicalPx(0); 4],
            output: None,
        },
        behaviour: BehaviourDefaults {
            visibility: VisibilityPolicy::AlwaysVisible,
            triggers: RevealTriggers::default(),
        },
        capabilities: EnvelopeCapabilities {
            extrusion: false,
            connective_hull: false,
            bridge: false,
            shape_aware_input: true,
        },
    };
    resolve_inheritance(
        &defaults,
        &Default::default(),
        &Default::default(),
        None,
        None,
    )
}

fn append_arc(path: &mut PathBuilder, radius: f32, to: gpui::Point<gpui::Pixels>) {
    if radius <= 0.01 {
        path.line_to(to);
        return;
    }
    path.arc_to(point(px(radius), px(radius)), px(0.0), false, true, to);
}

fn frame_path(
    bounds: Bounds<gpui::Pixels>,
    frame: &BarGeometryFrame,
    stroke: bool,
) -> gpui::Path<gpui::Pixels> {
    let mut path = if stroke {
        PathBuilder::stroke(px(1.0))
    } else {
        PathBuilder::fill()
    };
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let rect = frame.envelope;
    let l = rect.left();
    let t = rect.top();
    let r = rect.right();
    let b = rect.bottom();
    let radius = rect.radius();
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));

    path.move_to(p(l + radius, t));
    path.line_to(p(r - radius, t));
    append_arc(&mut path, radius, p(r, t + radius));
    path.line_to(p(r, b - radius));
    append_arc(&mut path, radius, p(r - radius, b));
    path.line_to(p(l + radius, b));
    append_arc(&mut path, radius, p(l, b - radius));
    path.line_to(p(l, t + radius));
    append_arc(&mut path, radius, p(l + radius, t));
    path.close();
    path.build().expect("building Bar qualification hull")
}

#[derive(Clone, Debug)]
struct BarGeometryObservation {
    header: GeometryFrameHeader,
    state: BarState,
    progress_permille: u16,
    frame: BarGeometryFrame,
    reservation_zone: u32,
}

impl BarGeometryObservation {
    fn log_json(&self) {
        println!(
            "BAR_GEOMETRY_OBSERVATION {{{},\"state\":\"{:?}\",\"recipe\":\"{}\",\"recipe_revision\":{},\"progress_permille\":{},\"frame\":[{:.2},{:.2},{:.2},{:.2}],\"radius\":{:.2},\"reservation_zone\":{}}}",
            self.header.json_fields(),
            self.state,
            self.frame.recipe.id,
            self.frame.recipe.revision,
            self.progress_permille,
            self.frame.envelope.left(),
            self.frame.envelope.top(),
            self.frame.envelope.width(),
            self.frame.envelope.height(),
            self.frame.envelope.radius(),
            self.reservation_zone,
        );
    }
}

fn debug_rect_path(
    bounds: Bounds<gpui::Pixels>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> gpui::Path<gpui::Pixels> {
    let mut path = PathBuilder::stroke(px(1.0));
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
    path.move_to(p(x, y));
    path.line_to(p(x + width, y));
    path.line_to(p(x + width, y + height));
    path.line_to(p(x, y + height));
    path.close();
    path.build().expect("building Bar debug bounds")
}

fn debug_line_path(
    bounds: Bounds<gpui::Pixels>,
    from: (f32, f32),
    to: (f32, f32),
) -> gpui::Path<gpui::Pixels> {
    let mut path = PathBuilder::stroke(px(1.0));
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    path.move_to(point(px(ox + from.0), px(oy + from.1)));
    path.line_to(point(px(ox + to.0), px(oy + to.1)));
    path.build().expect("building Bar debug line")
}

impl BarQualification {
    fn new(cx: &mut Context<Self>) -> Self {
        let cycle = env::args().any(|arg| arg == "--morph-cycle");
        let reservation_cycle = env::args().any(|arg| arg == "--reservation-cycle");
        let geometry_lab = env::args().any(|arg| arg == "--geometry-lab");
        let geometry_view = GeometryDebugView::from_process_args();
        let qualification_output = env::args().find_map(|arg| {
            arg.strip_prefix("--qualification-output=")
                .map(RuntimeId::new)
        });
        let morph_progress = env::args()
            .find_map(|arg| {
                arg.strip_prefix("--morph-progress=")
                    .and_then(|value| value.parse::<u16>().ok())
            })
            .map(|value| value.min(1000));
        let qualification_reserve = env::args().find_map(|arg| {
            arg.strip_prefix("--qualification-reserve=")
                .and_then(|value| value.parse::<u32>().ok())
        });
        let state = if cycle || reservation_cycle || morph_progress.is_some() {
            BarState::Standard
        } else if env::args().any(|arg| arg == "--state=hug") {
            BarState::Hug
        } else {
            BarState::Standard
        };
        let initial_transition = morph_progress.map(|value| TransitionState {
            from: standard_visual(),
            to: hug_visual(),
            progress: NormalizedPermille::new(value),
            topology: TopologyPhase::Stable,
        });
        if cycle {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                for (from_state, to_state, from, to) in [
                    (BarState::Standard, BarState::Hug, standard_visual(), hug_visual()),
                    (BarState::Hug, BarState::Standard, hug_visual(), standard_visual()),
                ] {
                    println!("BAR_MORPH_BEGIN from={from_state:?} to={to_state:?}");
                    for step in 0..=10_u16 {
                        let progress = NormalizedPermille::new(step * 100);
                        let _ = this.update(cx, |view, cx| {
                            view.transition = Some(TransitionState {
                                from,
                                to,
                                progress,
                                topology: TopologyPhase::Stable,
                            });
                            let visual = interpolate_visual(from, to, progress);
                            let frame = resolve_bar_frame(view.viewport, visual);
                            println!(
                                "BAR_MORPH_SAMPLE from={from_state:?} to={to_state:?} progress={} x={} y={} width={} height={} radius={}",
                                progress.value(),
                                frame.envelope.left(),
                                frame.envelope.top(),
                                frame.envelope.width(),
                                frame.envelope.height(),
                                frame.envelope.radius()
                            );
                            cx.notify();
                        });
                        cx.background_executor().timer(Duration::from_millis(30)).await;
                    }
                    let _ = this.update(cx, |view, cx| {
                        view.state = to_state;
                        view.resolved = bar_model(to_state, SpaceReservationIntent::Overlay);
                        view.transition = None;
                        println!("BAR_MORPH_COMPLETE state={to_state:?}");
                        cx.notify();
                    });
                    cx.background_executor().timer(Duration::from_millis(220)).await;
                }
                println!("BAR_MORPH_SEQUENCE_COMPLETE state=Standard");
            }).detach();
        }
        if reservation_cycle {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.resolved = bar_model(
                        BarState::Standard,
                        SpaceReservationIntent::Reserve(LogicalPx(44)),
                    );
                    println!("RESERVATION_STATE intent=Reserve(44)");
                    cx.notify();
                });
                cx.background_executor()
                    .timer(Duration::from_millis(1400))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.resolved = bar_model(BarState::Standard, SpaceReservationIntent::Overlay);
                    println!("RESERVATION_STATE intent=Overlay");
                    cx.notify();
                });
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                println!("RESERVATION_SEQUENCE_COMPLETE intent=Overlay");
            })
            .detach();
        }
        let initial_reservation = qualification_reserve
            .map(|value| SpaceReservationIntent::Reserve(LogicalPx(value)))
            .unwrap_or(SpaceReservationIntent::Overlay);
        let mut resolved = bar_model(state, initial_reservation);
        resolved.compositor.output = qualification_output.clone();
        println!(
            "QUALIFICATION_REQUESTED_OUTPUT={}",
            qualification_output
                .as_ref()
                .map(RuntimeId::as_str)
                .unwrap_or("UNSPECIFIED")
        );
        println!("resolved_bar={resolved:#?}");
        Self {
            state,
            resolved,
            transition: initial_transition,
            geometry_lab,
            geometry_view,
            sample: 0,
            viewport: SurfaceViewport::unit(),
            compositor_revision: 0,
        }
    }
}

impl Render for BarQualification {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let viewport =
            viewport_from_size(window.viewport_size()).unwrap_or_else(SurfaceViewport::unit);
        if !self.viewport.approx_eq(viewport) {
            let previous = self.viewport;
            self.viewport = viewport;
            self.compositor_revision = self.compositor_revision.saturating_add(1);
            println!(
                "BAR_GEOMETRY_VIEWPORT_REBASE from={:.1}x{:.1} to={:.1}x{:.1}",
                previous.width, previous.height, viewport.width, viewport.height
            );
        }

        let visual = self
            .transition
            .map(|transition| {
                interpolate_visual(transition.from, transition.to, transition.progress)
            })
            .unwrap_or_else(|| match self.state {
                BarState::Standard => standard_visual(),
                BarState::Hug => hug_visual(),
            });
        let progress_permille = self
            .transition
            .map(|transition| transition.progress.value())
            .unwrap_or(1000);
        let frame = resolve_bar_frame(viewport, visual);
        let pointer_regions =
            project_bar_pointer_regions(&frame).expect("valid Bar pointer-region projection");
        let reservation_zone = match self.resolved.compositor.reservation {
            SpaceReservationIntent::Overlay => 0,
            SpaceReservationIntent::Reserve(LogicalPx(value)) => value,
            SpaceReservationIntent::Auto => {
                panic!("qualification Bar must not implicitly resolve Auto reservation")
            }
        };

        window.set_exclusive_edge(Anchor::TOP);
        window.set_exclusive_zone(px(reservation_zone as f32));
        let regions: Vec<_> = frame
            .input_regions
            .iter()
            .map(|region| gpui::Bounds {
                origin: point(px(region.left()), px(region.top())),
                size: gpui::Size::new(px(region.width()), px(region.height())),
            })
            .collect();
        window.set_input_region(Some(&regions));

        let configured = self.compositor_revision > 0;
        if configured {
            self.sample = self.sample.saturating_add(1);
        }
        let observation = BarGeometryObservation {
            header: GeometryFrameHeader::new(
                self.sample,
                "gnos.ux:bar:qualification",
                viewport,
                self.compositor_revision,
            ),
            state: self.state,
            progress_permille,
            frame: frame.clone(),
            reservation_zone,
        };
        if self.geometry_lab && configured {
            observation.log_json();
            println!(
                "{}",
                pointer_region_diagnostic(
                    "gnos.ux:bar:qualification",
                    self.compositor_revision,
                    &pointer_regions,
                )
            );
        }

        let geometry_lab = self.geometry_lab;
        let geometry_view = self.geometry_view;
        let fill = rgba(0x172a3aff);
        let border = rgba(0xd9e3e8ff);
        let faint = rgba(0x6f7b82aa);
        let opacity = visual.opacity.value() as f32 / 100.0;
        let bar_canvas = canvas(
            move |bounds, _, _| (bounds, observation),
            move |bounds, (_, observation), window, _| {
                if geometry_lab {
                    if geometry_view.includes(GeometryDebugView::Hull) {
                        window.paint_path(frame_path(bounds, &observation.frame, true), border);
                    }
                    if geometry_view.includes(GeometryDebugView::Content) {
                        window.paint_path(
                            debug_rect_path(
                                bounds,
                                observation.frame.envelope.left(),
                                observation.frame.envelope.top(),
                                observation.frame.envelope.width(),
                                observation.frame.envelope.height(),
                            ),
                            faint,
                        );
                    }
                    if geometry_view.includes(GeometryDebugView::Constraints)
                        && observation.reservation_zone > 0
                    {
                        window.paint_path(
                            debug_line_path(
                                bounds,
                                (0.0, observation.reservation_zone as f32),
                                (
                                    observation.header.viewport.width,
                                    observation.reservation_zone as f32,
                                ),
                            ),
                            faint,
                        );
                    }
                } else {
                    window.paint_path(frame_path(bounds, &observation.frame, false), fill);
                    window.paint_path(frame_path(bounds, &observation.frame, true), border);
                }
            },
        )
        .size_full()
        .opacity(if geometry_lab { 1.0 } else { opacity });

        let mut root = div().relative().size_full().child(bar_canvas);
        if self.geometry_lab {
            root = root.child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(8.0))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(border)
                    .child(format!(
                        "BAR {:?} sample={} viewport={:.0}x{:.0} frame=({:.1},{:.1}) {:.1}x{:.1} r={:.1} reserve={}",
                        self.state,
                        self.sample,
                        viewport.width,
                        viewport.height,
                        frame.envelope.left(),
                        frame.envelope.top(),
                        frame.envelope.width(),
                        frame.envelope.height(),
                        frame.envelope.radius(),
                        reservation_zone,
                    )),
            );
        }
        root
    }
}

fn main() {
    let geometry_lab = env::args().any(|arg| arg == "--geometry-lab");
    application().run(move |cx: &mut App| {
        let initial_size = if geometry_lab {
            gpui::Size::new(px(0.0), px(150.0))
        } else {
            gpui::Size::new(px(900.0), px(150.0))
        };
        let anchor = if geometry_lab {
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
        } else {
            Anchor::TOP
        };
        cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: initial_size,
                })),
                app_id: Some("gnos.ux.bar.envelope-qualification".into()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: "gnos.ux:bar:qualification".into(),
                    layer: Layer::Top,
                    anchor,
                    exclusive_zone: Some(px(0.0)),
                    exclusive_edge: Some(Anchor::TOP),
                    keyboard_interactivity: KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(BarQualification::new),
        )
        .expect("opening Bar qualification surface");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_frame_tracks_live_viewport_width_without_reference_resolution() {
        let standard = resolve_bar_frame(
            SurfaceViewport::try_new(1200.0, 150.0).expect("viewport"),
            standard_visual(),
        );
        let resized = resolve_bar_frame(
            SurfaceViewport::try_new(900.0, 150.0).expect("viewport"),
            standard_visual(),
        );

        assert_eq!(standard.envelope.left(), 0.0);
        assert_eq!(standard.envelope.width(), 1200.0);
        assert_eq!(resized.envelope.left(), 0.0);
        assert_eq!(resized.envelope.width(), 900.0);
        assert!(standard.validate().is_valid());
        assert!(resized.validate().is_valid());
    }

    #[test]
    fn hug_geometry_and_input_share_one_recipe_frame() {
        let viewport = SurfaceViewport::try_new(1000.0, 150.0).expect("viewport");
        let frame = resolve_bar_frame(viewport, hug_visual());

        assert_eq!(frame.input_regions, vec![frame.envelope]);
        let projected = project_bar_pointer_regions(&frame).expect("pointer-region projection");
        assert_eq!(
            projected.surface_local_rects().collect::<Vec<_>>(),
            frame.input_regions
        );
        assert!(frame.envelope.radius() > 0.0);
        assert!(frame.envelope.width() < viewport.width);
        assert!(frame.validate().is_valid());
    }

    #[test]
    fn telemetry_header_is_shared_with_reactive_geometry_lab() {
        let header = GeometryFrameHeader::new(
            9,
            "gnos.ux:bar:qualification",
            SurfaceViewport {
                width: 900.0,
                height: 150.0,
            },
            2,
        );
        assert_eq!(header.namespace, "gnos.ux:bar:qualification");
        assert!(
            header
                .json_fields()
                .contains("\"viewport\":[900.00,150.00]")
        );
    }
}
