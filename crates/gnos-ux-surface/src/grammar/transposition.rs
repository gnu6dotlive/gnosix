//! Involutive axis exchange for a cardinal parent-local solve. Not placement intent.
use super::{GeometryRect, Point2};
pub(crate) fn point(p: Point2) -> Point2 {
    Point2::new(p.y(), p.x())
}
pub(crate) fn rect(r: GeometryRect) -> GeometryRect {
    GeometryRect::try_new(r.top(), r.left(), r.bottom(), r.right(), r.radius())
        .expect("axis exchange preserves valid geometry")
}
