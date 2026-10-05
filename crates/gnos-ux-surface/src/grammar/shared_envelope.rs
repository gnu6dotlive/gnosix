//! A single rounded material envelope, partitioned into compact and content
//! regions. No product identity, attachment inference, or execution authority.
use super::{GeometryError, GeometryRect};

#[derive(Clone, Copy, Debug)]
pub struct SharedEnvelopeInput {
    pub structural: GeometryRect,
    pub painted_request: GeometryRect,
    pub safe: GeometryRect,
    pub content_width: f32,
    pub content_height: f32,
    pub horizontal_progress: f32,
    pub vertical_progress: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedCapacity {
    Full,
    Clamped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedEnvelopeGeometry {
    pub outer: GeometryRect,
    pub compact: GeometryRect,
    pub content: GeometryRect,
    pub structural: GeometryRect,
    pub painted_request: GeometryRect,
    pub capacity: SharedCapacity,
}

impl SharedEnvelopeGeometry {
    /// Bottom anchored. The structural minimum must fit; excess painted/content
    /// requests are clamped explicitly, never allowed to invert a region.
    pub fn resolve(i: SharedEnvelopeInput) -> Result<Self, GeometryError> {
        for r in [i.structural, i.painted_request, i.safe] {
            if !r.width().is_finite() || !r.height().is_finite() {
                return Err(GeometryError::NonFinite("shared.request_extent"));
            }
        }
        for (name, value) in [
            ("shared.content_width", i.content_width),
            ("shared.content_height", i.content_height),
            ("shared.radius", i.radius),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
            if value < 0.0 {
                return Err(GeometryError::InvalidParameter {
                    name,
                    value,
                    requirement: "nonnegative",
                });
            }
        }
        for p in [i.horizontal_progress, i.vertical_progress] {
            super::UnitInterval::try_new(p)?;
        }
        if i.structural.width() <= 0.0
            || i.structural.height() <= 0.0
            || i.safe.width() < i.structural.width()
            || i.safe.height() < i.structural.height()
            || i.structural.bottom() != i.safe.bottom()
            || i.structural.left() < i.safe.left()
            || i.structural.right() > i.safe.right()
        {
            return Err(GeometryError::InvalidParameter {
                name: "shared.structural",
                value: i.structural.width(),
                requirement: "positive bottom-anchored minimum inside safe bounds",
            });
        }
        if i.painted_request.bottom() > i.safe.bottom() {
            return Err(GeometryError::InvalidDirectionalPlacement {
                signed_gap: i.safe.bottom() - i.painted_request.bottom(),
            });
        }
        let request_left = i.structural.left().min(i.painted_request.left());
        let request_right = i.structural.right().max(i.painted_request.right());
        let request_w = request_right - request_left;
        let request_h = i.safe.bottom() - i.structural.top().min(i.painted_request.top());
        let wanted_w = request_w.max(i.content_width * i.horizontal_progress);
        let wanted_h = request_h + i.content_height * i.vertical_progress;
        let width = wanted_w.min(i.safe.width());
        let height = wanted_h.min(i.safe.height());
        let cx = (request_left + request_w * 0.5)
            .clamp(i.safe.left() + width * 0.5, i.safe.right() - width * 0.5);
        let outer = GeometryRect::try_new(
            cx - width * 0.5,
            i.safe.bottom() - height,
            cx + width * 0.5,
            i.safe.bottom(),
            i.radius,
        )?;
        let compact_h = request_h.min(height);
        let compact_w = request_w.min(width);
        let compact = GeometryRect::try_new(
            cx - compact_w * 0.5,
            i.safe.bottom() - compact_h,
            cx + compact_w * 0.5,
            i.safe.bottom(),
            0.0,
        )?;
        let content_w = (i.content_width * i.horizontal_progress).min(width);
        let content = GeometryRect::try_new(
            cx - content_w * 0.5,
            outer.top(),
            cx + content_w * 0.5,
            compact.top(),
            0.0,
        )?;
        Ok(Self {
            outer,
            compact,
            content,
            structural: i.structural,
            painted_request: i.painted_request,
            capacity: if wanted_w > width
                || wanted_h > height
                || request_left < i.safe.left()
                || request_right > i.safe.right()
            {
                SharedCapacity::Clamped
            } else {
                SharedCapacity::Full
            },
        })
    }

    pub fn contains_paint(&self, x: f32, y: f32) -> bool {
        rounded_contains(self.outer, x, y)
    }

    pub fn input_regions(&self) -> Vec<GeometryRect> {
        rounded_input(self.outer)
    }
}

pub fn rounded_contains(r: GeometryRect, x: f32, y: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !r.contains_point(x, y) {
        return false;
    }
    let radius = f64::from(r.radius());
    let (x, y) = (f64::from(x), f64::from(y));
    let cx = x.clamp(f64::from(r.left()) + radius, f64::from(r.right()) - radius);
    let cy = y.clamp(f64::from(r.top()) + radius, f64::from(r.bottom()) - radius);
    (x - cx).powi(2) + (y - cy).powi(2) <= radius.powi(2) + 1e-8
}

/// Conservative horizontal bands; clearance uses both exact band endpoints,
/// including the outer top/bottom. Integer edges also survive GPUI 0.3.5's
/// Wayland origin/size truncation without expanding outside paint.
pub fn rounded_input(r: GeometryRect) -> Vec<GeometryRect> {
    let mut result = Vec::new();
    let mut y = r.top().ceil();
    let end = r.bottom().floor();
    let band_height = (r.height() / 4096.0).ceil().max(4.0);
    for index in 0..4096 {
        if y >= end {
            break;
        }
        let bottom = if index == 4095 {
            end
        } else {
            (y + band_height).min(end)
        };
        if bottom <= y {
            break;
        }
        let clearance = (y - r.top()).min(r.bottom() - bottom).max(0.0);
        let radius = r.radius();
        let inset = if clearance >= radius {
            0.5
        } else {
            radius
                - (radius * radius - (radius - clearance).powi(2))
                    .max(0.0)
                    .sqrt()
                + 0.5
        };
        let left = (r.left() + inset).ceil();
        let right = (r.right() - inset).floor();
        if right > left {
            result.push(
                GeometryRect::try_new(left, y, right, bottom, 0.0)
                    .expect("finite conservative band"),
            );
        }
        y = bottom;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> SharedEnvelopeInput {
        let dock = GeometryRect::try_new(100.0, 400.0, 400.0, 460.0, 30.0).unwrap();
        SharedEnvelopeInput {
            structural: dock,
            painted_request: dock,
            safe: GeometryRect::try_new(10.0, 10.0, 490.0, 460.0, 0.0).unwrap(),
            content_width: 360.0,
            content_height: 160.0,
            horizontal_progress: 1.0,
            vertical_progress: 1.0,
            radius: 30.0,
        }
    }
    #[test]
    fn endpoints_partition_and_bottom_baseline() {
        let mut i = input();
        let open = SharedEnvelopeGeometry::resolve(i).unwrap();
        assert_eq!(open.outer.width(), 360.0);
        assert_eq!(open.outer.height(), 220.0);
        assert_eq!(open.content.bottom(), open.compact.top());
        assert_eq!(open.outer.bottom(), 460.0);
        i.horizontal_progress = 0.0;
        i.vertical_progress = 0.0;
        let closed = SharedEnvelopeGeometry::resolve(i).unwrap();
        assert_eq!(closed.outer, i.structural);
        assert_eq!(closed.content.height(), 0.0);
        i.content_width = 20.0;
        i.horizontal_progress = 1.0;
        i.vertical_progress = 1.0;
        assert_eq!(
            SharedEnvelopeGeometry::resolve(i).unwrap().outer.width(),
            300.0
        );
    }
    #[test]
    fn painted_request_does_not_change_structural_identity() {
        let mut i = input();
        i.painted_request = GeometryRect::try_new(40.0, 380.0, 460.0, 460.0, 30.0).unwrap();
        let g = SharedEnvelopeGeometry::resolve(i).unwrap();
        assert_eq!(g.structural, i.structural);
        assert_eq!(g.outer.width(), 420.0);
        assert_eq!(g.outer.height(), 240.0);
        assert_eq!(g.content.bottom(), g.compact.top());
        i.painted_request = GeometryRect::try_new(70.0, 370.0, 380.0, 440.0, 0.0).unwrap();
        let g = SharedEnvelopeGeometry::resolve(i).unwrap();
        assert!(g.outer.contains(i.painted_request));
        assert!(g.outer.contains(i.structural));
        assert_eq!(g.structural, i.structural);
        assert_eq!(g.outer.bottom(), 460.0);
        i.painted_request = GeometryRect::try_new(-10.0, 370.0, 380.0, 440.0, 0.0).unwrap();
        assert_eq!(
            SharedEnvelopeGeometry::resolve(i).unwrap().capacity,
            SharedCapacity::Clamped
        );
        i.painted_request = GeometryRect::try_new(70.0, 370.0, 380.0, 461.0, 0.0).unwrap();
        assert!(matches!(
            SharedEnvelopeGeometry::resolve(i),
            Err(GeometryError::InvalidDirectionalPlacement { .. })
        ));
    }
    #[test]
    fn clamps_excess_and_refuses_impossible_minimum_or_hostile_progress() {
        let mut i = input();
        i.content_height = 1000.0;
        let g = SharedEnvelopeGeometry::resolve(i).unwrap();
        assert_eq!(g.capacity, SharedCapacity::Clamped);
        assert_eq!(g.outer.top(), 10.0);
        i.safe = GeometryRect::try_new(10.0, 410.0, 490.0, 460.0, 0.0).unwrap();
        assert!(SharedEnvelopeGeometry::resolve(i).is_err());
        for p in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut i = input();
            i.vertical_progress = p;
            assert!(SharedEnvelopeGeometry::resolve(i).is_err());
        }
    }
    #[test]
    fn every_input_band_corner_is_inside_paint_during_motion_and_capacity_clamp() {
        for step in 0..=100 {
            let mut i = input();
            i.horizontal_progress = step as f32 / 100.0;
            i.vertical_progress = i.horizontal_progress;
            i.content_width = 700.0;
            let g = SharedEnvelopeGeometry::resolve(i).unwrap();
            assert!(g.content.height() >= 0.0);
            for r in g.input_regions() {
                assert_eq!(r.left().fract(), 0.0);
                assert_eq!(r.top().fract(), 0.0);
                assert_eq!(r.width().fract(), 0.0);
                assert_eq!(r.height().fract(), 0.0);
                for x in [r.left(), r.right()] {
                    for y in [r.top(), r.bottom()] {
                        assert!(g.contains_paint(x, y), "{g:?} {r:?}");
                    }
                }
            }
        }
    }
}
