//! A derived surface extruded from its parent along one screen edge.
//!
//! One primitive for every transient surface that grows out of something
//! (a Bar popout out of the rail, a drawer out of a frame border):
//! - **motion:** one openness phase on the shared second-order response,
//!   snapped under reduced motion, never faded;
//! - **reveal:** the part of the child that has left the parent, measured
//!   from the joint, so content slides out of the parent rather than
//!   appearing in place;
//! - **outline:** the connective hull that fuses the revealed child to the
//!   parent with concave joints.
//!
//! The parent's edge is a modelled fact supplied by the caller (the rail is
//! on the left, the launcher's border is at the bottom); nothing here infers
//! it from where rectangles happen to sit.

use super::{
    ConnectiveHullGeometry, ConnectiveHullInput, GeometryError, GeometryRect, PhaseChannel,
    PhaseResponseProfile, Point2, SecondOrderSpec, SettlePolicy, TemporalResponseMode,
    UnitInterval, reactive_envelope::ReactiveAttachment,
    reactive_envelope::ReactiveGrowthDirection,
};
use crate::envelope::SurfaceEdge;

/// The product's qualified reactive response (`ReactiveEnvelopeTuning`).
pub const EXTRUSION_RESPONSE: f32 = 92.0;
pub const EXTRUSION_DAMPING: f32 = 13.0;

fn profile() -> PhaseResponseProfile {
    PhaseResponseProfile::new(
        SecondOrderSpec::new(EXTRUSION_RESPONSE, EXTRUSION_DAMPING),
        // Reduced motion: slower and critically damped, no overshoot.
        SecondOrderSpec::new(EXTRUSION_RESPONSE * 2.25, EXTRUSION_DAMPING * 1.65),
        SettlePolicy::new(0.001, 0.02, true),
    )
}

/// Openness of one extruded surface: 0 inside the parent, 1 fully out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtrusionMotion {
    channel: PhaseChannel,
    open: bool,
    settled: bool,
}

impl Default for ExtrusionMotion {
    fn default() -> Self {
        Self::closed()
    }
}

impl ExtrusionMotion {
    pub fn closed() -> Self {
        Self {
            channel: PhaseChannel::clamped(0.0, 0.0).expect("finite"),
            open: false,
            settled: true,
        }
    }

    /// Retarget. Returns whether the target changed.
    pub fn set_open(&mut self, open: bool) -> bool {
        if self.open == open {
            return false;
        }
        self.open = open;
        self.settled = false;
        true
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn phase(&self) -> f32 {
        self.channel.phase().value()
    }

    /// Whether another step would change anything.
    pub fn needs_frame(&self) -> bool {
        !self.settled
    }

    /// Whether any part is out of the parent.
    pub fn is_visible(&self) -> bool {
        self.phase() > 0.0
    }

    /// Advance by `dt` seconds. Returns whether it still moves.
    pub fn step(&mut self, dt: f32, mode: TemporalResponseMode) -> Result<bool, GeometryError> {
        if self.settled {
            return Ok(false);
        }
        let target = UnitInterval::try_new(if self.open { 1.0 } else { 0.0 })?;
        let dt = if dt.is_finite() {
            dt.clamp(1.0e-4, 0.1)
        } else {
            1.0 / 60.0
        };
        let step = self.channel.step(target, profile(), mode, dt)?;
        self.settled = step.settled;
        Ok(!self.settled)
    }

    /// Jump to the target (used when a surface must not animate).
    pub fn snap(&mut self) {
        let phase = if self.open { 1.0 } else { 0.0 };
        self.channel = PhaseChannel::clamped(phase, 0.0).expect("finite");
        self.settled = true;
    }
}

/// A critically damped follower for a length that tracks its content (a
/// drawer's height as rows arrive): it never overshoots, so a surface never
/// grows past its content and shrinks back.
pub const FOLLOW_RESPONSE: f32 = 300.0;
pub const FOLLOW_DAMPING: f32 = 34.6;

/// One length on the follower. `settled` snaps it to the target.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Follow {
    pub value: f32,
    pub velocity: f32,
}

