//! FOCUS-01B: one surface handing off from one content to the next.
//!
//! When the pointer slides from one Bar module to a neighbour of its kin
//! (status icons, tray items), the surface does not go back into the Bar and
//! come out again: it changes shape and its content passes along. Two parts,
//! one spring:
//!
//! - **The frame** ([`MorphFrame`]): each of the body's four edges follows
//!   its target on the shared critically damped follower ([`Follow`]). The
//!   box changes shape continuously, never overshoots its content, and stays
//!   a member of the shell's field the whole way (no tear, no seam).
//! - **The content** ([`ContentStrip`]): the contents of one kin stand side
//!   by side on a strip, in the order of their sources along the Bar; the
//!   frame is a window panning over it. Rules:
//!   1. *Spatial.* A neighbour's content lies on the side its module lies:
//!      sliding right brings the next content in from the right, the last
//!      one leaves to the left. Turning back pans back to a content still
//!      on the strip; nothing jumps.
//!   2. *Rigid.* The contents move as one strip, so they never overlap and
//!      never part: no gap, no double image.
//!   3. *Exact travel.* A content travels its own extent, so it leaves
//!      precisely through the frame's edge; no opacity, no fade (GNU6
//!      motion: geometry only).
//!   4. *One tempo.* The pan rides the same follower as the frame's edges,
//!      so shape and content start, move and settle together, with no
//!      timer and no separate curve. Under reduced motion both snap.
//!   5. *Joint-anchored.* Each content is laid from the edge the surface
//!      grows out of, so a change of depth reveals or hides from the far
//!      edge, as the extrusion itself does.

use super::{Follow, TemporalResponseMode};

/// Tolerance under which a length is settled, in px.
const SETTLE_PX: f32 = 0.25;

/// A body `[left, top, right, bottom]` whose edges follow their targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphFrame {
    edges: [Follow; 4],
    target: [f32; 4],
}

impl MorphFrame {
    /// At `rect`, still.
    pub fn at(rect: [f32; 4]) -> Self {
        Self {
            edges: rect.map(Follow::at),
            target: rect,
        }
    }

    /// Change shape toward `rect`, from wherever it stands. Returns whether
    /// the target changed.
    pub fn retarget(&mut self, rect: [f32; 4]) -> bool {
        let changed = self.target != rect;
        self.target = rect;
        changed
    }

    pub fn target(&self) -> [f32; 4] {
        self.target
    }

    /// Where it stands now.
    pub fn rect(&self) -> [f32; 4] {
        self.edges.map(|e| e.value)
    }

    pub fn is_settled(&self) -> bool {
        self.edges
            .iter()
            .zip(self.target)
            .all(|(e, t)| e.is_settled(t, SETTLE_PX))
    }

    /// Advance by `dt` seconds. Returns whether it still moves.
    pub fn step(&mut self, dt: f32, mode: TemporalResponseMode) -> bool {
        let mut moving = false;
        for (edge, target) in self.edges.iter_mut().zip(self.target) {
            moving |= edge.step(target, dt, mode, SETTLE_PX);
        }
        moving
    }
}

/// The contents of one kin on a strip along the Bar, and the window over it.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentStrip<K> {
    /// Each content's start on the strip and its extent along the Bar.
    entries: Vec<(K, f32, f32)>,
    /// The strip coordinate at the window's leading edge.
    pan: Follow,
    target: f32,
}

impl<K: Copy + PartialEq> ContentStrip<K> {
    /// One content, in place (the surface comes out of the Bar with it).
    pub fn start(key: K, extent: f32) -> Self {
        Self {
            entries: vec![(key, 0.0, extent)],
            pan: Follow::at(0.0),
            target: 0.0,
        }
    }

    /// The content the window settles on.
    pub fn current(&self) -> Option<K> {
        self.entries
            .iter()
            .find(|(_, at, _)| *at == self.target)
            .map(|(k, _, _)| *k)
    }

    /// Hand off to `key`, whose module lies `ahead` (true: further along the
    /// Bar than the one shown). A content still on the strip is panned back
    /// to; a new one is laid beside the strip's end on its side.
    pub fn show(&mut self, key: K, extent: f32, ahead: bool) {
        if let Some(entry) = self.entries.iter_mut().find(|(k, _, _)| *k == key) {
            entry.2 = extent;
            self.target = entry.1;
            return;
        }
        let at = if ahead {
            self.entries
                .iter()
                .map(|(_, at, e)| at + e)
                .fold(f32::NEG_INFINITY, f32::max)
        } else {
            self.entries
                .iter()
                .map(|(_, at, _)| *at)
                .fold(f32::INFINITY, f32::min)
                - extent
        };
        let at = if at.is_finite() { at } else { 0.0 };
        self.entries.push((key, at, extent));
        self.target = at;
    }

