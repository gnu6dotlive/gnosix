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
    use gnos_ux_surface::grammar::HullCommand;
    let mut path = PathBuilder::fill();
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p =
        |geometry_point: Point2| point(px(ox + geometry_point.x()), px(oy + geometry_point.y()));
    for index in 0..hull.command_count() {
        match hull.command(index).expect("command index within hull") {
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
    path.build().expect("building connective product hull")
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
