//! Qualification surface for the generic connective-hull input projection.
//!
//! This is a bounded, read-only qualification surface, not product logic. It
//! consumes the *production* consumer value - `ProductLayout::frame()` and its
//! `input_regions`, the exact vector the scene hands to
//! `Window::set_input_region` - so a live run observes what the product would
//! submit to the compositor rather than a reimplementation of it.
//!
//! The debt it qualifies (`GENERIC_CONNECTIVE_HULL_INPUT_PROJECTION`) is that
//! the previous projection accepted input outside the painted hull at a strip
//! edge. The discriminating evidence is a point that the old projection covered
//! and the corrected one rejects (`old \ new`), sitting on a transparent pixel,
//! next to a counter-witness inside the paint. The surface logs pointer motion
//! classified against both the production input regions and the painted hull,
//! so a physical click at each point can be read as accepted / passed through.
//!
//! It is compiled only for the `qualification-surfaces` build and never runs
//! unless `GNOS_UX_QUALIFY_HULL_INPUT` is set.

use gnos_ux_surface::{
    grammar::{
        BarReservationPolicy, BarSensingPolicy, ConnectiveHullGeometry, GeometryRect,
        SurfaceViewport, connective_hull::hull_probe, reactive_envelope::ReactiveEnvelopeSolver,
    },
    scene::{MonitorSceneId, SceneEpoch},
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, DispatchPhase, MouseDownEvent, MouseExitEvent, MouseMoveEvent,
    PathBuilder, Render, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, canvas, div, point, px, rgba,
};

use crate::product_scene::ProductLayout;

/// One discriminating point the operator can click.
#[derive(Clone, Debug)]
pub struct HullInputPoint {
    pub id: &'static str,
    pub x: f32,
    pub y: f32,
    /// Whether the shipped input projection accepts the point.
    pub in_new_region: bool,
    /// Whether the previous three-sample projection would have accepted it.
    pub in_old_region: bool,
    /// Whether the painted hull covers the point.
    pub in_paint: bool,
}

/// The measured qualification state for one settled product frame.
#[derive(Clone, Debug)]
pub struct HullInputReport {
    /// The exact vector the scene hands to `set_input_region` (consumer truth).
    pub input_regions: Vec<GeometryRect>,
    /// The generic connective-hull contribution inside that vector.
    pub hull_regions: Vec<GeometryRect>,
    /// The painted hull itself, for the faithful surface fill.
    pub hull: ConnectiveHullGeometry,
    /// The previous projection's output, for the regression boundary.
    pub old_regions: Vec<GeometryRect>,
    pub points: Vec<HullInputPoint>,
}

fn contains(region: GeometryRect, x: f32, y: f32) -> bool {
    x >= region.left() && x <= region.right() && y >= region.top() && y <= region.bottom()
}

fn in_regions(regions: &[GeometryRect], x: f32, y: f32) -> bool {
    regions.iter().any(|region| contains(*region, x, y))
}

/// The previous projection, reproduced exactly, for the `old \ new` set.
///
/// This is the three-sample interior scan and min/max span merge that the
/// correction replaced. It exists only here, to name the regression boundary;
/// it is not used by any product path.
fn legacy_input_regions(
    hull: gnos_ux_surface::grammar::ConnectiveHullGeometry,
    band_height: f32,
) -> Vec<GeometryRect> {
    let step = band_height.clamp(1.0, 8.0);
    let contours = hull_probe::contours(hull);
    let mut regions = Vec::new();
    let mut top = hull.bounds().top();
    while top < hull.bounds().bottom() {
        let bottom = (top + step).min(hull.bounds().bottom());
        for contour in &contours {
            let ys = [top + 0.25, (top + bottom) * 0.5, bottom - 0.25];
            let mut left = f32::NEG_INFINITY;
            let mut right = f32::INFINITY;
            let mut empty = false;
            for y in ys {
                let spans = hull_probe::filled_intervals_at(contour, y);
                if spans.is_empty() {
                    empty = true;
                    break;
                }
                left = left.max(spans[0].0);
                right = right.min(spans[spans.len() - 1].1);
            }
            if !empty
                && right > left + 1.0
                && let Ok(region) = GeometryRect::try_new(left + 0.5, top, right - 0.5, bottom, 0.0)
            {
                regions.push(region);
            }
        }
        top = bottom;
    }
    regions
}

/// The production text size at the default scale (`ProductTextScale::default`).
const PRODUCT_FONT_PX: f32 = 14.0;

