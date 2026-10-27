//! Resolved tray item geometry, used to derive interaction anchors.
//!
//! A widget reports the rectangle it actually resolved. It never supplies a
//! screen coordinate. Translating that rectangle into the protocol's
//! coordinate hint happens here, where the owning output origin is known, so a
//! cardinal-placement change recomputes the hint from geometry instead of
//! reusing a stale position.

use crate::product_bar::BarOrientation;

/// A rectangle resolved inside one output's logical coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedItemRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub orientation: BarOrientation,
}

impl ResolvedItemRect {
    pub fn new(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        orientation: BarOrientation,
    ) -> Option<Self> {
        let rect = Self {
            left,
            top,
            right,
            bottom,
            orientation,
        };
        rect.is_resolved().then_some(rect)
    }

    /// A rectangle is usable as an anchor source only when it is finite,
    /// non-empty, and correctly ordered.
    pub fn is_resolved(&self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .iter()
            .all(|value| value.is_finite())
            && self.right > self.left
            && self.bottom > self.top
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// The point a click inside this rectangle targets.
    pub fn center(&self) -> (f32, f32) {
        (
            self.left + self.width() / 2.0,
            self.top + self.height() / 2.0,
        )
    }

    /// Lays items out along the tray's own axis, in order.
    ///
    /// This is how the Bar turns a presentation model into real rectangles; it
    /// is deliberately geometry-only and knows nothing about bus identity.
    pub fn layout_run(
        orientation: BarOrientation,
        origin: (f32, f32),
        extent: f32,
        item_extents: &[f32],
        gap: f32,
    ) -> Vec<Option<ResolvedItemRect>> {
        if !extent.is_finite() || !gap.is_finite() || gap < 0.0 {
            return vec![None; item_extents.len()];
        }

        match orientation {
            BarOrientation::Horizontal => {
                let mut cursor = origin.0;
                item_extents
                    .iter()
                    .map(|item_extent| {
                        let width = sanitize(*item_extent);
                        let rect = ResolvedItemRect::new(
                            cursor,
                            origin.1,
                            cursor + width,
                            origin.1 + extent,
                            BarOrientation::Horizontal,
                        );
                        cursor += width + gap;
                        rect
                    })
                    .collect()
            }
            BarOrientation::Vertical => {
                let mut cursor = origin.1;
                item_extents
                    .iter()
                    .map(|item_extent| {
                        let height = sanitize(*item_extent);
                        let rect = ResolvedItemRect::new(
                            origin.0,
                            cursor,
                            origin.0 + extent,
                            cursor + height,
                            BarOrientation::Vertical,
                        );
                        cursor += height + gap;
                        rect
                    })
                    .collect()
            }
        }
    }
}

fn sanitize(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> BarOrientation {
        BarOrientation::Horizontal
    }

    fn v() -> BarOrientation {
        BarOrientation::Vertical
    }

    #[test]
    fn a_rect_must_be_finite_and_non_empty_to_anchor() {
        assert!(ResolvedItemRect::new(0.0, 0.0, 10.0, 10.0, h()).is_some());
        assert!(
            ResolvedItemRect::new(10.0, 0.0, 10.0, 10.0, h()).is_none(),
            "a zero-width rect is not a resolvable click target"
        );
        assert!(
            ResolvedItemRect::new(20.0, 0.0, 10.0, 10.0, h()).is_none(),
            "an inverted rect is not a resolvable click target"
        );
        assert!(ResolvedItemRect::new(f32::NAN, 0.0, 10.0, 10.0, h()).is_none());
    }

    #[test]
    fn center_is_the_point_actually_clicked() {
        let rect = ResolvedItemRect::new(10.0, 20.0, 30.0, 60.0, h()).unwrap();
        assert_eq!(rect.center(), (20.0, 40.0));
    }

    #[test]
    fn horizontal_and_vertical_runs_lay_out_on_their_own_axis() {
        let horizontal = ResolvedItemRect::layout_run(h(), (5.0, 2.0), 20.0, &[10.0, 12.0], 4.0);
        assert_eq!(horizontal[0].unwrap().left, 5.0);
        assert_eq!(horizontal[0].unwrap().right, 15.0);
        assert_eq!(horizontal[1].unwrap().left, 19.0, "gap separates items");
        assert_eq!(horizontal[1].unwrap().top, 2.0);

        let vertical = ResolvedItemRect::layout_run(v(), (2.0, 5.0), 20.0, &[10.0, 12.0], 4.0);
        assert_eq!(vertical[0].unwrap().top, 5.0);
        assert_eq!(vertical[0].unwrap().bottom, 15.0);
        assert_eq!(vertical[1].unwrap().top, 19.0);
        assert_eq!(vertical[1].unwrap().left, 2.0);
    }

    #[test]
    fn a_cardinal_change_of_orientation_produces_different_geometry() {
        let horizontal = ResolvedItemRect::layout_run(h(), (0.0, 0.0), 10.0, &[4.0, 4.0], 2.0);
        let vertical = ResolvedItemRect::layout_run(v(), (0.0, 0.0), 10.0, &[4.0, 4.0], 2.0);
        assert_ne!(
            horizontal[1].unwrap(),
            vertical[1].unwrap(),
            "moving the bar to another edge must move the resolved geometry"
        );
    }

    #[test]
    fn a_bad_run_yields_no_anchors_rather_than_fabricated_ones() {
        let degenerate = ResolvedItemRect::layout_run(h(), (0.0, 0.0), 10.0, &[4.0], f32::NAN);
        assert!(degenerate[0].is_none());

        let negative_gap = ResolvedItemRect::layout_run(h(), (0.0, 0.0), 10.0, &[4.0], -1.0);
        assert!(negative_gap[0].is_none());
    }

    #[test]
    fn non_finite_item_extents_collapse_to_zero_rather_than_poisoning_the_run() {
        let run = ResolvedItemRect::layout_run(h(), (0.0, 0.0), 10.0, &[f32::NAN, 4.0], 1.0);
        assert!(
            run[0].is_none(),
            "an unresolvable extent yields no rectangle"
        );
        assert!(run[1].is_some(), "the run continues for its neighbours");
    }
}
