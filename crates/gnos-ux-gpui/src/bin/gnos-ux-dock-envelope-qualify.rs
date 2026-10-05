#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

use std::{env, time::Duration};

use gnos_ux_gpui::geometry_lab::{
    DockGeometryFrame, GeometryDebugView, GeometryFrameHeader, GeometryRect, SurfaceViewport,
    pointer_region_diagnostic, resolve_dock_frame, viewport_from_size,
};
use gnos_ux_surface::envelope::{
    BehaviourDefaults, ChromePresentation, CompositorIntentDefaults, EnvelopeCapabilities,
    EnvelopeDefaults, GeometryMode, KeyboardIntent, LogicalPx, MaterialFamilyId, MaterialRecipe,
    OpacityPercent, OutputRelation, RevealTriggers, SpaceReservationIntent, SurfaceEdge,
    VisibilityPolicy, resolve_inheritance,
};
use gnos_ux_surface::pointer_regions::project_dock_pointer_regions;
use gpui::layer_shell::*;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, FontWeight, PathBuilder, Render, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, canvas, div, point, px, rgba,
};
use gpui_platform::application;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockAtlasState {
    Visible,
    Hidden,
}

impl DockAtlasState {
    fn label(self) -> &'static str {
        match self {
            Self::Visible => "VISIBLE",
            Self::Hidden => "HIDDEN",
        }
    }
}

struct DockQualification {
    state: DockAtlasState,
    visible_fraction: f32,
    geometry_lab: bool,
    geometry_view: GeometryDebugView,
    sample: u64,
    viewport: SurfaceViewport,
    compositor_revision: u64,
}

