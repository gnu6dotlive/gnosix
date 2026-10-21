//! TOPO-01: where the native rail sits, and what follows from it.
//!
//! The rail is a vertical Bar on the left or right edge (TOPO-01A), or a
//! horizontal Bar on the top or bottom edge (TOPO-01C). Everything that used
//! to assume "the rail is on the left" asks this type instead:
//! - the frame takes the rail's thickness on the rail's side;
//! - Bar popouts grow away from the rail;
//! - pointer input is routed in rail space: `across` the rail (0 at the
//!   outer edge) and `along` it (0 at its start, top or left);
//! - centred drawers centre on the area inside the frame's insets;
//! - edge drawers that would share the rail's edge move to the opposite edge
//!   (operator decision: a drawer whose edge the Bar takes moves across).

use crate::reference_bar::{BAR_INNER_WIDTH, BORDER_THICKNESS, SMALL_PADDING};
use gnos_ux_surface::envelope::SurfaceEdge;

/// The rail's thickness: inner width plus the larger of padding and border
/// on both sides.
pub const RAIL_WIDTH: f32 = BAR_INNER_WIDTH
    + 2.0
        * if SMALL_PADDING > BORDER_THICKNESS {
            SMALL_PADDING
        } else {
            BORDER_THICKNESS
        };

/// BAR-01D (B03): the rail's adjustable metrics, one current value per
/// scene thread, set from the durable profile and eased by the scene (so a
/// change is continuous). The defaults are the reference's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailMetrics {
    /// The rail's thickness (default `RAIL_WIDTH`).
    pub thickness: f32,
    /// The spacing between the Bar's modules, as a factor of the reference's.
    pub gap_scale: f32,
    /// The active window title's width on a horizontal Bar.
    pub title_width: f32,
}

impl RailMetrics {
    pub const DEFAULT: Self = Self {
        thickness: RAIL_WIDTH,
        gap_scale: 1.0,
        title_width: crate::reference_bar::ROW_ACTIVE_WINDOW_TITLE_WIDTH,
    };
}

impl Default for RailMetrics {
    fn default() -> Self {
        Self::DEFAULT
    }
}

thread_local! {
    static METRICS: std::cell::Cell<RailMetrics> = const { std::cell::Cell::new(RailMetrics::DEFAULT) };
}

/// The current rail metrics.
pub fn metrics() -> RailMetrics {
    METRICS.with(std::cell::Cell::get)
}

/// Set the current rail metrics (the scene, every frame).
pub fn set_metrics(metrics: RailMetrics) {
    METRICS.with(|cell| cell.set(metrics));
}