impl Follow {
    pub fn at(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
        }
    }

    pub fn is_settled(&self, target: f32, tolerance: f32) -> bool {
        (self.value - target).abs() <= tolerance && self.velocity.abs() <= 1.0
    }

    /// Advance toward `target`. Returns whether it still moves.
    pub fn step(
        &mut self,
        target: f32,
        dt: f32,
        mode: TemporalResponseMode,
        tolerance: f32,
    ) -> bool {
        self.step_at_tempo(target, 1.0, dt, mode, tolerance)
    }

    /// `step` with the spring's speed scaled by `tempo` (2 settles in half
    /// the time). Stiffness scales by `tempo²` and damping by `tempo`, so the
    /// damping ratio, and with it the absence of overshoot, is kept. A tempo
    /// that is not finite and positive is 1.
    pub fn step_at_tempo(
        &mut self,
        target: f32,
        tempo: f32,
        dt: f32,
        mode: TemporalResponseMode,
        tolerance: f32,
    ) -> bool {
        let tempo = if tempo.is_finite() && tempo > 0.0 {
            tempo
        } else {
            1.0
        };
        if mode == TemporalResponseMode::Instant || !target.is_finite() || !self.value.is_finite() {
            *self = Self::at(if target.is_finite() { target } else { 0.0 });
            return false;
        }
        let dt = if dt.is_finite() {
            dt.clamp(1.0e-4, 0.1)
        } else {
            1.0 / 60.0
        };
        let mut channel = super::SecondOrderChannel::new(self.value, self.velocity);
        if channel
            .step(
                target,
                SecondOrderSpec::new(FOLLOW_RESPONSE * tempo * tempo, FOLLOW_DAMPING * tempo),
                dt,
            )
            .is_err()
        {
            *self = Self::at(target);
            return false;
        }
        self.value = channel.position();
        self.velocity = channel.velocity();
        if self.is_settled(target, tolerance) {
            *self = Self::at(target);
            return false;
        }
        true
    }
}

/// FOCUS-01D (make room): how deep a body extruded from `edge` may come out
/// without meeting `other`, measured from its joint; `None` when they do not
/// meet across its depth (any depth is clear).
pub fn clear_depth(body: GeometryRect, edge: SurfaceEdge, other: GeometryRect) -> Option<f32> {
    let (bl, bt, br, bb) = (body.left(), body.top(), body.right(), body.bottom());
    let (ol, ot, or, ob) = (other.left(), other.top(), other.right(), other.bottom());
    let (across, near, far) = match edge {
        SurfaceEdge::Top => (or > bl && ol < br, ot - bt, ob - bt),
        SurfaceEdge::Bottom => (or > bl && ol < br, bb - ob, bb - ot),
        SurfaceEdge::Left => (ob > bt && ot < bb, ol - bl, or - bl),
        SurfaceEdge::Right => (ob > bt && ot < bb, br - or, br - ol),
    };
    let depth = match edge {
        SurfaceEdge::Top | SurfaceEdge::Bottom => bb - bt,
        SurfaceEdge::Left | SurfaceEdge::Right => br - bl,
    };
    // Apart across the body, behind its joint or past its far edge: clear.
    if !across || far <= 0.0 || near >= depth {
        return None;
    }
    Some(near.max(0.0))
}

/// The openness a surface of `depth` keeps when it may come out no deeper
/// than `limit` (FOCUS-01D): it stands back into its edge by the overlap, no
/// more.
pub fn within_depth(openness: f32, depth: f32, limit: Option<f32>) -> f32 {
    match limit {
        Some(limit) if depth > 0.0 => openness.min((limit / depth).clamp(0.0, 1.0)),
        _ => openness,
    }
}

