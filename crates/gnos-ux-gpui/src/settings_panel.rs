//! UXE-05B: the Settings surface, headless.
//!
//! One shell surface (D01): a panel centred on the area right of the Bar,
//! extruded from the frame's top border with the envelope grammar. There is
//! no window form. A navigation pane lists the pages by category; the page
//! area shows the current page's lines (`settings_pages`).
//!
//! It opens from Ctrl+, and closes with Escape, its close control, or a
//! press outside. Up/Down (and Tab/Shift-Tab) move through the pages with
//! blob.in's `ListCursor`; a click selects a page; the wheel scrolls a long
//! page. Nothing here writes a preference.

use crate::settings_pages::{Category, PAGES, PageId, SettingsAction, SettingsLine};
use blobin_ui_core::layout::Rect;
use blobin_ui_core::list::{CursorStep, Ends, ListCursor};
use gnos_ux_surface::envelope::SurfaceEdge;
use gnos_ux_surface::grammar::{
    ConnectiveHullGeometry, ExtrusionMotion, Follow, GeometryRect, TemporalResponseMode,
    border_parent, fused_hull, revealed,
};
use std::time::Instant;

pub const MAX_WIDTH: f32 = 880.0;
pub const NAV_WIDTH: f32 = 220.0;
pub const PADDING: f32 = 16.0;
pub const TITLE_HEIGHT: f32 = 40.0;
pub const CLOSE_SIZE: f32 = 32.0;
pub const NAV_ITEM: f32 = 34.0;
pub const NAV_CATEGORY: f32 = 26.0;
pub const LINE_GAP: f32 = 6.0;
/// Space kept between the panel and whatever lies below it.
const BOTTOM_MARGIN: f32 = 12.0;
/// The smallest navigation rows before the pane clips instead.
const NAV_ITEM_MIN: f32 = 24.0;
const NAV_CATEGORY_MIN: f32 = 20.0;
const SETTLE_PX: f32 = 0.25;
const WHEEL_STEP: f32 = 48.0;

