//! SHELL-FIELD-01C: a tray menu extrudes out of the Bar.
//!
//! The menu used to be a rounded card placed 8 px beside the rail. Now it
//! stands flush with the rail at its item and comes out of it like the other
//! surfaces: its visible part grows from the rail's edge (geometry only, no
//! fade), it is a member of the shell's field (so it is joined to the Bar by
//! fillets, one material with it), and its view paints only its rows.
//!
//! The menu's size is only known once laid out: the first frame lays it out
//! closed and measures it, then it opens. Closed, it retracts into the rail
//! the way it came, showing its last rows (inert) until it is in.

use crate::bar_popout_host::PopoutFrame;
use crate::rail_topology::RailSide;
use gnos_ux_surface::grammar::handoff::{ContentStrip, MorphFrame};
use gnos_ux_surface::grammar::{ExtrusionMotion, GeometryRect, TemporalResponseMode, revealed};
use std::time::Instant;

/// How far from the output's edges the menu keeps.
const MARGIN: f32 = 8.0;
/// How far before its item, along the rail, the menu starts.
const LEAD: f32 = 16.0;

/// One menu on the surface: its item, its offset along the rail from the
/// body's leading edge, and its laid-out size (`None` while unmeasured).
pub type MenuPart = (usize, f32, Option<(f32, f32)>);

#[derive(Debug, Default)]
pub struct TrayMenuExtrusion {
    motion: ExtrusionMotion,
    shown: bool,
    /// FOCUS-01B: which item's menu (its place among the tray's items).
    key: Option<usize>,
    /// Each laid-out menu, `(width, height)`, and where along the rail its
    /// item stands.
    sizes: Vec<(usize, (f32, f32))>,
    anchors: Vec<(usize, f32)>,
    /// Where on the rail it was opened (scene coordinates).
    at: (f32, f32),
    side: Option<RailSide>,
    viewport: (f32, f32),
    last_tick: Option<Instant>,
    /// FOCUS-01B: the body changing shape from one item's menu to the next,
    /// and the menus passing along it.
    morph: Option<MorphFrame>,
    strip: Option<ContentStrip<usize>>,
    /// The item whose menu stopped being shown (closed by its row, a press,
    /// its application), once, for the shell's attention.
    went_in: Option<usize>,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
}

impl TrayMenuExtrusion {
    fn along(&self, at: (f32, f32)) -> f32 {
        match self.side {
            Some(RailSide::Left | RailSide::Right) => at.1,
            _ => at.0,
        }
    }

    fn size_of(&self, key: usize) -> Option<(f32, f32)> {
        self.sizes.iter().find(|(k, _)| *k == key).map(|(_, s)| *s)
    }

    fn extent(&self, (w, h): (f32, f32)) -> f32 {
        if self.side.is_some_and(RailSide::is_horizontal) {
            w
        } else {
            h
        }
    }

    /// Whether item `key`'s menu is shown, opened from `at` on the rail.
    /// Returns whether anything changed. A first menu starts closed, to be
    /// measured; on screen, another item's menu takes over by a handoff
    /// (the surface stays out and morphs once the new menu is measured).
    pub fn sync(
        &mut self,
        shown: bool,
        key: usize,
        at: (f32, f32),
        side: RailSide,
        viewport: (f32, f32),
    ) -> bool {
        let changed = shown != self.shown || (shown && Some(key) != self.key);
        self.side = Some(side);
        self.viewport = viewport;
        if shown && changed {
            let handoff = self.shown && self.motion.is_visible() && self.strip.is_some();
            if !handoff {
                // A first menu: closed until measured.
                self.motion = ExtrusionMotion::closed();
                self.sizes.clear();
                self.anchors.clear();
                self.morph = None;
                self.strip = None;
            }
            self.key = Some(key);
            // Measured afresh: the application may have changed it.
            self.sizes.retain(|(k, _)| *k != key);
            let along = self.along(at);
            self.anchors.retain(|(k, _)| *k != key);
            self.anchors.push((key, along));
            self.at = at;
        }
        if !shown && self.shown {
            // It retracts into the rail, showing its last rows (the scene
            // keeps them, inert, until it is in).
            self.motion.set_open(false);
            self.went_in = self.key;
        }
        self.shown = shown;
        changed
    }

    /// The item whose menu just stopped being shown, once.
    pub fn take_went_in(&mut self) -> Option<usize> {
        self.went_in.take()
    }

