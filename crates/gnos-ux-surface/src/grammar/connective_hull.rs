//! Backend-independent closed contours for parent-derived reactive surfaces.

use super::{
    GeometryError, GeometryRect, Point2,
    reactive_envelope::{ReactiveAttachment, ReactiveGrowthDirection},
};

const MAX_COMMANDS: usize = 64;
const CONTOUR_TOLERANCE: f32 = 0.001;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HullCommand {
    Empty,
    MoveTo(Point2),
    LineTo(Point2),
    CubicTo {
        control1: Point2,
        control2: Point2,
        to: Point2,
    },
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectiveHullInput {
    pub parent: GeometryRect,
    pub child: GeometryRect,
    pub attachment: ReactiveAttachment,
    pub topology_phase: f32,
    pub attachment_phase: f32,
    pub release_phase: f32,
    pub reattach_phase: f32,
    pub neck_width: f32,
    pub mouth_width: f32,
    pub shoulder_run: f32,
    pub shoulder_center_x: f32,
    /// The child's external edge, on the edge facing *away* from the joint.
    ///
    /// Consumed across the child's far edge while the outline is traversed, so
    /// it belongs there in both directions. A contour supplied on the joint-facing
    /// edge forces the traversal to double back across itself, which cancels the
    /// child's interior under the fill rule.
    pub outer_contour: [Point2; 5],
    pub openness: f32,
    /// Which way the child grows away from the parent's joint.
    ///
    /// Explicit rather than inferred from where the two rectangles happen to sit:
    /// the joint is a modelled fact about the lineage, and reading it back out of
    /// geometry is exactly the inference the product layer rejects elsewhere.
    pub growth: ReactiveGrowthDirection,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectiveHullGeometry {
    commands: [HullCommand; MAX_COMMANDS],
    command_count: u8,
    bounds: GeometryRect,
    contour_count: u8,
    flush_joint: bool,
}

impl ConnectiveHullGeometry {
    pub fn resolve(input: ConnectiveHullInput) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("hull.attachment_phase", input.attachment_phase),
            ("hull.topology_phase", input.topology_phase),
            ("hull.release_phase", input.release_phase),
            ("hull.reattach_phase", input.reattach_phase),
            ("hull.neck_width", input.neck_width),
            ("hull.mouth_width", input.mouth_width),
            ("hull.shoulder_run", input.shoulder_run),
            ("hull.shoulder_center_x", input.shoulder_center_x),
            ("hull.openness", input.openness),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        let parent = input.parent;
        let child = input.child;
        let frame = JointFrame::new(input.growth);
        let signed_gap = frame.gap(parent, child);
        if signed_gap < 0.0 {
            return Err(GeometryError::InvalidDirectionalPlacement { signed_gap });
        }
        validate_outer_contour(input)?;
        let bounds = GeometryRect::try_new(
            parent.left().min(child.left()),
            parent.top().min(child.top()),
            parent.right().max(child.right()),
            parent.bottom().max(child.bottom()),
            0.0,
        )?;
        let mut builder = HullBuilder::new(bounds);
        if input.openness <= 0.01 || child.height() <= 1.0 {
            builder.rounded_rect(parent)?;
        } else if input.attachment == ReactiveAttachment::Detached {
            if input.reattach_phase > 0.001 {
                builder.detached_pair(input, smooth(input.reattach_phase), true)?;
            } else {
                let residual = 1.0 - smooth(input.release_phase);
                builder.detached_pair(input, residual, false)?;
            }
        } else if input.attachment == ReactiveAttachment::Bridged && input.reattach_phase < 0.999 {
            builder.detached_pair(input, smooth(input.reattach_phase), true)?;
        } else {
            if input.attachment == ReactiveAttachment::Fused && signed_gap == 0.0 {
                builder.flush_fused(input)?;
            } else {
                builder.connected(input)?;
            }
        }
        Ok(builder.finish())
    }

    /// Exchange axes of the exact closed outline, including all cubic controls.
    pub(crate) fn transposed(mut self) -> Self {
        use super::transposition::{point, rect};
        for command in &mut self.commands {
            *command = match *command {
                HullCommand::MoveTo(p) => HullCommand::MoveTo(point(p)),
                HullCommand::LineTo(p) => HullCommand::LineTo(point(p)),
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => HullCommand::CubicTo {
                    control1: point(control1),
                    control2: point(control2),
                    to: point(to),
                },
                other => other,
            };
        }
        self.bounds = rect(self.bounds);
        self
    }

    pub const fn bounds(self) -> GeometryRect {
        self.bounds
    }
    pub const fn command_count(self) -> usize {
        self.command_count as usize
    }
    pub fn command(self, index: usize) -> Option<HullCommand> {
        (index < self.command_count()).then(|| self.commands[index])
    }
    pub const fn contour_count(self) -> usize {
        self.contour_count as usize
    }

    /// Scan converts the same closed contours used for paint into narrow input bands.
    pub fn input_regions(self, band_height: f32) -> Vec<GeometryRect> {
        if self.flush_joint {
            return self.flush_input_regions(band_height);
        }
        let step = band_height.clamp(1.0, 8.0);
        let contours = self.flattened();
        let mut regions = Vec::new();
        let mut top = self.bounds.top();
        while top < self.bounds.bottom() {
            let bottom = (top + step).min(self.bounds.bottom());
            let mut intervals = Vec::new();
            for contour in &contours {
                let ys = [top + 0.25, (top + bottom) * 0.5, bottom - 0.25];
                let mut left = f32::NEG_INFINITY;
                let mut right = f32::INFINITY;
                for y in ys {
                    if let Some((a, b)) = scan_interval(contour, y) {
                        left = left.max(a);
                        right = right.min(b);
                    } else {
                        left = f32::INFINITY;
                        break;
                    }
                }
                if left.is_finite() && right > left + 1.0 {
                    intervals.push((left + 0.5, right - 0.5));
                }
            }
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (left, right) in intervals {
                if let Ok(region) = GeometryRect::try_new(left, top, right, bottom, 0.0) {
                    regions.push(region);
                }
            }
            top = bottom;
        }
        regions
    }

    // Flush bodies use polygon-conservative bands: intersect the painted
    // intervals at each vertex height and between vertices. Legacy non-flush
    // inputs retain their exact projection for Auto compatibility.
    fn flush_input_regions(self, band_height: f32) -> Vec<GeometryRect> {
        let contours = hull_probe::contours(self);
        let step = band_height.clamp(1.0, 8.0);
        let mut regions = Vec::new();
        let mut top = self.bounds.top();
        while top < self.bounds.bottom() {
            let bottom = (top + step).min(self.bounds.bottom());
            for contour in &contours {
                let mut ys = vec![top, bottom];
                ys.extend(
                    contour
                        .iter()
                        .map(|p| p.y())
                        .filter(|y| *y > top && *y < bottom),
                );
                ys.sort_by(f32::total_cmp);
                ys.dedup();
                let mids = ys
                    .windows(2)
                    .map(|w| (w[0] + w[1]) * 0.5)
                    .collect::<Vec<_>>();
                ys.extend(mids);
                let mut common = vec![(self.bounds.left(), self.bounds.right())];
                for y in ys {
                    let spans = hull_probe::filled_intervals_at(contour, y);
                    let mut next = Vec::new();
                    for &(a, b) in &common {
                        for &(c, d) in &spans {
                            let left = a.max(c);
                            let right = b.min(d);
                            if right > left {
                                next.push((left, right));
                            }
                        }
                    }
                    common = next;
                    if common.is_empty() {
                        break;
                    }
                }
                for (left, right) in common {
                    if right > left + 1.0 {
                        regions.push(
                            GeometryRect::try_new(left + 0.5, top, right - 0.5, bottom, 0.0)
                                .expect("bounded input band"),
                        );
                    }
                }
            }
            top = bottom;
        }
        regions
    }

    fn flattened(self) -> Vec<Vec<Point2>> {
        let mut contours: Vec<Vec<Point2>> = Vec::new();
        for command in self.commands.iter().take(self.command_count()) {
            match *command {
                HullCommand::Empty => break,
                HullCommand::MoveTo(point) => contours.push(vec![point]),
                HullCommand::LineTo(point) => {
                    if let Some(contour) = contours.last_mut() {
                        contour.push(point);
                    }
                }
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    if let Some(contour) = contours.last_mut()
                        && let Some(start) = contour.last().copied()
                    {
                        for index in 1..=12 {
                            contour.push(cubic(start, control1, control2, to, index as f32 / 12.0));
                        }
                    }
                }
                HullCommand::Close => {
                    if let Some(contour) = contours.last_mut()
                        && let Some(first) = contour.first().copied()
                    {
                        contour.push(first);
                    }
                }
            }
        }
        contours
    }
}