/// Resolve the settled product frame and derive the discriminating points.
///
/// The reactive (generic connective-hull) surface is the one this debt covers;
/// the AppStack C surface no longer consumes this hull, so the qualification
/// deliberately targets `layout.frame().input_regions` on the workspace body.
///
/// The solve uses the same entry point and inputs the production scene uses for
/// its settled state - `resolve_reactive_at_edge_with_policies` at the product
/// bar edge, default text size, visible bar and no hover sensing - so the hull
/// is the one production builds, not the library default the older helper picks.
pub fn qualify(viewport: SurfaceViewport) -> Option<HullInputReport> {
    let mut solver: Option<ReactiveEnvelopeSolver> = None;
    let solve = |solver: &mut Option<ReactiveEnvelopeSolver>| {
        ProductLayout::resolve_reactive_at_edge_with_policies(
            viewport,
            solver,
            true,
            false,
            None,
            gnos_ux_surface::envelope::SurfaceEdge::Top,
            PRODUCT_FONT_PX,
            BarReservationPolicy::ReserveOccupiedExtent,
            BarSensingPolicy::None,
        )
    };
    let mut layout = solve(&mut solver)?;
    // Settle to rest so the measured frame is deterministic.
    for _ in 0..400 {
        layout = solve(&mut solver)?;
    }
    let frame = layout.frame(MonitorSceneId::new(1), SceneEpoch::new(1));
    let reactive_frame = layout.reactive_frame()?;
    let hull = reactive_frame.hull;
    let hull_regions = reactive_frame.shape_input_regions();
    let new_regions = frame.input_regions.clone();
    let old_regions = legacy_input_regions(hull, 4.0);

    // A point the old projection covered but the new one does not, on a
    // transparent pixel if one exists; else the largest such point.
    let paint = hull_probe::contours(hull);
    let covers_paint = |x: f32, y: f32| {
        paint.iter().any(|contour| {
            hull_probe::filled_intervals_at(contour, y)
                .iter()
                .any(|&(a, b)| x >= a && x <= b)
        })
    };
    let mut points = Vec::new();
    let bounds = hull.bounds();
    let center = (
        (bounds.left() + bounds.right()) * 0.5,
        (bounds.top() + bounds.bottom()) * 0.5,
    );
    // Horizontal distance from a point to the nearest new region on its own row.
    // This is the width of the sliver the corrected projection rejected, so the
    // largest one is the point a physical click can actually land in.
    let sliver_width = |x: f32, y: f32| -> f32 {
        new_regions
            .iter()
            .filter(|region| y >= region.top() && y <= region.bottom())
            .map(|region| {
                if x < region.left() {
                    region.left() - x
                } else if x > region.right() {
                    x - region.right()
                } else {
                    0.0
                }
            })
            .fold(f32::INFINITY, f32::min)
    };
    // Pick A in the widest transparent old-only sliver, and B as the painted
    // in-region point nearest the body centre - far from A, so both are easy to
    // click and the two outcomes are unambiguous.
    let mut best_a: Option<(f32, f32)> = None;
    let mut best_a_width = 0.0_f32;
    let mut best_b: Option<(f32, f32)> = None;
    let mut best_b_distance = f32::INFINITY;
    let step = 1.0;
    let mut y = bounds.top();
    while y <= bounds.bottom() {
        let mut x = bounds.left();
        while x <= bounds.right() {
            let distance = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
            let old_in = in_regions(&old_regions, x, y);
            let new_in = in_regions(&new_regions, x, y);
            let painted = covers_paint(x, y);
            if old_in && !new_in && !painted {
                let width = sliver_width(x, y);
                if width.is_finite() && width > best_a_width {
                    best_a = Some((x, y));
                    best_a_width = width;
                }
            }
            if new_in && painted && distance < best_b_distance {
                best_b = Some((x, y));
                best_b_distance = distance;
            }
            x += step;
        }
        y += step;
    }

    if let Some((ax, ay)) = best_a {
        points.push(HullInputPoint {
            id: "A_old_minus_new_transparent",
            x: ax,
            y: ay,
            in_new_region: in_regions(&new_regions, ax, ay),
            in_old_region: true,
            in_paint: covers_paint(ax, ay),
        });
    }
    if let Some((bx, by)) = best_b {
        points.push(HullInputPoint {
            id: "B_new_region_painted",
            x: bx,
            y: by,
            in_new_region: in_regions(&new_regions, bx, by),
            in_old_region: in_regions(&old_regions, bx, by),
            in_paint: covers_paint(bx, by),
        });
    }
    Some(HullInputReport {
        input_regions: new_regions,
        hull_regions,
        hull,
        old_regions,
        points,
    })
}

