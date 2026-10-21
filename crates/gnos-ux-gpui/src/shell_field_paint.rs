//! SHELL-FIELD-01B: the frame, the Bar and the surfaces out of them painted
//! as one shape.
//!
//! The shell used to be painted in pieces: the frame's four strips and its
//! corner fillets, its inner shadow along the bare frame, then each surface's
//! own hull on top (`paint_popout_hull`). Under a translucent surface the
//! frame's shadow showed through, and every piece had its own antialiased
//! edge: the joints read as layers laid side by side.
//!
//! Here the shell is one signed distance field (`gnos_ux_surface`'s
//! `ShellField`): the frame's hollow, with every revealed surface joined to it
//! by a smooth union whose radius is the fillet. It is filled as one path (the
//! output with the hollow's edge as its hole), so there is no internal edge,
//! and its inner shadow runs along that same edge, around the surfaces too.
//! The surfaces' views then paint only their content.
//!
//! The contour is traced when the surfaces move (about 2 ms for an output
//! with four surfaces out) and reused while nothing moves.
//! Receipt: `GNOS_UX_SHELL_FIELD`.

use crate::bar_popout_host::PopoutFrame;
use crate::product_shell_material::{ShellBorder, ShellMaterial};
use crate::rail_topology::RailSide;
use gnos_ux_surface::grammar::Point2;
use gnos_ux_surface::grammar::shell_field::{FieldBox, ShellField};
use gpui::{Bounds, PathBuilder, Pixels, Window, point, px, rgba};
use std::sync::atomic::{AtomicU32, Ordering};

/// The colour the field paints this frame (0: the field is off). A surface's
/// view leaves its hull to the field when it would paint this same colour;
/// a surface with its own opacity is not in the field and paints its hull.
static FIELD_COLOUR: AtomicU32 = AtomicU32::new(0);

/// The grid the edge is traced on, in pixels.
const CELL: f32 = 4.0;
/// The inner shadow, in bands of this many pixels.
const SHADOW_BAND: f32 = 3.0;
/// How far the inner shadow reaches into the hollow.
const SHADOW_REACH: f32 = 15.0;

/// Whether the operator has the shell painted as one shape (Settings ›
/// Advanced).
pub fn enabled() -> bool {
    crate::surface_prefs::advanced("shell_field") != 0
}

/// A revealed surface as a member of the field: its visible part, pushed
/// into the rail by its radius so its joint is all fillet.
pub fn member(frame: &PopoutFrame) -> FieldBox {
    let v = frame.visible;
    let r = v.radius().max(crate::reference_bar::BORDER_ROUNDING);
    let (mut l, mut t, mut rt, mut b) = (v.left(), v.top(), v.right(), v.bottom());
    match frame.side {
        RailSide::Left => l -= r,
        RailSide::Right => rt += r,
        RailSide::Top => t -= r,
        RailSide::Bottom => b += r,
    }
    FieldBox::new(l, t, rt, b, r)
}

/// A surface given as its rectangle (a drawer, Settings): each side flush
/// with the frame's hollow, or past it, reaches into the frame by its radius,
/// so the field joins it there; the other sides stay its own rounded edge.
pub fn member_flush(
    (l, t, r, b): (f32, f32, f32, f32),
    hole: crate::product_shell_material::Rect,
) -> FieldBox {
    const FLUSH: f32 = 1.5;
    let radius = crate::reference_bar::BORDER_ROUNDING;
    let reach = |side: f32, edge: f32, outward: f32| {
        if (side - edge) * outward >= -FLUSH {
            side + outward * radius
        } else {
            side
        }
    };
    FieldBox::new(
        reach(l, hole.x0, -1.0),
        reach(t, hole.y0, -1.0),
        reach(r, hole.x1, 1.0),
        reach(b, hole.y1, 1.0),
        radius,
    )
}

/// The traced shell: its edge and the bands of its inner shadow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShellShape {
    pub edge: Vec<Vec<Point2>>,
    /// (alpha, loops) per shadow band, nearest the edge first.
    pub shadow: Vec<(f32, Vec<Vec<Point2>>)>,
}

/// The shape, recomputed only when its inputs change.
#[derive(Debug, Default)]
pub struct ShellFieldCache {
    key: Option<String>,
    shape: ShellShape,
    /// (members, edge loops) last receipted.
    reported: Option<(usize, usize)>,
}

