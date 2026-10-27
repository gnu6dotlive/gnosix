//! WS-02: the workspace manager, out of the Bar's workspaces module.
//!
//! Resting the pointer on the workspaces module extrudes the panel from the
//! Bar, in continuity with the module (the hull, position, clip and size;
//! no fade), on whichever edge the Bar stands. It lists every workspace the
//! Bar shows, each with its windows (the application's icon and title).
//! - A click on a workspace goes to it; a click on a window focuses it
//!   (the compositor brings its workspace along).
//! - When it comes out and goes back in is the shell's attention's
//!   (FOCUS-01, `crate::attention`): by default it stays while the pointer
//!   is on the module, the panel or the gap between, and retracts
//!   `LEAVE_GRACE` after it leaves them.
//!
//! It is pure: the scene gives it what the compositor reports
//! (`WorkspaceModel`) and turns its effects into the typed R13 requests.

use crate::bar_popout_host::PopoutFrame;
use crate::rail_topology::RailSide;
use crate::utilities_panel::beside_module;
use gnos_i18n::t;
use gnos_ux_surface::grammar::{ExtrusionMotion, TemporalResponseMode};
use std::time::{Duration, Instant};

pub const PANEL_WIDTH: f32 = 340.0;
/// How long the panel waits after the pointer left before it retracts (the
/// default grace of its attendant).
pub const LEAVE_GRACE: Duration = Duration::from_millis(280);
const PAD: f32 = 14.0;
const HEADER: f32 = 20.0;
const GAP: f32 = 8.0;
const CARD_HEAD: f32 = 30.0;
const ROW: f32 = 28.0;
/// At most this many windows are listed per workspace (the rest counted).
pub const MAX_WINDOWS: usize = 6;
/// WS-03: a miniature's height by default (Settings).
pub const TILE: f32 = 120.0;
/// The panel never grows past this height (nor past the output).
const MAX_HEIGHT: f32 = 640.0;
const HEIGHT_RATE: f32 = 18.0;
const HEIGHT_EPSILON: f32 = 0.25;

/// The space between a miniature's label and its box.
const LABEL: f32 = 18.0;
/// A window drawn smaller than this in a miniature keeps this size.
const MINI_MIN: f32 = 4.0;