/// Print the report as JSON without opening a window.
///
/// Read-only and headless: this is how the exact discriminating points are
/// obtained and handed to the operator before any live surface exists.
pub fn dump(viewport: SurfaceViewport) {
    use std::fmt::Write as _;
    let report = match qualify(viewport) {
        Some(report) => report,
        None => {
            eprintln!("HULL_INPUT_DUMP status=no_frame");
            return;
        }
    };
    let rect = |r: &GeometryRect| {
        format!(
            "[{:.2},{:.2},{:.2},{:.2}]",
            r.left(),
            r.top(),
            r.right(),
            r.bottom()
        )
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "HULL_INPUT_DUMP {{\"viewport\":[{:.1},{:.1}],\"input_regions\":[{}],\"hull_regions\":[{}],\"old_regions\":[{}],\"points\":[",
        viewport.width,
        viewport.height,
        report
            .input_regions
            .iter()
            .map(rect)
            .collect::<Vec<_>>()
            .join(","),
        report
            .hull_regions
            .iter()
            .map(rect)
            .collect::<Vec<_>>()
            .join(","),
        report
            .old_regions
            .iter()
            .map(rect)
            .collect::<Vec<_>>()
            .join(","),
    );
    for (index, point) in report.points.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"id\":\"{}\",\"x\":{:.2},\"y\":{:.2},\"in_new\":{},\"in_old\":{},\"in_paint\":{}}}",
            point.id, point.x, point.y, point.in_new_region, point.in_old_region, point.in_paint
        );
    }
    out.push_str("]}");
    println!("{out}");
}

/// A filled axis-aligned marker rectangle in window coordinates.
fn marker_path(bounds: Bounds<gpui::Pixels>, rect: GeometryRect) -> gpui::Path<gpui::Pixels> {
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
    let mut path = PathBuilder::fill();
    path.move_to(p(rect.left(), rect.top()));
    path.line_to(p(rect.right(), rect.top()));
    path.line_to(p(rect.right(), rect.bottom()));
    path.line_to(p(rect.left(), rect.bottom()));
    path.close();
    path.build().expect("marker path")
}

/// Map the report into GPUI window input regions (the production conversion).
fn window_regions(regions: &[GeometryRect]) -> Vec<Bounds<gpui::Pixels>> {
    regions
        .iter()
        .map(|region| Bounds {
            origin: point(px(region.left()), px(region.top())),
            size: gpui::Size::new(px(region.width()), px(region.height())),
        })
        .collect()
}

struct HullInputQualification {
    viewport: SurfaceViewport,
    report: HullInputReport,
}

impl HullInputQualification {
    fn new(_cx: &mut Context<Self>) -> Self {
        // Resolve at the qualification output size. The surface is anchored to
        // every edge, so the window viewport equals the output and surface-local
        // coordinates equal compositor-global ones - the operator clicks the
        // numbers logged here directly.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).expect("qualification viewport");
        let report = qualify(viewport).expect("settled reactive workspace envelope");
        for point in &report.points {
            println!(
                "HULL_INPUT_POINT id={} x={:.2} y={:.2} in_new={} in_old={} in_paint={}",
                point.id,
                point.x,
                point.y,
                point.in_new_region,
                point.in_old_region,
                point.in_paint
            );
        }
        println!(
            "HULL_INPUT_REGION_COUNT new={} old={}",
            report.input_regions.len(),
            report.old_regions.len()
        );
        Self { viewport, report }
    }
}

impl Render for HullInputQualification {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let live = SurfaceViewport::try_new(
            f32::from(window.viewport_size().width).max(1.0),
            f32::from(window.viewport_size().height).max(1.0),
        )
        .unwrap_or(self.viewport);
        if live != self.viewport {
            // Resolve against the real surface so regions, markers and the
            // logged coordinates all describe the same shape.
            if let Some(report) = qualify(live) {
                println!(
                    "HULL_INPUT_VIEWPORT_REBASED expected={:.0}x{:.0} live={:.0}x{:.0}",
                    self.viewport.width, self.viewport.height, live.width, live.height
                );
                for point in &report.points {
                    println!(
                        "HULL_INPUT_POINT id={} x={:.2} y={:.2} in_new={} in_old={} in_paint={}",
                        point.id,
                        point.x,
                        point.y,
                        point.in_new_region,
                        point.in_old_region,
                        point.in_paint
                    );
                }
                self.viewport = live;
                self.report = report;
            }
        }
        // The production conversion: the exact regions the scene would submit.
        window.set_input_region(Some(&window_regions(&self.report.input_regions)));