/// The longitudinal frame one parent/child pair is described in.
///
/// The joint is the parent's growth-facing edge and local y is positive along
/// the direction of growth, so the joint sits at local 0 and the child lies at
/// positive local y whichever way the body grows. Every longitudinal quantity -
/// the gap, the neck radius, the corner centres, the contour placement - is
/// expressed here rather than against a world axis.
///
/// The accessors return the caller's own edge coordinate instead of a value
/// rebuilt from a shifted local coordinate. That is deliberate. Writing the
/// outline as `joint + (world - joint)` is algebraically identical, and in
/// `f32` it is not *bit* identical: `((a - b) + b)` can differ from `a` by an
/// ulp. This construction is on the path R09/R13 qualified, so "identical" has
/// to mean identical. Selecting the edge keeps the Downward outline exactly the
/// outline it already was, and the direction-neutral work is then provably
/// additive.
#[derive(Clone, Copy, Debug)]
struct JointFrame {
    direction: ReactiveGrowthDirection,
}

impl JointFrame {
    fn new(direction: ReactiveGrowthDirection) -> Self {
        Self { direction }
    }

    /// `+1` when growth proceeds toward increasing world y, `-1` otherwise.
    fn sign(self) -> f32 {
        self.direction.sign()
    }

    /// The parent's growth-facing edge: the joint the child grows off.
    fn parent_joint(self, parent: GeometryRect) -> f32 {
        self.direction.joint(parent)
    }

    /// The parent's edge away from the joint.
    fn parent_far(self, parent: GeometryRect) -> f32 {
        self.direction.joint_opposite(parent)
    }

    /// The child's edge facing the joint.
    fn child_facing(self, child: GeometryRect) -> f32 {
        match self.direction {
            ReactiveGrowthDirection::Downward => child.top(),
            ReactiveGrowthDirection::Upward => child.bottom(),
        }
    }

    /// The child's edge away from the joint.
    fn child_far(self, child: GeometryRect) -> f32 {
        match self.direction {
            ReactiveGrowthDirection::Downward => child.bottom(),
            ReactiveGrowthDirection::Upward => child.top(),
        }
    }

    /// Displace a longitudinal coordinate along the growth axis.
    ///
    /// A positive `amount` moves in the direction the body grows. Applied to the
    /// parent's far edge that moves toward the joint; applied to the joint it
    /// moves away from it. One displacement covers both because for either
    /// direction of growth those two are the same displacement.
    fn along_growth(self, edge: f32, amount: f32) -> f32 {
        if self.sign() > 0.0 {
            edge + amount
        } else {
            edge - amount
        }
    }

    /// Distance from the joint to the child's facing edge, never negative.
    fn gap(self, parent: GeometryRect, child: GeometryRect) -> f32 {
        (self.child_facing(child) - self.parent_joint(parent)) * self.sign()
    }

    /// Hold a longitudinal coordinate on the child's side of the joint.
    ///
    /// A residual tail must not reach back through the parent. "Back through"
    /// is a larger y when growing downward and a smaller y when growing upward,
    /// so the comparison has to follow the growth axis rather than the world.
    fn never_past_joint(self, value: f32, joint: f32) -> f32 {
        if self.sign() > 0.0 {
            value.max(joint)
        } else {
            value.min(joint)
        }
    }

    /// Reflect an angle about the horizontal axis.
    ///
    /// Mirroring the outline in y maps every corner sweep to its negation, which
    /// is what keeps the corner tangents continuous in the mirrored case instead
    /// of reversing the arc and leaving a cusp at every joint.
    fn arc(self, angle: f32) -> f32 {
        if self.sign() > 0.0 { angle } else { -angle }
    }
}

struct HullBuilder {
    geometry: ConnectiveHullGeometry,
}

/// Samples run left to right across the child's far boundary. Interior points
/// may indent into its body, but cannot cross the facing roundover. The public
/// resolver rejects malformed samples instead of replacing them with a line.
fn validate_outer_contour(input: ConnectiveHullInput) -> Result<(), GeometryError> {
    let child = input.child;
    let frame = JointFrame::new(input.growth);
    let far = frame.child_far(child);
    let left = child.left() + child.radius();
    let right = child.right() - child.radius();
    for (index, point) in input.outer_contour.iter().enumerate() {
        let invalid = |reason| GeometryError::InvalidOuterContour { index, reason };
        if !point.x().is_finite() || !point.y().is_finite() {
            return Err(invalid("coordinates must be finite"));
        }
        if point.x() < child.left() - CONTOUR_TOLERANCE
            || point.x() > child.right() + CONTOUR_TOLERANCE
            || point.y() < child.top() - CONTOUR_TOLERANCE
            || point.y() > child.bottom() + CONTOUR_TOLERANCE
        {
            return Err(invalid("point lies outside child bounds"));
        }
        // A collapsed/dormant child is not consumed by the path builder.
        if input.openness <= 0.01 || child.height() <= 1.0 {
            continue;
        }
        if point.x() < left - CONTOUR_TOLERANCE || point.x() > right + CONTOUR_TOLERANCE {
            return Err(invalid("point crosses a side roundover"));
        }
        if index > 0 && point.x() < input.outer_contour[index - 1].x() {
            return Err(invalid("samples must traverse left to right"));
        }
        if (point.y() - frame.child_facing(child)) * frame.sign()
            < child.radius() - CONTOUR_TOLERANCE
        {
            return Err(invalid("point crosses the facing roundover"));
        }
        if index == 0 || index == 4 {
            let endpoint_x = if index == 0 { left } else { right };
            if (point.x() - endpoint_x).abs() > CONTOUR_TOLERANCE
                || (point.y() - far).abs() > CONTOUR_TOLERANCE
            {
                return Err(invalid("endpoint must meet its far-edge roundover"));
            }
        }
    }
    Ok(())
}

