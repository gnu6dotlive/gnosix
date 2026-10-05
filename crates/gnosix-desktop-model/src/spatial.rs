use crate::output::{LogicalPoint, LogicalSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LogicalRect {
    pub origin: LogicalPoint,
    pub size: LogicalSize,
}

impl LogicalRect {
    pub const fn new(origin: LogicalPoint, size: LogicalSize) -> Self {
        Self { origin, size }
    }

    fn edges(self) -> (i64, i64, i64, i64) {
        let left = i64::from(self.origin.x);
        let top = i64::from(self.origin.y);
        let right = left + i64::from(self.size.width.get());
        let bottom = top + i64::from(self.size.height.get());
        (left, top, right, bottom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpatialRelation {
    Equal,
    Contains,
    ContainedBy,
    Overlaps,
    Touches,
    Disjoint,
}

pub fn classify_spatial_relation(a: LogicalRect, b: LogicalRect) -> SpatialRelation {
    if a == b {
        return SpatialRelation::Equal;
    }

    let (al, at, ar, ab) = a.edges();
    let (bl, bt, br, bb) = b.edges();

    let a_contains_b = al <= bl && at <= bt && ar >= br && ab >= bb;
    if a_contains_b {
        return SpatialRelation::Contains;
    }

    let b_contains_a = bl <= al && bt <= at && br >= ar && bb >= ab;
    if b_contains_a {
        return SpatialRelation::ContainedBy;
    }

    let overlap_x = al.max(bl) < ar.min(br);
    let overlap_y = at.max(bt) < ab.min(bb);
    if overlap_x && overlap_y {
        return SpatialRelation::Overlaps;
    }

    let closed_overlap_x = al.max(bl) <= ar.min(br);
    let closed_overlap_y = at.max(bt) <= ab.min(bb);
    let touches_vertical_boundary = (ar == bl || br == al) && closed_overlap_y;
    let touches_horizontal_boundary = (ab == bt || bb == at) && closed_overlap_x;

    if touches_vertical_boundary || touches_horizontal_boundary {
        SpatialRelation::Touches
    } else {
        SpatialRelation::Disjoint
    }
}