        let report = self.report.clone();
        let canvas = canvas(
            move |bounds, _, _| (bounds, report.clone()),
            move |_paint_bounds, (gpui_bounds, report), window, _| {
                let move_regions = report.input_regions.clone();
                let old_regions = report.old_regions.clone();
                let click_regions = report.input_regions.clone();
                // Faithful fill: the production hull path, so the painted shape
                // and the input region derive from the same geometry.
                window.paint_path(
                    crate::product_hull::connective_hull_path(gpui_bounds, report.hull),
                    rgba(0x00a0ffff),
                );
                // Markers are painted here, on top of the hull, rather than as
                // sibling divs the canvas would cover.
                for point in &report.points {
                    let color = if point.id.starts_with("A_") {
                        rgba(0xff3b30ff)
                    } else {
                        rgba(0x34c759ff)
                    };
                    let radius = if point.id.starts_with("A_") {
                        10.0
                    } else {
                        6.0
                    };
                    let marker = GeometryRect::try_new(
                        point.x - radius,
                        point.y - radius,
                        point.x + radius,
                        point.y + radius,
                        0.0,
                    )
                    .expect("marker rect");
                    window.paint_path(marker_path(gpui_bounds, marker), color);
                }
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, _| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let x = f32::from(event.position.x);
                    let y = f32::from(event.position.y);
                    println!(
                        "HULL_INPUT_POINTER x={x:.2} y={y:.2} accepted={} old_would={}",
                        in_regions(&move_regions, x, y),
                        in_regions(&old_regions, x, y)
                    );
                });
                window.on_mouse_event(|_: &MouseExitEvent, phase, _, _| {
                    println!("HULL_INPUT_POINTER_EXIT phase={phase:?}");
                });
                window.on_mouse_event(move |event: &MouseDownEvent, phase, _, _| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let x = f32::from(event.position.x);
                    let y = f32::from(event.position.y);
                    println!(
                        "HULL_INPUT_CLICK x={x:.2} y={y:.2} accepted={}",
                        in_regions(&click_regions, x, y)
                    );
                });
            },
        )
        .size_full();

        div().relative().size_full().child(canvas)
    }
}

/// Entry point used by the qualification binary.
pub fn run() {
    // No window at all if the live flag is absent is handled by the binary; here
    // the window is the full anchored layer surface.
    gpui_platform::application().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: gpui::Size::new(px(1280.0), px(800.0)),
                })),
                app_id: Some("gnos.ux.hull-input-qualification".into()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
                    namespace: "gnos.ux:hull-input:qualification".into(),
                    layer: gpui::layer_shell::Layer::Top,
                    anchor: gpui::layer_shell::Anchor::TOP
                        | gpui::layer_shell::Anchor::BOTTOM
                        | gpui::layer_shell::Anchor::LEFT
                        | gpui::layer_shell::Anchor::RIGHT,
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| cx.new(HullInputQualification::new),
        )
        .expect("opening hull input qualification surface");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The consumer value itself excludes the legacy sliver and still accepts
    /// the painted body: the causal chain the live gate then confirms physically.
    #[test]
    fn consumer_frame_excludes_the_legacy_sliver_and_accepts_inside_paint() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let report = qualify(viewport).expect("settled reactive workspace envelope");

        assert!(
            !report.hull_regions.is_empty(),
            "the generic hull must contribute input regions"
        );
        // The hull contribution reaches the compositor as whole pixels, and is
        // carried unchanged inside the consumer frame vector.
        for region in &report.hull_regions {
            for value in [region.left(), region.top(), region.right(), region.bottom()] {
                assert_eq!(
                    value,
                    value.trunc(),
                    "the hull handed {value} to the compositor, which truncates"
                );
            }
            assert!(
                report.input_regions.contains(region),
                "hull region {region:?} is missing from the consumer frame input vector"
            );
        }

        let a = report
            .points
            .iter()
            .find(|point| point.id.starts_with("A_"))
            .expect("a point the old projection covered and the new consumer frame rejects");
        assert!(
            a.in_old_region && !a.in_new_region && !a.in_paint,
            "point A must be old-only, outside the consumer frame, and transparent: {a:?}"
        );

        let b = report
            .points
            .iter()
            .find(|point| point.id.starts_with("B_"))
            .expect("a counter-witness inside the paint");
        assert!(
            b.in_new_region && b.in_paint,
            "point B must be inside the consumer frame and painted: {b:?}"
        );
    }
}
