//! SURF-02B (S01–S05): the session panel, out of the Bar's power button.
//!
//! A click on the power button extrudes the panel from the Bar, as
//! Utilities comes out of the status icons, on whichever edge the Bar
//! stands. It holds five actions and, when some program holds them back,
//! who does and why. It is pure: the scene gives it what logind reports
//! (`SessionModel`) and turns its effects into the typed requests of
//! `gnos_utilities::session`.
//!
//! - Lock and suspend take a click; log out, reboot and power off fire only
//!   after a press held for `HOLD`, shown by a fill (S03). Releasing early,
//!   or leaving the tile, fires nothing.
//! - An action that an inhibitor blocks is held too, and the hold forces
//!   past it (S04): the scene sends it as interactive, so logind asks polkit.
//! - An action logind does not offer is shown unavailable and takes no
//!   press.
//! - A press outside closes the panel, as does Escape.

use crate::bar_popout_host::PopoutFrame;
use crate::rail_topology::RailSide;
use crate::utilities_panel::beside_module;
use gnos_i18n::t;
use gnos_utilities::SessionAction;
use gnos_ux_surface::grammar::{ExtrusionMotion, TemporalResponseMode};
use std::time::{Duration, Instant};

pub const PANEL_WIDTH: f32 = 336.0;
/// S03: how long a heavy (or forcing) press is held before it fires.
pub const HOLD: Duration = Duration::from_millis(1000);

/// SET-02A: the operator's hold (`HOLD` by default).
fn hold() -> Duration {
    #[cfg(feature = "component-lab")]
    {
        crate::surface_prefs::current().session_hold()
    }
    #[cfg(not(feature = "component-lab"))]
    {
        HOLD
    }
}
const PAD: f32 = 14.0;
const HEADER: f32 = 20.0;
const GAP: f32 = 10.0;
const TILE: f32 = 56.0;
const TILE_HEIGHT: f32 = 66.0;
const LINE: f32 = 22.0;
/// At most this many blockers are listed (the rest are counted).
const MAX_BLOCKERS: usize = 4;
const HEIGHT_RATE: f32 = 18.0;
const HEIGHT_EPSILON: f32 = 0.25;

pub fn label(action: SessionAction) -> &'static str {
    match action {
        SessionAction::Lock => t("Lock"),
        SessionAction::Suspend => t("Suspend"),
        SessionAction::Logout => t("Log out"),
        SessionAction::Reboot => t("Reboot"),
        SessionAction::PowerOff => t("Power off"),
    }
}

/// One action as logind reports it now.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionTile {
    pub action: SessionAction,
    /// `Some(false)`: logind does not offer it; `None`: not heard yet.
    pub available: Option<bool>,
    /// How many `block` inhibitors hold it back.
    pub blocked: usize,
    pub pending: bool,
    pub note: Option<String>,
}

impl SessionTile {
    /// Whether a press must be held: a heavy action, or a forced one.
    pub fn held(&self) -> bool {
        self.action.held() || self.blocked > 0
    }

