//! BAR-02A: the overflow panel, out of the Bar's "…" module.
//!
//! When an edge is too short for every module (SET-02C), the modules that
//! give way leave the Bar and this panel lists them, so they stay within
//! reach. A click on a row uses the module: its own panel opens, or its
//! action is carried out. The tray lists each of its items as a row, and
//! the active window shows its title. Click the "…" again, press outside or
//! Escape to close it.

use crate::bar_popout_host::PopoutFrame;
use crate::rail_topology::RailSide;
use crate::reference_bar::IdleBarEntry;
use crate::utilities_panel::beside_module;
use gnos_i18n::t;
use gnos_ux_surface::grammar::{ExtrusionMotion, TemporalResponseMode};
use std::time::Instant;

pub const PANEL_WIDTH: f32 = 260.0;
const PAD: f32 = 14.0;
const HEADER: f32 = 20.0;
const GAP: f32 = 6.0;
const ROW: f32 = 36.0;
const HEIGHT_RATE: f32 = 18.0;
const HEIGHT_EPSILON: f32 = 0.25;

/// What each hidden module is called, and its icon key.
pub fn describe(entry: IdleBarEntry) -> (&'static str, &'static str) {
    match entry {
        IdleBarEntry::Workspaces => (t("Workspaces"), "grid_view"),
        IdleBarEntry::ActiveWindow => (t("Active window"), "web_asset"),
        IdleBarEntry::Tray => (t("Tray"), "widgets"),
        IdleBarEntry::Clock => (t("Clock"), "schedule"),
        IdleBarEntry::StatusIcons => (t("Quick settings"), "tune"),
        IdleBarEntry::Power => (t("Session"), "power_settings_new"),
        // Never hidden into the "…": the Bar's own parts, and the donor's
        // logo button, which Gnosix does not boot.
        IdleBarEntry::Logo | IdleBarEntry::Spacer | IdleBarEntry::Overflow => ("", "more_horiz"),
    }
}

/// What a row stands for: a hidden module, or one item of a hidden tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowPick {
    Module(IdleBarEntry),
    TrayItem(usize),
}

