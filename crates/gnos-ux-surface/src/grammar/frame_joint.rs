//! A surface flush against one or more frame borders, as one outline.
//!
//! The connective hull joins a child to a single parent. A drawer that sits in
//! a frame corner (the toast drawer: top and right) or spans the frame (the
//! notification centre: top, right and bottom) touches several borders at
//! once. Its outline follows one rule per corner:
//! - both adjacent sides flush, or neither: a convex arc (the frame's own
//!   cut-out, or a free corner);
//! - exactly one side flush: a concave fillet that flares along the flush
//!   side, so the surface joins that border like a hull joint.
//!
//! The outline is traversed clockwise as `HullCommand`s, so the same painter
//! draws hulls and frame joints.

use super::{GeometryRect, HullCommand, Point2};

/// Quarter-circle cubic constant.
const KAPPA: f32 = 0.552_284_8;

/// The child's sides that lie on a frame inner edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlushSides {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameJointOutline {
    commands: Vec<HullCommand>,
    bounds: GeometryRect,
}

impl FrameJointOutline {
    pub fn commands(&self) -> &[HullCommand] {
        &self.commands
    }

    /// The child plus its fillet flares.
    pub fn bounds(&self) -> GeometryRect {
        self.bounds
    }
}

fn add(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x() + b.x(), a.y() + b.y())
}

fn scale(v: Point2, k: f32) -> Point2 {
    Point2::new(v.x() * k, v.y() * k)
}