pub fn line_height(line: &SettingsLine) -> f32 {
    match line {
        SettingsLine::Header(_) => 30.0,
        SettingsLine::Caption(_) => 22.0,
        SettingsLine::Value { .. }
        | SettingsLine::State { .. }
        | SettingsLine::Choice { .. }
        | SettingsLine::Stepper { .. } => 34.0,
        SettingsLine::Info { kind, .. } => match kind {
            crate::bar_status_popouts::InfoRowKind::Profiles { .. } => 44.0,
            _ => 34.0,
        },
        SettingsLine::UnderConstruction { .. } => 160.0,
        // MODS-01B: a name over a description line.
        SettingsLine::Module { .. } => 52.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsKey {
    Up,
    Down,
    Tab,
    BackTab,
    Left,
    Right,
    /// Enter or Space.
    Activate,
    Escape,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsOutcome {
    pub consumed: bool,
    pub changed: bool,
    pub opened: bool,
    pub closed: bool,
    /// A control asked for this edit; the scene turns it into a typed
    /// request for the profile authority.
    pub action: Option<SettingsAction>,
}

/// Width of a Choice's value button and of each Stepper button. The value
/// button never takes more than 40 % of the line, so the label stays legible.
pub const CHOICE_WIDTH: f32 = 180.0;

fn choice_width(line_width: f32) -> f32 {
    CHOICE_WIDTH.min(line_width * 0.4)
}

/// The navigation pane: its full width, or 35 % of a narrow panel.
fn nav_width(panel_width: f32) -> f32 {
    NAV_WIDTH.min(panel_width * 0.35)
}
pub const STEP_SIZE: f32 = 28.0;

#[derive(Clone, Debug, PartialEq)]
pub struct NavPaint {
    pub category: Option<Category>,
    pub page: Option<PageId>,
    pub rect: Rect,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinePaint {
    pub line: SettingsLine,
    pub rect: Rect,
    /// Hit targets for the line's controls.
    pub buttons: Vec<(Rect, SettingsAction)>,
}

fn line_buttons(line: &SettingsLine, rect: Rect) -> Vec<(Rect, SettingsAction)> {
    let mid = rect.y + (rect.height - STEP_SIZE) / 2.0;
    match line {
        SettingsLine::Choice { action, .. } => vec![(
            Rect::new(
                rect.x + rect.width - choice_width(rect.width),
                mid,
                choice_width(rect.width),
                STEP_SIZE,
            ),
            *action,
        )],
        SettingsLine::Stepper { down, up, .. } => vec![
            (
                Rect::new(
                    rect.x + rect.width - 2.0 * STEP_SIZE - 8.0,
                    mid,
                    STEP_SIZE,
                    STEP_SIZE,
                ),
                *down,
            ),
            (
                Rect::new(rect.x + rect.width - STEP_SIZE, mid, STEP_SIZE, STEP_SIZE),
                *up,
            ),
        ],
        // MODS-01B: [place] [earlier] [later], from the right edge.
        SettingsLine::Module {
            actions: Some([place, earlier, later]),
            ..
        } => {
            let later_x = rect.x + rect.width - STEP_SIZE;
            let earlier_x = later_x - STEP_SIZE - 8.0;
            let place_w = MODULE_PLACE_WIDTH.min(rect.width * 0.3);
            vec![
                (
                    Rect::new(earlier_x - 12.0 - place_w, mid, place_w, STEP_SIZE),
                    *place,
                ),
                (Rect::new(earlier_x, mid, STEP_SIZE, STEP_SIZE), *earlier),
                (Rect::new(later_x, mid, STEP_SIZE, STEP_SIZE), *later),
            ]
        }
        _ => Vec::new(),
    }
}

/// MODS-01B: the width of a catalogue module's place button.
pub const MODULE_PLACE_WIDTH: f32 = 128.0;

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsFrame {
    /// Content box at its sliding position.
    pub panel: Rect,
    pub hull: Option<ConnectiveHullGeometry>,
    /// Paint nothing above this y: the frame's inner top edge.
    pub clip_top: f32,
    /// Paint nothing below this y: a bottom Bar's inner edge when the panel
    /// stands on it (TOPO-01C2b, T06); the output's height otherwise.
    pub clip_bottom: f32,
    pub title: Rect,
    pub close: Rect,
    pub nav: Vec<NavPaint>,
    /// The page control holding keyboard focus, when focus is in the page.
    pub focused: Option<(Rect, SettingsAction)>,
    pub page_area: Rect,
    pub lines: Vec<LinePaint>,
    pub page: PageId,
    /// The navigation's column, and how far it is scrolled out of how far it
    /// can be (SET-02: more pages than a short output holds).
    pub nav_area: Rect,
    pub nav_scroll: f32,
    pub nav_max: f32,
    /// SET-04: the control under the pointer, and whether it is held down.
    pub hovered: Option<Target>,
    pub pressed: bool,
}

/// SET-04: what a click at a point would act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Close(Rect),
    Nav(Rect),
    Button(Rect, SettingsAction),
}

impl Target {
    pub fn rect(self) -> Rect {
        match self {
            Self::Close(r) | Self::Nav(r) | Self::Button(r, _) => r,
        }
    }

    /// For the receipt.
    pub fn slug(self) -> String {
        match self {
            Self::Close(_) => "close".to_owned(),
            Self::Nav(_) => "nav".to_owned(),
            Self::Button(_, a) => a.slug().to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    viewport: (f32, f32),
    border: f32,
    bar: f32,
    /// Right inset (the rail's width when the rail is on the right).
    right: f32,
    /// The panel stays above this y (the Dock's top, or the bottom border).
    floor: f32,
    /// TOPO-01C: the inner top edge the panel extrudes from: the border, or
    /// a top Bar's inner edge (operator decision T04).
    top: f32,
    /// TOPO-01C2b (T06): `Some((anchor, ceiling))` when the panel stands on a
    /// bottom Bar's inner edge `anchor` and stays below `ceiling` (the Dock's
    /// bottom, at the top of the output).
    stand: Option<(f32, f32)>,
}

impl Geometry {
    fn width(self) -> f32 {
        let free = self.viewport.0 - self.bar - self.right;
        MAX_WIDTH.min(free - 50.0).max(3.0 * PADDING + 1.0)
    }
    fn x(self) -> f32 {
        let free = self.viewport.0 - self.bar - self.right;
        self.bar + ((free - self.width()) / 2.0).max(0.0)
    }
    fn max_height(self) -> f32 {
        if let Some((anchor, ceiling)) = self.stand {
            return (anchor - ceiling.max(self.border) - BOTTOM_MARGIN)
                .max(TITLE_HEIGHT + 3.0 * PADDING);
        }
        let floor = self.floor.min(self.viewport.1 - self.border);
        (floor - self.top - BOTTOM_MARGIN).max(TITLE_HEIGHT + 3.0 * PADDING)
    }
}

const CATEGORIES: f32 = 5.0;

fn nav_height_with(item: f32, category: f32) -> f32 {
    CATEGORIES * category + PAGES.len() as f32 * item
}

/// Navigation row heights that fit `available`, compacted down to a floor;
/// below that the pane clips.
fn nav_rows(available: f32) -> (f32, f32) {
    let full = nav_height_with(NAV_ITEM, NAV_CATEGORY);
    if available >= full {
        return (NAV_ITEM, NAV_CATEGORY);
    }
    let scale = (available / full).max(0.0);
    let category = (NAV_CATEGORY * scale).max(NAV_CATEGORY_MIN);
    // The pages share what the categories leave, so the last one fits
    // whenever the floors allow it.
    let item =
        ((available - CATEGORIES * category) / PAGES.len() as f32).clamp(NAV_ITEM_MIN, NAV_ITEM);
    (item, category)
}

pub struct SettingsHost {
    geometry: Option<Geometry>,
    open: bool,
    cursor: ListCursor,
    /// Keyboard focus is in the page's controls rather than the navigation.
    in_page: bool,
    control: ListCursor,
    lines: Vec<SettingsLine>,
    scroll: f32,
    /// The navigation's scroll; `None` keeps the current page in view.
    nav_scroll: Option<f32>,
    motion: ExtrusionMotion,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
    height: Follow,
    press: Option<(f32, f32)>,
    /// SET-04: where the pointer is over the scene.
    pointer: Option<(f32, f32)>,
    last_tick: Option<Instant>,
}

impl Default for SettingsHost {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsHost {
    pub fn new() -> Self {
        let mut cursor = ListCursor::new(Ends::Clamp);
        cursor.reset(PAGES.len());
        Self {
            geometry: None,
            open: false,
            cursor,
            in_page: false,
            control: ListCursor::new(Ends::Clamp),
            lines: Vec::new(),
            scroll: 0.0,
            nav_scroll: None,
            motion: ExtrusionMotion::closed(),
            give_way: None,
            height: Follow::at(f32::NAN),
            press: None,
            pointer: None,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether keyboard focus is in the page's controls.
    pub fn focus_in_page(&self) -> bool {
        self.in_page
    }

    /// The page's controls in reading order.
    fn controls(&self) -> Vec<SettingsAction> {
        self.lines
            .iter()
            .flat_map(|line| match line {
                SettingsLine::Choice { action, .. } => vec![*action],
                SettingsLine::Stepper { down, up, .. } => vec![*down, *up],
                _ => Vec::new(),
            })
            .collect()
    }

    pub fn page(&self) -> PageId {
        PAGES[self.cursor.current().unwrap_or(0)].0
    }

    fn content_height(&self) -> f32 {
        self.lines
            .iter()
            .map(|l| line_height(l) + LINE_GAP)
            .sum::<f32>()
    }

    fn target_height(&self, g: Geometry) -> f32 {
        let body = nav_height_with(NAV_ITEM, NAV_CATEGORY).max(self.content_height());
        (TITLE_HEIGHT + body + 3.0 * PADDING).min(g.max_height())
    }

    fn page_viewport(&self, g: Geometry) -> f32 {
        (self.height.value - TITLE_HEIGHT - 3.0 * PADDING)
            .max(0.0)
            .min(g.max_height())
    }

    fn clamp_scroll(&mut self) {
        if let Some(g) = self.geometry {
            let max = (self.content_height() - self.page_viewport(g)).max(0.0);
            self.scroll = self.scroll.clamp(0.0, max);
        }
    }

    pub fn set_geometry(
        &mut self,
        viewport: (f32, f32),
        border: f32,
        bar: f32,
        right: f32,
        floor: f32,
    ) -> bool {
        self.set_geometry_on(viewport, border, bar, right, floor, border)
    }

    /// TOPO-01C: as `set_geometry`, extruding from `top` (a top Bar's inner
    /// edge, fused to the Bar like its popouts).
    pub fn set_geometry_on(
        &mut self,
        viewport: (f32, f32),
        border: f32,
        bar: f32,
        right: f32,
        floor: f32,
        top: f32,
    ) -> bool {
        self.apply_geometry(Geometry {
            viewport,
            border,
            bar,
            right,
            floor,
            top,
            stand: None,
        })
    }

    /// TOPO-01C2b (T06): the panel stands on a bottom Bar's inner edge
    /// `anchor`, fused to it, and stays below `ceiling`.
    pub fn set_geometry_standing(
        &mut self,
        viewport: (f32, f32),
        border: f32,
        bar: f32,
        right: f32,
        ceiling: f32,
        anchor: f32,
    ) -> bool {
        self.apply_geometry(Geometry {
            viewport,
            border,
            bar,
            right,
            floor: viewport.1,
            top: border,
            stand: Some((anchor, ceiling)),
        })
    }

    fn apply_geometry(&mut self, g: Geometry) -> bool {
        if self.geometry == Some(g) {
            return false;
        }
        self.geometry = Some(g);
        if !self.height.value.is_finite() || !self.motion.is_visible() {
            self.height = Follow::at(self.target_height(g));
        }
        self.clamp_scroll();
        true
    }

    /// The current page's lines, rebuilt by the scene from fresh facts.
    pub fn set_lines(&mut self, lines: Vec<SettingsLine>) -> bool {
        if lines == self.lines {
            return false;
        }
        self.lines = lines;
        // Keep the focused control across a refresh of the same page shape.
        let controls = self.controls().len();
        if controls != self.control.len() {
            self.control.reset(controls);
            if controls == 0 {
                self.in_page = false;
            }
        }
        if !self.motion.is_visible()
            && let Some(g) = self.geometry
        {
            self.height = Follow::at(self.target_height(g));
        }
        self.clamp_scroll();
        true
    }

    pub fn open(&mut self) -> SettingsOutcome {
        let mut outcome = SettingsOutcome {
            consumed: true,
            ..Default::default()
        };
        if !self.open {
            self.open = true;
            self.motion.set_open(true);
            outcome.opened = true;
            outcome.changed = true;
        }
        outcome
    }

    pub fn close(&mut self) -> SettingsOutcome {
        let mut outcome = SettingsOutcome {
            consumed: true,
            ..Default::default()
        };
        if self.open {
            self.open = false;
            self.motion.set_open(false);
            self.press = None;
            outcome.closed = true;
            outcome.changed = true;
        }
        outcome
    }

    /// Open on `page` (from a popout's "Open settings"); an open panel
    /// switches page.
    pub fn open_page(&mut self, page: PageId) -> SettingsOutcome {
        let index = PAGES.iter().position(|(p, _)| *p == page).unwrap_or(0);
        let switched = self.select(index);
        let mut outcome = self.open();
        outcome.changed |= switched;
        outcome
    }

    pub fn toggle(&mut self) -> SettingsOutcome {
        if self.open { self.close() } else { self.open() }
    }

    fn select(&mut self, index: usize) -> bool {
        if self.cursor.select(index) {
            self.scroll = 0.0;
            self.in_page = false;
            return true;
        }
        false
    }

    pub fn key(&mut self, key: SettingsKey) -> SettingsOutcome {
        if !self.open {
            return SettingsOutcome::default();
        }
        let mut outcome = SettingsOutcome {
            consumed: true,
            ..Default::default()
        };
        if self.in_page {
            match key {
                SettingsKey::Escape | SettingsKey::Left => {
                    self.in_page = false;
                    outcome.changed = true;
                }
                SettingsKey::Up | SettingsKey::BackTab => {
                    outcome.changed = self.control.step(CursorStep::Previous, 1);
                }
                SettingsKey::Down | SettingsKey::Tab | SettingsKey::Right => {
                    outcome.changed = self.control.step(CursorStep::Next, 1);
                }
                SettingsKey::Activate => {
                    outcome.action = self
                        .control
                        .current()
                        .and_then(|i| self.controls().get(i).copied());
                }
            }
            return outcome;
        }
        let step = match key {
            SettingsKey::Escape => return self.close(),
            SettingsKey::Up | SettingsKey::BackTab => CursorStep::Previous,
            SettingsKey::Down | SettingsKey::Tab => CursorStep::Next,
            SettingsKey::Right | SettingsKey::Activate => {
                // Into the page, when it has controls.
                if !self.controls().is_empty() {
                    self.in_page = true;
                    self.control.reset(self.controls().len());
                    outcome.changed = true;
                }
                return outcome;
            }
            SettingsKey::Left => return outcome,
        };
        outcome.changed = self.cursor.step(step, 1);
        if outcome.changed {
            self.scroll = 0.0;
            self.nav_scroll = None;
        }
        outcome
    }

    /// SET-04: the pointer moved; whether what it is over changed.
    /// Whether `(x, y)` is on the panel as it stands now.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.frame().is_some_and(|f| {
            let p = f.panel;
            x >= p.x
                && x < p.x + p.width
                && y >= p.y.max(f.clip_top)
                && y < (p.y + p.height).min(f.clip_bottom)
        })
    }

    /// CURSOR-01: what the pointer is over on the panel (`None`: off it).
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        if !self.contains(x, y) {
            return None;
        }
        let f = self.frame()?;
        let p = (x, y);
        let target = Self::target_at(f.close, &f.nav, f.page_area, &f.lines, p);
        Some(if target.is_some() {
            crate::cursor::Affordance::Press
        } else {
            crate::cursor::Affordance::Inert
        })
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> bool {
        let before = self.frame().and_then(|f| f.hovered);
        self.pointer = Some((x, y));
        self.frame().and_then(|f| f.hovered) != before
    }

    /// What a click at `(x, y)` would act on, the rules `pointer_up` follows.
    fn target_at(
        close: Rect,
        nav: &[NavPaint],
        page_area: Rect,
        lines: &[LinePaint],
        (x, y): (f32, f32),
    ) -> Option<Target> {
        if close.contains(x, y) {
            return Some(Target::Close(close));
        }
        if page_area.contains(x, y)
            && let Some((r, a)) = lines
                .iter()
                .flat_map(|l| l.buttons.iter())
                .find(|(r, _)| r.contains(x, y))
        {
            return Some(Target::Button(*r, *a));
        }
        nav.iter()
            .find(|n| n.page.is_some() && n.rect.contains(x, y))
            .map(|n| Target::Nav(n.rect))
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) -> SettingsOutcome {
        if !self.open {
            return SettingsOutcome::default();
        }
        match self.frame() {
            Some(frame) if frame.panel.contains(x, y) => {
                self.press = Some((x, y));
                self.pointer = Some((x, y));
                SettingsOutcome {
                    consumed: true,
                    ..Default::default()
                }
            }
            // A press outside closes (focus-grab semantics).
            _ => self.close(),
        }
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> SettingsOutcome {
        let Some((px, py)) = self.press.take() else {
            return SettingsOutcome::default();
        };
        let mut outcome = SettingsOutcome {
            consumed: true,
            ..Default::default()
        };
        if (x - px).hypot(y - py) > 6.0 {
            return outcome;
        }
        let Some(frame) = self.frame() else {
            return outcome;
        };
        if frame.close.contains(x, y) {
            return self.close();
        }
        if let Some(action) = frame
            .lines
            .iter()
            .filter(|_| frame.page_area.contains(x, y))
            .flat_map(|l| l.buttons.iter())
            .find_map(|(r, a)| r.contains(x, y).then_some(*a))
        {
            outcome.action = Some(action);
            return outcome;
        }
        if let Some(index) = frame.nav.iter().find_map(|n| {
            n.rect
                .contains(x, y)
                .then_some(n.page)
                .flatten()
                .and_then(|p| PAGES.iter().position(|(q, _)| *q == p))
        }) {
            outcome.changed = self.select(index);
        }
        outcome
    }

    pub fn wheel(&mut self, x: f32, y: f32, delta_notches: f32) -> SettingsOutcome {
        let Some(frame) = self.frame() else {
            return SettingsOutcome::default();
        };
        if !self.open || !frame.panel.contains(x, y) {
            return SettingsOutcome::default();
        }
        let before = (self.scroll, frame.nav_scroll);
        if frame.page_area.contains(x, y) && delta_notches.is_finite() {
            self.scroll += delta_notches * WHEEL_STEP;
            self.clamp_scroll();
        }
        if frame.nav_area.contains(x, y) && delta_notches.is_finite() {
            self.nav_scroll =
                Some((frame.nav_scroll + delta_notches * WHEEL_STEP).clamp(0.0, frame.nav_max));
        }
        let nav_after = self.nav_scroll.unwrap_or(frame.nav_scroll);
        SettingsOutcome {
            consumed: true,
            changed: (self.scroll, nav_after) != before,
            ..Default::default()
        }
    }

    pub fn needs_frame(&self) -> bool {
        let Some(g) = self.geometry else {
            return false;
        };
        self.motion.needs_frame()
            || (self.motion.is_visible()
                && !self.height.is_settled(self.target_height(g), SETTLE_PX))
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .clamp(1.0e-4, 0.1);
        self.last_tick = Some(now);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        if self.motion.step(dt, mode).is_err() {
            self.motion.snap();
        }
        if let Some(g) = self.geometry {
            let target = self.target_height(g);
            self.height.step(target, dt, mode, SETTLE_PX);
            self.clamp_scroll();
        }
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// Input routed here: the whole output while open (a press outside
    /// closes), nothing while closed.
    pub fn input_rects(&self) -> Vec<Rect> {
        match self.geometry {
            Some(g) if self.open || self.motion.is_visible() => {
                vec![Rect::new(0.0, 0.0, g.viewport.0, g.viewport.1)]
            }
            _ => Vec::new(),
        }
    }

    /// FOCUS-01D: the body it has fully out, and the edge it comes out of.
    pub fn body(&self) -> Option<(GeometryRect, SurfaceEdge)> {
        let g = self.geometry?;
        if !self.open {
            return None;
        }
        let height = self.height.value.max(0.0);
        let (edge, top) = match g.stand {
            Some((anchor, _)) => (SurfaceEdge::Bottom, anchor - height),
            None => (SurfaceEdge::Top, g.top),
        };
        let rect = GeometryRect::try_new(g.x(), top, g.x() + g.width(), top + height, 0.0).ok()?;
        Some((rect, edge))
    }

    /// FOCUS-01D: how deep it may come out (`None`: fully). Returns whether
    /// that changed.
    pub fn set_give_way(&mut self, limit: Option<f32>) -> bool {
        let changed = self.give_way != limit;
        self.give_way = limit;
        changed
    }

    pub fn frame(&self) -> Option<SettingsFrame> {
        let g = self.geometry?;
        if !self.motion.is_visible() {
            return None;
        }
        let height = self.height.value.max(0.0);
        // FOCUS-01D: it stands back into its edge by what a surface standing
        // with it occupies of its depth (make room).
        let phase =
            gnos_ux_surface::grammar::within_depth(self.motion.phase(), height, self.give_way);
        if phase <= 0.0 {
            return None;
        }
        let x = g.x();
        let width = g.width();
        // Content slides down out of the top border, or up out of a bottom
        // Bar (T06).
        let (edge, anchor, clip_top, clip_bottom, panel_y, rest_y) = match g.stand {
            Some((anchor, _)) => (
                SurfaceEdge::Bottom,
                anchor,
                0.0,
                anchor,
                anchor - height * phase,
                anchor - height,
            ),
            None => (
                SurfaceEdge::Top,
                g.top,
                g.top,
                g.viewport.1,
                g.top - height * (1.0 - phase),
                g.top,
            ),
        };
        let panel = Rect::new(x, panel_y, width, height);
        let hull = GeometryRect::try_new(
            x,
            rest_y,
            x + width,
            rest_y + height,
            crate::reference_bar::BORDER_ROUNDING.min(height / 2.0),
        )
        .ok()
        .and_then(|child| revealed(child, edge, phase))
        .and_then(|visible| {
            let border = border_parent(
                edge,
                anchor,
                0.0,
                g.viewport.0,
                crate::reference_bar::BORDER_ROUNDING,
            )?;
            fused_hull(border, visible, edge)
        });
        let title = Rect::new(
            panel.x + PADDING,
            panel.y + PADDING,
            width - 3.0 * PADDING - CLOSE_SIZE,
            TITLE_HEIGHT,
        );
        let close = Rect::new(
            panel.x + width - PADDING - CLOSE_SIZE,
            panel.y + PADDING + (TITLE_HEIGHT - CLOSE_SIZE) / 2.0,
            CLOSE_SIZE,
            CLOSE_SIZE,
        );
        let body_top = title.y + TITLE_HEIGHT + PADDING;
        let body_bottom = panel.y + height - PADDING;
        let (nav_item, nav_category) = nav_rows(body_bottom - body_top);
        let nav_w = nav_width(width);
        let mut nav = Vec::new();
        let mut y = 0.0;
        let mut last = None;
        for (index, (page, category)) in PAGES.into_iter().enumerate() {
            if last != Some(category) {
                nav.push(NavPaint {
                    category: Some(category),
                    page: None,
                    rect: Rect::new(panel.x + PADDING, y, nav_w, nav_category),
                    current: false,
                });
                y += nav_category;
                last = Some(category);
            }
            nav.push(NavPaint {
                category: None,
                page: Some(page),
                rect: Rect::new(panel.x + PADDING, y, nav_w, nav_item),
                current: self.cursor.current() == Some(index),
            });
            y += nav_item;
        }
        // Rows were laid out from 0; scroll them into the body. Unless the
        // operator scrolled it, the column keeps the current page in view.
        let available = (body_bottom - body_top).max(0.0);
        let nav_max = (y - available).max(0.0);
        let nav_scroll = match self.nav_scroll {
            Some(scroll) => scroll,
            None => nav
                .iter()
                .find(|n| n.current)
                .map_or(0.0, |n| n.rect.y + n.rect.height - available),
        }
        .clamp(0.0, nav_max);
        for n in &mut nav {
            n.rect.y += body_top - nav_scroll;
        }
        // Rows that would cross the body's edges are not painted.
        nav.retain(|n| n.rect.y >= body_top - 0.5 && n.rect.y + n.rect.height <= body_bottom + 0.5);
        let nav_area = Rect::new(panel.x + PADDING, body_top, nav_w, available);
        let page_x = panel.x + 2.0 * PADDING + nav_w;
        let page_area = Rect::new(
            page_x,
            body_top,
            (panel.x + width - PADDING - page_x).max(0.0),
            self.page_viewport(g),
        );
        let mut y = page_area.y - self.scroll;
        let lines: Vec<LinePaint> = self
            .lines
            .iter()
            .filter_map(|line| {
                let h = line_height(line);
                let rect = Rect::new(page_area.x, y, page_area.width, h);
                y += h + LINE_GAP;
                (rect.y + h > page_area.y && rect.y < page_area.y + page_area.height).then(|| {
                    LinePaint {
                        line: line.clone(),
                        rect,
                        buttons: line_buttons(line, rect),
                    }
                })
            })
            .collect();
        let focused = self
            .in_page
            .then(|| self.control.current())
            .flatten()
            .and_then(|index| {
                lines
                    .iter()
                    .flat_map(|l: &LinePaint| l.buttons.iter().copied())
                    .nth(index)
            });
        let hovered = self
            .pointer
            .and_then(|p| Self::target_at(close, &nav, page_area, &lines, p));
        // Held: pressed on the control the pointer is still over.
        let pressed = hovered.is_some()
            && self
                .press
                .and_then(|p| Self::target_at(close, &nav, page_area, &lines, p))
                == hovered;
        Some(SettingsFrame {
            hovered,
            pressed,
            focused,
            panel,
            hull,
            clip_top,
            clip_bottom,
            title,
            close,
            nav,
            page_area,
            lines,
            page: self.page(),
            nav_area,
            nav_scroll,
            nav_max,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: (f32, f32) = (1280.0, 800.0);

    fn host() -> SettingsHost {
        let mut host = SettingsHost::new();
        host.set_geometry(VIEWPORT, 10.0, 60.0, 10.0, 790.0);
        host.set_lines(vec![
            SettingsLine::Header("Bar".into()),
            SettingsLine::Value {
                label: "Edge".into(),
                value: "Left".into(),
            },
        ]);
        host
    }

    fn settle(host: &mut SettingsHost) {
        host.tick(Instant::now(), true);
    }

    #[test]
    fn the_control_under_the_pointer_shows_hover_then_press() {
        let mut host = host();
        host.open();
        settle(&mut host);
        let frame = host.frame().expect("open");
        let close = frame.close;
        let (cx, cy) = (close.x + close.width / 2.0, close.y + close.height / 2.0);
        assert!(host.pointer_moved(cx, cy), "over the close control");
        let frame = host.frame().unwrap();
        assert_eq!(frame.hovered, Some(Target::Close(close)));
        assert!(!frame.pressed);
        assert!(!host.pointer_moved(cx + 1.0, cy), "still the same control");
        host.pointer_down(cx, cy);
        assert!(host.frame().unwrap().pressed, "held");
        // Dragged off it: no longer pressed, nothing hovered there.
        let away = (
            frame.page_area.x + 1.0,
            frame.page_area.y + frame.page_area.height - 1.0,
        );
        assert!(host.pointer_moved(away.0, away.1));
        let frame = host.frame().unwrap();
        assert_eq!((frame.hovered, frame.pressed), (None, false));
        // A page in the navigation.
        let nav = frame.nav.iter().find(|n| n.page.is_some()).unwrap().rect;
        host.pointer_moved(nav.x + 4.0, nav.y + 4.0);
        assert_eq!(host.frame().unwrap().hovered, Some(Target::Nav(nav)));
    }

    #[test]
    fn closed_paints_nothing_and_takes_no_input() {
        let host = host();
        assert!(host.frame().is_none());
        assert!(host.input_rects().is_empty());
    }

    #[test]
    fn under_a_bottom_bar_it_stands_on_the_bar_below_the_dock() {
        let mut host = SettingsHost::new();
        // Dock at the top (bottom edge 80), Bar's inner edge at 740.
        host.set_geometry_standing(VIEWPORT, 10.0, 10.0, 10.0, 80.0, 740.0);
        host.set_lines(vec![SettingsLine::Header("Bar".into())]);
        assert!(host.open().opened);
        settle(&mut host);
        let frame = host.frame().expect("open");
        assert_eq!(
            frame.panel.y + frame.panel.height,
            740.0,
            "stands on the Bar"
        );
        assert!(frame.panel.y >= 80.0, "stays below the Dock");
        assert_eq!((frame.clip_top, frame.clip_bottom), (0.0, 740.0));
        assert!(frame.hull.is_some(), "fused to the Bar");
        assert!(frame.title.y < frame.page_area.y, "title still on top");
    }

    #[test]
    fn under_a_top_bar_it_extrudes_from_the_bar() {
        let mut host = SettingsHost::new();
        host.set_geometry_on(VIEWPORT, 10.0, 10.0, 10.0, 790.0, 60.0);
        host.set_lines(vec![SettingsLine::Header("Bar".into())]);
        assert!(host.open().opened);
        settle(&mut host);
        let frame = host.frame().expect("open");
        assert_eq!(frame.panel.y, 60.0, "top on the Bar's inner edge");
        assert_eq!(frame.clip_top, 60.0);
        assert!(frame.hull.is_some(), "fused to the Bar");
        assert_eq!(frame.panel.x, 10.0 + (1280.0 - 20.0 - MAX_WIDTH) / 2.0);
    }

    #[test]
    fn opens_extruded_from_the_top_border_centred_right_of_the_bar() {
        let mut host = host();
        assert!(host.open().opened);
        settle(&mut host);
        let frame = host.frame().expect("open");
        assert_eq!(frame.panel.y, 10.0, "top on the inner edge");
        assert_eq!(frame.panel.width, MAX_WIDTH);
        assert_eq!(frame.panel.x, 60.0 + (1280.0 - 70.0 - MAX_WIDTH) / 2.0);
        assert!(frame.hull.is_some(), "fused to the top border");
        assert_eq!(frame.page, PageId::Appearance);
        assert_eq!(
            frame.nav.iter().filter(|n| n.page.is_some()).count(),
            PAGES.len()
        );
        assert_eq!(frame.nav.iter().filter(|n| n.category.is_some()).count(), 5);
        assert!(frame.panel.y + frame.panel.height <= 790.0 - 12.0);
    }

    #[test]
    fn the_panel_stays_above_the_floor_and_the_navigation_compacts_to_fit() {
        let mut host = SettingsHost::new();
        host.set_geometry((1353.0, 693.0), 10.0, 60.0, 10.0, 598.0);
        host.open();
        settle(&mut host);
        let frame = host.frame().unwrap();
        assert!(frame.panel.y + frame.panel.height <= 598.0 - 12.0 + 0.01);
        let last = frame.nav.last().unwrap().rect;
        assert!(last.y + last.height <= frame.panel.y + frame.panel.height - PADDING + 0.5);
        let first = frame.nav.first().unwrap().rect;
        assert!(first.y >= frame.nav_area.y - 0.5);
        // More pages than the column holds even compacted: it scrolls, and
        // the wheel brings the last page into view.
        let shown = frame.nav.iter().filter(|n| n.page.is_some()).count();
        if shown < PAGES.len() {
            assert!(frame.nav_max > 0.0);
            let a = frame.nav_area;
            host.wheel(a.x + 4.0, a.y + 4.0, 100.0);
            let frame = host.frame().unwrap();
            assert_eq!(frame.nav_scroll, frame.nav_max);
            assert_eq!(
                frame.nav.last().unwrap().page,
                Some(PAGES[PAGES.len() - 1].0)
            );
        }
    }

    #[test]
    fn the_keyboard_keeps_the_current_page_in_a_scrolled_navigation() {
        let mut host = SettingsHost::new();
        host.set_geometry((1353.0, 693.0), 10.0, 60.0, 10.0, 420.0);
        host.open();
        settle(&mut host);
        for _ in 0..PAGES.len() {
            host.key(SettingsKey::Down);
        }
        settle(&mut host);
        let frame = host.frame().unwrap();
        assert!(frame.nav_max > 0.0, "a short panel scrolls its navigation");
        assert!(
            frame.nav.iter().any(|n| n.current),
            "the current page is in view"
        );
    }

    #[test]
    fn full_motion_slides_out_and_settles_without_input() {
        let mut host = host();
        host.open();
        let t0 = Instant::now();
        let mut frames = 0;
        while host.tick(t0 + std::time::Duration::from_millis(16 * frames), false) {
            frames += 1;
            assert!(frames < 200);
        }
        assert!(frames > 5);
        assert_eq!(host.frame().unwrap().panel.y, 10.0);
    }

    #[test]
    fn keys_move_through_pages_and_escape_closes() {
        let mut host = host();
        host.open();
        assert!(
            !host.key(SettingsKey::Up).changed,
            "clamped at the first page"
        );
        assert!(host.key(SettingsKey::Down).changed);
        assert!(host.key(SettingsKey::Tab).changed);
        assert_eq!(host.page(), PageId::Display);
        host.key(SettingsKey::BackTab);
        assert_eq!(host.page(), PageId::WallpaperAndStyle);
        assert!(host.key(SettingsKey::Escape).closed);
        assert!(
            !host.key(SettingsKey::Down).consumed,
            "keys belong to an open panel"
        );
    }

    #[test]
    fn a_click_selects_a_page_the_close_control_closes_and_outside_closes() {
        let mut host = host();
        host.open();
        settle(&mut host);
        let frame = host.frame().unwrap();
        let bar = frame
            .nav
            .iter()
            .find(|n| n.page == Some(PageId::Bar))
            .unwrap()
            .rect;
        let (x, y) = (bar.x + 20.0, bar.y + 10.0);
        host.pointer_down(x, y);
        assert!(host.pointer_up(x, y).changed);
        assert_eq!(host.page(), PageId::Bar);
        let close = host.frame().unwrap().close;
        host.pointer_down(close.x + 5.0, close.y + 5.0);
        assert!(host.pointer_up(close.x + 5.0, close.y + 5.0).closed);
        let mut host2 = self::host();
        host2.open();
        settle(&mut host2);
        assert!(host2.pointer_down(5.0, 700.0).closed);
    }

    #[test]
    fn a_click_on_a_control_reports_its_action_only() {
        let mut host = host();
        host.set_lines(vec![
            SettingsLine::Choice {
                label: "Always visible".into(),
                value: "Default".into(),
                action: SettingsAction::CyclePersistent,
            },
            SettingsLine::Stepper {
                label: "Workspaces shown".into(),
                value: "5".into(),
                down: SettingsAction::WorkspaceCountDown,
                up: SettingsAction::WorkspaceCountUp,
            },
        ]);
        host.open();
        settle(&mut host);
        let frame = host.frame().unwrap();
        let click = |host: &mut SettingsHost, r: Rect| {
            host.pointer_down(r.x + 4.0, r.y + 4.0);
            host.pointer_up(r.x + 4.0, r.y + 4.0).action
        };
        let choice = frame.lines[0].buttons[0].0;
        assert_eq!(
            click(&mut host, choice),
            Some(SettingsAction::CyclePersistent)
        );
        let up = frame.lines[1].buttons[1].0;
        assert_eq!(click(&mut host, up), Some(SettingsAction::WorkspaceCountUp));
        let label = frame.lines[0].rect;
        assert_eq!(click(&mut host, label), None, "the label is not a control");
    }

    #[test]
    fn a_narrow_panel_keeps_labels_legible() {
        let mut host = SettingsHost::new();
        host.set_geometry((513.0, 933.0), 10.0, 60.0, 10.0, 850.0);
        host.set_lines(vec![SettingsLine::Choice {
            label: "Always visible".into(),
            value: "Default".into(),
            action: SettingsAction::CyclePersistent,
        }]);
        host.open();
        settle(&mut host);
        let frame = host.frame().unwrap();
        let line = &frame.lines[0];
        assert!(line.buttons[0].0.width <= line.rect.width * 0.4 + 0.01);
        assert!(frame.nav[1].rect.width <= frame.panel.width * 0.35 + 0.01);
        assert!(frame.page_area.width > 150.0, "{}", frame.page_area.width);
    }

    #[test]
    fn the_keyboard_reaches_and_activates_page_controls() {
        let mut host = host();
        host.set_lines(vec![
            SettingsLine::Choice {
                label: "Always visible".into(),
                value: "Default".into(),
                action: SettingsAction::CyclePersistent,
            },
            SettingsLine::Stepper {
                label: "Workspaces shown".into(),
                value: "5".into(),
                down: SettingsAction::WorkspaceCountDown,
                up: SettingsAction::WorkspaceCountUp,
            },
        ]);
        host.open();
        settle(&mut host);
        assert_eq!(
            host.key(SettingsKey::Activate).action,
            None,
            "Enter on the nav enters the page"
        );
        assert!(host.focus_in_page());
        let frame = host.frame().unwrap();
        assert_eq!(
            frame.focused.map(|f| f.1),
            Some(SettingsAction::CyclePersistent)
        );
        assert_eq!(
            host.key(SettingsKey::Activate).action,
            Some(SettingsAction::CyclePersistent)
        );
        host.key(SettingsKey::Down);
        host.key(SettingsKey::Down);
        assert_eq!(
            host.key(SettingsKey::Activate).action,
            Some(SettingsAction::WorkspaceCountUp)
        );
        // A refresh with the same controls keeps the focus.
        host.set_lines(vec![
            SettingsLine::Choice {
                label: "Always visible".into(),
                value: "On".into(),
                action: SettingsAction::CyclePersistent,
            },
            SettingsLine::Stepper {
                label: "Workspaces shown".into(),
                value: "6".into(),
                down: SettingsAction::WorkspaceCountDown,
                up: SettingsAction::WorkspaceCountUp,
            },
        ]);
        assert_eq!(
            host.key(SettingsKey::Activate).action,
            Some(SettingsAction::WorkspaceCountUp)
        );
        assert!(host.key(SettingsKey::Escape).changed);
        assert!(
            !host.focus_in_page(),
            "Escape in the page returns to the nav"
        );
        assert!(host.is_open());
        assert!(host.key(SettingsKey::Escape).closed);
    }

    #[test]
    fn a_long_page_scrolls_inside_its_area() {
        let mut host = host();
        host.set_lines(
            (0..60)
                .map(|i| SettingsLine::Caption(format!("line {i}")))
                .collect(),
        );
        host.open();
        settle(&mut host);
        let area = host.frame().unwrap().page_area;
        let out = host.wheel(area.x + 10.0, area.y + 10.0, 3.0);
        assert!(out.changed);
        let first = &host.frame().unwrap().lines[0];
        assert!(first.rect.y < area.y + 1.0);
        assert_ne!(first.line.label(), "line 0");
    }
}