/// What the panel shows besides the modules' names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverflowDetails {
    /// The tray's items, by title, in the Bar's order.
    pub tray: Vec<String>,
    /// The focused window's title.
    pub active_window: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OverflowRow {
    Header {
        text: &'static str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Item {
        pick: OverflowPick,
        label: String,
        symbol: &'static str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl OverflowRow {
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Header {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Item {
                x,
                y,
                width,
                height,
                ..
            } => (*x, *y, *width, *height),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverflowOutcome {
    pub consumed: bool,
    pub changed: bool,
    /// The row the operator chose to use.
    pub pick: Option<OverflowPick>,
}

pub struct OverflowPanel {
    open: bool,
    motion: ExtrusionMotion,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
    anchor: Option<f32>,
    side: RailSide,
    viewport: (f32, f32),
    hidden: Vec<IdleBarEntry>,
    details: OverflowDetails,
    hovered: Option<OverflowPick>,
    press: Option<OverflowPick>,
    shown_height: Option<f32>,
    last_tick: Option<Instant>,
}

impl Default for OverflowPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl OverflowPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            motion: ExtrusionMotion::closed(),
            give_way: None,
            anchor: None,
            side: RailSide::Left,
            viewport: (0.0, 0.0),
            hidden: Vec::new(),
            details: OverflowDetails::default(),
            hovered: None,
            press: None,
            shown_height: None,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn hovered(&self) -> Option<OverflowPick> {
        self.hovered
    }

    pub fn hidden(&self) -> &[IdleBarEntry] {
        &self.hidden
    }

    /// Where the panel stands; nothing hidden (or no "…") closes it.
    pub fn place(
        &mut self,
        side: RailSide,
        viewport: (f32, f32),
        anchor: Option<f32>,
        hidden: &[IdleBarEntry],
    ) -> bool {
        let changed = self.side != side
            || self.viewport != viewport
            || self.anchor != anchor
            || self.hidden != hidden;
        self.side = side;
        self.viewport = viewport;
        self.anchor = anchor.or(self
            .anchor
            .filter(|_| self.side == side && !hidden.is_empty()));
        self.hidden = hidden.to_vec();
        if self.hidden.is_empty() {
            self.close();
        }
        changed
    }

    /// The tray's items and the window's title. Returns whether they changed.
    pub fn set_details(&mut self, details: OverflowDetails) -> bool {
        let changed = self.details != details;
        self.details = details;
        changed
    }

    /// The rows' picks, in order: a hidden tray with items gives one per item.
    fn picks(&self) -> Vec<OverflowPick> {
        self.hidden
            .iter()
            .flat_map(|entry| match entry {
                IdleBarEntry::Tray if !self.details.tray.is_empty() => (0..self.details.tray.len())
                    .map(OverflowPick::TrayItem)
                    .collect(),
                e => vec![OverflowPick::Module(*e)],
            })
            .collect()
    }

    fn describe_pick(&self, pick: OverflowPick) -> (String, &'static str) {
        match pick {
            OverflowPick::TrayItem(i) => (
                self.details.tray.get(i).cloned().unwrap_or_default(),
                describe(IdleBarEntry::Tray).1,
            ),
            OverflowPick::Module(IdleBarEntry::ActiveWindow) => {
                let (name, symbol) = describe(IdleBarEntry::ActiveWindow);
                (
                    self.details
                        .active_window
                        .clone()
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| name.to_owned()),
                    symbol,
                )
            }
            OverflowPick::Module(e) => {
                let (name, symbol) = describe(e);
                (name.to_owned(), symbol)
            }
        }
    }

    pub fn toggle(&mut self) -> bool {
        if self.open {
            self.close()
        } else if self.anchor.is_some() && !self.hidden.is_empty() {
            self.open = true;
            self.motion.set_open(true);
            true
        } else {
            false
        }
    }

    pub fn close(&mut self) -> bool {
        self.press = None;
        self.hovered = None;
        if !self.open {
            return false;
        }
        self.open = false;
        self.motion.set_open(false);
        true
    }

    fn height(&self) -> f32 {
        PAD + HEADER + GAP + self.picks().len() as f32 * ROW + PAD
    }

    pub fn needs_frame(&self) -> bool {
        self.motion.is_open() != self.open
            || self.motion.needs_frame()
            || self
                .shown_height
                .is_some_and(|h| (h - self.height()).abs() > HEIGHT_EPSILON)
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
            Some(h) if !was_hidden && !reduced => {
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

    pub fn rows(&self) -> Vec<OverflowRow> {
        let inner = PANEL_WIDTH - 2.0 * PAD;
        let mut rows = vec![OverflowRow::Header {
            text: t("More"),
            x: PAD,
            y: PAD,
            width: inner,
            height: HEADER,
        }];
        let mut y = PAD + HEADER + GAP;
        for pick in self.picks() {
            let (label, symbol) = self.describe_pick(pick);
            rows.push(OverflowRow::Item {
                pick,
                label,
                symbol,
                x: PAD,
                y,
                width: inner,
                height: ROW,
            });
            y += ROW;
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

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.local(x, y).is_some()
    }

    fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let frame = self.frame()?;
        let v = frame.visible;
        (x >= v.left() && x < v.right() && y >= v.top() && y < v.bottom())
            .then(|| (x - frame.rect.left(), y - frame.rect.top()))
    }

    /// CURSOR-01: what the pointer is over on the panel (`None`: off it).
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        self.local(x, y)?;
        Some(if self.item_at(x, y).is_some() {
            crate::cursor::Affordance::Press
        } else {
            crate::cursor::Affordance::Inert
        })
    }

    fn item_at(&self, x: f32, y: f32) -> Option<OverflowPick> {
        let (lx, ly) = self.local(x, y)?;
        self.rows().into_iter().find_map(|r| match r {
            OverflowRow::Item {
                pick,
                x,
                y,
                width,
                height,
                ..
            } if lx >= x && lx < x + width && ly >= y && ly < y + height => Some(pick),
            _ => None,
        })
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) -> OverflowOutcome {
        let mut o = OverflowOutcome::default();
        if !self.open {
            return o;
        }
        if self.local(x, y).is_none() {
            o.changed = self.close();
            return o;
        }
        o.consumed = true;
        self.press = self.item_at(x, y);
        o
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> OverflowOutcome {
        let hovered = self.item_at(x, y);
        let changed = hovered != self.hovered;
        self.hovered = hovered;
        OverflowOutcome {
            consumed: self.local(x, y).is_some(),
            changed,
            ..Default::default()
        }
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> OverflowOutcome {
        let mut o = OverflowOutcome::default();
        let Some(press) = self.press.take() else {
            o.consumed = self.open && self.local(x, y).is_some();
            return o;
        };
        o.consumed = true;
        if self.item_at(x, y) == Some(press) {
            o.changed = true;
            o.pick = Some(press);
        }
        o
    }

    pub fn escape(&mut self) -> bool {
        self.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> OverflowPanel {
        let mut p = OverflowPanel::new();
        p.place(
            RailSide::Top,
            (1000.0, 800.0),
            Some(900.0),
            &[IdleBarEntry::ActiveWindow, IdleBarEntry::Tray],
        );
        p
    }

    fn settle(p: &mut OverflowPanel) {
        let mut now = Instant::now();
        for _ in 0..200 {
            now += std::time::Duration::from_millis(16);
            if !p.tick(now, false) {
                break;
            }
        }
    }

    #[test]
    fn it_opens_only_with_something_hidden_and_lists_it() {
        let mut empty = OverflowPanel::new();
        empty.place(RailSide::Top, (1000.0, 800.0), Some(900.0), &[]);
        assert!(!empty.toggle(), "nothing hidden: nothing to open");
        let mut p = panel();
        assert!(p.toggle() && p.is_open());
        settle(&mut p);
        let items = p
            .rows()
            .iter()
            .filter(|r| matches!(r, OverflowRow::Item { .. }))
            .count();
        assert_eq!(items, 2);
        assert!(p.frame().is_some_and(|f| f.openness > 0.99));
    }

    #[test]
    fn a_click_on_a_row_uses_the_module_and_outside_closes() {
        let mut p = panel();
        p.toggle();
        settle(&mut p);
        let f = p.frame().unwrap();
        let (x, y, w, h) = p
            .rows()
            .iter()
            .find_map(|r| {
                matches!(
                    r,
                    OverflowRow::Item {
                        pick: OverflowPick::Module(IdleBarEntry::Tray),
                        ..
                    }
                )
                .then(|| r.rect())
            })
            .unwrap();
        let (cx, cy) = (f.rect.left() + x + w / 2.0, f.rect.top() + y + h / 2.0);
        p.pointer_down(cx, cy);
        assert_eq!(
            p.pointer_up(cx, cy).pick,
            Some(OverflowPick::Module(IdleBarEntry::Tray))
        );
        assert!(p.pointer_down(5.0, 700.0).changed && !p.is_open());
    }

    #[test]
    fn a_hidden_tray_lists_its_items_and_the_window_its_title() {
        let mut p = panel();
        assert!(p.set_details(OverflowDetails {
            tray: vec!["Steam".into(), "Discord".into()],
            active_window: Some("notes.md".into()),
        }));
        let items: Vec<(OverflowPick, String)> = p
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                OverflowRow::Item { pick, label, .. } => Some((pick, label)),
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (
                    OverflowPick::Module(IdleBarEntry::ActiveWindow),
                    "notes.md".into()
                ),
                (OverflowPick::TrayItem(0), "Steam".into()),
                (OverflowPick::TrayItem(1), "Discord".into()),
            ]
        );
        assert!(!p.set_details(p.details.clone()), "unchanged");
    }

    #[test]
    fn it_closes_when_nothing_is_hidden_any_more() {
        let mut p = panel();
        p.toggle();
        p.place(RailSide::Top, (1000.0, 800.0), Some(900.0), &[]);
        assert!(!p.is_open());
    }
}