/// The part of `child` that has left the parent at `phase`: it keeps the
/// joint side and grows away from `edge` (the parent's screen edge).
pub fn revealed(child: GeometryRect, edge: SurfaceEdge, phase: f32) -> Option<GeometryRect> {
    let phase = if phase.is_finite() {
        phase.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (l, t, r, b) = (child.left(), child.top(), child.right(), child.bottom());
    let (l, t, r, b) = match edge {
        SurfaceEdge::Left => (l, t, l + (r - l) * phase, b),
        SurfaceEdge::Right => (r - (r - l) * phase, t, r, b),
        SurfaceEdge::Top => (l, t, r, t + (b - t) * phase),
        SurfaceEdge::Bottom => (l, b - (b - t) * phase, r, b),
    };
    if r - l < 0.5 || b - t < 0.5 {
        return None;
    }
    let radius = child.radius().min((r - l) / 2.0).min((b - t) / 2.0);
    GeometryRect::try_new(l, t, r, b, radius).ok()
}

/// The child's position along the slide: the full-size rectangle moved so
/// only its revealed part is out of the parent. Content painted here slides
/// with the reveal instead of being uncovered in place.
pub fn sliding(child: GeometryRect, edge: SurfaceEdge, phase: f32) -> GeometryRect {
    let phase = if phase.is_finite() {
        phase.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (dx, dy) = match edge {
        SurfaceEdge::Left => (-child.width() * (1.0 - phase), 0.0),
        SurfaceEdge::Right => (child.width() * (1.0 - phase), 0.0),
        SurfaceEdge::Top => (0.0, -child.height() * (1.0 - phase)),
        SurfaceEdge::Bottom => (0.0, child.height() * (1.0 - phase)),
    };
    GeometryRect::try_new(
        child.left() + dx,
        child.top() + dy,
        child.right() + dx,
        child.bottom() + dy,
        child.radius(),
    )
    .expect("translation preserves valid geometry")
}

/// A frame border as an extrusion parent.
///
/// The visible border is thinner than the joint radius, and the joint's
/// concave fillets take their size from the parent's radius. So the parent is
/// the region beyond the frame's inner edge, `2 × radius` deep, spanning
/// `[lo, hi]` along the edge. Callers mask painting to the inner side, so
/// only the child and its fillets show.
pub fn border_parent(
    edge: SurfaceEdge,
    inner: f32,
    lo: f32,
    hi: f32,
    radius: f32,
) -> Option<GeometryRect> {
    let depth = 2.0 * radius.max(0.5);
    let (l, t, r, b) = match edge {
        SurfaceEdge::Bottom => (lo, inner, hi, inner + depth),
        SurfaceEdge::Top => (lo, inner - depth, hi, inner),
        SurfaceEdge::Right => (inner, lo, inner + depth, hi),
        SurfaceEdge::Left => (inner - depth, lo, inner, hi),
    };
    GeometryRect::try_new(l, t, r, b, radius.max(0.0)).ok()
}

/// Parent and revealed child as one outline, fused flush at the joint.
///
/// `parent` lies along `edge` and `child` touches it on the joint side.
/// `None` when the joint is refused (for example a child longer than the
/// parent along the joint).
pub fn fused_hull(
    parent: GeometryRect,
    child: GeometryRect,
    edge: SurfaceEdge,
) -> Option<ConnectiveHullGeometry> {
    let r = child.radius();
    let vertical_joint = matches!(edge, SurfaceEdge::Left | SurfaceEdge::Right);
    // The far edge, traversed in increasing order along the joint.
    let (far, lo, hi) = match edge {
        SurfaceEdge::Left => (child.right(), child.top(), child.bottom()),
        SurfaceEdge::Right => (child.left(), child.top(), child.bottom()),
        SurfaceEdge::Top => (child.bottom(), child.left(), child.right()),
        SurfaceEdge::Bottom => (child.top(), child.left(), child.right()),
    };
    let span = (hi - r) - (lo + r);
    let contour = [0.0, 0.25, 0.5, 0.75, 1.0].map(|f| {
        let along = lo + r + span * f;
        if vertical_joint {
            Point2::new(far, along)
        } else {
            Point2::new(along, far)
        }
    });
    let joint = hi - lo;
    ConnectiveHullGeometry::resolve_from_edge(
        ConnectiveHullInput {
            parent,
            child,
            attachment: ReactiveAttachment::Fused,
            topology_phase: 1.0,
            attachment_phase: 1.0,
            release_phase: 0.0,
            reattach_phase: 1.0,
            neck_width: joint,
            mouth_width: joint,
            shoulder_run: 0.0,
            shoulder_center_x: (lo + hi) / 2.0,
            outer_contour: contour,
            openness: 1.0,
            // Replaced from `edge` by `resolve_from_edge`.
            growth: ReactiveGrowthDirection::Downward,
        },
        edge,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_stands_back_by_exactly_the_overlap() {
        let r = |l: f32, t: f32, r: f32, b: f32| GeometryRect::try_new(l, t, r, b, 0.0).unwrap();
        // Settings hanging from the top border (60..890), a popout hanging
        // from the same edge over its right part: Settings goes all the way.
        let settings = r(100.0, 60.0, 980.0, 890.0);
        assert_eq!(
            clear_depth(settings, SurfaceEdge::Top, r(750.0, 60.0, 1070.0, 390.0)),
            Some(0.0)
        );
        // A popout out of a left rail, across Settings' lower part: Settings
        // keeps the 540 px above it.
        assert_eq!(
            clear_depth(settings, SurfaceEdge::Top, r(60.0, 600.0, 380.0, 760.0)),
            Some(540.0)
        );
        assert_eq!(within_depth(1.0, 830.0, Some(540.0)), 540.0 / 830.0);
        // Apart across it, or past its far edge: nothing to give.
        assert_eq!(
            clear_depth(settings, SurfaceEdge::Top, r(990.0, 60.0, 1070.0, 390.0)),
            None
        );
        assert_eq!(
            clear_depth(settings, SurfaceEdge::Top, r(100.0, 900.0, 300.0, 950.0)),
            None
        );
        assert_eq!(within_depth(0.7, 830.0, None), 0.7);
        // Mirrored edges.
        let left = r(60.0, 100.0, 400.0, 500.0);
        assert_eq!(
            clear_depth(left, SurfaceEdge::Left, r(300.0, 200.0, 500.0, 300.0)),
            Some(240.0)
        );
        let bottom = r(100.0, 400.0, 500.0, 900.0);
        assert_eq!(
            clear_depth(bottom, SurfaceEdge::Bottom, r(0.0, 300.0, 200.0, 700.0)),
            Some(200.0)
        );
    }

    fn rect(l: f32, t: f32, r: f32, b: f32, radius: f32) -> GeometryRect {
        GeometryRect::try_new(l, t, r, b, radius).unwrap()
    }

    fn run(motion: &mut ExtrusionMotion, mode: TemporalResponseMode) -> (usize, f32) {
        let mut frames = 0;
        let mut peak: f32 = 0.0;
        while motion.step(1.0 / 60.0, mode).unwrap() {
            frames += 1;
            peak = peak.max(motion.phase());
            assert!(frames < 600, "never settles");
        }
        (frames, peak)
    }

    #[test]
    fn opens_and_closes_on_the_second_order_response_and_settles_exactly() {
        let mut motion = ExtrusionMotion::closed();
        assert!(!motion.needs_frame());
        assert!(motion.set_open(true));
        assert!(!motion.set_open(true));
        let (frames, peak) = run(&mut motion, TemporalResponseMode::Full);
        assert!((10..90).contains(&frames), "{frames}");
        assert!(peak <= 1.0, "the phase is clamped: no over-extension");
        assert_eq!(motion.phase(), 1.0);
        motion.set_open(false);
        run(&mut motion, TemporalResponseMode::Full);
        assert_eq!(motion.phase(), 0.0);
        assert!(!motion.is_visible());
    }

    #[test]
    fn the_follower_reaches_its_target_without_overshoot() {
        let mut follow = Follow::at(0.0);
        let mut peak: f32 = 0.0;
        let mut frames = 0;
        while follow.step(400.0, 1.0 / 60.0, TemporalResponseMode::Full, 0.25) {
            peak = peak.max(follow.value);
            frames += 1;
            assert!(frames < 200);
        }
        assert_eq!(follow.value, 400.0);
        assert!(peak <= 400.0, "{peak}");
        assert!(frames < 60, "{frames}");
        assert!(!follow.step(100.0, 0.016, TemporalResponseMode::Instant, 0.25));
        assert_eq!(follow.value, 100.0);
    }

    #[test]
    fn a_border_parent_gives_the_joint_its_concave_fillets() {
        use crate::grammar::connective_hull::hull_probe;
        let inner = 683.0;
        let child = GeometryRect::try_new(156.5, inner - 160.0, 788.5, inner, 25.0).unwrap();
        let width_at = |parent: GeometryRect, y: f32| {
            let hull = fused_hull(parent, child, SurfaceEdge::Bottom).unwrap();
            hull_probe::contours(hull)
                .iter()
                .flat_map(|c| hull_probe::filled_intervals_at(c, y))
                .map(|(a, b)| b - a)
                .fold(0.0f32, f32::max)
        };
        let border = border_parent(SurfaceEdge::Bottom, inner, 0.0, 945.0, 25.0).unwrap();
        assert!(
            width_at(border, inner - 1.0) > child.width() + 30.0,
            "fillets flare out"
        );
        assert_eq!(
            width_at(border, inner - 40.0),
            child.width(),
            "above the joint: the child"
        );
        // A bare 10 px strip cannot carry a 25 px joint.
        let strip = GeometryRect::try_new(0.0, inner, 945.0, inner + 10.0, 5.0).unwrap();
        assert!(width_at(strip, inner - 1.0) < child.width() + 6.0);
    }

    #[test]
    fn instant_mode_and_snap_jump_to_the_target() {
        let mut motion = ExtrusionMotion::closed();
        motion.set_open(true);
        assert!(
            !motion
                .step(1.0 / 60.0, TemporalResponseMode::Instant)
                .unwrap()
        );
        assert_eq!(motion.phase(), 1.0);
        motion.set_open(false);
        motion.snap();
        assert_eq!(motion.phase(), 0.0);
        assert!(!motion.needs_frame());
    }

    #[test]
    fn a_retarget_mid_flight_keeps_the_velocity_continuous() {
        let mut motion = ExtrusionMotion::closed();
        motion.set_open(true);
        for _ in 0..4 {
            motion.step(1.0 / 60.0, TemporalResponseMode::Full).unwrap();
        }
        let mid = motion.phase();
        assert!(mid > 0.0 && mid < 1.0);
        motion.set_open(false);
        motion.step(1.0 / 60.0, TemporalResponseMode::Full).unwrap();
        // Still moving outward for a moment: no jump back.
        assert!((motion.phase() - mid).abs() < 0.2);
    }

    #[test]
    fn reveal_keeps_the_joint_side_for_every_edge() {
        let child = rect(100.0, 200.0, 400.0, 500.0, 25.0);
        let half = |edge| revealed(child, edge, 0.5).unwrap();
        assert_eq!(
            (
                half(SurfaceEdge::Left).left(),
                half(SurfaceEdge::Left).width()
            ),
            (100.0, 150.0)
        );
        assert_eq!(
            (
                half(SurfaceEdge::Right).right(),
                half(SurfaceEdge::Right).width()
            ),
            (400.0, 150.0)
        );
        assert_eq!(
            (
                half(SurfaceEdge::Top).top(),
                half(SurfaceEdge::Top).height()
            ),
            (200.0, 150.0)
        );
        assert_eq!(
            (
                half(SurfaceEdge::Bottom).bottom(),
                half(SurfaceEdge::Bottom).height()
            ),
            (500.0, 150.0)
        );
        assert!(revealed(child, SurfaceEdge::Left, 0.0).is_none());
        assert_eq!(revealed(child, SurfaceEdge::Bottom, 1.0).unwrap(), child);
        let thin = revealed(child, SurfaceEdge::Left, 0.05).unwrap();
        assert!(thin.radius() <= thin.width() / 2.0);
    }

    #[test]
    fn sliding_moves_content_from_inside_the_parent() {
        let child = rect(100.0, 200.0, 400.0, 500.0, 25.0);
        assert_eq!(sliding(child, SurfaceEdge::Left, 0.0).right(), 100.0);
        assert_eq!(sliding(child, SurfaceEdge::Bottom, 0.0).top(), 500.0);
        assert_eq!(sliding(child, SurfaceEdge::Right, 1.0), child);
    }

    #[test]
    fn fused_hull_joins_the_child_to_its_parent_on_every_edge() {
        let (w, h, border) = (1280.0, 800.0, 10.0);
        // Rail on the left, popout beside it.
        let rail = rect(0.0, 0.0, 60.0, h, 25.0);
        let popout = rect(60.0, 300.0, 380.0, 520.0, 25.0);
        // Border strips and drawers.
        let bottom = rect(0.0, h - border, w, h, 0.0);
        let launcher = rect(324.0, 400.0, 956.0, h - border, 25.0);
        let right = rect(w - border, 0.0, w, h, 0.0);
        let side = rect(w - border - 430.0, 200.0, w - border, 600.0, 25.0);
        let top = rect(0.0, 0.0, w, border, 0.0);
        let shelf = rect(400.0, border, 880.0, 300.0, 25.0);
        for (parent, child, edge) in [
            (rail, popout, SurfaceEdge::Left),
            (bottom, launcher, SurfaceEdge::Bottom),
            (right, side, SurfaceEdge::Right),
            (top, shelf, SurfaceEdge::Top),
        ] {
            let hull = fused_hull(parent, child, edge).unwrap_or_else(|| panic!("{edge:?}"));
            let bounds = hull.bounds();
            assert_eq!(bounds.left(), parent.left().min(child.left()), "{edge:?}");
            assert_eq!(
                bounds.right(),
                parent.right().max(child.right()),
                "{edge:?}"
            );
            assert_eq!(bounds.top(), parent.top().min(child.top()), "{edge:?}");
            assert_eq!(
                bounds.bottom(),
                parent.bottom().max(child.bottom()),
                "{edge:?}"
            );
            // A partly revealed child joins too.
            let part = revealed(child, edge, 0.3).unwrap();
            assert!(fused_hull(parent, part, edge).is_some(), "{edge:?} partial");
        }
    }
}