impl ShellFieldCache {
    pub fn shape(
        &mut self,
        output: (f32, f32),
        border: &ShellBorder,
        members: &[FieldBox],
        material: &ShellMaterial,
    ) -> &ShellShape {
        let hole = border.hole(output);
        let blend = crate::surface_prefs::advanced("joint_fillet_px") as f32;
        let key = format!(
            "{output:?}|{:.1},{:.1},{:.1},{:.1},{:.1}|{blend}|{}|{:?}",
            hole.x0,
            hole.y0,
            hole.x1,
            hole.y1,
            border.radius,
            members
                .iter()
                .map(|m| format!("{:.1},{:.1},{:.1},{:.1}", m.left, m.top, m.right, m.bottom))
                .collect::<Vec<_>>()
                .join(";"),
            material.shadow.map(|s| (s.alpha, s.sigma)),
        );
        if self.key.as_deref() != Some(key.as_str()) {
            let started = std::time::Instant::now();
            let field = ShellField {
                output,
                hollow: FieldBox::new(hole.x0, hole.y0, hole.x1, hole.y1, border.radius),
                members: members.to_vec(),
                blend,
            };
            let bands: Vec<f32> = match material.shadow {
                Some(_) => (0..)
                    .map(|i| (i as f32 + 0.5) * SHADOW_BAND)
                    .take_while(|l| *l < SHADOW_REACH)
                    .collect(),
                None => Vec::new(),
            };
            let levels: Vec<f32> = std::iter::once(0.0).chain(bands.iter().copied()).collect();
            let mut traced = field.contours(CELL, &levels).into_iter();
            let edge = traced.next().unwrap_or_default();
            let shadow = bands
                .iter()
                .zip(traced)
                .map(|(level, loops)| {
                    let alpha = material.shadow.map_or(0.0, |s| s.paint_alpha(*level));
                    (alpha, loops)
                })
                .collect();
            let continuous = edge.len() == 1;
            // One receipt per change of what the shell is made of, not per
            // frame of an extrusion.
            let summary = (members.len(), edge.len());
            if self.reported != Some(summary) {
                self.reported = Some(summary);
                println!(
                    "GNOS_UX_SHELL_FIELD output={:.0}x{:.0} members={} edge_loops={} continuous={continuous} points={} ms={:.2}",
                    output.0,
                    output.1,
                    members.len(),
                    edge.len(),
                    edge.iter().map(Vec::len).sum::<usize>(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            self.shape = ShellShape { edge, shadow };
            self.key = Some(key);
        }
        &self.shape
    }
}

/// Paint the shell: its inner shadow along the edge, then the shell itself
/// as one path (the output with the edge's loops as holes).
pub fn paint(
    bounds: Bounds<Pixels>,
    shape: &ShellShape,
    material: &ShellMaterial,
    window: &mut Window,
) {
    let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let at = |p: &Point2| point(px(ox + p.x()), px(oy + p.y()));
    if let Some(shadow) = material.shadow {
        for (alpha, loops) in &shape.shadow {
            let colour = crate::product_shell_material::Rgba {
                rgb: shadow.rgb,
                a: *alpha,
            };
            let mut builder = PathBuilder::stroke(px(SHADOW_BAND));
            for lp in loops {
                let points: Vec<_> = lp.iter().map(at).collect();
                builder.add_polygon(&points, true);
            }
            if let Ok(path) = builder.build() {
                window.paint_path(path, rgba(u32::from_be_bytes(colour.to_u8())));
            }
        }
    }
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let mut builder = PathBuilder::fill();
    builder.add_polygon(
        &[
            point(px(ox), px(oy)),
            point(px(ox + w), px(oy)),
            point(px(ox + w), px(oy + h)),
            point(px(ox), px(oy + h)),
        ],
        true,
    );
    for lp in &shape.edge {
        let points: Vec<_> = lp.iter().map(at).collect();
        builder.add_polygon(&points, true);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, rgba(u32::from_be_bytes(material.surface.to_u8())));
    }
}

/// The colour the field paints this frame, or `None` when it is off.
pub fn set_field_colour(colour: Option<u32>) {
    FIELD_COLOUR.store(colour.unwrap_or(0), Ordering::Relaxed);
}

/// Whether the field already paints a hull of this colour.
pub fn paints(colour: gpui::Rgba) -> bool {
    let field = FIELD_COLOUR.load(Ordering::Relaxed);
    field != 0 && u32::from(colour) == field
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_ux_surface::grammar::GeometryRect;

    #[test]
    fn a_drawer_reaches_into_the_frame_on_its_flush_sides_only() {
        let hole = crate::product_shell_material::Rect {
            x0: 10.0,
            y0: 60.0,
            x1: 990.0,
            y1: 790.0,
        };
        // A toast drawer in the bottom-right corner.
        let m = member_flush((560.0, 690.0, 990.0, 790.0), hole);
        assert_eq!((m.left, m.top), (560.0, 690.0), "free sides stay");
        assert_eq!((m.right, m.bottom), (1015.0, 815.0), "flush sides reach");
    }

    #[test]
    fn a_surface_out_of_a_top_bar_reaches_into_it() {
        let frame = PopoutFrame {
            rect: GeometryRect::try_new(100.0, 60.0, 400.0, 300.0, 25.0).unwrap(),
            visible: GeometryRect::try_new(100.0, 60.0, 400.0, 300.0, 25.0).unwrap(),
            openness: 1.0,
            hull: None,
            side: RailSide::Top,
        };
        let m = member(&frame);
        assert_eq!(
            (m.left, m.top, m.right, m.bottom),
            (100.0, 35.0, 400.0, 300.0)
        );
    }
}