/// Outline of `child` joined to the frame along its `flush` sides. `radius`
/// is the frame rounding, bounded by half the child's smaller side. `None`
/// for a child under a pixel in either axis.
pub fn frame_joint_outline(
    child: GeometryRect,
    flush: FlushSides,
    radius: f32,
) -> Option<FrameJointOutline> {
    if child.width() < 1.0 || child.height() < 1.0 || !radius.is_finite() {
        return None;
    }
    let r = radius
        .max(0.0)
        .min(child.width() / 2.0)
        .min(child.height() / 2.0);
    let (x0, y0, x1, y1) = (child.left(), child.top(), child.right(), child.bottom());
    let up = Point2::new(0.0, -1.0);
    let down = Point2::new(0.0, 1.0);
    let left = Point2::new(-1.0, 0.0);
    let right = Point2::new(1.0, 0.0);
    // Clockwise: (corner, arriving direction, leaving direction, arriving
    // side flush, leaving side flush).
    let corners = [
        (Point2::new(x0, y0), up, right, flush.left, flush.top),
        (Point2::new(x1, y0), right, down, flush.top, flush.right),
        (Point2::new(x1, y1), down, left, flush.right, flush.bottom),
        (Point2::new(x0, y1), left, up, flush.bottom, flush.left),
    ];
    // (A, B, tangent at A, tangent at B) for each corner.
    let arcs = corners.map(|(p, d_in, d_out, in_flush, out_flush)| {
        if in_flush && !out_flush {
            // Flare forward along the arriving (flush) side, then turn back in.
            (
                add(p, scale(d_in, r)),
                add(p, scale(d_out, r)),
                scale(d_in, -1.0),
                d_out,
            )
        } else if !in_flush && out_flush {
            // Turn out to flare backward along the leaving (flush) side.
            (
                add(p, scale(d_in, -r)),
                add(p, scale(d_out, -r)),
                d_in,
                scale(d_out, -1.0),
            )
        } else {
            (
                add(p, scale(d_in, -r)),
                add(p, scale(d_out, r)),
                d_in,
                d_out,
            )
        }
    });
    let mut commands = vec![HullCommand::MoveTo(arcs[0].1)];
    let mut min = (x0, y0);
    let mut max = (x1, y1);
    for &(a, b, ta, tb) in arcs[1..].iter().chain(arcs[..1].iter()) {
        commands.push(HullCommand::LineTo(a));
        commands.push(HullCommand::CubicTo {
            control1: add(a, scale(ta, KAPPA * r)),
            control2: add(b, scale(tb, -KAPPA * r)),
            to: b,
        });
        for q in [a, b] {
            min = (min.0.min(q.x()), min.1.min(q.y()));
            max = (max.0.max(q.x()), max.1.max(q.y()));
        }
    }
    commands.push(HullCommand::Close);
    Some(FrameJointOutline {
        commands,
        bounds: GeometryRect::try_new(min.0, min.1, max.0, max.1, 0.0).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flattened polygon (24 steps per cubic) and an even-odd inside test.
    fn polygon(outline: &FrameJointOutline) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        let mut last = (0.0, 0.0);
        for command in outline.commands() {
            match *command {
                HullCommand::MoveTo(p) | HullCommand::LineTo(p) => {
                    last = (p.x(), p.y());
                    out.push(last);
                }
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    let (p0, p1, p2, p3) = (
                        last,
                        (control1.x(), control1.y()),
                        (control2.x(), control2.y()),
                        (to.x(), to.y()),
                    );
                    for i in 1..=24 {
                        let t = i as f32 / 24.0;
                        let u = 1.0 - t;
                        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                        let x = w[0] * p0.0 + w[1] * p1.0 + w[2] * p2.0 + w[3] * p3.0;
                        let y = w[0] * p0.1 + w[1] * p1.1 + w[2] * p2.1 + w[3] * p3.1;
                        out.push((x, y));
                    }
                    last = p3;
                }
                _ => {}
            }
        }
        out
    }

    fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
        let mut hit = false;
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                hit = !hit;
            }
            j = i;
        }
        hit
    }

    #[test]
    fn a_frame_spanning_drawer_flares_into_top_and_bottom_and_rounds_into_the_frame_corners() {
        // The notification centre: flush top, right and bottom.
        let child = GeometryRect::try_new(840.0, 10.0, 1270.0, 790.0, 0.0).unwrap();
        let outline = frame_joint_outline(
            child,
            FlushSides {
                top: true,
                right: true,
                bottom: true,
                left: false,
            },
            25.0,
        )
        .unwrap();
        let poly = polygon(&outline);
        assert!(inside(&poly, 1000.0, 400.0), "the body");
        assert!(
            inside(&poly, 838.0, 11.0),
            "fillet flares left along the top border"
        );
        assert!(inside(&poly, 838.0, 789.0), "and along the bottom border");
        assert!(!inside(&poly, 838.0, 60.0), "not along the free left side");
        assert!(
            !inside(&poly, 1269.5, 10.5),
            "the frame corner is cut out (convex)"
        );
        assert_eq!(outline.bounds().left(), 815.0);
    }

    #[test]
    fn a_corner_drawer_flares_left_along_the_top_and_down_along_the_right() {
        // The toast drawer: flush top and right.
        let child = GeometryRect::try_new(840.0, 10.0, 1270.0, 200.0, 0.0).unwrap();
        let outline = frame_joint_outline(
            child,
            FlushSides {
                top: true,
                right: true,
                ..Default::default()
            },
            25.0,
        )
        .unwrap();
        let poly = polygon(&outline);
        assert!(inside(&poly, 838.0, 11.0), "left flare along the top");
        assert!(
            inside(&poly, 1269.0, 202.0),
            "downward flare along the right"
        );
        assert!(
            !inside(&poly, 841.0, 199.0),
            "the free corner is rounded (convex)"
        );
        assert_eq!(outline.bounds().bottom(), 225.0);
    }

    #[test]
    fn a_free_child_is_a_rounded_rectangle_and_tiny_children_are_refused() {
        let child = GeometryRect::try_new(0.0, 0.0, 100.0, 40.0, 0.0).unwrap();
        let outline = frame_joint_outline(child, FlushSides::default(), 25.0).unwrap();
        assert_eq!(
            outline.bounds(),
            child,
            "radius clamps to half the height; no flare"
        );
        assert!(!inside(&polygon(&outline), 1.0, 1.0));
        let tiny = GeometryRect::try_new(0.0, 0.0, 100.0, 0.5, 0.0).unwrap();
        assert!(frame_joint_outline(tiny, FlushSides::default(), 25.0).is_none());
    }
}