    /// Advance by `dt` seconds; contents the window has left are dropped
    /// once it is still. `window` is the frame's extent along the Bar.
    /// Returns whether it still moves.
    pub fn step(&mut self, dt: f32, mode: TemporalResponseMode, window: f32) -> bool {
        let moving = self.pan.step(self.target, dt, mode, SETTLE_PX);
        if !moving {
            let (lo, hi) = (self.pan.value, self.pan.value + window.max(0.0));
            let target = self.target;
            self.entries
                .retain(|(_, at, e)| *at == target || (at + e > lo && *at < hi));
        }
        moving
    }

    pub fn is_settled(&self) -> bool {
        self.pan.is_settled(self.target, SETTLE_PX)
    }

    /// Each content on the strip and its offset from the window's leading
    /// edge along the Bar (0: in place).
    pub fn offsets(&self) -> Vec<(K, f32)> {
        self.entries
            .iter()
            .map(|(k, at, _)| (*k, at - self.pan.value))
            .collect()
    }

    /// The offset of `key`'s content, if it is on the strip.
    pub fn offset_of(&self, key: K) -> Option<f32> {
        self.offsets()
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, o)| o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 240.0;
    const FULL: TemporalResponseMode = TemporalResponseMode::Full;

    fn run<F: FnMut() -> bool>(mut step: F) -> usize {
        (1..2000).find(|_| !step()).expect("settles")
    }

    #[test]
    fn the_frame_changes_shape_without_overshooting() {
        let mut f = MorphFrame::at([100.0, 50.0, 400.0, 300.0]);
        assert!(f.retarget([300.0, 50.0, 520.0, 200.0]));
        let mut previous = f.rect();
        run(|| {
            let moving = f.step(DT, FULL);
            let now = f.rect();
            // Every edge moves monotonically toward its target.
            for k in 0..4 {
                let (a, b, t) = (previous[k], now[k], f.target()[k]);
                assert!((b - t).abs() <= (a - t).abs() + 1e-3, "edge {k} overshoots");
            }
            previous = now;
            moving
        });
        assert_eq!(f.rect(), [300.0, 50.0, 520.0, 200.0]);
        assert!(f.is_settled());
    }

    #[test]
    fn content_passes_along_the_bar_as_one_rigid_strip() {
        let mut s = ContentStrip::start('a', 300.0);
        s.show('b', 220.0, true);
        assert_eq!(s.offset_of('b'), Some(300.0), "laid beside, ahead");
        let mut previous = s.offsets();
        run(|| {
            let moving = s.step(DT, FULL, 300.0);
            let now = s.offsets();
            // Rigid: the two contents keep their spacing (never overlap,
            // never part), and the outgoing one only moves away.
            let (a, b) = (s.offset_of('a'), s.offset_of('b'));
            if let (Some(a), Some(b)) = (a, b) {
                assert!((b - a - 300.0).abs() < 1e-3);
                let before = previous.iter().find(|(k, _)| *k == 'a').unwrap().1;
                assert!(a <= before + 1e-3);
            }
            previous = now;
            moving
        });
        assert_eq!(
            s.offsets(),
            vec![('b', 0.0)],
            "the outgoing one left the window"
        );
        assert_eq!(s.current(), Some('b'));
    }

    #[test]
    fn turning_back_pans_back_without_a_jump() {
        let mut s = ContentStrip::start('a', 300.0);
        s.show('b', 220.0, true);
        for _ in 0..12 {
            s.step(DT, FULL, 300.0);
        }
        let midway = s.offset_of('a').unwrap();
        assert!(midway < 0.0 && midway > -300.0);
        // Back to 'a', still on the strip: the pan turns, nothing jumps.
        s.show('a', 300.0, false);
        assert_eq!(s.offset_of('a'), Some(midway));
        run(|| s.step(DT, FULL, 300.0));
        assert_eq!(s.offsets(), vec![('a', 0.0)]);
        // Behind: a content laid before the strip's start.
        s.show('z', 120.0, false);
        assert_eq!(s.offset_of('z'), Some(-120.0));
    }

    #[test]
    fn reduced_motion_snaps_shape_and_content_together() {
        let mut f = MorphFrame::at([0.0, 0.0, 100.0, 100.0]);
        f.retarget([50.0, 0.0, 200.0, 80.0]);
        assert!(!f.step(DT, TemporalResponseMode::Instant));
        assert_eq!(f.rect(), [50.0, 0.0, 200.0, 80.0]);
        let mut s = ContentStrip::start(1, 100.0);
        s.show(2, 150.0, true);
        assert!(!s.step(DT, TemporalResponseMode::Instant, 150.0));
        assert_eq!(s.offsets(), vec![(2, 0.0)]);
    }
}
