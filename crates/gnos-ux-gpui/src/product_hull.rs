//! The one place a derived body's connective hull is turned into a paintable
//! path.
//!
//! This exists so the hull is realized identically everywhere. The production
//! scene and the component lab both paint the same `ConnectiveHullGeometry`
//! through this function, which is what makes a visual review of a lab fixture
//! a review of the geometry production will actually ship.
//!
//! The failure this prevents is specific: a lab that draws its own rounded
//! rectangle "because it looks right in a fixture" approves a shape the product
//! never renders. A seam hidden behind content in the lab is a seam in
//! production too, and nothing downstream would catch it.

use gpui::{Bounds, Path, PathBuilder, Pixels, point, px};

use gnos_ux_surface::grammar::{ConnectiveHullGeometry, Point2};

/// Build the fill path for a derived body's connective hull.
///
/// `bounds` is the surface the hull's geometry is expressed in; the hull's own
/// coordinates are surface-local, so this is the surface origin offset and
/// nothing else.
pub fn connective_hull_path(bounds: Bounds<Pixels>, hull: ConnectiveHullGeometry) -> Path<Pixels> {
    let commands: Vec<_> = (0..hull.command_count())
        .map(|index| hull.command(index).expect("command index within hull"))
        .collect();
    outline_path(bounds, &commands).expect("building connective product hull")
}

/// Build the fill path for any grammar outline (hull or frame joint).
pub fn outline_path(
    bounds: Bounds<Pixels>,
    commands: &[gnos_ux_surface::grammar::HullCommand],
) -> Option<Path<Pixels>> {
    use gnos_ux_surface::grammar::HullCommand;
    let mut path = PathBuilder::fill();
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p =
        |geometry_point: Point2| point(px(ox + geometry_point.x()), px(oy + geometry_point.y()));
    for &command in commands {
        match command {
            HullCommand::Empty => break,
            HullCommand::MoveTo(p0) => path.move_to(p(p0)),
            HullCommand::LineTo(p0) => path.line_to(p(p0)),
            HullCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                path.cubic_bezier_to(p(to), p(control1), p(control2));
            }
            HullCommand::Close => path.close(),
        }
    }
    path.build().ok()
}

/// A drawer flush against frame borders, as a fill path in scene space.
pub fn frame_joint_path(
    panel: blobin_ui_core::layout::Rect,
    flush: gnos_ux_surface::grammar::FlushSides,
) -> Option<Path<Pixels>> {
    let child = gnos_ux_surface::grammar::GeometryRect::try_new(
        panel.x,
        panel.y,
        panel.x + panel.width,
        panel.y + panel.height,
        0.0,
    )
    .ok()?;
    let outline = gnos_ux_surface::grammar::frame_joint_outline(
        child,
        flush,
        crate::reference_bar::BORDER_ROUNDING,
    )?;
    outline_path(
        Bounds::new(point(px(0.0), px(0.0)), gpui::size(px(0.0), px(0.0))),
        outline.commands(),
    )
}

/// A deterministic, text description of a derived body's hull.
///
/// Evidence needs the shape as data, not only as pixels: a reviewer has to be
/// able to check continuity, the joint, and the shoulder and neck widths
/// without trusting a screenshot. The command stream is the hull's own geometry,
/// so this reports what the solver produced rather than a summary of it.
pub fn describe_hull(hull: &ConnectiveHullGeometry) -> String {
    use gnos_ux_surface::grammar::HullCommand;
    let mut out = String::new();
    out.push_str(&format!("commands={} ", hull.command_count()));
    let bounds = hull.bounds();
    out.push_str(&format!(
        "hull_bounds={:.2},{:.2}..{:.2},{:.2} ",
        bounds.left(),
        bounds.top(),
        bounds.right(),
        bounds.bottom()
    ));
    for index in 0..hull.command_count() {
        match hull.command(index).expect("command index within hull") {
            HullCommand::Empty => {
                out.push_str("empty");
                break;
            }
            HullCommand::MoveTo(p0) => {
                out.push_str(&format!("M{:.2},{:.2} ", p0.x(), p0.y()));
            }
            HullCommand::LineTo(p0) => {
                out.push_str(&format!("L{:.2},{:.2} ", p0.x(), p0.y()));
            }
            HullCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                out.push_str(&format!(
                    "C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} ",
                    control1.x(),
                    control1.y(),
                    control2.x(),
                    control2.y(),
                    to.x(),
                    to.y()
                ));
            }
            HullCommand::Close => out.push('Z'),
        }
    }
    out.trim_end().to_owned()
}

fn append_arc(path: &mut PathBuilder, radius: f32, sweep: bool, to: gpui::Point<gpui::Pixels>) {
    if radius <= 0.01 {
        path.line_to(to);
    } else {
        path.arc_to(point(px(radius), px(radius)), px(0.0), false, sweep, to);
    }
}