fn dock_model() -> gnos_ux_surface::envelope::ResolvedEnvelopeState {
    let defaults = EnvelopeDefaults {
        geometry: GeometryMode::Hug,
        relation: OutputRelation::EdgeBound,
        material: MaterialRecipe {
            family: MaterialFamilyId::new("qualification-dock-atlas"),
            blur: LogicalPx(0),
            border: LogicalPx(1),
            shadow: LogicalPx(0),
        },
        chrome: ChromePresentation {
            background_visible: false,
            opacity: OpacityPercent::new(100),
            border_visible: true,
            shadow_visible: false,
        },
        compositor: CompositorIntentDefaults {
            edge: SurfaceEdge::Bottom,
            keyboard: KeyboardIntent::None,
            reservation: SpaceReservationIntent::Overlay,
            margins: [LogicalPx(0); 4],
            output: None,
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

fn geometry_rect_path(
    bounds: Bounds<gpui::Pixels>,
    rect: GeometryRect,
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
    let radius = rect
        .radius()
        .min(rect.width().abs() * 0.5)
        .min(rect.height().abs() * 0.5);

    path.move_to(p(rect.left() + radius, rect.top()));
    path.line_to(p(rect.right() - radius, rect.top()));
    append_arc(&mut path, radius, p(rect.right(), rect.top() + radius));
    path.line_to(p(rect.right(), rect.bottom() - radius));
    append_arc(&mut path, radius, p(rect.right() - radius, rect.bottom()));
    path.line_to(p(rect.left() + radius, rect.bottom()));
    append_arc(&mut path, radius, p(rect.left(), rect.bottom() - radius));
    path.line_to(p(rect.left(), rect.top() + radius));
    append_arc(&mut path, radius, p(rect.left() + radius, rect.top()));
    path.close();
    path.build().expect("building Dock atlas Envelope")
}

#[derive(Clone, Debug)]
struct DockGeometryObservation {
    header: GeometryFrameHeader,
    state: DockAtlasState,
    frame: DockGeometryFrame,
}

impl DockGeometryObservation {
    fn log_json(&self) {
        let input = self
            .frame
            .input_regions
            .iter()
            .map(|rect| {
                format!(
                    "[{:.2},{:.2},{:.2},{:.2}]",
                    rect.left(),
                    rect.top(),
                    rect.right(),
                    rect.bottom()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            concat!(
                "DOCK_GEOMETRY_OBSERVATION {{",
                "{},",
                "\"state\":\"{}\",",
                "\"recipe\":\"{}\",\"recipe_revision\":{},",
                "\"visible_fraction\":{:.4},",
                "\"envelope\":[{:.2},{:.2},{:.2},{:.2}],",
                "\"visible_envelope\":[{:.2},{:.2},{:.2},{:.2}],",
                "\"radius\":{:.2},",
                "\"reveal\":[{:.2},{:.2},{:.2},{:.2}],",
                "\"input\":[{}]",
                "}}"
            ),
            self.header.json_fields(),
            self.state.label(),
            self.frame.recipe.id,
            self.frame.recipe.revision,
            self.frame.visible_fraction.value(),
            self.frame.envelope.left(),
            self.frame.envelope.top(),
            self.frame.envelope.right(),
            self.frame.envelope.bottom(),
            self.frame.visible_envelope.left(),
            self.frame.visible_envelope.top(),
            self.frame.visible_envelope.right(),
            self.frame.visible_envelope.bottom(),
            self.frame.visible_envelope.radius(),
            self.frame.reveal_region.left(),
            self.frame.reveal_region.top(),
            self.frame.reveal_region.right(),
            self.frame.reveal_region.bottom(),
            input,
        );
    }
}

impl DockQualification {
    fn new(cx: &mut Context<Self>) -> Self {
        let geometry_lab = env::args().any(|arg| arg == "--geometry-lab");
        let geometry_view = GeometryDebugView::from_process_args();
        let cycle = env::args().any(|arg| arg == "--dock-atlas-cycle");
        let requested_fraction = env::args()
            .find_map(|arg| {
                arg.strip_prefix("--dock-visible-fraction=")
                    .and_then(|value| value.parse::<f32>().ok())
            })
            .map(|value| value.clamp(0.0, 1.0));
        let state = if env::args().any(|arg| arg == "--state=hidden") {
            DockAtlasState::Hidden
        } else {
            DockAtlasState::Visible
        };
        let visible_fraction = requested_fraction.unwrap_or(match state {
            DockAtlasState::Visible => 1.0,
            DockAtlasState::Hidden => 0.0,
        });

        let resolved = dock_model();
        println!("resolved_dock_atlas={resolved:#?}");
        println!(
            "DOCK_ATLAS_AUTHORITY reservation=Overlay visibility=AutoHide geometry=Hug relation=EdgeBound"
        );

        if cycle {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(350))
                    .await;
                for (from, to) in [(1.0_f32, 0.0_f32), (0.0_f32, 1.0_f32)] {
                    println!("DOCK_ATLAS_MORPH_BEGIN from={from:.1} to={to:.1}");
                    for step in 0..=10_u16 {
                        let t = step as f32 / 10.0;
                        let fraction = from + (to - from) * t;
                        let _ = this.update(cx, |view, cx| {
                            view.visible_fraction = fraction;
                            view.state = if fraction <= 0.001 {
                                DockAtlasState::Hidden
                            } else {
                                DockAtlasState::Visible
                            };
                            println!(
                                "DOCK_ATLAS_MORPH_SAMPLE step={} visible_fraction={fraction:.4}",
                                step
                            );
                            cx.notify();
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(45))
                            .await;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(280))
                        .await;
                }
                println!("DOCK_ATLAS_MORPH_SEQUENCE_COMPLETE visible_fraction=1.0000");
            })
            .detach();
        }

        Self {
            state,
            visible_fraction,
            geometry_lab,
            geometry_view,
            sample: 0,
            viewport: SurfaceViewport::unit(),
            compositor_revision: 0,
        }
    }
}

impl Render for DockQualification {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let viewport =
            viewport_from_size(window.viewport_size()).unwrap_or_else(SurfaceViewport::unit);
        if !self.viewport.approx_eq(viewport) {
            let previous = self.viewport;
            self.viewport = viewport;
            self.compositor_revision = self.compositor_revision.saturating_add(1);
            println!(
                "DOCK_GEOMETRY_VIEWPORT_REBASE from={:.1}x{:.1} to={:.1}x{:.1}",
                previous.width, previous.height, viewport.width, viewport.height
            );
        }

        let frame = resolve_dock_frame(viewport, self.visible_fraction);
        let pointer_regions =
            project_dock_pointer_regions(&frame).expect("valid Dock pointer-region projection");
        let regions: Vec<_> = frame
            .input_regions
            .iter()
            .map(|region| gpui::Bounds {
                origin: point(px(region.left()), px(region.top())),
                size: gpui::Size::new(px(region.width().max(1.0)), px(region.height().max(1.0))),
            })
            .collect();
        window.set_input_region(Some(&regions));
        window.set_exclusive_zone(px(0.0));

        let configured =
            self.compositor_revision > 0 && viewport.width > 1.0 && viewport.height > 1.0;
        if configured {
            self.sample = self.sample.saturating_add(1);
        }
        let observation = DockGeometryObservation {
            header: GeometryFrameHeader::new(
                self.sample,
                "gnos.ux:dock:atlas-qualification",
                viewport,
                self.compositor_revision,
            ),
            state: self.state,
            frame: frame.clone(),
        };
        if self.geometry_lab && configured {
            observation.log_json();
            println!(
                "{}",
                pointer_region_diagnostic(
                    "gnos.ux:dock:atlas-qualification",
                    self.compositor_revision,
                    &pointer_regions,
                )
            );
        }

        let geometry_lab = self.geometry_lab;
        let geometry_view = self.geometry_view;
        let primary = rgba(0xe6edf2ff);
        let secondary = rgba(0x91a0aaff);
        let faint = rgba(0x66747e88);
        let failure = rgba(0xe66a6aff);

        let canvas = canvas(
            move |bounds, _, _| (bounds, observation),
            move |bounds, (_, observation), window, _| {
                let frame = &observation.frame;
                if frame.visible_fraction.value() > 0.001 {
                    window.paint_path(
                        geometry_rect_path(bounds, frame.visible_envelope, true),
                        primary,
                    );
                }

                if geometry_lab {
                    if geometry_view.includes(GeometryDebugView::Hull) {
                        window.paint_path(geometry_rect_path(bounds, frame.envelope, true), faint);
                    }
                    if geometry_view.includes(GeometryDebugView::Constraints) {
                        window.paint_path(
                            geometry_rect_path(bounds, frame.reveal_region, true),
                            secondary,
                        );
                    }
                    if geometry_view.includes(GeometryDebugView::Content) {
                        for region in &frame.input_regions {
                            window.paint_path(geometry_rect_path(bounds, *region, true), faint);
                        }
                    }
                    if frame.visible_fraction.value() <= 0.001 && frame.input_regions.len() != 1 {
                        window.paint_path(
                            geometry_rect_path(bounds, frame.reveal_region, true),
                            failure,
                        );
                    }
                }
            },
        )
        .size_full();

        let mut root = div().relative().size_full().child(canvas);
        if self.geometry_lab {
            root = root.child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(8.0))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(primary)
                    .child(format!(
                        "DOCK {} sample={} viewport={:.0}x{:.0} visible={:.2} envelope=({:.1},{:.1})-({:.1},{:.1}) reveal_h={:.1} input_regions={}",
                        self.state.label(),
                        self.sample,
                        viewport.width,
                        viewport.height,
                        frame.visible_fraction.value(),
                        frame.envelope.left(),
                        frame.envelope.top(),
                        frame.envelope.right(),
                        frame.envelope.bottom(),
                        frame.reveal_region.height(),
                        frame.input_regions.len(),
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
            gpui::Size::new(px(0.0), px(0.0))
        } else {
            gpui::Size::new(px(900.0), px(560.0))
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
                    origin: point(px(0.0), px(0.0)),
                    size: initial_size,
                })),
                app_id: Some("gnos.ux.dock.envelope-atlas-qualification".into()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: "gnos.ux:dock:atlas-qualification".into(),
                    layer: Layer::Top,
                    anchor,
                    exclusive_zone: Some(px(0.0)),
                    keyboard_interactivity: KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(DockQualification::new),
        )
        .expect("opening Dock atlas qualification surface");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_dock_envelope_and_input_share_the_same_frame() {
        let frame = resolve_dock_frame(
            SurfaceViewport::try_new(1200.0, 720.0).expect("viewport"),
            1.0,
        );
        assert_eq!(frame.visible_envelope, frame.envelope);
        assert_eq!(frame.input_regions[0], frame.visible_envelope);
        assert_eq!(frame.input_regions[1], frame.reveal_region);
        let projected = project_dock_pointer_regions(&frame).expect("pointer-region projection");
        let visible = projected
            .regions()
            .iter()
            .find(|region| region.id().as_str() == "dock.input.visible")
            .expect("visible declaration");
        let reveal = projected
            .regions()
            .iter()
            .find(|region| region.id().as_str() == "dock.input.reveal")
            .expect("reveal declaration");
        assert_eq!(visible.bounds(), frame.visible_envelope);
        assert_eq!(reveal.bounds(), frame.reveal_region);
        assert!(frame.validate().is_valid());
    }

    #[test]
    fn hidden_dock_retains_only_reveal_input() {
        let frame = resolve_dock_frame(
            SurfaceViewport::try_new(1200.0, 720.0).expect("viewport"),
            0.0,
        );
        assert_eq!(frame.input_regions, vec![frame.reveal_region]);
        let projected = project_dock_pointer_regions(&frame).expect("pointer-region projection");
        assert_eq!(
            projected.surface_local_rects().collect::<Vec<_>>(),
            frame.input_regions
        );
        assert_eq!(projected.regions()[0].id().as_str(), "dock.input.reveal");
        assert!(!projected.regions()[0].decoratable());
        assert!(frame.visible_envelope.height().abs() < 0.01);
        assert!(frame.validate().is_valid());
    }

    #[test]
    fn dock_geometry_reflows_without_reference_width() {
        let wide = resolve_dock_frame(
            SurfaceViewport::try_new(1200.0, 720.0).expect("viewport"),
            1.0,
        );
        let narrow = resolve_dock_frame(
            SurfaceViewport::try_new(900.0, 560.0).expect("viewport"),
            1.0,
        );
        assert!((wide.envelope.left() / 1200.0 - narrow.envelope.left() / 900.0).abs() < 0.001);
        assert!((wide.envelope.right() / 1200.0 - narrow.envelope.right() / 900.0).abs() < 0.001);
        assert!((wide.envelope.top() / 720.0 - narrow.envelope.top() / 560.0).abs() < 0.001);
    }
}