    /// Item `key`'s menu laid out at this size: the first opens; a next one
    /// joins the strip beside the one shown, on its item's side.
    pub fn measured(&mut self, key: usize, size: (f32, f32)) -> bool {
        if !self.shown || Some(key) != self.key || self.size_of(key) == Some(size) {
            return false;
        }
        self.sizes.retain(|(k, _)| *k != key);
        self.sizes.push((key, size));
        let extent = self.extent(size);
        match &mut self.strip {
            None => {
                self.strip = Some(ContentStrip::start(key, extent));
                self.motion.set_open(true);
            }
            Some(strip) => {
                let from = strip
                    .current()
                    .and_then(|k| self.anchors.iter().find(|(a, _)| *a == k))
                    .map(|(_, at)| *at);
                let to = self
                    .anchors
                    .iter()
                    .find(|(a, _)| *a == key)
                    .map(|(_, at)| *at);
                let ahead = matches!((from, to), (Some(f), Some(t)) if t > f);
                strip.show(key, extent, ahead);
            }
        }
        true
    }

    pub fn needs_frame(&self) -> bool {
        self.motion.needs_frame()
            || self.morph.is_some_and(|m| !m.is_settled())
            || self.strip.as_ref().is_some_and(|s| !s.is_settled())
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |p| now.duration_since(p).as_secs_f32())
            .clamp(0.0, 0.1);
        self.last_tick = Some(now);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        if self.motion.step(dt, mode).is_err() {
            self.motion.snap();
        }
        if let Some(target) = self.target_rect() {
            let target = [target.left(), target.top(), target.right(), target.bottom()];
            match &mut self.morph {
                Some(m) => {
                    m.retarget(target);
                    m.step(dt, mode);
                }
                None => self.morph = Some(MorphFrame::at(target)),
            }
        }
        let window = self.window();
        if let Some(strip) = &mut self.strip {
            strip.step(dt, mode, window);
        }
        if !self.shown && !self.motion.is_visible() {
            self.morph = None;
            self.strip = None;
        }
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// The body's extent along the rail now.
    fn window(&self) -> f32 {
        self.morph.map_or(0.0, |m| {
            let [l, t, r, b] = m.rect();
            if self.side.is_some_and(RailSide::is_horizontal) {
                r - l
            } else {
                b - t
            }
        })
    }

    /// The menu rectangle the shown item asks for: flush with the rail at
    /// its item, inside the output. While the next item's menu is not yet
    /// measured, the one shown keeps its place.
    fn target_rect(&self) -> Option<GeometryRect> {
        let key = self
            .key
            .filter(|k| self.size_of(*k).is_some())
            .or_else(|| self.strip.as_ref()?.current())?;
        let (w, h) = self.size_of(key)?;
        let side = self.side?;
        let (vw, vh) = self.viewport;
        let rail = crate::rail_topology::rail_width();
        let at = self
            .anchors
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(self.along(self.at), |(_, a)| *a);
        let along = |at: f32, len: f32, extent: f32| {
            (at - LEAD).clamp(MARGIN, (extent - MARGIN - len).max(MARGIN))
        };
        let (l, t) = match side {
            RailSide::Top => (along(at, w, vw), rail),
            RailSide::Bottom => (along(at, w, vw), vh - rail - h),
            RailSide::Left => (rail, along(at, h, vh)),
            RailSide::Right => (vw - rail - w, along(at, h, vh)),
        };
        GeometryRect::try_new(l, t, l + w, t + h, 0.0).ok()
    }

    /// FOCUS-01D: the body it has fully out, and the edge it comes out of.
    pub fn body(&self) -> Option<(GeometryRect, gnos_ux_surface::envelope::SurfaceEdge)> {
        Some((self.target_rect()?, self.side?.edge()))
    }

    /// FOCUS-01D: how deep it may come out (`None`: fully). Returns whether
    /// that changed.
    pub fn set_give_way(&mut self, limit: Option<f32>) -> bool {
        let changed = self.give_way != limit;
        self.give_way = limit;
        changed
    }

    /// The menu's frame while any part of it is out of the rail.
    pub fn frame(&self) -> Option<PopoutFrame> {
        let side = self.side?;
        let [l, t, r, b] = self.morph?.rect();
        // FOCUS-01D: it stands back into the Bar by what a surface standing
        // with it occupies of its depth (make room).
        let depth = if side.is_horizontal() { b - t } else { r - l };
        let openness =
            gnos_ux_surface::grammar::within_depth(self.motion.phase(), depth, self.give_way);
        if openness <= 0.0 {
            return None;
        }
        let rect = GeometryRect::try_new(
            l,
            t,
            r,
            b,
            crate::reference_bar::BORDER_ROUNDING
                .min((r - l) / 2.0)
                .min((b - t) / 2.0),
        )
        .ok()?;
        let visible = revealed(rect, side.edge(), openness)?;
        Some(PopoutFrame {
            rect,
            visible,
            openness,
            hull: None,
            side,
        })
    }