pub fn rounded_rect_path(
    bounds: Bounds<gpui::Pixels>,
    rect: gnos_ux_surface::grammar::GeometryRect,
) -> gpui::Path<gpui::Pixels> {
    let mut path = PathBuilder::fill();
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
    let radius = rect
        .radius()
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);

    path.move_to(p(rect.left() + radius, rect.top()));
    path.line_to(p(rect.right() - radius, rect.top()));
    append_arc(
        &mut path,
        radius,
        true,
        p(rect.right(), rect.top() + radius),
    );
    path.line_to(p(rect.right(), rect.bottom() - radius));
    append_arc(
        &mut path,
        radius,
        true,
        p(rect.right() - radius, rect.bottom()),
    );
    path.line_to(p(rect.left() + radius, rect.bottom()));
    append_arc(
        &mut path,
        radius,
        true,
        p(rect.left(), rect.bottom() - radius),
    );
    path.line_to(p(rect.left(), rect.top() + radius));
    append_arc(&mut path, radius, true, p(rect.left() + radius, rect.top()));
    path.close();
    path.build().expect("building scene rounded rect")
}

/// One shared shell realization, used by the production scene and both lab
/// routes. Inners paint content only. Retains the existing Dock material.
pub fn paint_shared_dock(
    bounds: Bounds<Pixels>,
    outer: gnos_ux_surface::grammar::GeometryRect,
    window: &mut gpui::Window,
) {
    window.paint_path(rounded_rect_path(bounds, outer), gpui::rgba(0x263844ff));
}

/// An outline as rectangles inside it (`[x0, y0, x1, y1]`). The blur
/// protocol takes rectangles only; a shape that is not one (the Deck with its
/// hump and sheet, a panel fused to the Bar with its fillets and rounded
/// corners) asks for its blur as this staircase. Each band of `step` px spans
/// what is inside the outline over the whole band, so the blur never shows
/// beyond the painted edge; bands with the same spans are merged, so straight
/// walls cost one rectangle and only the curves are cut finely.
pub fn outline_bands(
    commands: &[gnos_ux_surface::grammar::HullCommand],
    step: f32,
) -> Vec<[f32; 4]> {
    let polygons = flatten(commands);
    let points = polygons.iter().flatten();
    let (Some(top), Some(bottom)) = (
        points.clone().map(|p| p.1).reduce(f32::min),
        points.map(|p| p.1).reduce(f32::max),
    ) else {
        return Vec::new();
    };
    let crossings = |y: f32| -> Vec<f32> {
        let mut xs: Vec<f32> = polygons
            .iter()
            .flat_map(|poly| {
                poly.iter()
                    .zip(poly.iter().cycle().skip(1))
                    .filter(move |(a, b)| (a.1 <= y) != (b.1 <= y))
                    .map(move |(&(ax, ay), &(bx, by))| ax + (y - ay) / (by - ay) * (bx - ax))
            })
            .collect();
        xs.sort_by(f32::total_cmp);
        xs
    };
    let step = step.max(1.0);
    let mut bands: Vec<[f32; 4]> = Vec::new();
    // The spans of the band being grown, and where it started.
    let mut open: Option<(Vec<(f32, f32)>, f32)> = None;
    let mut y0 = top;
    while y0 < bottom {
        let y1 = (y0 + step).min(bottom);
        // Inside over the whole band: the narrowest of its spans at its top,
        // middle and bottom (a little inside, where the outline is crossed).
        let samples = [y0 + 0.01, (y0 + y1) / 2.0, y1 - 0.01].map(crossings);
        let spans: Vec<(f32, f32)> = if samples.iter().all(|xs| xs.len() == samples[1].len()) {
            samples[1]
                .chunks_exact(2)
                .enumerate()
                .map(|(k, _)| {
                    let left = samples.iter().map(|xs| xs[2 * k]).fold(f32::MIN, f32::max);
                    let right = samples
                        .iter()
                        .map(|xs| xs[2 * k + 1])
                        .fold(f32::MAX, f32::min);
                    (left, right)
                })
                .filter(|(l, r)| r - l >= 1.0)
                .collect()
        } else {
            // The outline turns within the band (a top or a bottom): leave
            // it unblurred rather than blur past the edge.
            Vec::new()
        };
        let same = open.as_ref().is_some_and(|(o, _)| {
            o.len() == spans.len()
                && o.iter()
                    .zip(&spans)
                    .all(|(a, b)| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5)
        });
        if same {
            // Merged: keep what is inside every merged band.
            if let Some((o, _)) = open.as_mut() {
                for (a, b) in o.iter_mut().zip(&spans) {
                    *a = (a.0.max(b.0), a.1.min(b.1));
                }
            }
        } else {
            if let Some((o, start)) = open.take() {
                bands.extend(o.into_iter().map(|(l, r)| [l, start, r, y0]));
            }
            if !spans.is_empty() {
                open = Some((spans, y0));
            }
        }
        y0 = y1;
    }
    if let Some((o, start)) = open {
        bands.extend(o.into_iter().map(|(l, r)| [l, start, r, bottom]));
    }
    bands
}