#[derive(Clone, Debug, PartialEq)]
pub struct WsWindow {
    pub title: String,
    /// The Material Symbol the Bar's window marks use for its application.
    pub symbol: String,
    pub focused: bool,
    /// WS-03: where it stands on its output, `[left, top, right, bottom]` as
    /// fractions of the output (`None`: not placed).
    pub bounds: Option<[f32; 4]>,
    /// WS-04: the window, for its live thumbnail (`None`: none is taken).
    pub surface: Option<gnosix_desktop_model::SurfaceId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WsCard {
    pub label: String,
    pub active: bool,
    pub windows: Vec<WsWindow>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceModel {
    pub cards: Vec<WsCard>,
    /// WS-03: the output's width over its height (a miniature's shape).
    pub aspect: f32,
}

/// WS-03: how the panel shows the workspaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WsView {
    /// Each workspace and its windows, as a list.
    #[default]
    List,
    /// Each workspace as a miniature of its output, its windows where they
    /// stand, in a grid.
    Map,
    /// The miniatures side by side, in one row.
    Strip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceEffect {
    /// Go to the workspace at this index of the model.
    Activate(usize),
    /// Focus this window (workspace index, window index).
    Focus(usize, usize),
    /// WS-02B: move this window (workspace index, window index) onto the
    /// workspace at `to`.
    Move {
        card: usize,
        index: usize,
        to: usize,
    },
}

/// WS-02B: a window carried by the pointer, from a press on its row.
#[derive(Clone, Debug, PartialEq)]
pub struct Carried {
    pub card: usize,
    pub index: usize,
    pub window: WsWindow,
    /// Where the pointer is (scene coordinates).
    pub at: (f32, f32),
    /// The other workspace it would land on.
    pub over: Option<usize>,
}

/// How far a press travels before it carries the window.
const DRAG_START: f32 = 8.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceOutcome {
    pub consumed: bool,
    pub changed: bool,
    pub effects: Vec<WorkspaceEffect>,
}

/// The parts, in panel coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum WsRow {
    Header {
        text: &'static str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Card {
        index: usize,
        label: String,
        active: bool,
        count: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Window {
        card: usize,
        index: usize,
        window: WsWindow,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// "No windows", or "and N more".
    Note {
        text: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// WS-03: a workspace's miniature: its label above, its box below.
    Tile {
        index: usize,
        label: String,
        active: bool,
        count: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// WS-03: a window in a miniature, where it stands on the output.
    Mini {
        card: usize,
        index: usize,
        window: WsWindow,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl WsRow {
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Header {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Card {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Window {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Note {
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
            | Self::Mini {
                x,
                y,
                width,
                height,
                ..
            } => (*x, *y, *width, *height),
        }
    }

    /// The workspace a row belongs to.
    fn card(&self) -> Option<usize> {
        match self {
            Self::Card { index, .. } | Self::Tile { index, .. } => Some(*index),
            Self::Window { card, .. } | Self::Mini { card, .. } => Some(*card),
            _ => None,
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        let (rx, ry, rw, rh) = self.rect();
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    }

    fn effect(&self) -> Option<WorkspaceEffect> {
        match self {
            Self::Card { index, .. } | Self::Tile { index, .. } => {
                Some(WorkspaceEffect::Activate(*index))
            }
            Self::Window { card, index, .. } | Self::Mini { card, index, .. } => {
                Some(WorkspaceEffect::Focus(*card, *index))
            }
            _ => None,
        }
    }
}

pub struct WorkspacePanel {
    open: bool,
    motion: ExtrusionMotion,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
    anchor: Option<f32>,
    side: RailSide,
    viewport: (f32, f32),
    model: WorkspaceModel,
    press: Option<WorkspaceEffect>,
    /// WS-02: the windows each workspace lists (Settings).
    max_windows: usize,
    /// WS-03: how the workspaces are shown, and a miniature's height.
    view: WsView,
    tile: f32,
    /// Where the press landed, to tell a click from a carry.
    press_at: Option<(f32, f32)>,
    carry: Option<Carried>,
    /// The row under the pointer, tinted.
    hovered: Option<WorkspaceEffect>,
    shown_height: Option<f32>,
    last_tick: Option<Instant>,
}

impl Default for WorkspacePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspacePanel {
    pub fn new() -> Self {
        Self {
            open: false,
            motion: ExtrusionMotion::closed(),
            give_way: None,
            anchor: None,
            side: RailSide::Left,
            viewport: (0.0, 0.0),
            model: WorkspaceModel::default(),
            press: None,
            max_windows: MAX_WINDOWS,
            view: WsView::default(),
            tile: TILE,
            press_at: None,
            carry: None,
            hovered: None,
            shown_height: None,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn model(&self) -> &WorkspaceModel {
        &self.model
    }

    pub fn hovered(&self) -> Option<WorkspaceEffect> {
        self.hovered
    }

    /// Where the panel stands. While the Bar lays its modules out again
    /// (a workspace switch resizes the module), the module's place is
    /// briefly unknown: the panel keeps the last one rather than closing
    /// and coming straight back out. A travel closes it explicitly.
    pub fn place(&mut self, side: RailSide, viewport: (f32, f32), anchor: Option<f32>) -> bool {
        let anchor = anchor.or(self.anchor.filter(|_| self.side == side));
        let changed = self.side != side || self.viewport != viewport || self.anchor != anchor;
        self.side = side;
        self.viewport = viewport;
        self.anchor = anchor;
        changed
    }

    /// WS-03: how the workspaces are shown, and a miniature's height.
    pub fn set_view(&mut self, view: WsView, tile: f32) -> bool {
        let tile = tile.max(LABEL);
        let changed = self.view != view || self.tile != tile;
        self.view = view;
        self.tile = tile;
        changed
    }

    pub fn view(&self) -> WsView {
        self.view
    }

    /// The output's shape, from the model (16:9 when unknown).
    fn aspect(&self) -> f32 {
        if self.model.aspect.is_finite() && self.model.aspect > 0.0 {
            self.model.aspect
        } else {
            16.0 / 9.0
        }
    }

    /// A miniature's box `(width, height)`: the operator's height, narrowed
    /// to fit the panel's inner width when the output is wide.
    fn tile_size(&self) -> (f32, f32) {
        let (mut w, mut h) = (self.tile * self.aspect(), self.tile);
        let inner = PANEL_WIDTH - 2.0 * PAD;
        if self.view == WsView::Map && w > inner {
            h *= inner / w;
            w = inner;
        }
        (w, h)
    }

    /// The panel's width: the list's and the map's are fixed; the strip
    /// holds every miniature in a row, within the output.
    pub fn width(&self) -> f32 {
        match self.view {
            WsView::List | WsView::Map => PANEL_WIDTH,
            WsView::Strip => {
                let n = self.model.cards.len().max(1) as f32;
                let (w, _) = self.tile_size();
                let want = 2.0 * PAD + n * w + (n - 1.0) * GAP;
                let room = (self.viewport.0 - 2.0 * PAD).max(PANEL_WIDTH);
                want.max(PANEL_WIDTH).min(room)
            }
        }
    }

    /// Miniatures per row in the map.
    fn columns(&self) -> usize {
        let (w, _) = self.tile_size();
        let inner = PANEL_WIDTH - 2.0 * PAD;
        (((inner + GAP) / (w + GAP)).floor() as usize).max(1)
    }

    /// WS-02: how many windows each workspace lists.
    pub fn set_max_windows(&mut self, n: usize) -> bool {
        let n = n.max(1);
        let changed = n != self.max_windows;
        self.max_windows = n;
        changed
    }

    /// FOCUS-01: out or in, as the shell's attention decides (hover,
    /// click, keyboard, Escape and their timings live there). Returns
    /// whether it changed.
    pub fn set_open(&mut self, open: bool) -> bool {
        if !open {
            return self.close();
        }
        if self.open || self.anchor.is_none() {
            return false;
        }
        self.open = true;
        self.motion.set_open(true);
        true
    }

    pub fn set_model(&mut self, model: WorkspaceModel) -> bool {
        if model == self.model {
            return false;
        }
        self.model = model;
        true
    }

    pub fn close(&mut self) -> bool {
        self.press = None;
        self.press_at = None;
        self.carry = None;
        self.hovered = None;
        if !self.open {
            return false;
        }
        self.open = false;
        self.motion.set_open(false);
        true
    }

    fn content_height(&self) -> f32 {
        let (_, th) = self.tile_size();
        let cell = GAP + LABEL + th;
        match self.view {
            WsView::Map => {
                let rows = self.model.cards.len().div_ceil(self.columns()).max(1);
                return PAD + HEADER + rows as f32 * cell + PAD;
            }
            WsView::Strip => return PAD + HEADER + cell + PAD,
            WsView::List => {}
        }
        let mut h = PAD + HEADER;
        for card in &self.model.cards {
            h += GAP + CARD_HEAD;
            let shown = card.windows.len().min(self.max_windows);
            let notes =
                usize::from(card.windows.is_empty() || card.windows.len() > self.max_windows);
            h += (shown + notes) as f32 * ROW;
        }
        h + PAD
    }

    fn height(&self) -> f32 {
        let room = (self.viewport.1 - 2.0 * PAD).max(ROW * 4.0);
        self.content_height().min(MAX_HEIGHT).min(room)
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
        // The height follows the content (a window opening, closing).
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

    pub fn rows(&self) -> Vec<WsRow> {
        if self.view != WsView::List {
            return self.tile_rows();
        }
        let inner = PANEL_WIDTH - 2.0 * PAD;
        let limit = self.shown_height.unwrap_or_else(|| self.height()) - PAD;
        let mut rows = vec![WsRow::Header {
            text: t("Workspaces"),
            x: PAD,
            y: PAD,
            width: inner,
            height: HEADER,
        }];
        let mut y = PAD + HEADER;
        for (index, card) in self.model.cards.iter().enumerate() {
            y += GAP;
            if y + CARD_HEAD > limit {
                break;
            }
            rows.push(WsRow::Card {
                index,
                label: card.label.clone(),
                active: card.active,
                count: card.windows.len(),
                x: PAD,
                y,
                width: inner,
                height: CARD_HEAD,
            });
            y += CARD_HEAD;
            if card.windows.is_empty() {
                if y + ROW <= limit {
                    rows.push(WsRow::Note {
                        text: t("No windows").to_owned(),
                        x: PAD + 34.0,
                        y,
                        width: inner - 34.0,
                        height: ROW,
                    });
                }
                y += ROW;
                continue;
            }
            for (i, window) in card.windows.iter().take(self.max_windows).enumerate() {
                if y + ROW > limit {
                    break;
                }
                rows.push(WsRow::Window {
                    card: index,
                    index: i,
                    window: window.clone(),
                    x: PAD + 10.0,
                    y,
                    width: inner - 10.0,
                    height: ROW,
                });
                y += ROW;
            }
            let more = card.windows.len().saturating_sub(self.max_windows);
            if more > 0 && y + ROW <= limit {
                rows.push(WsRow::Note {
                    text: gnos_i18n::tf("and {more} more", &[("more", &more.to_string())]),
                    x: PAD + 34.0,
                    y,
                    width: inner - 34.0,
                    height: ROW,
                });
                y += ROW;
            }
        }
        rows
    }

    /// WS-03: the miniatures (a grid for the map, a row for the strip),
    /// each window drawn where it stands on its output, the focused one last
    /// (above).
    fn tile_rows(&self) -> Vec<WsRow> {
        let width = self.width();
        let inner = width - 2.0 * PAD;
        let limit = self.shown_height.unwrap_or_else(|| self.height()) - PAD;
        let (tw, th) = self.tile_size();
        let columns = match self.view {
            WsView::Strip => self.model.cards.len().max(1),
            _ => self.columns(),
        };
        let mut rows = vec![WsRow::Header {
            text: t("Workspaces"),
            x: PAD,
            y: PAD,
            width: inner,
            height: HEADER,
        }];
        for (index, card) in self.model.cards.iter().enumerate() {
            let (col, row) = (index % columns, index / columns);
            let x = PAD + col as f32 * (tw + GAP);
            let y = PAD + HEADER + GAP + row as f32 * (GAP + LABEL + th);
            if y + LABEL + th > limit + PAD || x + tw > width - PAD + 0.5 {
                continue;
            }
            rows.push(WsRow::Tile {
                index,
                label: card.label.clone(),
                active: card.active,
                count: card.windows.len(),
                x,
                y,
                width: tw,
                height: LABEL + th,
            });
            let (bx, by) = (x, y + LABEL);
            let mut order: Vec<usize> = (0..card.windows.len()).collect();
            order.sort_by_key(|i| card.windows[*i].focused);
            for i in order {
                let window = &card.windows[i];
                let Some([l, t, r, b]) = window.bounds else {
                    continue;
                };
                let (l, t) = (l.clamp(0.0, 1.0), t.clamp(0.0, 1.0));
                let (r, b) = (r.clamp(l, 1.0), b.clamp(t, 1.0));
                let w = ((r - l) * tw).max(MINI_MIN).min(tw);
                let h = ((b - t) * th).max(MINI_MIN).min(th);
                rows.push(WsRow::Mini {
                    card: index,
                    index: i,
                    window: window.clone(),
                    x: (bx + l * tw).min(bx + tw - w),
                    y: (by + t * th).min(by + th - h),
                    width: w,
                    height: h,
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
            self.width()
        };
        let openness =
            gnos_ux_surface::grammar::within_depth(self.motion.phase(), depth, self.give_way);
        if openness <= 0.0 {
            return None;
        }
        beside_module(self.side, self.viewport, anchor, self.width(), h, openness)
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
        let full = beside_module(self.side, self.viewport, anchor, self.width(), h, 1.0)?;
        Some((full.rect, self.side.edge()))
    }

    /// FOCUS-01D: how deep it may come out (`None`: fully). Returns whether
    /// that changed.
    pub fn set_give_way(&mut self, limit: Option<f32>) -> bool {
        let changed = self.give_way != limit;
        self.give_way = limit;
        changed
    }

    /// Whether the pointer is on the panel as it shows.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.local(x, y).is_some()
    }

    fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let frame = self.frame()?;
        let v = frame.visible;
        (x >= v.left() && x < v.right() && y >= v.top() && y < v.bottom())
            .then(|| (x - frame.rect.left(), y - frame.rect.top()))
    }

    /// The window being carried, for the view to draw under the pointer.
    pub fn carried(&self) -> Option<&Carried> {
        self.carry.as_ref()
    }

    /// The workspace whose card, window or note lies under the pointer.
    /// CURSOR-01: what the pointer is over on the panel (`None`: off it).
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        self.local(x, y)?;
        Some(if self.effect_at(x, y).is_some() {
            crate::cursor::Affordance::Press
        } else {
            crate::cursor::Affordance::Inert
        })
    }

    /// CURSOR-01: a window carried, and whether the place under the pointer
    /// takes it (another workspace) or refuses it (its own).
    pub fn carry_at(&self, x: f32, y: f32) -> Option<crate::cursor::Carry> {
        let carry = self.carry.as_ref()?;
        let card = self.card_at(x, y);
        Some(crate::cursor::Carry {
            drop: card.map(|c| c != carry.card),
        })
    }

    /// The workspace under the pointer: the topmost row there (a window in
    /// a miniature before the miniature), a note counting for its card.
    fn card_at(&self, x: f32, y: f32) -> Option<usize> {
        let (lx, ly) = self.local(x, y)?;
        let mut current = None;
        let mut hit = None;
        for row in self.rows() {
            if let Some(card) = row
                .card()
                .filter(|_| matches!(row, WsRow::Card { .. } | WsRow::Tile { .. }))
            {
                current = Some(card);
            }
            if row.contains(lx, ly) {
                hit = match row {
                    WsRow::Header { .. } => None,
                    WsRow::Note { .. } => current,
                    ref r => r.card(),
                };
            }
        }
        hit
    }

    /// What a press on the topmost row under the pointer does.
    fn effect_at(&self, x: f32, y: f32) -> Option<WorkspaceEffect> {
        let (lx, ly) = self.local(x, y)?;
        self.rows()
            .into_iter()
            .rev()
            .find(|r| r.contains(lx, ly))
            .and_then(|r| r.effect())
    }

    /// A primary press. Outside the panel (and not on the module, which the
    /// scene routes first) it closes it.
    pub fn pointer_down(&mut self, x: f32, y: f32) -> WorkspaceOutcome {
        let mut outcome = WorkspaceOutcome::default();
        if !self.open {
            return outcome;
        }
        // Outside it: not its press (the shell's attention closes it).
        if self.local(x, y).is_none() {
            return outcome;
        }
        outcome.consumed = true;
        self.press = self.effect_at(x, y);
        self.press_at = Some((x, y));
        outcome
    }

    /// The row under the pointer takes a tint.
    pub fn pointer_moved(&mut self, x: f32, y: f32) -> WorkspaceOutcome {
        // WS-02B: a press on a window that travels carries it.
        if self.carry.is_none()
            && let (Some(WorkspaceEffect::Focus(card, index)), Some((px0, py0))) =
                (self.press, self.press_at)
            && (x - px0).hypot(y - py0) > DRAG_START
            && let Some(window) = self.rows().into_iter().find_map(|r| match r {
                WsRow::Window {
                    card: c,
                    index: i,
                    window,
                    ..
                }
                | WsRow::Mini {
                    card: c,
                    index: i,
                    window,
                    ..
                } if c == card && i == index => Some(window),
                _ => None,
            })
        {
            self.carry = Some(Carried {
                card,
                index,
                window,
                at: (x, y),
                over: None,
            });
        }
        if let Some(carry) = self.carry.as_mut() {
            carry.at = (x, y);
            let source = carry.card;
            let over = self.card_at(x, y).filter(|c| *c != source);
            if let Some(carry) = self.carry.as_mut() {
                carry.over = over;
            }
            self.hovered = over.map(WorkspaceEffect::Activate);
            return WorkspaceOutcome {
                consumed: true,
                changed: true,
                ..Default::default()
            };
        }
        let hovered = self.effect_at(x, y);
        let changed = hovered != self.hovered;
        self.hovered = hovered;
        WorkspaceOutcome {
            consumed: self.local(x, y).is_some(),
            changed,
            ..Default::default()
        }
    }

    /// A press and release on one row carries out what it names.
    pub fn pointer_up(&mut self, x: f32, y: f32) -> WorkspaceOutcome {
        let mut outcome = WorkspaceOutcome::default();
        self.press_at = None;
        // WS-02B: a carried window lands on the workspace under the pointer,
        // or nowhere.
        if let Some(carry) = self.carry.take() {
            self.press = None;
            self.hovered = None;
            outcome.consumed = true;
            outcome.changed = true;
            if let Some(to) = self.card_at(x, y).filter(|c| *c != carry.card) {
                outcome.effects.push(WorkspaceEffect::Move {
                    card: carry.card,
                    index: carry.index,
                    to,
                });
            }
            return outcome;
        }
        let Some(press) = self.press.take() else {
            outcome.consumed = self.open && self.local(x, y).is_some();
            return outcome;
        };
        outcome.consumed = true;
        if self.effect_at(x, y) == Some(press) {
            outcome.changed = true;
            outcome.effects.push(press);
        }
        outcome
    }

    /// Escape while a window is carried puts it back; returns whether one
    /// was. (Escape on the panel itself is the shell's attention's.)
    pub fn escape_carry(&mut self) -> bool {
        if self.carry.take().is_none() {
            return false;
        }
        self.press = None;
        self.press_at = None;
        self.hovered = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> WorkspaceModel {
        let w = |title: &str, focused| WsWindow {
            title: title.into(),
            symbol: "terminal".into(),
            focused,
            bounds: Some([0.0, 0.0, 0.5, 1.0]),
            surface: None,
        };
        WorkspaceModel {
            cards: vec![
                WsCard {
                    label: "1".into(),
                    active: true,
                    windows: vec![w("Shell", true), w("Editor", false)],
                },
                WsCard {
                    label: "2".into(),
                    active: false,
                    windows: Vec::new(),
                },
                WsCard {
                    label: "3".into(),
                    active: false,
                    windows: (0..9).map(|i| w(&format!("Tab {i}"), false)).collect(),
                },
            ],
            aspect: 16.0 / 9.0,
        }
    }

    fn panel() -> WorkspacePanel {
        let mut p = WorkspacePanel::new();
        p.place(RailSide::Left, (1280.0, 1000.0), Some(300.0));
        p.set_model(model());
        p
    }

    fn settle(p: &mut WorkspacePanel, from: Instant) -> Instant {
        let mut now = from;
        for _ in 0..200 {
            now += Duration::from_millis(16);
            if !p.tick(now, false) {
                break;
            }
        }
        now
    }

    fn centre(p: &WorkspacePanel, pick: impl Fn(&WsRow) -> bool) -> (f32, f32) {
        let f = p.frame().expect("open");
        let row = p.rows().into_iter().find(|r| pick(r)).expect("row");
        let (x, y, w, h) = row.rect();
        (f.rect.left() + x + w / 2.0, f.rect.top() + y + h / 2.0)
    }

    #[test]
    fn the_map_draws_each_window_where_it_stands_and_carries_it_between_miniatures() {
        let mut p = panel();
        assert!(p.set_view(WsView::Map, 90.0));
        p.set_open(true);
        settle(&mut p, Instant::now());
        let rows = p.rows();
        let tiles: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                WsRow::Tile { index, .. } => Some((*index, r.rect())),
                _ => None,
            })
            .collect();
        assert_eq!(tiles.len(), 3, "every workspace has its miniature");
        // 16:9 at 90 px high is 160 wide: one per row in a 312 px panel.
        let (_, (x0, y0, w0, h0)) = tiles[0];
        assert_eq!((w0, h0 - LABEL), (160.0, 90.0));
        assert!(tiles[1].1.1 > y0, "the next row");
        // The windows stand in the left half (their bounds), the focused one
        // drawn last.
        let minis: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                WsRow::Mini {
                    card: 0,
                    index,
                    window,
                    ..
                } => Some((*index, window.focused, r.rect())),
                _ => None,
            })
            .collect();
        assert_eq!(minis.len(), 2);
        assert!(minis.last().unwrap().1, "focused above");
        let (_, _, (mx, my, mw, mh)) = minis[0];
        assert_eq!((mx, my, mw, mh), (x0, y0 + LABEL, 80.0, 90.0));
        // A press on a window focuses it; on the miniature's free part,
        // goes to the workspace.
        let f = p.frame().unwrap();
        let at = |x: f32, y: f32| (f.rect.left() + x, f.rect.top() + y);
        let (wx, wy) = at(mx + 10.0, my + 10.0);
        // Both stand in the same place: the press reaches the one above.
        p.pointer_down(wx, wy);
        assert_eq!(
            p.pointer_up(wx, wy).effects,
            vec![WorkspaceEffect::Focus(0, 0)]
        );
        let (fx, fy) = at(x0 + w0 - 10.0, y0 + LABEL + 10.0);
        p.pointer_down(fx, fy);
        assert_eq!(
            p.pointer_up(fx, fy).effects,
            vec![WorkspaceEffect::Activate(0)]
        );
        // Carried onto the third miniature: it moves there.
        let (_, (x2, y2, w2, h2)) = tiles[2];
        let (tx, ty) = at(x2 + w2 / 2.0, y2 + h2 / 2.0);
        p.pointer_down(wx, wy);
        p.pointer_moved(wx, wy + 20.0);
        p.pointer_moved(tx, ty);
        assert_eq!(p.carried().map(|c| c.over), Some(Some(2)));
        assert_eq!(
            p.pointer_up(tx, ty).effects,
            vec![WorkspaceEffect::Move {
                card: 0,
                index: 0,
                to: 2
            }]
        );
    }

    #[test]
    fn the_strip_holds_the_miniatures_in_one_row_within_the_output() {
        let mut p = panel();
        p.set_view(WsView::Strip, 100.0);
        p.set_open(true);
        settle(&mut p, Instant::now());
        let tops: Vec<f32> = p
            .rows()
            .iter()
            .filter(|r| matches!(r, WsRow::Tile { .. }))
            .map(|r| r.rect().1)
            .collect();
        assert_eq!(tops.len(), 3);
        assert!(tops.iter().all(|t| *t == tops[0]), "one row");
        let w = 100.0 * 16.0 / 9.0;
        assert!((p.width() - (2.0 * PAD + 3.0 * w + 2.0 * GAP)).abs() < 0.01);
        assert!(p.frame().unwrap().rect.width() <= 1280.0);
    }

    #[test]
    fn the_windows_listed_follow_the_setting() {
        let mut p = panel();
        let t0 = Instant::now();
        p.set_open(true);
        settle(&mut p, t0);
        let windows = |p: &WorkspacePanel| {
            p.rows()
                .iter()
                .filter(|r| matches!(r, WsRow::Window { card: 2, .. }))
                .count()
        };
        assert_eq!(windows(&p), MAX_WINDOWS);
        assert!(p.set_max_windows(3));
        assert_eq!(windows(&p), 3);
        let more = p
            .rows()
            .into_iter()
            .any(|r| matches!(r, WsRow::Note { ref text, .. } if text.contains('6')));
        assert!(more, "the rest is counted: and 6 more");
        assert!(!p.set_max_windows(3), "unchanged");
    }

    #[test]
    fn a_window_carried_onto_another_workspace_moves_there() {
        let mut p = panel();
        let t0 = Instant::now();
        p.set_open(true);
        settle(&mut p, t0);
        let editor = centre(&p, |r| {
            matches!(
                r,
                WsRow::Window {
                    card: 0,
                    index: 1,
                    ..
                }
            )
        });
        let second = centre(&p, |r| matches!(r, WsRow::Card { index: 1, .. }));
        let first = centre(&p, |r| matches!(r, WsRow::Card { index: 0, .. }));
        // A short press is still a click: it focuses.
        p.pointer_down(editor.0, editor.1);
        p.pointer_moved(editor.0 + 3.0, editor.1);
        assert!(p.carried().is_none());
        assert_eq!(
            p.pointer_up(editor.0 + 3.0, editor.1).effects,
            vec![WorkspaceEffect::Focus(0, 1)]
        );
        // Carried onto workspace 2: it moves there.
        p.pointer_down(editor.0, editor.1);
        p.pointer_moved(editor.0, editor.1 + 20.0);
        p.pointer_moved(second.0, second.1);
        let carried = p.carried().expect("carried");
        assert_eq!(
            (carried.window.title.as_str(), carried.over),
            ("Editor", Some(1))
        );
        assert_eq!(
            p.hovered(),
            Some(WorkspaceEffect::Activate(1)),
            "target tinted"
        );
        assert_eq!(
            p.pointer_up(second.0, second.1).effects,
            vec![WorkspaceEffect::Move {
                card: 0,
                index: 1,
                to: 1
            }]
        );
        // Dropped back on its own workspace: nothing.
        p.pointer_down(editor.0, editor.1);
        p.pointer_moved(first.0, first.1 + 30.0);
        assert_eq!(p.carried().map(|c| c.over), Some(None));
        assert!(p.pointer_up(first.0, first.1).effects.is_empty());
        // Escape puts a carried window back.
        p.pointer_down(editor.0, editor.1);
        p.pointer_moved(second.0, second.1);
        assert!(p.escape_carry() && p.carried().is_none() && p.is_open());
    }

    #[test]
    fn it_lists_each_workspace_with_its_windows_and_counts_the_rest() {
        let mut p = panel();
        p.set_open(true);
        settle(&mut p, Instant::now());
        let rows = p.rows();
        let cards = rows
            .iter()
            .filter(|r| matches!(r, WsRow::Card { .. }))
            .count();
        assert_eq!(cards, 3);
        let windows = rows
            .iter()
            .filter(|r| matches!(r, WsRow::Window { .. }))
            .count();
        assert_eq!(windows, 2 + MAX_WINDOWS);
        assert!(
            rows.iter()
                .any(|r| matches!(r, WsRow::Note { text, .. } if text == "No windows"))
        );
        assert!(
            rows.iter()
                .any(|r| matches!(r, WsRow::Note { text, .. } if text.contains('3')))
        );
    }

    #[test]
    fn a_click_goes_to_a_workspace_or_focuses_a_window_and_outside_closes() {
        let mut p = panel();
        let t0 = Instant::now();
        p.set_open(true);
        settle(&mut p, t0);
        let (x, y) = centre(&p, |r| matches!(r, WsRow::Card { index: 1, .. }));
        p.pointer_down(x, y);
        assert_eq!(
            p.pointer_up(x, y).effects,
            vec![WorkspaceEffect::Activate(1)]
        );
        let (x, y) = centre(&p, |r| {
            matches!(
                r,
                WsRow::Window {
                    card: 0,
                    index: 1,
                    ..
                }
            )
        });
        assert!(p.pointer_moved(x, y).changed);
        assert_eq!(p.hovered(), Some(WorkspaceEffect::Focus(0, 1)));
        p.pointer_down(x, y);
        assert_eq!(
            p.pointer_up(x, y).effects,
            vec![WorkspaceEffect::Focus(0, 1)]
        );
        // A press that slides off its row fires nothing.
        p.pointer_down(x, y);
        let (x2, y2) = centre(&p, |r| matches!(r, WsRow::Card { index: 2, .. }));
        assert!(p.pointer_up(x2, y2).effects.is_empty());
        // Outside: not its press.
        let outside = p.pointer_down(1200.0, 900.0);
        assert!(!outside.consumed && !outside.changed && p.is_open());
    }

    #[test]
    fn it_never_grows_past_the_output_and_keeps_its_place_through_a_relayout() {
        let mut p = WorkspacePanel::new();
        p.place(RailSide::Left, (1280.0, 400.0), Some(200.0));
        p.set_model(model());
        p.set_open(true);
        settle(&mut p, Instant::now());
        let f = p.frame().unwrap();
        assert!(f.rect.height() <= 400.0 - 2.0 * PAD + 0.5);
        let bottom = p
            .rows()
            .iter()
            .map(|r| r.rect().1 + r.rect().3)
            .fold(0.0, f32::max);
        assert!(bottom <= f.rect.height() + 0.5, "no row past the panel");
        // A relayout: the module's place is briefly unknown; the panel
        // stays where it is, open.
        p.place(RailSide::Left, (1280.0, 400.0), None);
        assert!(p.is_open() && p.frame().is_some());
    }
}