    /// Each menu on the surface, its offset along the rail from the body's
    /// leading edge and its size. The shown item's menu, not yet measured,
    /// is laid just past the body's far end so it can be measured unseen.
    pub fn contents(&self) -> Vec<MenuPart> {
        let mut out: Vec<MenuPart> = self
            .strip
            .as_ref()
            .map(|s| {
                s.offsets()
                    .into_iter()
                    .map(|(k, o)| (k, o, self.size_of(k)))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(key) = self.key
            && self.shown
            && !out.iter().any(|(k, _, _)| *k == key)
        {
            out.push((key, self.window().max(1.0) * 4.0, None));
        }
        out
    }

    /// Where the first laid-out menu stands, for measuring before it opens:
    /// its position when open (a closed menu is clipped to nothing).
    pub fn origin(&self) -> Option<(f32, f32)> {
        let side = self.side?;
        let rail = crate::rail_topology::rail_width();
        let (vw, vh) = self.viewport;
        let a = self.along(self.at);
        Some(match side {
            RailSide::Top => ((a - LEAD).max(MARGIN), rail),
            RailSide::Bottom => ((a - LEAD).max(MARGIN), vh - rail),
            RailSide::Left => (rail, (a - LEAD).max(MARGIN)),
            RailSide::Right => (vw - rail, (a - LEAD).max(MARGIN)),
        })
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// The item whose menu is shown (or was last).
    pub fn key(&self) -> Option<usize> {
        self.key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_menu_is_measured_closed_then_extrudes_flush_from_the_rail() {
        let mut m = TrayMenuExtrusion::default();
        assert!(m.sync(true, 0, (300.0, 30.0), RailSide::Top, (1000.0, 800.0)));
        assert!(m.frame().is_none(), "unmeasured: nothing shown");
        assert!(m.measured(0, (220.0, 160.0)));
        let mut now = Instant::now();
        for _ in 0..120 {
            now += Duration::from_millis(16);
            if !m.tick(now, false) {
                break;
            }
        }
        let f = m.frame().expect("open");
        let rail = crate::rail_topology::rail_width();
        assert_eq!(f.rect.top(), rail, "flush with the rail");
        assert_eq!(f.rect.left(), 284.0, "at its item");
        assert!(f.openness > 0.99);
        // Closed: it retracts where it stood, then nothing is left out.
        m.sync(false, 0, (0.0, 0.0), RailSide::Top, (1000.0, 800.0));
        assert!(m.needs_frame());
        now += Duration::from_millis(16);
        m.tick(now, false);
        let f = m.frame().expect("retracting");
        assert_eq!(f.rect.left(), 284.0, "in place");
        assert!(f.openness < 1.0);
        for _ in 0..120 {
            now += Duration::from_millis(16);
            if !m.tick(now, false) {
                break;
            }
        }
        assert!(m.frame().is_none(), "in");
    }

    #[test]
    fn the_next_item_s_menu_takes_over_by_morphing() {
        let mut m = TrayMenuExtrusion::default();
        let view = (1000.0, 800.0);
        m.sync(true, 0, (300.0, 30.0), RailSide::Top, view);
        m.measured(0, (220.0, 160.0));
        let mut now = Instant::now();
        let run = |m: &mut TrayMenuExtrusion, now: &mut Instant| {
            for _ in 0..400 {
                *now += Duration::from_millis(4);
                if !m.tick(*now, false) {
                    return;
                }
            }
            panic!("settles");
        };
        run(&mut m, &mut now);
        let first = m.frame().expect("open").rect;
        // The next item (to the right): the surface stays out; its menu is
        // laid past the far end, unseen, until measured.
        assert!(m.sync(true, 1, (340.0, 30.0), RailSide::Top, view));
        assert_eq!(m.frame().expect("still out").rect, first);
        let pending = m.contents();
        assert_eq!(pending.len(), 2);
        assert!(pending[1].1 >= first.width(), "measured off-window");
        assert!(m.measured(1, (260.0, 120.0)));
        let mut both = false;
        for _ in 0..400 {
            now += Duration::from_millis(4);
            let moving = m.tick(now, false);
            let f = m.frame().expect("never goes in");
            assert_eq!(f.openness, 1.0);
            both |= m.contents().len() == 2;
            if !moving {
                break;
            }
        }
        assert!(both, "the two menus passed along together");
        let f = m.frame().unwrap().rect;
        assert_eq!((f.width(), f.height()), (260.0, 120.0));
        assert_eq!(f.left(), 324.0, "at its item");
        assert_eq!(m.contents(), vec![(1, 0.0, Some((260.0, 120.0)))]);
    }
}