/// The rail's current thickness.
pub fn rail_width() -> f32 {
    metrics().thickness
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RailSide {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl RailSide {
    /// The rail side for a Bar edge preference.
    pub fn from_edge(edge: SurfaceEdge) -> Self {
        match edge {
            SurfaceEdge::Left => Self::Left,
            SurfaceEdge::Right => Self::Right,
            SurfaceEdge::Top => Self::Top,
            SurfaceEdge::Bottom => Self::Bottom,
        }
    }

    pub fn edge(self) -> SurfaceEdge {
        match self {
            Self::Left => SurfaceEdge::Left,
            Self::Right => SurfaceEdge::Right,
            Self::Top => SurfaceEdge::Top,
            Self::Bottom => SurfaceEdge::Bottom,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    /// A horizontal Bar: modules run left to right.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }

    /// The opposite side: where edge drawers go.
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
        }
    }

    /// The rail's rectangle `(x, y, width, height)` on a viewport.
    pub fn rail_rect(self, (width, height): (f32, f32)) -> (f32, f32, f32, f32) {
        match self {
            Self::Left => (0.0, 0.0, rail_width(), height),
            Self::Right => (width - rail_width(), 0.0, rail_width(), height),
            Self::Top => (0.0, 0.0, width, rail_width()),
            Self::Bottom => (0.0, height - rail_width(), width, rail_width()),
        }
    }

    pub fn rail_contains(self, viewport: (f32, f32), x: f32, y: f32) -> bool {
        let (rx, ry, rw, rh) = self.rail_rect(viewport);
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    }

    /// Left edge of a vertical rail (0 for a horizontal one).
    pub fn rail_x(self, viewport_width: f32) -> f32 {
        match self {
            Self::Right => viewport_width - rail_width(),
            _ => 0.0,
        }
    }

    /// The rail's length: the output height for a vertical rail, its width
    /// for a horizontal one.
    pub fn length(self, (width, height): (f32, f32)) -> f32 {
        if self.is_horizontal() { width } else { height }
    }

    /// Frame insets on the left and right: the rail's width on its side,
    /// the border on the other.
    pub fn insets(self) -> (f32, f32) {
        let (left, _, right, _) = self.insets4();
        (left, right)
    }

    /// Frame insets `(left, top, right, bottom)`.
    pub fn insets4(self) -> (f32, f32, f32, f32) {
        let b = BORDER_THICKNESS;
        match self {
            Self::Left => (rail_width(), b, b, b),
            Self::Right => (b, b, rail_width(), b),
            Self::Top => (b, rail_width(), b, b),
            Self::Bottom => (b, b, b, rail_width()),
        }
    }

    /// A scene point in rail space `(across, along)`, so rail logic written
    /// for a left rail runs unchanged on every side.
    pub fn to_rail(self, x: f32, y: f32, (width, height): (f32, f32)) -> (f32, f32) {
        match self {
            Self::Left => (x, y),
            Self::Right => (width - x, y),
            Self::Top => (y, x),
            Self::Bottom => (height - y, x),
        }
    }

    /// An x coordinate in a vertical rail's own space (0 at the rail's outer
    /// edge).
    pub fn to_rail_x(self, x: f32, viewport_width: f32) -> f32 {
        match self {
            Self::Right => viewport_width - x,
            _ => x,
        }
    }

    /// Mirror a horizontal span `[x, x + width)` when the rail is on the
    /// right; identity otherwise.
    pub fn mirror_span(self, x: f32, width: f32, viewport_width: f32) -> f32 {
        match self {
            Self::Right => viewport_width - x - width,
            _ => x,
        }
    }
}

/// BAR-01C (B06): how a surface whose place depends on the rail's side
/// follows a travel leg. While its place stays the same it does not move.
/// Where the new side moves it (the Deck under a bottom Bar, the drawers
/// beside a right one, Settings on a horizontal one), it retracts into its
/// edge over the first part of the leg, stays in during the middle, and
/// comes out of its new edge over the last part: position only, nothing
/// fades.
#[derive(Clone, Debug, PartialEq)]
pub struct Relocation {
    pub from: RailSide,
    pub to: RailSide,
    /// The leg's progress, 0..=1.
    pub progress: f32,
    /// The frame insets the leg starts from: the source side's, or where a
    /// retargeted leg left them (BAR-01J).
    start_insets: (f32, f32),
    /// The relocation a retarget interrupted, at the moment it did: each
    /// surface goes on from where that one left it.
    prior: Option<Box<Relocation>>,
}

impl Relocation {
    /// The retraction ends here and the extrusion starts here, by default
    /// (SET-03 `retract_split_pct` moves both, symmetrically).
    pub const OUT_END: f32 = 0.4;
    pub const IN_START: f32 = 0.6;

    fn out_end() -> f32 {
        #[cfg(feature = "component-lab")]
        return crate::surface_prefs::advanced("retract_split_pct") as f32 / 100.0;
        #[cfg(not(feature = "component-lab"))]
        Self::OUT_END
    }

    fn in_start() -> f32 {
        1.0 - Self::out_end()
    }

    pub fn new(from: RailSide, to: RailSide, progress: f32) -> Self {
        Self {
            from,
            to,
            progress: Self::bounded(progress),
            start_insets: from.insets(),
            prior: None,
        }
    }

    /// BAR-01J (B09): a leg that starts where `prior` was interrupted. It
    /// starts from the side `prior` had reached and from the insets it had;
    /// each surface starts from its retraction then.
    pub fn rebased(prior: Relocation, to: RailSide, progress: f32) -> Self {
        Self {
            from: prior.side(),
            to,
            progress: Self::bounded(progress),
            start_insets: prior.insets(),
            prior: Some(Box::new(prior)),
        }
    }

    fn bounded(progress: f32) -> f32 {
        if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// The side a relocating surface follows now: the source while it
    /// retracts, the target while it comes out.
    pub fn side(&self) -> RailSide {
        if self.progress < 0.5 {
            self.from
        } else {
            self.to
        }
    }

    /// How far out of place a surface that the leg moves is: 0 in place, 1
    /// fully in its edge.
    pub fn retraction(&self) -> f32 {
        self.moving(0.0)
    }

    fn ease(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// A moving surface's retraction, starting from `start`: it finishes
    /// retracting, stays in, and comes out of its new edge.
    fn moving(&self, start: f32) -> f32 {
        let p = self.progress;
        if p < Self::out_end() {
            start + (1.0 - start) * Self::ease(p / Self::out_end())
        } else if p < Self::in_start() {
            1.0
        } else {
            1.0 - Self::ease((p - Self::in_start()) / (1.0 - Self::in_start()))
        }
    }

    /// How much of the travel's clearance a bottom Deck takes (BAR-01C):
    /// none at either end of a leg, all of it in between, eased over the
    /// first and last 15 %. A rebased leg starts from what the interrupted
    /// one had (BAR-01J), so the Deck never drops onto the arriving Bar.
    pub fn clearance(&self) -> f32 {
        const RAMP: f32 = 0.15;
        let p = self.progress;
        let base = Self::ease(p.min(1.0 - p) / RAMP);
        match &self.prior {
            None => base,
            Some(prior) => {
                let start = prior.clearance();
                start + (base - start) * Self::ease(p / RAMP)
            }
        }
    }

    /// The left and right frame insets, eased from where the leg starts to
    /// the target side's over the leg.
    pub fn insets(&self) -> (f32, f32) {
        let t = Self::ease(self.progress);
        let (a, b) = (self.start_insets, self.to.insets());
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }

    /// The retraction of a surface whose place is `place(side)`: 0 when the
    /// two sides give it the same place (after a retarget, it comes back
    /// out from where the interrupted leg left it).
    pub fn of<T: PartialEq>(&self, place: impl Fn(RailSide) -> T) -> f32 {
        self.of_dyn(&place)
    }

    fn of_dyn<T: PartialEq>(&self, place: &dyn Fn(RailSide) -> T) -> f32 {
        let start = self.prior.as_ref().map_or(0.0, |p| p.of_dyn(place));
        if place(self.from) == place(self.to) {
            start * (1.0 - Self::ease(self.progress / Self::out_end()))
        } else {
            self.moving(start)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rail_is_sixty_wide_and_mirrors_cleanly() {
        assert_eq!(RAIL_WIDTH, 60.0);
        assert_eq!(RailSide::from_edge(SurfaceEdge::Right), RailSide::Right);
        assert_eq!(RailSide::from_edge(SurfaceEdge::Top), RailSide::Top);
        assert_eq!(RailSide::Right.rail_x(1280.0), 1220.0);
        assert_eq!(RailSide::Right.insets(), (10.0, 60.0));
        assert_eq!(RailSide::Right.to_rail_x(1250.0, 1280.0), 30.0);
        assert_eq!(RailSide::Right.mirror_span(60.0, 320.0, 1280.0), 900.0);
        assert_eq!(RailSide::Left.mirror_span(60.0, 320.0, 1280.0), 60.0);
        assert_eq!(RailSide::Left.opposite(), RailSide::Right);
    }

    #[test]
    fn a_horizontal_rail_routes_in_rail_space_and_insets_its_edge() {
        let viewport = (1280.0, 800.0);
        assert!(RailSide::Top.is_horizontal() && RailSide::Bottom.is_horizontal());
        assert_eq!(RailSide::Top.rail_rect(viewport), (0.0, 0.0, 1280.0, 60.0));
        assert_eq!(
            RailSide::Bottom.rail_rect(viewport),
            (0.0, 740.0, 1280.0, 60.0)
        );
        assert_eq!(RailSide::Top.insets4(), (10.0, 60.0, 10.0, 10.0));
        assert_eq!(RailSide::Bottom.insets4(), (10.0, 10.0, 10.0, 60.0));
        assert_eq!(RailSide::Top.to_rail(400.0, 30.0, viewport), (30.0, 400.0));
        assert_eq!(
            RailSide::Bottom.to_rail(400.0, 790.0, viewport),
            (10.0, 400.0)
        );
        assert_eq!(
            RailSide::Right.to_rail(1250.0, 300.0, viewport),
            (30.0, 300.0)
        );
        assert!(RailSide::Bottom.rail_contains(viewport, 5.0, 760.0));
        assert!(!RailSide::Bottom.rail_contains(viewport, 5.0, 700.0));
        assert_eq!(RailSide::Bottom.length(viewport), 1280.0);
        assert_eq!(RailSide::Top.opposite(), RailSide::Bottom);
    }

    #[test]
    fn a_retargeted_leg_starts_where_the_interrupted_one_was() {
        // BAR-01J (B09): every surface, every interruption point, every new
        // target: no jump at the retarget, and the leg ends in place.
        let sides = [
            RailSide::Left,
            RailSide::Top,
            RailSide::Right,
            RailSide::Bottom,
        ];
        let places: [&dyn Fn(RailSide) -> bool; 3] = [
            &|s| s == RailSide::Bottom,
            &|s| s == RailSide::Right,
            &|s| s == RailSide::Top,
        ];
        for a in sides {
            for b in sides {
                for c in sides {
                    if a == b || b == c {
                        continue;
                    }
                    for k in 0..=20 {
                        let prior = Relocation::new(a, b, k as f32 / 20.0);
                        let start = Relocation::rebased(prior.clone(), c, 0.0);
                        let end = Relocation::rebased(prior.clone(), c, 1.0);
                        assert_eq!(start.side(), prior.side());
                        assert_eq!(start.insets(), prior.insets());
                        assert!((start.clearance() - prior.clearance()).abs() < 1.0e-5);
                        assert!(end.clearance().abs() < 1.0e-5);
                        assert_eq!(end.insets(), c.insets());
                        for place in places {
                            let (was, now) = (prior.of(place), start.of(place));
                            assert!(
                                (was - now).abs() < 1.0e-5,
                                "{a:?}->{b:?} at {k}/20 then {c:?}: {was} vs {now}"
                            );
                            assert!(end.of(place).abs() < 1.0e-5);
                            // Small steps never jump.
                            let mut last = now;
                            for j in 1..=100 {
                                let r = Relocation::rebased(prior.clone(), c, j as f32 / 100.0)
                                    .of(place);
                                assert!(
                                    (r - last).abs() < 0.08,
                                    "{a:?}->{b:?}->{c:?}: step {j}: {last} -> {r}"
                                );
                                last = r;
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_relocating_surface_goes_in_then_comes_out_of_its_new_edge() {
        let deck_on_top = |side: RailSide| side == RailSide::Bottom;
        let at = |p| Relocation::new(RailSide::Right, RailSide::Bottom, p);
        assert_eq!(at(0.0).of(deck_on_top), 0.0);
        assert_eq!(at(0.4).of(deck_on_top), 1.0);
        assert_eq!(at(0.5).of(deck_on_top), 1.0);
        assert_eq!(at(1.0).of(deck_on_top), 0.0);
        assert_eq!(at(0.3).side(), RailSide::Right);
        assert_eq!(at(0.7).side(), RailSide::Bottom);
        let mut previous = 0.0;
        for step in 0..=40 {
            let r = at(step as f32 / 100.0).retraction();
            assert!(r >= previous, "it retracts monotonically");
            previous = r;
        }
        // A surface that keeps its place does not move.
        let stays = Relocation::new(RailSide::Left, RailSide::Top, 0.5);
        assert_eq!(stays.of(deck_on_top), 0.0);
        assert_eq!(
            Relocation::new(RailSide::Left, RailSide::Top, f32::NAN).progress,
            1.0
        );
    }
}