/// The outline's subpaths as polygons (cubics flattened).
fn flatten(commands: &[gnos_ux_surface::grammar::HullCommand]) -> Vec<Vec<(f32, f32)>> {
    use gnos_ux_surface::grammar::HullCommand;
    const CURVE_STEPS: usize = 12;
    let mut polygons: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    for &command in commands {
        match command {
            HullCommand::Empty => break,
            HullCommand::MoveTo(p) => {
                if current.len() > 2 {
                    polygons.push(std::mem::take(&mut current));
                }
                current = vec![(p.x(), p.y())];
            }
            HullCommand::LineTo(p) => current.push((p.x(), p.y())),
            HullCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let Some(&(x0, y0)) = current.last() else {
                    continue;
                };
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    current.push((
                        a * x0 + b * control1.x() + c * control2.x() + d * to.x(),
                        a * y0 + b * control1.y() + c * control2.y() + d * to.y(),
                    ));
                }
            }
            HullCommand::Close => {
                if current.len() > 2 {
                    polygons.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if current.len() > 2 {
        polygons.push(current);
    }
    polygons
}

/// A rounded rectangle as outline commands (cubic quarter circles).
pub fn rounded_rect_commands(
    rect: gnos_ux_surface::grammar::GeometryRect,
) -> Vec<gnos_ux_surface::grammar::HullCommand> {
    use gnos_ux_surface::grammar::HullCommand;
    const K: f32 = 0.552_284_8;
    let r = rect
        .radius()
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);
    let (l, t, rt, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let p = Point2::new;
    let corner = |from: (f32, f32), c: (f32, f32), to: (f32, f32)| HullCommand::CubicTo {
        control1: p(from.0 + (c.0 - from.0) * K, from.1 + (c.1 - from.1) * K),
        control2: p(to.0 + (c.0 - to.0) * K, to.1 + (c.1 - to.1) * K),
        to: p(to.0, to.1),
    };
    vec![
        HullCommand::MoveTo(p(l + r, t)),
        HullCommand::LineTo(p(rt - r, t)),
        corner((rt - r, t), (rt, t), (rt, t + r)),
        HullCommand::LineTo(p(rt, b - r)),
        corner((rt, b - r), (rt, b), (rt - r, b)),
        HullCommand::LineTo(p(l + r, b)),
        corner((l + r, b), (l, b), (l, b - r)),
        HullCommand::LineTo(p(l, t + r)),
        corner((l, t + r), (l, t), (l + r, t)),
        HullCommand::Close,
    ]
}

#[cfg(test)]
mod band_tests {
    use super::*;
    use gnos_ux_surface::grammar::{GeometryRect, HullCommand};

    fn inside(bands: &[[f32; 4]], x: f32, y: f32) -> bool {
        bands
            .iter()
            .any(|b| x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3])
    }

    #[test]
    fn straight_walls_cost_one_band_and_a_step_follows_its_shape() {
        let p = Point2::new;
        let rect = [
            HullCommand::MoveTo(p(10.0, 10.0)),
            HullCommand::LineTo(p(110.0, 10.0)),
            HullCommand::LineTo(p(110.0, 30.0)),
            HullCommand::LineTo(p(10.0, 30.0)),
            HullCommand::Close,
        ];
        assert_eq!(outline_bands(&rect, 2.0), vec![[10.0, 10.0, 110.0, 30.0]]);
        let hump = [
            HullCommand::MoveTo(p(0.0, 20.0)),
            HullCommand::LineTo(p(40.0, 20.0)),
            HullCommand::LineTo(p(40.0, 0.0)),
            HullCommand::LineTo(p(60.0, 0.0)),
            HullCommand::LineTo(p(60.0, 20.0)),
            HullCommand::LineTo(p(100.0, 20.0)),
            HullCommand::LineTo(p(100.0, 40.0)),
            HullCommand::LineTo(p(0.0, 40.0)),
            HullCommand::Close,
        ];
        let bands = outline_bands(&hump, 2.0);
        assert!(inside(&bands, 50.0, 5.0) && inside(&bands, 5.0, 35.0));
        assert!(!inside(&bands, 20.0, 10.0), "beside the hump stays clear");
    }

    #[test]
    fn a_rounded_rectangle_is_never_blurred_past_its_corners() {
        let rect = GeometryRect::try_new(0.0, 0.0, 200.0, 100.0, 25.0).unwrap();
        let bands = outline_bands(&rounded_rect_commands(rect), 2.0);
        // The corner's outside (near the square corner) is clear; its inside
        // and the straight middle are covered.
        assert!(!inside(&bands, 1.0, 1.0) && !inside(&bands, 199.0, 99.0));
        assert!(inside(&bands, 25.0, 25.0) && inside(&bands, 100.0, 50.0));
        // Every band lies inside the circle of its corner.
        for b in &bands {
            for (x, y) in [(b[0], b[1]), (b[0], b[3]), (b[2], b[1]), (b[2], b[3])] {
                let cx = x.clamp(25.0, 175.0);
                let cy = y.clamp(25.0, 75.0);
                assert!(
                    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= 25.2,
                    "{b:?}"
                );
            }
        }
        assert!(bands.len() < 40, "straight middle merged: {}", bands.len());
    }
}