    fn pressable(&self) -> bool {
        self.available != Some(false) && !self.pending
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionModel {
    pub tiles: Vec<SessionTile>,
    /// One line per blocker: who, what, why.
    pub blockers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEffect {
    Request { action: SessionAction, force: bool },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionOutcome {
    pub consumed: bool,
    pub changed: bool,
    pub effects: Vec<SessionEffect>,
}

/// The parts, in panel coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionRow {
    Header {
        text: &'static str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Tile {
        tile: SessionTile,
        /// 0..1 while a hold is under way.
        hold: Option<f32>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Line {
        text: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl SessionRow {
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Header {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Tile {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Line {
                x,
                y,
                width,
                height,
                ..
            } => (*x, *y, *width, *height),
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        let (rx, ry, rw, rh) = self.rect();
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    }
}

#[derive(Clone, Copy, Debug)]
struct Press {
    action: SessionAction,
    started: Instant,
    held: bool,
    force: bool,
    fired: bool,
}

pub struct SessionPanel {
    open: bool,
    motion: ExtrusionMotion,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
    anchor: Option<f32>,
    side: RailSide,
    viewport: (f32, f32),
    model: SessionModel,
    press: Option<Press>,
    /// An action released before its hold completed: its caption says so.
    hint: Option<SessionAction>,
    shown_height: Option<f32>,
    now: Option<Instant>,
    last_tick: Option<Instant>,
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            motion: ExtrusionMotion::closed(),
            give_way: None,
            anchor: None,
            side: RailSide::Left,
            viewport: (0.0, 0.0),
            model: SessionModel::default(),
            press: None,
            hint: None,
            shown_height: None,
            now: None,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn model(&self) -> &SessionModel {
        &self.model
    }

    pub fn hint(&self) -> Option<SessionAction> {
        self.hint
    }

    pub fn place(&mut self, side: RailSide, viewport: (f32, f32), anchor: Option<f32>) -> bool {
        let changed = self.side != side || self.viewport != viewport || self.anchor != anchor;
        self.side = side;
        self.viewport = viewport;
        self.anchor = anchor;
        changed
    }

    pub fn set_model(&mut self, model: SessionModel) -> bool {
        if model == self.model {
            return false;
        }
        // A press on an action that became unpressable ends.
        if let Some(press) = self.press
            && !model
                .tiles
                .iter()
                .any(|t| t.action == press.action && t.pressable())
        {
            self.press = None;
        }
        self.model = model;
        true
    }

    pub fn toggle_open(&mut self) -> bool {
        if self.open {
            self.close()
        } else {
            self.open = true;
            self.hint = None;
            self.motion.set_open(true);
            true
        }
    }

    pub fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.press = None;
        self.motion.set_open(false);
        true
    }

    fn height(&self) -> f32 {
        let lines = self.model.blockers.len().min(MAX_BLOCKERS)
            + usize::from(self.model.blockers.len() > MAX_BLOCKERS);
        let blockers = if lines > 0 {
            GAP + HEADER + lines as f32 * LINE
        } else {
            0.0
        };
        PAD + HEADER + GAP + TILE_HEIGHT + blockers + PAD
    }

    /// Whether a frame is needed: motion, height, or a hold under way.
    pub fn needs_frame(&self) -> bool {
        self.motion.is_open() != self.open
            || self.motion.needs_frame()
            || self
                .shown_height
                .is_some_and(|h| (h - self.height()).abs() > HEIGHT_EPSILON)
            || self.press.is_some_and(|p| p.held && !p.fired)
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |p| now.duration_since(p).as_secs_f32())
            .clamp(0.0, 0.1);
        self.last_tick = Some(now);
        let was_hidden = self.motion.phase() <= 0.0;
        self.motion.set_open(self.open);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        if self.motion.step(dt, mode).is_err() {
            self.motion.snap();
        }
        let target = self.height();
        self.shown_height = Some(match self.shown_height {
            Some(h) if !reduced && !was_hidden => {
                let next = h + (target - h) * (1.0 - (-HEIGHT_RATE * dt).exp());
                if (next - target).abs() <= HEIGHT_EPSILON {
                    target
                } else {
                    next
                }
            }
            _ => target,
        });
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// When the held press under way started, if one is.
    pub fn held_press(&self) -> Option<Instant> {
        self.press.filter(|p| p.held && !p.fired).map(|p| p.started)
    }

    /// A held press that has lasted `HOLD` fires, once. `now` is wall
    /// time (the hold's fill follows it too, not the motion clock).
    pub fn poll(&mut self, now: Instant) -> Option<SessionEffect> {
        self.now = Some(now);
        let press = self.press.as_mut()?;
        if press.held && !press.fired && now.duration_since(press.started) >= hold() {
            press.fired = true;
            return Some(SessionEffect::Request {
                action: press.action,
                force: press.force,
            });
        }
        None
    }

    fn hold_progress(&self, action: SessionAction) -> Option<f32> {
        let press = self.press.filter(|p| p.action == action && p.held)?;
        if press.fired {
            return Some(1.0);
        }
        let now = self.now.unwrap_or(press.started);
        Some(
            (now.saturating_duration_since(press.started).as_secs_f32() / hold().as_secs_f32())
                .clamp(0.0, 1.0),
        )
    }

    pub fn rows(&self) -> Vec<SessionRow> {
        let inner = PANEL_WIDTH - 2.0 * PAD;
        let mut rows = vec![SessionRow::Header {
            text: t("Session"),
            x: PAD,
            y: PAD,
            width: inner,
            height: HEADER,
        }];
        let mut y = PAD + HEADER + GAP;
        let n = self.model.tiles.len().max(1) as f32;
        let pitch = ((inner - TILE * n) / (n - 1.0).max(1.0)).max(4.0);
        for (i, tile) in self.model.tiles.iter().enumerate() {
            rows.push(SessionRow::Tile {
                tile: tile.clone(),
                hold: self.hold_progress(tile.action),
                x: PAD + i as f32 * (TILE + pitch),
                y,
                width: TILE,
                height: TILE_HEIGHT,
            });
        }
        y += TILE_HEIGHT;
        if !self.model.blockers.is_empty() {
            y += GAP;
            rows.push(SessionRow::Header {
                text: t("Held back by"),
                x: PAD,
                y,
                width: inner,
                height: HEADER,
            });
            y += HEADER;
            for text in self.model.blockers.iter().take(MAX_BLOCKERS) {
                rows.push(SessionRow::Line {
                    text: text.clone(),
                    x: PAD,
                    y,
                    width: inner,
                    height: LINE,
                });
                y += LINE;
            }
            let more = self.model.blockers.len().saturating_sub(MAX_BLOCKERS);
            if more > 0 {
                rows.push(SessionRow::Line {
                    text: gnos_i18n::tf("and {more} more", &[("more", &more.to_string())]),
                    x: PAD,
                    y,
                    width: inner,
                    height: LINE,
                });
            }
        }
        rows
    }

    pub fn frame(&self) -> Option<PopoutFrame> {
        let anchor = self.anchor?;
        let h = self.shown_height.unwrap_or_else(|| self.height());
        // FOCUS-01D: it stands back into the Bar by what a surface standing
        // with it occupies of its depth (make room).
        let depth = if self.side.is_horizontal() {
            h
        } else {
            PANEL_WIDTH
        };
        let openness =
            gnos_ux_surface::grammar::within_depth(self.motion.phase(), depth, self.give_way);
        if openness <= 0.0 {
            return None;
        }
        beside_module(self.side, self.viewport, anchor, PANEL_WIDTH, h, openness)
    }

    /// FOCUS-01D: the body it has fully out, and the edge it comes out of.
    pub fn body(
        &self,
    ) -> Option<(
        gnos_ux_surface::grammar::GeometryRect,
        gnos_ux_surface::envelope::SurfaceEdge,
    )> {
        let anchor = self.anchor?;
        let h = self.shown_height.unwrap_or_else(|| self.height());
        let full = beside_module(self.side, self.viewport, anchor, PANEL_WIDTH, h, 1.0)?;
        Some((full.rect, self.side.edge()))
    }

    /// FOCUS-01D: how deep it may come out (`None`: fully). Returns whether
    /// that changed.
    pub fn set_give_way(&mut self, limit: Option<f32>) -> bool {
        let changed = self.give_way != limit;
        self.give_way = limit;
        changed
    }

    fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let frame = self.frame()?;
        let v = frame.visible;
        (x >= v.left() && x < v.right() && y >= v.top() && y < v.bottom())
            .then(|| (x - frame.rect.left(), y - frame.rect.top()))
    }

    /// CURSOR-01: what the pointer is over on the panel (`None`: off it): a
    /// tile it can press, or one logind does not offer now.
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        self.local(x, y)?;
        Some(match self.tile_at(x, y) {
            Some(tile) if tile.pressable() => crate::cursor::Affordance::Press,
            Some(_) => crate::cursor::Affordance::Disabled,
            None => crate::cursor::Affordance::Inert,
        })
    }

    /// CURSOR-01: an action under way.
    pub fn busy(&self) -> bool {
        self.model.tiles.iter().any(|t| t.pending)
    }

    fn tile_at(&self, x: f32, y: f32) -> Option<SessionTile> {
        let (lx, ly) = self.local(x, y)?;
        self.rows().into_iter().find_map(|r| match r {
            SessionRow::Tile { ref tile, .. } if r.contains(lx, ly) => Some(tile.clone()),
            _ => None,
        })
    }

    /// A primary press. Outside the panel (and not on the power button,
    /// which the scene routes first) it closes it.
    pub fn pointer_down(&mut self, x: f32, y: f32, now: Instant) -> SessionOutcome {
        let mut outcome = SessionOutcome::default();
        if !self.open {
            return outcome;
        }
        if self.local(x, y).is_none() {
            outcome.changed = self.close();
            return outcome;
        }
        outcome.consumed = true;
        if let Some(tile) = self.tile_at(x, y).filter(SessionTile::pressable) {
            self.hint = None;
            self.now = Some(now);
            self.press = Some(Press {
                action: tile.action,
                started: now,
                held: tile.held(),
                force: tile.blocked > 0,
                fired: false,
            });
            outcome.changed = true;
        }
        outcome
    }

    /// Leaving the pressed tile cancels the press.
    pub fn pointer_moved(&mut self, x: f32, y: f32) -> SessionOutcome {
        let mut outcome = SessionOutcome {
            consumed: self.local(x, y).is_some(),
            ..Default::default()
        };
        if let Some(press) = self.press
            && !press.fired
            && self.tile_at(x, y).map(|t| t.action) != Some(press.action)
        {
            self.press = None;
            outcome.changed = true;
        }
        outcome
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> SessionOutcome {
        let mut outcome = SessionOutcome::default();
        let Some(press) = self.press.take() else {
            outcome.consumed = self.local(x, y).is_some();
            return outcome;
        };
        outcome.consumed = true;
        outcome.changed = true;
        let here = self.tile_at(x, y).map(|t| t.action) == Some(press.action);
        if !press.held && here {
            outcome.effects.push(SessionEffect::Request {
                action: press.action,
                force: false,
            });
        } else if press.held && !press.fired {
            // Released early: nothing fires; the caption says to hold.
            self.hint = Some(press.action);
        }
        outcome
    }

    pub fn escape(&mut self) -> bool {
        self.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(blocked: usize, power_off: Option<bool>) -> SessionModel {
        SessionModel {
            tiles: SessionAction::ALL
                .iter()
                .map(|&action| SessionTile {
                    action,
                    available: if action == SessionAction::PowerOff {
                        power_off
                    } else {
                        Some(true)
                    },
                    blocked: if action.inhibited_by() == Some("shutdown") {
                        blocked
                    } else {
                        0
                    },
                    pending: false,
                    note: None,
                })
                .collect(),
            blockers: (0..blocked)
                .map(|i| format!("App {i} · shutdown: busy"))
                .collect(),
        }
    }

    fn open_panel(side: RailSide, m: SessionModel) -> SessionPanel {
        let mut p = SessionPanel::new();
        p.place(side, (1280.0, 800.0), Some(760.0));
        p.set_model(m);
        p.toggle_open();
        p.tick(Instant::now(), true);
        p
    }

    fn centre(p: &SessionPanel, action: SessionAction) -> (f32, f32) {
        let f = p.frame().unwrap();
        let r = p
            .rows()
            .into_iter()
            .find(|r| matches!(r, SessionRow::Tile { tile, .. } if tile.action == action))
            .unwrap();
        let (x, y, w, h) = r.rect();
        (f.rect.left() + x + w / 2.0, f.rect.top() + y + h / 2.0)
    }

    #[test]
    fn it_extrudes_from_the_bar_beside_the_power_button_on_any_side() {
        for side in [
            RailSide::Left,
            RailSide::Right,
            RailSide::Top,
            RailSide::Bottom,
        ] {
            let p = open_panel(side, model(0, Some(true)));
            let f = p.frame().unwrap();
            assert!(f.hull.is_some(), "{side:?}");
            let rail = crate::rail_topology::rail_width();
            match side {
                RailSide::Left => assert_eq!(f.rect.left(), rail),
                RailSide::Right => assert_eq!(f.rect.right(), 1280.0 - rail),
                RailSide::Top => assert_eq!(f.rect.top(), rail),
                RailSide::Bottom => assert_eq!(f.rect.bottom(), 800.0 - rail),
            }
        }
    }

    #[test]
    fn lock_and_suspend_take_a_click() {
        let mut p = open_panel(RailSide::Left, model(0, Some(true)));
        let now = Instant::now();
        for action in [SessionAction::Lock, SessionAction::Suspend] {
            let (x, y) = centre(&p, action);
            assert!(p.pointer_down(x, y, now).consumed);
            assert_eq!(
                p.pointer_up(x, y).effects,
                vec![SessionEffect::Request {
                    action,
                    force: false
                }]
            );
        }
    }

    #[test]
    fn heavy_actions_fire_only_after_the_hold_and_once() {
        let mut p = open_panel(RailSide::Left, model(0, Some(true)));
        let start = Instant::now();
        let (x, y) = centre(&p, SessionAction::PowerOff);
        p.pointer_down(x, y, start);
        // Released early: nothing, and the caption asks for a hold.
        assert!(p.poll(start + Duration::from_millis(600)).is_none());
        assert!(p.pointer_up(x, y).effects.is_empty());
        assert_eq!(p.hint(), Some(SessionAction::PowerOff));
        // Held: the fill grows, then it fires once.
        p.pointer_down(x, y, start);
        assert_eq!(p.held_press(), Some(start));
        assert!(p.poll(start + Duration::from_millis(500)).is_none());
        let hold = p.rows().into_iter().find_map(|r| match r {
            SessionRow::Tile { tile, hold, .. } if tile.action == SessionAction::PowerOff => hold,
            _ => None,
        });
        assert!(hold.is_some_and(|h| (h - 0.5).abs() < 0.05), "{hold:?}");
        assert_eq!(
            p.poll(start + HOLD),
            Some(SessionEffect::Request {
                action: SessionAction::PowerOff,
                force: false
            })
        );
        assert!(p.poll(start + HOLD * 2).is_none(), "once");
        assert!(p.pointer_up(x, y).effects.is_empty());
        // Leaving the tile while holding cancels.
        let (x, y) = centre(&p, SessionAction::Reboot);
        p.pointer_down(x, y, start);
        assert!(p.pointer_moved(x + 200.0, y).changed);
        assert!(p.poll(start + HOLD).is_none());
    }

    #[test]
    fn a_blocked_action_is_held_and_forces_and_an_unoffered_one_takes_no_press() {
        let mut p = open_panel(RailSide::Bottom, model(2, Some(true)));
        assert!(
            p.rows()
                .iter()
                .any(|r| matches!(r, SessionRow::Line { text, .. } if text.starts_with("App 1")))
        );
        let start = Instant::now();
        let (x, y) = centre(&p, SessionAction::Reboot);
        p.pointer_down(x, y, start);
        assert_eq!(
            p.poll(start + HOLD),
            Some(SessionEffect::Request {
                action: SessionAction::Reboot,
                force: true
            })
        );
        p.pointer_up(x, y);
        let mut p = open_panel(RailSide::Left, model(0, Some(false)));
        let (x, y) = centre(&p, SessionAction::PowerOff);
        p.pointer_down(x, y, start);
        assert!(p.poll(start + HOLD).is_none());
        assert!(p.pointer_up(x, y).effects.is_empty());
    }

    #[test]
    fn a_press_outside_or_escape_closes_it() {
        let mut p = open_panel(RailSide::Left, model(0, Some(true)));
        let out = p.pointer_down(1200.0, 20.0, Instant::now());
        assert!(out.changed && !out.consumed && !p.is_open());
        let mut p = open_panel(RailSide::Left, model(0, Some(true)));
        assert!(p.escape() && !p.is_open());
    }
}