impl HullBuilder {
    fn new(bounds: GeometryRect) -> Self {
        Self {
            geometry: ConnectiveHullGeometry {
                commands: [HullCommand::Empty; MAX_COMMANDS],
                command_count: 0,
                bounds,
                contour_count: 0,
                flush_joint: false,
            },
        }
    }
    fn push(&mut self, command: HullCommand) {
        let index = self.geometry.command_count as usize;
        assert!(index < MAX_COMMANDS, "connective hull command capacity");
        self.geometry.commands[index] = command;
        self.geometry.command_count += 1;
    }
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(HullCommand::MoveTo(Point2::new(x, y)));
        self.geometry.contour_count += 1;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(HullCommand::LineTo(Point2::new(x, y)));
    }
    fn cubic_to(&mut self, c1: (f32, f32), c2: (f32, f32), to: (f32, f32)) {
        self.push(HullCommand::CubicTo {
            control1: Point2::new(c1.0, c1.1),
            control2: Point2::new(c2.0, c2.1),
            to: Point2::new(to.0, to.1),
        });
    }
    fn close(&mut self) {
        self.push(HullCommand::Close);
    }
    fn finish(self) -> ConnectiveHullGeometry {
        self.geometry
    }

    fn corner(&mut self, center: (f32, f32), radius: f32, start_angle: f32, end_angle: f32) {
        if radius <= 0.01 {
            return;
        }
        let (sx, sy) = (
            center.0 + radius * start_angle.cos(),
            center.1 + radius * start_angle.sin(),
        );
        let (ex, ey) = (
            center.0 + radius * end_angle.cos(),
            center.1 + radius * end_angle.sin(),
        );
        let sweep = end_angle - start_angle;
        let handle = 4.0 / 3.0 * (sweep / 4.0).tan() * radius;
        let c1 = (
            sx - start_angle.sin() * handle,
            sy + start_angle.cos() * handle,
        );
        let c2 = (ex + end_angle.sin() * handle, ey - end_angle.cos() * handle);
        self.cubic_to(c1, c2, (ex, ey));
    }
    fn rounded_rect(&mut self, rect: GeometryRect) -> Result<(), GeometryError> {
        let r = rect
            .radius()
            .min(rect.width() * 0.5)
            .min(rect.height() * 0.5);
        self.move_to(rect.left() + r, rect.top());
        self.line_to(rect.right() - r, rect.top());
        self.corner(
            (rect.right() - r, rect.top() + r),
            r,
            -std::f32::consts::FRAC_PI_2,
            0.0,
        );
        self.line_to(rect.right(), rect.bottom() - r);
        self.corner(
            (rect.right() - r, rect.bottom() - r),
            r,
            0.0,
            std::f32::consts::FRAC_PI_2,
        );
        self.line_to(rect.left() + r, rect.bottom());
        self.corner(
            (rect.left() + r, rect.bottom() - r),
            r,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        );
        self.line_to(rect.left(), rect.top() + r);
        self.corner(
            (rect.left() + r, rect.top() + r),
            r,
            std::f32::consts::PI,
            3.0 * std::f32::consts::FRAC_PI_2,
        );
        self.close();
        Ok(())
    }
    fn connected(&mut self, input: ConnectiveHullInput) -> Result<(), GeometryError> {
        let p = input.parent;
        let c = input.child;
        // Every longitudinal quantity below is read through this frame, so the
        // outline is the same shape whichever way the child grew.
        let f = JointFrame::new(input.growth);
        let joint = f.parent_joint(p);
        let parent_far = f.parent_far(p);
        let child_facing = f.child_facing(c);
        let pr = p.radius().min(p.height() * 0.5);
        let cr = c.radius().min(c.height() * 0.5);
        let center = input.shoulder_center_x.clamp(p.left(), p.right());
        let child_half = c.width() * 0.5;
        let child_right = c.right() - cr;
        let child_left = c.left() + cr;
        let available_mouth_half = (child_right - center)
            .min(center - child_left)
            .min(p.width() * 0.48)
            .max(8.0);
        let mouth_half = (input.mouth_width * 0.5).min(available_mouth_half).max(8.0);
        // The joint gap, measured along the growth axis. This is the quantity
        // that was hard-wired to `child.top() - parent.bottom()`, which collapses
        // to zero for an upward child and leaves the neck with nothing to span.
        let gap = f.gap(p, c);
        // At rest this is exactly the shared corner radius. During fusion the
        // opposing roundovers deform only as much as the remaining gap permits.
        let neck_radius = pr.min(cr).min(gap * 0.5).min((mouth_half - 4.0).max(0.0));
        let maximum_neck_half = (mouth_half - neck_radius).max(4.0);
        let bridged_half = (input.neck_width * 0.5).clamp(4.0, maximum_neck_half);
        let fused_half = (child_half * 0.68).min(maximum_neck_half);
        let topology = smooth(input.topology_phase);
        let neck_half = fused_half + (bridged_half - fused_half) * topology;
        let rounded_right = center + neck_half + neck_radius;
        let rounded_left = center - neck_half - neck_radius;
        let attach_right = (center + mouth_half).min(p.right() - pr);
        let attach_left = (center - mouth_half).max(p.left() + pr);
        self.move_to(p.left() + pr, parent_far);
        self.line_to(p.right() - pr, parent_far);
        self.corner(
            (p.right() - pr, f.along_growth(parent_far, pr)),
            pr,
            f.arc(-std::f32::consts::FRAC_PI_2),
            f.arc(0.0),
        );
        self.line_to(p.right(), f.along_growth(joint, -pr));
        self.corner(
            (p.right() - pr, f.along_growth(joint, -pr)),
            pr,
            f.arc(0.0),
            f.arc(std::f32::consts::FRAC_PI_2),
        );
        self.line_to(attach_right, joint);
        self.line_to(rounded_right, joint);
        if neck_radius > 0.01 {
            self.corner(
                (rounded_right, f.along_growth(joint, neck_radius)),
                neck_radius,
                f.arc(-std::f32::consts::FRAC_PI_2),
                f.arc(-std::f32::consts::PI),
            );
        } else {
            self.line_to(center + neck_half, joint);
        }
        self.line_to(
            center + neck_half,
            f.along_growth(child_facing, -neck_radius),
        );
        if neck_radius > 0.01 {
            self.corner(
                (rounded_right, f.along_growth(child_facing, -neck_radius)),
                neck_radius,
                f.arc(std::f32::consts::PI),
                f.arc(std::f32::consts::FRAC_PI_2),
            );
        } else {
            self.line_to(rounded_right, child_facing);
        }
        self.line_to(child_right, child_facing);
        self.child_outer_edge(input);
        self.line_to(rounded_left, child_facing);
        if neck_radius > 0.01 {
            self.corner(
                (rounded_left, f.along_growth(child_facing, -neck_radius)),
                neck_radius,
                f.arc(std::f32::consts::FRAC_PI_2),
                f.arc(0.0),
            );
        } else {
            self.line_to(center - neck_half, child_facing);
        }
        self.line_to(center - neck_half, f.along_growth(joint, neck_radius));
        if neck_radius > 0.01 {
            self.corner(
                (rounded_left, f.along_growth(joint, neck_radius)),
                neck_radius,
                f.arc(0.0),
                f.arc(-std::f32::consts::FRAC_PI_2),
            );
        } else {
            self.line_to(rounded_left, joint);
        }
        self.line_to(attach_left, joint);
        self.line_to(p.left() + pr, joint);
        self.corner(
            (p.left() + pr, f.along_growth(joint, -pr)),
            pr,
            f.arc(std::f32::consts::FRAC_PI_2),
            f.arc(std::f32::consts::PI),
        );
        self.line_to(p.left(), f.along_growth(parent_far, pr));
        self.corner(
            (p.left() + pr, f.along_growth(parent_far, pr)),
            pr,
            f.arc(std::f32::consts::PI),
            f.arc(3.0 * std::f32::consts::FRAC_PI_2),
        );
        self.close();
        Ok(())
    }

    /// One exterior boundary at a flush joint. The child's facing roundovers
    /// are replaced by concave shoulders; there is no inner border or neck
    /// traversed twice along the shared edge.
    fn flush_fused(&mut self, input: ConnectiveHullInput) -> Result<(), GeometryError> {
        self.geometry.flush_joint = true;
        let p = input.parent;
        let c = input.child;
        let f = JointFrame::new(input.growth);
        let joint = f.parent_joint(p);
        let far = f.parent_far(p);
        let pr = p.radius();
        let cr = c.radius();
        if c.left() < p.left() + pr || c.right() > p.right() - pr {
            return Err(GeometryError::InvalidParameter {
                name: "hull.fused_joint_span",
                value: c.width(),
                requirement: "child side edges inside parent's straight joint edge",
            });
        }
        let shoulder = pr
            .min(cr)
            .min(c.left() - (p.left() + pr))
            .min(p.right() - pr - c.right())
            .min(c.height() - cr);
        self.move_to(p.left() + pr, far);
        self.line_to(p.right() - pr, far);
        self.corner(
            (p.right() - pr, f.along_growth(far, pr)),
            pr,
            f.arc(-std::f32::consts::FRAC_PI_2),
            f.arc(0.0),
        );
        self.line_to(p.right(), f.along_growth(joint, -pr));
        self.corner(
            (p.right() - pr, f.along_growth(joint, -pr)),
            pr,
            f.arc(0.0),
            f.arc(std::f32::consts::FRAC_PI_2),
        );
        self.line_to(c.right() + shoulder, joint);
        if shoulder > 0.01 {
            self.corner(
                (c.right() + shoulder, f.along_growth(joint, shoulder)),
                shoulder,
                f.arc(-std::f32::consts::FRAC_PI_2),
                f.arc(-std::f32::consts::PI),
            );
        } else {
            self.line_to(c.right(), joint);
        }
        let child_far = f.child_far(c);
        self.line_to(c.right(), f.along_growth(child_far, -cr));
        self.corner(
            (c.right() - cr, f.along_growth(child_far, -cr)),
            cr,
            f.arc(0.0),
            f.arc(std::f32::consts::FRAC_PI_2),
        );
        self.far_contour(input);
        self.corner(
            (c.left() + cr, f.along_growth(child_far, -cr)),
            cr,
            f.arc(std::f32::consts::FRAC_PI_2),
            f.arc(std::f32::consts::PI),
        );
        self.line_to(c.left(), f.along_growth(joint, shoulder));
        if shoulder > 0.01 {
            self.corner(
                (c.left() - shoulder, f.along_growth(joint, shoulder)),
                shoulder,
                f.arc(0.0),
                f.arc(-std::f32::consts::FRAC_PI_2),
            );
        } else {
            self.line_to(c.left(), joint);
        }
        self.line_to(p.left() + pr, joint);
        self.corner(
            (p.left() + pr, f.along_growth(joint, -pr)),
            pr,
            f.arc(std::f32::consts::FRAC_PI_2),
            f.arc(std::f32::consts::PI),
        );
        self.line_to(p.left(), f.along_growth(far, pr));
        self.corner(
            (p.left() + pr, f.along_growth(far, pr)),
            pr,
            f.arc(std::f32::consts::PI),
            f.arc(3.0 * std::f32::consts::FRAC_PI_2),
        );
        self.close();
        Ok(())
    }
    /// The child's own outline, from its facing edge round its far edge and
    /// back, with `outer_contour` laid along that far edge.
    ///
    /// The contour belongs on the edge *away* from the joint - the child's
    /// bottom edge growing downward, its top edge growing upward - because that
    /// is the edge this traversal is crossing. Reading it from the joint-facing
    /// edge instead makes the traversal jump back across the child and close the
    /// outline through itself.
    fn child_outer_edge(&mut self, input: ConnectiveHullInput) {
        let child = input.child;
        let f = JointFrame::new(input.growth);
        let facing = f.child_facing(child);
        let far = f.child_far(child);
        let radius = child.radius().min(child.height() * 0.5);
        self.corner(
            (child.right() - radius, f.along_growth(facing, radius)),
            radius,
            f.arc(-std::f32::consts::FRAC_PI_2),
            f.arc(0.0),
        );
        self.line_to(child.right(), f.along_growth(far, -radius));
        self.corner(
            (child.right() - radius, f.along_growth(far, -radius)),
            radius,
            f.arc(0.0),
            f.arc(std::f32::consts::FRAC_PI_2),
        );
        self.far_contour(input);
        self.corner(
            (child.left() + radius, f.along_growth(far, -radius)),
            radius,
            f.arc(std::f32::consts::FRAC_PI_2),
            f.arc(std::f32::consts::PI),
        );
        self.line_to(child.left(), f.along_growth(facing, radius));
        self.corner(
            (child.left() + radius, f.along_growth(facing, radius)),
            radius,
            f.arc(std::f32::consts::PI),
            f.arc(3.0 * std::f32::consts::FRAC_PI_2),
        );
    }
    fn far_contour(&mut self, input: ConnectiveHullInput) {
        for index in (0..4).rev() {
            let a = input.outer_contour[index + 1].tuple();
            let b = input.outer_contour[index].tuple();
            let run = (a.0 - b.0) / 3.0;
            self.cubic_to((a.0 - run, a.1), (b.0 + run, b.1), b);
        }
    }
    fn detached_pair(
        &mut self,
        input: ConnectiveHullInput,
        residual: f32,
        reattaching: bool,
    ) -> Result<(), GeometryError> {
        let p = input.parent;
        let c = input.child;
        let f = JointFrame::new(input.growth);
        let joint = f.parent_joint(p);
        let parent_far = f.parent_far(p);
        let child_facing = f.child_facing(c);
        let child_far = f.child_far(c);
        let center = (p.left() + p.right()) * 0.5;
        let depth_scale = if reattaching { 0.5 } else { 0.35 };
        let depth = (f.gap(p, c) * depth_scale).min(p.width() * 0.08) * residual.clamp(0.0, 1.0);
        let base =
            (input.neck_width * 0.30).clamp(3.0, p.width() * 0.16) * residual.clamp(0.0, 1.0);
        if depth <= 0.1 || base <= 0.1 {
            self.rounded_rect(p)?;
            self.rounded_rect(c)?;
            return Ok(());
        }
        let pr = p.radius().min(p.height() * 0.5);
        let tip_y = f.along_growth(joint, depth);
        let tip_half = (base * 0.12).max(0.5);
        self.move_to(p.left() + pr, parent_far);
        self.line_to(p.right() - pr, parent_far);
        self.corner(
            (p.right() - pr, f.along_growth(parent_far, pr)),
            pr,
            f.arc(-std::f32::consts::FRAC_PI_2),
            f.arc(0.0),
        );
        self.line_to(p.right(), f.along_growth(joint, -pr));
        self.corner(
            (p.right() - pr, f.along_growth(joint, -pr)),
            pr,
            f.arc(0.0),
            f.arc(std::f32::consts::FRAC_PI_2),
        );
        self.line_to(center + base, joint);
        self.cubic_to(
            (center + base * 0.65, f.along_growth(joint, depth * 0.35)),
            (center + tip_half, f.along_growth(tip_y, -depth * 0.2)),
            (center + tip_half, tip_y),
        );
        self.cubic_to(
            (
                center + tip_half * 0.3,
                f.along_growth(tip_y, tip_half * 0.25),
            ),
            (
                center - tip_half * 0.3,
                f.along_growth(tip_y, tip_half * 0.25),
            ),
            (center - tip_half, tip_y),
        );
        self.cubic_to(
            (center - tip_half, f.along_growth(tip_y, -depth * 0.2)),
            (center - base * 0.65, f.along_growth(joint, depth * 0.35)),
            (center - base, joint),
        );
        self.line_to(p.left() + pr, joint);
        self.corner(
            (p.left() + pr, f.along_growth(joint, -pr)),
            pr,
            f.arc(std::f32::consts::FRAC_PI_2),
            f.arc(std::f32::consts::PI),
        );
        self.line_to(p.left(), f.along_growth(parent_far, pr));
        self.corner(
            (p.left() + pr, f.along_growth(parent_far, pr)),
            pr,
            f.arc(std::f32::consts::PI),
            f.arc(3.0 * std::f32::consts::FRAC_PI_2),
        );
        self.close();
        let cr = c.radius().min(c.height() * 0.5);
        // The residual tip is measured from the child's facing edge back toward
        // the joint, and is held on the child's side of it.
        let tip_y = f.never_past_joint(f.along_growth(child_facing, -depth), joint);
        self.move_to(c.left() + cr, child_facing);
        self.line_to(center - base, child_facing);
        self.cubic_to(
            (
                center - base * 0.65,
                f.along_growth(child_facing, -depth * 0.35),
            ),
            (center - tip_half, f.along_growth(tip_y, depth * 0.2)),
            (center - tip_half, tip_y),
        );
        self.cubic_to(
            (
                center - tip_half * 0.3,
                f.along_growth(tip_y, -tip_half * 0.25),
            ),
            (
                center + tip_half * 0.3,
                f.along_growth(tip_y, -tip_half * 0.25),
            ),
            (center + tip_half, tip_y),
        );
        self.cubic_to(
            (
                center + tip_half,
                f.along_growth(child_facing, -depth * 0.35),
            ),
            (center + base * 0.65, child_facing),
            (center + base, child_facing),
        );
        self.line_to(c.right() - cr, child_facing);
        self.corner(
            (c.right() - cr, f.along_growth(child_facing, cr)),
            cr,
            f.arc(-std::f32::consts::FRAC_PI_2),
            f.arc(0.0),
        );
        self.line_to(c.right(), f.along_growth(child_far, -cr));
        self.corner(
            (c.right() - cr, f.along_growth(child_far, -cr)),
            cr,
            f.arc(0.0),
            f.arc(std::f32::consts::FRAC_PI_2),
        );
        self.line_to(c.left() + cr, child_far);
        self.corner(
            (c.left() + cr, f.along_growth(child_far, -cr)),
            cr,
            f.arc(std::f32::consts::FRAC_PI_2),
            f.arc(std::f32::consts::PI),
        );
        self.line_to(c.left(), f.along_growth(child_facing, cr));
        self.corner(
            (c.left() + cr, f.along_growth(child_facing, cr)),
            cr,
            f.arc(std::f32::consts::PI),
            f.arc(3.0 * std::f32::consts::FRAC_PI_2),
        );
        self.close();
        Ok(())
    }
}
fn smooth(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn cubic(a: Point2, b: Point2, c: Point2, d: Point2, t: f32) -> Point2 {
    let u = 1.0 - t;
    Point2::new(
        u * u * u * a.x() + 3.0 * u * u * t * b.x() + 3.0 * u * t * t * c.x() + t * t * t * d.x(),
        u * u * u * a.y() + 3.0 * u * u * t * b.y() + 3.0 * u * t * t * c.y() + t * t * t * d.y(),
    )
}
fn scan_interval(points: &[Point2], y: f32) -> Option<(f32, f32)> {
    let mut xs = Vec::new();
    for edge in points.windows(2) {
        let a = edge[0];
        let b = edge[1];
        if (a.y() <= y && b.y() > y) || (b.y() <= y && a.y() > y) {
            let t = (y - a.y()) / (b.y() - a.y());
            xs.push(a.x() + (b.x() - a.x()) * t);
        }
    }
    xs.sort_by(f32::total_cmp);
    (xs.len() >= 2).then(|| (xs[0], *xs.last().unwrap()))
}

/// Software qualification of a hull as a filled planar region.
///
/// A command stream that satisfies every bounds check can still paint almost
/// nothing: a self-intersecting outline cancels most of itself under the fill
/// rule, so `bounds` reports a large body while the interior is empty. These
/// helpers measure the filled region, which is the thing a reviewer actually
/// sees, and they are deliberately independent of how the outline was built.
pub mod hull_probe {
    use super::{ConnectiveHullGeometry, HullCommand, Point2};

    /// Fixed subdivision for software qualification, not an analytic guarantee
    /// about arbitrary cubic curves. Construction invariants also matter.
    const FLATTEN_STEPS: usize = 24;

    /// Every contour of the hull as a closed polyline, cubics flattened.
    pub fn contours(hull: ConnectiveHullGeometry) -> Vec<Vec<Point2>> {
        let mut out: Vec<Vec<Point2>> = Vec::new();
        for index in 0..hull.command_count() {
            match hull.command(index).expect("command index within hull") {
                HullCommand::Empty => break,
                HullCommand::MoveTo(point) => out.push(vec![point]),
                HullCommand::LineTo(point) => {
                    if let Some(contour) = out.last_mut() {
                        contour.push(point);
                    }
                }
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    if let Some(contour) = out.last_mut()
                        && let Some(start) = contour.last().copied()
                    {
                        for step in 1..=FLATTEN_STEPS {
                            contour.push(cubic(
                                start,
                                control1,
                                control2,
                                to,
                                step as f32 / FLATTEN_STEPS as f32,
                            ));
                        }
                    }
                }
                HullCommand::Close => {
                    if let Some(contour) = out.last_mut()
                        && let Some(first) = contour.first().copied()
                    {
                        contour.push(first);
                    }
                }
            }
        }
        out
    }

    fn cubic(start: Point2, c1: Point2, c2: Point2, to: Point2, t: f32) -> Point2 {
        let u = 1.0 - t;
        Point2::new(
            u * u * u * start.x()
                + 3.0 * u * u * t * c1.x()
                + 3.0 * u * t * t * c2.x()
                + t * t * t * to.x(),
            u * u * u * start.y()
                + 3.0 * u * u * t * c1.y()
                + 3.0 * u * t * t * c2.y()
                + t * t * t * to.y(),
        )
    }

    fn orient(o: Point2, a: Point2, b: Point2) -> f32 {
        (a.x() - o.x()) * (b.y() - o.y()) - (a.y() - o.y()) * (b.x() - o.x())
    }

    /// Proper segment crossing.
    ///
    /// Both pairs of sides must be *strictly* opposite, so a touch at a shared
    /// point and a collinear overlap are both excluded. The outline legitimately
    /// lays several edges along one line - the joint edge in particular - and a
    /// test that counts those reports a simple outline as broken.
    fn crosses(p: Point2, q: Point2, r: Point2, s: Point2) -> bool {
        fn opposite(a: f32, b: f32) -> bool {
            (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0)
        }
        opposite(orient(r, s, p), orient(r, s, q)) && opposite(orient(p, q, r), orient(p, q, s))
    }

    /// How many pairs of non-adjacent edges of a closed contour intersect.
    ///
    /// A closed outline that encloses a region must be simple. Self-intersection
    /// is what makes the fill rule - rather than the geometry - decide what gets
    /// painted, so a count above zero is a construction defect and not a
    /// presentation choice.
    pub fn self_intersections(contour: &[Point2]) -> usize {
        const EPS: f32 = 0.001;
        let mut points: Vec<Point2> = Vec::new();
        for &p in contour {
            if points.last().is_none_or(|q| p.distance_to(*q) > EPS) {
                points.push(p);
            }
        }
        if points.len() > 1 && points[0].distance_to(*points.last().unwrap()) <= EPS {
            points.pop();
        }
        let n = points.len();
        if n < 3 {
            return 0;
        }
        let on_segment = |p: Point2, a: Point2, b: Point2| {
            orient(a, b, p).abs() <= EPS * a.distance_to(b)
                && p.x() >= a.x().min(b.x()) - EPS
                && p.x() <= a.x().max(b.x()) + EPS
                && p.y() >= a.y().min(b.y()) - EPS
                && p.y() <= a.y().max(b.y()) + EPS
        };
        let mut hits = 0;
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];
            for j in i + 1..n {
                let c = points[j];
                let d = points[(j + 1) % n];
                if j == i + 1 || (i == 0 && j == n - 1) {
                    // Consecutive collinear edges are fine only when they keep
                    // traversing forward. Reversal retraces the boundary.
                    let dot = (b.x() - a.x()) * (d.x() - c.x()) + (b.y() - a.y()) * (d.y() - c.y());
                    if dot < 0.0
                        && orient(a, b, c).abs() <= EPS * a.distance_to(b)
                        && orient(a, b, d).abs() <= EPS * a.distance_to(b)
                    {
                        hits += 1;
                    }
                    continue;
                }
                if crosses(a, b, c, d)
                    || on_segment(a, c, d)
                    || on_segment(b, c, d)
                    || on_segment(c, a, b)
                    || on_segment(d, a, b)
                {
                    hits += 1;
                }
            }
        }
        hits
    }

    /// Total horizontal extent of the filled region on the scanline `y`.
    ///
    /// Pairs the crossings in order, which is the even-odd rule; that is the
    /// conservative reading, because it is the one that reports a thin or
    /// cancelled interior as thin rather than as filled.
    pub fn filled_width_at(contour: &[Point2], y: f32) -> f32 {
        filled_intervals_at(contour, y)
            .iter()
            .map(|&(a, b)| b - a)
            .sum()
    }

    pub fn filled_intervals_at(contour: &[Point2], y: f32) -> Vec<(f32, f32)> {
        let n = contour.len();
        let mut xs = Vec::new();
        for i in 0..n {
            let a = contour[i];
            let b = contour[(i + 1) % n];
            if a.y() == b.y() {
                continue;
            }
            if (a.y() <= y && b.y() > y) || (b.y() <= y && a.y() > y) {
                let t = (y - a.y()) / (b.y() - a.y());
                xs.push(a.x() + t * (b.x() - a.x()));
            }
        }
        xs.sort_by(f32::total_cmp);
        xs.chunks_exact(2).map(|pair| (pair[0], pair[1])).collect()
    }

    /// The narrowest filled width over the band `y in [lo, hi]`, sampled.
    ///
    /// A hull that fills its body everywhere but pinches to a stem somewhere
    /// inside it passes a check that only samples the middle, so the band is
    /// sampled rather than probed at one height.
    pub fn narrowest_filled_width(contour: &[Point2], lo: f32, hi: f32, samples: usize) -> f32 {
        let mut narrowest = f32::INFINITY;
        for index in 0..=samples {
            let y = lo + (hi - lo) * index as f32 / samples as f32;
            narrowest = narrowest.min(filled_width_at(contour, y));
        }
        narrowest
    }

    /// Every coordinate named by the hull, including cubic control points.
    pub fn all_points(hull: ConnectiveHullGeometry) -> Vec<Point2> {
        let mut out = Vec::new();
        for index in 0..hull.command_count() {
            match hull.command(index).expect("command") {
                HullCommand::Empty => break,
                HullCommand::MoveTo(p) | HullCommand::LineTo(p) => out.push(p),
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    out.push(control1);
                    out.push(control2);
                    out.push(to);
                }
                HullCommand::Close => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Downward outline, captured before the hull became direction-neutral.
    ///
    /// One entry per command, in order, formatted at full `f32` precision.
    const EXPECTED_DOWNWARD_COMMANDS: &[&str] = &[
        "M 2.780000000e2 1.600000000e1",
        "L 1.002000000e3 1.600000000e1",
        "C 1.011941101e3 1.600000000e1 1.020000000e3 2.405887413e1 1.020000000e3 3.400000000e1",
        "L 1.020000000e3 5.400000000e1",
        "C 1.020000000e3 6.394112396e1 1.011941101e3 7.200000000e1 1.002000000e3 7.200000000e1",
        "L 8.110000000e2 7.200000000e1",
        "L 7.590000000e2 7.200000000e1",
        "C 7.562385864e2 7.200000000e1 7.540000000e2 7.423857880e1 7.540000000e2 7.700000000e1",
        "L 7.540000000e2 7.700000000e1",
        "C 7.540000000e2 7.976142120e1 7.562385864e2 8.200000000e1 7.590000000e2 8.200000000e1",
        "L 8.690000000e2 8.200000000e1",
        "C 8.789411011e2 8.200000000e1 8.870000000e2 9.005887604e1 8.870000000e2 1.000000000e2",
        "L 8.870000000e2 2.200000000e2",
        "C 8.870000000e2 2.299411316e2 8.789411011e2 2.380000000e2 8.690000000e2 2.380000000e2",
        "C 8.308333130e2 2.380000000e2 7.926666870e2 2.380000000e2 7.545000000e2 2.380000000e2",
        "C 7.163333130e2 2.380000000e2 6.781666870e2 2.380000000e2 6.400000000e2 2.380000000e2",
        "C 6.018333130e2 2.380000000e2 5.636666870e2 2.380000000e2 5.255000000e2 2.380000000e2",
        "C 4.873333435e2 2.380000000e2 4.491666565e2 2.380000000e2 4.110000000e2 2.380000000e2",
        "C 4.010588684e2 2.380000000e2 3.930000000e2 2.299411316e2 3.930000000e2 2.200000000e2",
        "L 3.930000000e2 1.000000000e2",
        "C 3.930000000e2 9.005887604e1 4.010588684e2 8.200000000e1 4.110000000e2 8.200000000e1",
        "L 5.210000000e2 8.200000000e1",
        "C 5.237614136e2 8.200000000e1 5.260000000e2 7.976142120e1 5.260000000e2 7.700000000e1",
        "L 5.260000000e2 7.700000000e1",
        "C 5.260000000e2 7.423857880e1 5.237614136e2 7.200000000e1 5.210000000e2 7.200000000e1",
        "L 4.690000000e2 7.200000000e1",
        "L 2.780000000e2 7.200000000e1",
        "C 2.680588684e2 7.200000000e1 2.600000000e2 6.394112396e1 2.600000000e2 5.400000000e1",
        "L 2.600000000e2 3.400000000e1",
        "C 2.600000000e2 2.405887604e1 2.680588684e2 1.600000000e1 2.780000000e2 1.600000000e1",
        "Close",
    ];
    use crate::grammar::reactive_envelope::ReactiveGrowthDirection;
    fn input(attachment: ReactiveAttachment) -> ConnectiveHullInput {
        let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
        let child = GeometryRect::try_new(393.0, 82.0, 887.0, 238.0, 18.0).unwrap();
        ConnectiveHullInput {
            parent,
            child,
            attachment,
            topology_phase: 1.0,
            attachment_phase: 1.0,
            release_phase: 0.0,
            reattach_phase: 1.0,
            neck_width: 228.0,
            mouth_width: 342.0,
            shoulder_run: 0.0,
            shoulder_center_x: 640.0,
            outer_contour: std::array::from_fn(|i| {
                Point2::new(
                    child.left()
                        + child.radius()
                        + (child.width() - 2.0 * child.radius()) * i as f32 / 4.0,
                    child.bottom(),
                )
            }),
            openness: 1.0,
            growth: ReactiveGrowthDirection::Downward,
        }
    }

    #[test]
    fn invalid_directional_placement_is_not_a_flush_joint() {
        for growth in [
            ReactiveGrowthDirection::Upward,
            ReactiveGrowthDirection::Downward,
        ] {
            let mut i = if growth == ReactiveGrowthDirection::Upward {
                mirrored_input(ReactiveAttachment::Fused)
            } else {
                input(ReactiveAttachment::Fused)
            };
            let joint = JointFrame::new(growth).parent_joint(i.parent);
            let (top, bottom) = if growth == ReactiveGrowthDirection::Upward {
                (joint - 99.0, joint + 1.0)
            } else {
                (joint - 1.0, joint + 99.0)
            };
            i.child =
                GeometryRect::try_new(i.child.left(), top, i.child.right(), bottom, 18.0).unwrap();
            assert!(
                matches!(ConnectiveHullGeometry::resolve(i), Err(GeometryError::InvalidDirectionalPlacement { signed_gap }) if signed_gap == -1.0)
            );
        }
    }

    #[test]
    fn exactly_flush_directional_placement_is_valid() {
        for growth in [
            ReactiveGrowthDirection::Upward,
            ReactiveGrowthDirection::Downward,
        ] {
            let mut i = if growth == ReactiveGrowthDirection::Upward {
                mirrored_input(ReactiveAttachment::Fused)
            } else {
                input(ReactiveAttachment::Fused)
            };
            let frame = JointFrame::new(growth);
            let shift = frame.parent_joint(i.parent) - frame.child_facing(i.child);
            i.child = GeometryRect::try_new(
                i.child.left(),
                i.child.top() + shift,
                i.child.right(),
                i.child.bottom() + shift,
                i.child.radius(),
            )
            .unwrap();
            i.outer_contour = i.outer_contour.map(|p| Point2::new(p.x(), p.y() + shift));
            let hull = ConnectiveHullGeometry::resolve(i).unwrap();
            assert_eq!(hull.contour_count(), 1);
            assert_body_enclosed(hull, i.child, "flush");
        }
    }

    #[test]
    fn nonfinite_outer_contours_fail_closed() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for axis in [0, 1] {
                // Point2's private coordinates prevent malformed hull input
                // from being constructed through safe public APIs.
                let point = if axis == 0 {
                    Point2::try_new(value, 0.0)
                } else {
                    Point2::try_new(0.0, value)
                };
                assert!(matches!(point, Err(GeometryError::NonFinite(_))));
            }
        }
    }

    #[test]
    fn out_of_bounds_outer_contours_fail_closed() {
        let mut i = mirrored_input(ReactiveAttachment::Bridged);
        i.outer_contour[2] = Point2::new(i.outer_contour[2].x(), i.child.top() - 2.0);
        assert!(matches!(
            ConnectiveHullGeometry::resolve(i),
            Err(GeometryError::InvalidOuterContour { index: 2, .. })
        ));
    }

    #[test]
    fn malformed_contour_endpoints_and_orientation_fail_closed() {
        let base = mirrored_input(ReactiveAttachment::Fused);
        for kind in 0..3 {
            let mut i = base;
            match kind {
                0 => i.outer_contour[0] = Point2::new(i.outer_contour[0].x(), i.child.bottom()),
                1 => i.outer_contour.swap(1, 3),
                _ => i.outer_contour[4] = Point2::new(i.child.right(), i.child.top()),
            }
            assert!(matches!(
                ConnectiveHullGeometry::resolve(i),
                Err(GeometryError::InvalidOuterContour { .. })
            ));
        }
    }
    /// The exact Downward command stream, as a standing guard.
    ///
    /// This path is the one R09/R13 qualified, so "the downward hull did not
    /// move" has to be a fact the suite enforces rather than a claim a reader
    /// takes on trust from a diff. The expectation is generated from the
    /// pre-change output and compared with full `f32` formatting, so a
    /// difference of one unit in the last place is a failure, not a rounding
    /// tolerance. Making the hull direction-neutral is supposed to be purely
    /// additive: the Downward outline must come out byte-for-byte unchanged.
    #[test]
    fn the_downward_command_stream_is_unchanged() {
        let hull = ConnectiveHullGeometry::resolve(input(ReactiveAttachment::Fused)).unwrap();
        let actual = format_commands(hull);
        assert_eq!(
            actual.len(),
            EXPECTED_DOWNWARD_COMMANDS.len(),
            "command count"
        );
        for (index, (actual, expected)) in actual
            .iter()
            .zip(EXPECTED_DOWNWARD_COMMANDS.iter())
            .enumerate()
        {
            assert_eq!(actual, expected, "command {index} moved");
        }
    }

    /// Format a hull's commands, one line per command.
    ///
    /// Compared as a line table rather than one concatenated string so a
    /// mismatch names the command that moved. Full `f32` formatting means a
    /// one-unit difference in the last place is still a difference.
    fn format_commands(hull: ConnectiveHullGeometry) -> Vec<String> {
        let mut out = Vec::new();
        for index in 0..hull.command_count() {
            match hull.command(index).expect("command index within hull") {
                HullCommand::Empty => out.push("Empty".to_owned()),
                HullCommand::MoveTo(p) => out.push(format!("M {:.9e} {:.9e}", p.x(), p.y())),
                HullCommand::LineTo(p) => out.push(format!("L {:.9e} {:.9e}", p.x(), p.y())),
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => out.push(format!(
                    "C {:.9e} {:.9e} {:.9e} {:.9e} {:.9e} {:.9e}",
                    control1.x(),
                    control1.y(),
                    control2.x(),
                    control2.y(),
                    to.x(),
                    to.y()
                )),
                HullCommand::Close => out.push("Close".to_owned()),
            }
        }
        out
    }

    // -----------------------------------------------------------------
    // Direction neutrality, held at the hull level.
    //
    // These build the hull directly rather than through the solver, so a failure
    // names the hull construction instead of a symptom three layers up.

    /// The reflection axis the mirror cases reflect about.
    const MIRROR: f32 = 300.0;

    /// Reflect a rectangle about `MIRROR`.
    fn reflect(rect: GeometryRect) -> GeometryRect {
        GeometryRect::try_new(
            rect.left(),
            MIRROR - rect.bottom(),
            rect.right(),
            MIRROR - rect.top(),
            rect.radius(),
        )
        .expect("reflected rect stays normalized")
    }

    /// The `input()` case mirrored into the other growth direction.
    ///
    /// The contour moves with the reflection, because it belongs on the child's
    /// far edge - which is the top edge once the case grows upward.
    fn mirrored_input(attachment: ReactiveAttachment) -> ConnectiveHullInput {
        let base = input(attachment);
        let parent = reflect(base.parent);
        let child = reflect(base.child);
        ConnectiveHullInput {
            parent,
            child,
            attachment,
            outer_contour: base
                .outer_contour
                .map(|point| Point2::new(point.x(), MIRROR - point.y())),
            growth: ReactiveGrowthDirection::Upward,
            ..base
        }
    }

    fn reflected_y(hull: ConnectiveHullGeometry) -> Vec<f32> {
        let mut out = Vec::new();
        for point in hull_probe::all_points(hull) {
            out.push(MIRROR - point.y());
        }
        out
    }

    fn raw_y(hull: ConnectiveHullGeometry) -> Vec<f32> {
        hull_probe::all_points(hull)
            .iter()
            .map(|point| point.y())
            .collect()
    }

    #[test]
    fn an_upward_hull_is_the_downward_hull_reflected() {
        // The strong statement about the change: the upward outline is not a
        // second construction that happens to look plausible, it is the same
        // construction read through a different axis. If a future edit
        // special-cases upward anywhere in here, this fails.
        for attachment in [
            ReactiveAttachment::Fused,
            ReactiveAttachment::Bridged,
            ReactiveAttachment::Detached,
        ] {
            let down = ConnectiveHullGeometry::resolve(input(attachment)).expect("downward");
            let up = ConnectiveHullGeometry::resolve(mirrored_input(attachment)).expect("upward");
            assert_eq!(
                down.command_count(),
                up.command_count(),
                "{attachment:?}: command count must match under reflection"
            );
            // The downward outline raw, against the upward one reflected. Not
            // both reflected: that compares the upward hull against its own
            // mirror image, which passes for any self-symmetric outline and so
            // would not prove the two constructions agree at all.
            for (index, (downward, upward)) in
                raw_y(down).iter().zip(reflected_y(up).iter()).enumerate()
            {
                assert!(
                    (downward - upward).abs() < 1e-3,
                    "{attachment:?}: point {index} is not the reflection of its \
                     downward counterpart: {downward} vs {upward}"
                );
            }
        }
    }

    /// The child's body must be enclosed by the painted region.
    fn assert_body_enclosed(hull: ConnectiveHullGeometry, child: GeometryRect, label: &str) {
        let contours = hull_probe::contours(hull);
        for contour in &contours {
            let crossings = hull_probe::self_intersections(contour);
            assert_eq!(
                crossings, 0,
                "{label}: the outline must be simple; {crossings} self-intersections \
                 means the fill rule decides what is painted"
            );
        }
        let inset = child.height() * 0.25;
        let narrowest = hull_probe::narrowest_filled_width(
            &contours[0],
            child.top() + inset,
            child.bottom() - inset,
            24,
        );
        let required = child.width() * 0.9;
        assert!(
            narrowest >= required,
            "{label}: the child body must be filled; narrowest painted span \
             {narrowest:.1}px is below 90% of the body width {required:.1}px"
        );
    }

    #[test]
    fn a_bridged_upward_hull_fills_the_child_body() {
        let i = mirrored_input(ReactiveAttachment::Bridged);
        let hull = ConnectiveHullGeometry::resolve(i).expect("upward bridged");
        assert_body_enclosed(hull, i.child, "upward bridged");
    }

    #[test]
    fn a_bridged_downward_hull_fills_the_child_body() {
        let i = input(ReactiveAttachment::Bridged);
        let hull = ConnectiveHullGeometry::resolve(i).expect("downward bridged");
        assert_body_enclosed(hull, i.child, "downward bridged");
    }

    #[test]
    fn a_fused_upward_hull_fills_the_child_body() {
        let i = mirrored_input(ReactiveAttachment::Fused);
        let hull = ConnectiveHullGeometry::resolve(i).expect("upward fused");
        assert_body_enclosed(hull, i.child, "upward fused");
    }

    #[test]
    fn every_command_lies_within_the_reported_hull_bounds() {
        // Bounds are what an integrator clips and hit-tests against. A command
        // outside them paints or accepts input the caller never accounted for.
        for (label, i) in [
            ("downward fused", input(ReactiveAttachment::Fused)),
            ("downward bridged", input(ReactiveAttachment::Bridged)),
            ("upward fused", mirrored_input(ReactiveAttachment::Fused)),
            (
                "upward bridged",
                mirrored_input(ReactiveAttachment::Bridged),
            ),
        ] {
            let hull = ConnectiveHullGeometry::resolve(i).expect(label);
            let bounds = hull.bounds();
            for (index, point) in hull_probe::all_points(hull).iter().enumerate() {
                assert!(
                    point.x() >= bounds.left() - 0.5
                        && point.x() <= bounds.right() + 0.5
                        && point.y() >= bounds.top() - 0.5
                        && point.y() <= bounds.bottom() + 0.5,
                    "{label}: command {index} at ({}, {}) escapes bounds {bounds:?}",
                    point.x(),
                    point.y()
                );
            }
        }
    }

    #[test]
    fn input_regions_stay_inside_the_painted_hull() {
        // Input is derived from the painted contours, so a directional change
        // that moved the outline without moving the input bands would accept
        // clicks where nothing is drawn.
        for (label, i) in [
            ("downward fused", input(ReactiveAttachment::Fused)),
            ("upward fused", mirrored_input(ReactiveAttachment::Fused)),
        ] {
            let hull = ConnectiveHullGeometry::resolve(i).expect(label);
            let bounds = hull.bounds();
            let regions = hull.input_regions(4.0);
            assert!(
                !regions.is_empty(),
                "{label}: a visible hull must be inputtable"
            );
            for region in &regions {
                assert!(
                    region.left() >= bounds.left() - 1.0
                        && region.right() <= bounds.right() + 1.0
                        && region.top() >= bounds.top() - 1.0
                        && region.bottom() <= bounds.bottom() + 1.0,
                    "{label}: input region {region:?} escapes the painted bounds {bounds:?}"
                );
            }
        }
    }

    /// Print the hull as a filled region for the upward case and its mirror.
    ///
    /// The evidence this tranche owes is a before/after comparison of painted
    /// coverage, and a screenshot is a poor instrument for it: the same outline
    /// can be judged correct from a thumbnail. This rasterises the flattened
    /// outline under the even-odd rule and reports what a fill actually covers,
    /// which is what a capture shows, so the two can be compared directly.
    ///
    /// Runs with `cargo test -- --nocapture`. It asserts nothing the other tests
    /// do not already assert; its value is the numbers it prints.
    #[test]
    fn coverage_report() {
        for (label, i) in [
            ("upward", mirrored_input(ReactiveAttachment::Bridged)),
            ("downward", input(ReactiveAttachment::Bridged)),
        ] {
            let hull = ConnectiveHullGeometry::resolve(i).expect(label);
            let contours = hull_probe::contours(hull);
            let crossings: usize = contours
                .iter()
                .map(|contour| hull_probe::self_intersections(contour))
                .sum();
            let inset = i.child.height() * 0.25;
            let narrowest = hull_probe::narrowest_filled_width(
                &contours[0],
                i.child.top() + inset,
                i.child.bottom() - inset,
                24,
            );
            let bounds = hull.bounds();
            println!(
                "COVERAGE {label} self_intersections={crossings} \
declared_bounds={:.2},{:.2}..{:.2},{:.2} \
narrowest_body_span={:.2} body_width={:.2} coverage={:.3} contours={}",
                bounds.left(),
                bounds.top(),
                bounds.right(),
                bounds.bottom(),
                narrowest,
                i.child.width(),
                narrowest / i.child.width().max(1.0),
                contours.len(),
            );
        }
    }

    #[test]
    fn bridged_hull_is_one_continuous_contour_with_shoulders() {
        let hull = ConnectiveHullGeometry::resolve(input(ReactiveAttachment::Bridged)).unwrap();
        assert_eq!(hull.contour_count(), 1);
        let commands = (0..hull.command_count())
            .filter_map(|i| hull.command(i))
            .collect::<Vec<_>>();
        assert!(
            commands
                .iter()
                .filter(|c| matches!(c, HullCommand::CubicTo { .. }))
                .count()
                >= 12
        );
        assert!(commands.iter().any(|c| matches!(c, HullCommand::Close)));
        assert!(hull.input_regions(4.0).len() > 10);
    }
    #[test]
    fn bridged_neck_roundovers_use_the_shared_corner_radius() {
        let mut input = input(ReactiveAttachment::Bridged);
        input.child = GeometryRect::try_new(393.0, 108.0, 887.0, 264.0, 18.0).unwrap();
        input.outer_contour = input
            .outer_contour
            .map(|p| Point2::new(p.x(), input.child.bottom()));
        input.neck_width = 100.0;
        input.mouth_width = 200.0;

        let hull = ConnectiveHullGeometry::resolve(input).unwrap();
        let expected_end = Point2::new(690.0, 90.0);
        let quarter_handle = 4.0 / 3.0 * (std::f32::consts::FRAC_PI_4 / 2.0).tan() * 18.0;
        let right_upper = (0..hull.command_count())
            .filter_map(|index| hull.command(index))
            .find(|command| {
                matches!(
                    command,
                    HullCommand::CubicTo { to, .. }
                        if (to.x() - expected_end.x()).abs() < 0.001
                            && (to.y() - expected_end.y()).abs() < 0.001
                )
            })
            .expect("shared-radius quarter arc endpoint");

        let HullCommand::CubicTo {
            control1, control2, ..
        } = right_upper
        else {
            unreachable!();
        };
        assert!((control1.x() - (708.0 - quarter_handle)).abs() < 0.001);
        assert!((control1.y() - 72.0).abs() < 0.001);
        assert!((control2.x() - 690.0).abs() < 0.001);
        assert!((control2.y() - (90.0 - quarter_handle)).abs() < 0.001);
    }

    #[test]
    fn detached_hull_has_two_independent_closed_contours() {
        let mut input = input(ReactiveAttachment::Detached);
        input.release_phase = 1.0;
        let hull = ConnectiveHullGeometry::resolve(input).unwrap();
        assert_eq!(hull.contour_count(), 2);
        assert_eq!(
            (0..hull.command_count())
                .filter(|i| hull.command(*i) == Some(HullCommand::Close))
                .count(),
            2
        );
    }
}
