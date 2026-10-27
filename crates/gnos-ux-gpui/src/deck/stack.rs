//! DECK-01C: the AppStack in the Deck (D02, D03, D05, D07).
//!
//! The AppStack is no longer a drawer: the Deck itself widens and extrudes a
//! grid of appcards from its far edge, as one material. At phase 0 the
//! envelope is the Deck at rest; at phase 1 it is the Deck row plus the grid
//! above it (below it when the Deck hangs from the top). Its width and its
//! height follow the content on the envelope's second-order response, so a
//! new query reshapes it continuously.
//!
//! - **The control becomes the search field.** The Deck row keeps its cards;
//!   the AppStack control's slot stretches by exactly what the Deck widened,
//!   and the scene puts the search field in it (D07).
//! - **Tiles** (D05) are laid out in reading order, at most six columns and
//!   four rows; more rows scroll. The content rides the envelope's far edge
//!   and is clipped to the part that has extruded, so nothing fades.
//! - **Input** keeps the launcher's grammar (D03): the AppStack control, or a
//!   press on the border under the Deck dragged inward past 50 px, opens it;
//!   Escape, a drag back toward the border, a press outside or a successful
//!   launch closes it. The keyboard moves a 2D cursor; Enter launches.
//!   A press on a tile that travels is a drag of that application
//!   (DECK-01D): the AppStack retracts and the scene carries it to the Deck.
//!
//! - **DECK-02** (A01–A04): tabs (All, then categories), the frequent row
//!   and letter headings with an empty search, a grid that fits the
//!   results while searching, the search modes as chips, and tiles that
//!   show whether an application runs or is pinned (`stack_content`).
//!
//! The host never launches anything. It returns the current tile's catalog
//! identity, and the scene hands that to the typed launch capability, which
//! re-resolves it against the same catalog generation.

use super::HEIGHT;
use super::stack_content::{
    ContentLayout, Laid, StackTab, chip_rects, lay_out, marked, tab_rects, tabs,
};
use crate::product_application_search::{ApplicationSearchMode, ApplicationSearchQuery};
use blobin_ui_core::layout::Rect;
use gnos_ux_surface::grammar::{
    ExtrusionMotion, Follow, HullCommand, Point2, TemporalResponseMode,
};
use gnosix_application_catalog::{CatalogGeneration, DesktopEntryId};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// One tile (D05).
pub const TILE: f32 = 96.0;
pub const TILE_ICON: f32 = 48.0;
pub const TILE_GAP: f32 = 8.0;
pub const TILE_RADIUS: f32 = 20.0;
/// How far a hovered or current tile rises on each side.
pub const TILE_RISE: f32 = 3.0;
/// Inside the envelope, around the grid.
pub const GRID_PAD: f32 = 16.0;
/// Between the grid and the Deck row.
const ROW_GAP: f32 = 6.0;
const MAX_COLUMNS: usize = 6;
/// The grid's viewport holds at most four rows of tiles and two headings;
/// more scrolls.
const MAX_CONTENT: f32 = 4.0 * TILE + 3.0 * TILE_GAP + 2.0 * (super::stack_content::HEADING + 4.0);
/// Between the tabs, the grid and the chips.
const BAND_GAP: f32 = 8.0;
/// The search field is never narrower than this once open.
pub const SEARCH_MIN: f32 = 280.0;
/// The "No results" block.
pub const EMPTY_HEIGHT: f32 = 96.0;
/// One line of launch feedback at the grid's far edge.
pub const FEEDBACK_HEIGHT: f32 = 28.0;
const FEEDBACK_GAP: f32 = 8.0;
/// A drag from the border that travels this far inward opens the AppStack;
/// one from inside toward the border closes it.
const OPEN_DRAG: f32 = 50.0;
/// A press that travels further is a drag, never a click.
const PRESS_SLOP: f32 = 6.0;
/// The border strip that opens the AppStack reaches this far past the Deck.
const OPENER_REACH: f32 = 48.0;
const SETTLE_PX: f32 = 0.25;

/// One application, already resolved from the catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppRow {
    pub entry: DesktopEntryId,
    pub generation: CatalogGeneration,
    pub name: String,
    /// Comment, else generic name, else name.
    pub detail: String,
    /// Resolved icon file; None paints a letter badge.
    pub icon: Option<PathBuf>,
    pub favourite: bool,
    /// The entry's freedesktop categories (DECK-02, A01).
    pub categories: Vec<String>,
    /// A04: a window of it is open; it is pinned to the Deck.
    pub running: bool,
    pub pinned: bool,
}

/// Successful launches this session, per entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchFrequency(BTreeMap<DesktopEntryId, u32>);

impl LaunchFrequency {
    pub fn record(&mut self, entry: &DesktopEntryId) {
        *self.0.entry(entry.clone()).or_default() += 1;
    }

    pub fn get(&self, entry: &DesktopEntryId) -> u32 {
        self.0.get(entry).copied().unwrap_or(0)
    }
}

/// Order search results for display.
///
/// `rows` arrive in search order: favourites, then score, then name. With
/// no query every score ties, so frequency becomes the second key; the sort
/// is stable and keeps the name order inside equal frequencies. With a
/// query, search order stands.
pub fn order_rows(
    mut rows: Vec<AppRow>,
    frequency: &LaunchFrequency,
    query_empty: bool,
) -> Vec<AppRow> {
    if query_empty {
        rows.sort_by(|a, b| {
            b.favourite
                .cmp(&a.favourite)
                .then_with(|| frequency.get(&b.entry).cmp(&frequency.get(&a.entry)))
        });
    }
    rows
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackKey {
    Up,
    Down,
    Left,
    Right,
    Tab,
    BackTab,
    PageUp,
    PageDown,
    Enter,
    Escape,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StackOutcome {
    /// The event belonged to the AppStack; stop routing it.
    pub consumed: bool,
    /// Repaint.
    pub changed: bool,
    /// The scene focuses the search field.
    pub opened: bool,
    /// The scene clears the search field.
    pub closed: bool,
    /// Launch this tile through the typed capability.
    pub launch: Option<(DesktopEntryId, CatalogGeneration)>,
    /// A tile was pulled out of the grid (DECK-01D): the AppStack retracts
    /// and the scene carries this application to the Deck.
    pub drag: Option<AppRow>,
    /// A chip was pressed (DECK-02, A03): the scene rewrites the field's
    /// mode prefix.
    pub mode: Option<ApplicationSearchMode>,
}

/// Where the Deck stands, from the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackGeometry {
    pub viewport: (f32, f32),
    pub border: f32,
    /// The Deck at rest (its row), in scene coordinates.
    pub rest: Rect,
    /// The AppStack control's slot at rest.
    pub rest_control: Rect,
    /// The Deck stands on the bottom border; otherwise it hangs from the top.
    pub grows_up: bool,
    /// The horizontal range the envelope stays within.
    pub span: (f32, f32),
    /// How far the envelope may extrude: the y it must not pass.
    pub limit: f32,
}

impl StackGeometry {
    fn columns(&self) -> usize {
        let width = self.span.1 - self.span.0 - 2.0 * GRID_PAD;
        let fit = ((width + TILE_GAP) / (TILE + TILE_GAP)).floor();
        (fit.max(1.0) as usize).min(MAX_COLUMNS)
    }

    /// Room between the Deck row and the limit.
    fn room(&self) -> f32 {
        if self.grows_up {
            self.rest.y - self.limit
        } else {
            self.limit - (self.rest.y + self.rest.height)
        }
        .max(0.0)
    }

    /// The border strip, under (or over) the Deck, that opens the AppStack.
    fn opener(&self) -> Rect {
        let strip = self.border.max(2.0);
        Rect::new(
            self.rest.x - OPENER_REACH,
            if self.grows_up {
                self.viewport.1 - strip
            } else {
                0.0
            },
            self.rest.width + 2.0 * OPENER_REACH,
            strip,
        )
    }

    /// Travel away from the Deck's border (the opening direction).
    fn inward(&self, from: f32, to: f32) -> f32 {
        if self.grows_up { from - to } else { to - from }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TilePaint {
    pub index: usize,
    pub row: AppRow,
    pub rect: Rect,
    pub current: bool,
    pub hovered: bool,
    /// The name's characters the query matched (A02).
    pub marked: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StackFrame {
    pub phase: f32,
    /// The one material: the Deck row and the grid it extruded.
    pub envelope: Rect,
    pub radius: f32,
    /// The Deck row inside the envelope.
    pub row: Rect,
    /// How far the row's content moved from rest.
    pub row_shift: f32,
    /// The control's slot, stretched into the search field.
    pub control: Rect,
    /// The extruded part; tiles, feedback and the empty state are clipped
    /// to it.
    pub clip: Rect,
    /// The grid's viewport, riding the envelope's far edge.
    pub grid: Rect,
    pub columns: usize,
    pub feedback: Option<(Rect, String)>,
    pub tiles: Vec<TilePaint>,
    pub empty: bool,
    /// DECK-02: the letter headings painted, the tab strip (the current
    /// one marked) and the chips (the active one marked).
    pub headings: Vec<(String, Rect)>,
    pub tabs: Vec<(StackTab, Rect, bool)>,
    pub chips: Vec<(ApplicationSearchMode, &'static str, Rect, bool)>,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    x: f32,
    y: f32,
    tile: Option<usize>,
    moved: bool,
}

pub struct StackHost {
    geometry: Option<StackGeometry>,
    open: bool,
    rows: Vec<AppRow>,
    query: String,
    /// DECK-02: the tab shown and the tabs there are; the search's mode
    /// and text; the content laid out. `current` and `hovered` count tiles
    /// in the layout's reading order.
    tab: StackTab,
    tabs: Vec<StackTab>,
    mode: ApplicationSearchMode,
    text: String,
    layout: ContentLayout,
    current: Option<usize>,
    scroll: f32,
    hovered: Option<usize>,
    feedback: Option<String>,
    frequency: LaunchFrequency,
    /// The extrusion out of the Deck (envelope grammar).
    motion: ExtrusionMotion,
    /// The open envelope's extra height and width, following the content.
    extra: Follow,
    width: Follow,
    /// Border press waiting to become an opening drag: y.
    opener: Option<f32>,
    press: Option<Press>,
    last_tick: Option<Instant>,
}

impl Default for StackHost {
    fn default() -> Self {
        Self::new()
    }
}

impl StackHost {
    pub fn new() -> Self {
        Self {
            geometry: None,
            open: false,
            rows: Vec::new(),
            query: String::new(),
            tab: StackTab::All,
            tabs: vec![StackTab::All],
            mode: ApplicationSearchMode::Name,
            text: String::new(),
            layout: ContentLayout::default(),
            current: None,
            scroll: 0.0,
            hovered: None,
            feedback: None,
            frequency: LaunchFrequency::default(),
            motion: ExtrusionMotion::closed(),
            extra: Follow::at(0.0),
            width: Follow::at(0.0),
            opener: None,
            press: None,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Any part is out of the Deck (open, or still retracting).
    pub fn is_visible(&self) -> bool {
        self.motion.is_visible()
    }

    pub fn frequency(&self) -> &LaunchFrequency {
        &self.frequency
    }

    pub fn current(&self) -> Option<&AppRow> {
        self.current
            .and_then(|n| self.layout.tile(n))
            .and_then(|(row, _)| self.rows.get(row))
    }

    /// The tile `n` (reading order)'s application.
    fn tile_row(&self, n: usize) -> Option<&AppRow> {
        self.layout.tile(n).and_then(|(row, _)| self.rows.get(row))
    }

    pub fn columns(&self) -> usize {
        self.layout.columns.max(1)
    }

    pub fn tab(&self) -> StackTab {
        self.tab
    }

    pub fn tabs(&self) -> &[StackTab] {
        &self.tabs
    }

    fn searching(&self) -> bool {
        !self.text.is_empty()
    }

    /// Lay the content out again (rows, tab, search or width changed).
    fn relayout(&mut self) {
        let Some(g) = self.geometry else {
            return;
        };
        self.tabs = tabs(&self.rows);
        if !self.tabs.contains(&self.tab) {
            self.tab = StackTab::All;
        }
        self.layout = lay_out(
            &self.rows,
            &self.frequency,
            self.tab,
            self.searching(),
            g.columns(),
        );
        let n = self.layout.tiles.len();
        if self.current.is_some_and(|c| c >= n) {
            self.current = n.checked_sub(1);
        }
        if self.hovered.is_some_and(|h| h >= n) {
            self.hovered = None;
        }
    }

    fn grid_width(&self) -> f32 {
        let c = self.columns() as f32;
        c * TILE + (c - 1.0) * TILE_GAP
    }

    /// The envelope's width when open: the grid (as wide as the results
    /// while searching, A02), or a search field of at least `SEARCH_MIN`,
    /// whichever is wider, inside the span.
    fn open_width(&self, g: StackGeometry) -> f32 {
        let grid = self.grid_width() + 2.0 * GRID_PAD;
        let search = g.rest.width + (SEARCH_MIN - g.rest_control.width).max(0.0);
        grid.max(search).max(g.rest.width).min(g.span.1 - g.span.0)
    }

    fn inner_width(&self, g: StackGeometry) -> f32 {
        self.open_width(g) - 2.0 * GRID_PAD
    }

    /// Tabs show when there are two categories at least: with one, its
    /// tab would only repeat All.
    fn show_tabs(&self) -> bool {
        !self.searching() && self.tabs.len() > 2
    }

    /// The tab strip's band (its lines and a gap), or nothing.
    fn tabs_band(&self, g: StackGeometry) -> f32 {
        if self.show_tabs() {
            tab_rects(&self.tabs, 0.0, 0.0, self.inner_width(g)).1 + BAND_GAP
        } else {
            0.0
        }
    }

    /// The chips' band (their lines and a gap): always there (A03).
    fn chips_band(&self, g: StackGeometry) -> f32 {
        chip_rects(0.0, 0.0, self.inner_width(g)).1 + BAND_GAP
    }

    fn feedback_height(&self) -> f32 {
        if self.feedback.is_some() {
            FEEDBACK_HEIGHT + FEEDBACK_GAP
        } else {
            0.0
        }
    }

    /// The grid viewport's height: the content up to `MAX_CONTENT`, inside
    /// the room.
    fn content_height(&self, g: StackGeometry) -> f32 {
        let natural = if self.rows.is_empty() {
            EMPTY_HEIGHT
        } else {
            self.layout.height.min(MAX_CONTENT)
        };
        let cap = g.room()
            - GRID_PAD
            - ROW_GAP
            - self.feedback_height()
            - self.tabs_band(g)
            - self.chips_band(g);
        natural.min(cap).max(0.0)
    }

    fn target_extra(&self, g: StackGeometry) -> f32 {
        GRID_PAD
            + self.feedback_height()
            + self.tabs_band(g)
            + self.content_height(g)
            + self.chips_band(g)
            + ROW_GAP
    }

    fn rows_shown(&self, g: StackGeometry) -> usize {
        ((self.content_height(g) + TILE_GAP) / (TILE + TILE_GAP))
            .floor()
            .max(1.0) as usize
    }

    fn snap_size(&mut self, g: StackGeometry) {
        self.extra = Follow::at(self.target_extra(g));
        self.width = Follow::at(self.open_width(g));
    }

    /// Layout inputs. Returns whether anything visible changed.
    pub fn set_geometry(&mut self, geometry: StackGeometry) -> bool {
        if self.geometry == Some(geometry) {
            return false;
        }
        self.geometry = Some(geometry);
        self.relayout();
        if !self.open {
            self.snap_size(geometry);
        }
        self.clamp_scroll();
        true
    }

    /// New search results. The first tile becomes current when the query or
    /// the set of rows changed. Returns whether anything changed.
    pub fn set_rows(&mut self, rows: Vec<AppRow>, query: &str) -> bool {
        let same_rows = rows.len() == self.rows.len()
            && rows
                .iter()
                .zip(&self.rows)
                .all(|(a, b)| a.entry == b.entry && a.generation == b.generation);
        let query_changed = query != self.query;
        if same_rows && !query_changed && rows == self.rows {
            return false;
        }
        if query_changed {
            self.query = query.to_owned();
            let parsed = ApplicationSearchQuery::parse(query);
            self.mode = parsed.mode;
            self.text = parsed.text;
            self.feedback = None;
        }
        self.rows = rows;
        self.relayout();
        if query_changed || !same_rows {
            self.current = (!self.layout.tiles.is_empty()).then_some(0);
            self.scroll = 0.0;
            self.hovered = None;
        }
        if !self.open
            && let Some(g) = self.geometry
        {
            self.snap_size(g);
        }
        self.clamp_scroll();
        true
    }

    /// The scene's verdict on the last launch: None after success.
    pub fn set_feedback(&mut self, feedback: Option<String>) {
        self.feedback = feedback;
    }

    pub fn record_launch(&mut self, entry: &DesktopEntryId) {
        self.frequency.record(entry);
        self.relayout();
    }

    /// Show `tab` (DECK-02, A01): the first tile becomes current.
    pub fn select_tab(&mut self, tab: StackTab) -> bool {
        if tab == self.tab || !self.tabs.contains(&tab) {
            return false;
        }
        self.tab = tab;
        self.relayout();
        self.current = (!self.layout.tiles.is_empty()).then_some(0);
        self.scroll = 0.0;
        self.hovered = None;
        true
    }

    pub fn open(&mut self) -> StackOutcome {
        let mut outcome = StackOutcome {
            consumed: true,
            ..Default::default()
        };
        if !self.open {
            self.open = true;
            self.feedback = None;
            self.tab = StackTab::All;
            self.relayout();
            self.current = (!self.layout.tiles.is_empty()).then_some(0);
            self.scroll = 0.0;
            if let Some(g) = self.geometry
                && !self.motion.is_visible()
            {
                self.snap_size(g);
            }
            self.motion.set_open(true);
            outcome.opened = true;
            outcome.changed = true;
        }
        outcome
    }

    pub fn close(&mut self) -> StackOutcome {
        let mut outcome = StackOutcome {
            consumed: true,
            ..Default::default()
        };
        if self.open {
            self.open = false;
            self.motion.set_open(false);
            self.opener = None;
            self.press = None;
            self.hovered = None;
            self.feedback = None;
            outcome.closed = true;
            outcome.changed = true;
        }
        outcome
    }

    pub fn toggle(&mut self) -> StackOutcome {
        if self.open { self.close() } else { self.open() }
    }

    fn max_scroll(&self, g: StackGeometry) -> f32 {
        (self.layout.height - self.content_height(g)).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        if let Some(g) = self.geometry {
            self.scroll = self.scroll.clamp(0.0, self.max_scroll(g));
        }
    }

    fn keep_current_visible(&mut self) {
        let (Some(g), Some((_, rect))) = (
            self.geometry,
            self.current.and_then(|n| self.layout.tile(n)),
        ) else {
            return;
        };
        let top = rect.y;
        let viewport = self.content_height(g);
        if top < self.scroll {
            self.scroll = top;
        } else if top + TILE > self.scroll + viewport {
            self.scroll = top + TILE - viewport;
        }
        self.clamp_scroll();
    }

    fn launch_current(&self) -> Option<(DesktopEntryId, CatalogGeneration)> {
        self.current()
            .map(|row| (row.entry.clone(), row.generation.clone()))
    }

    pub fn key(&mut self, key: StackKey) -> StackOutcome {
        if !self.open {
            return StackOutcome::default();
        }
        let mut outcome = StackOutcome {
            consumed: true,
            ..Default::default()
        };
        let n = self.layout.tiles.len();
        let (Some(g), Some(i)) = (self.geometry, self.current.filter(|_| n > 0)) else {
            return match key {
                StackKey::Escape => self.close(),
                _ => outcome,
            };
        };
        let page = self.rows_shown(g);
        let last = n - 1;
        // Visual rows up or down; a page that runs out of rows lands on the
        // first or the last tile.
        let rows = |mut at: usize, steps: usize, direction: i32| {
            for _ in 0..steps {
                let next = self.layout.vertical(at, direction);
                if next == at {
                    return match (steps > 1, direction < 0) {
                        (false, _) => at,
                        (true, true) => 0,
                        (true, false) => last,
                    };
                }
                at = next;
            }
            at
        };
        let next = match key {
            StackKey::Left | StackKey::BackTab => i.saturating_sub(1),
            StackKey::Right | StackKey::Tab => (i + 1).min(last),
            // Up and Down move by visual rows, across headings, to the
            // tile nearest in x.
            StackKey::Up => rows(i, 1, -1),
            StackKey::Down => rows(i, 1, 1),
            StackKey::PageUp => rows(i, page, -1),
            StackKey::PageDown => rows(i, page, 1),
            StackKey::Enter => {
                outcome.launch = self.launch_current();
                return outcome;
            }
            StackKey::Escape => return self.close(),
        };
        if next != i {
            self.current = Some(next);
            self.keep_current_visible();
            outcome.changed = true;
        }
        outcome
    }

    /// CURSOR-01: what the pointer is over in the AppStack (`None`: off
    /// it): an application tile to press. (Its search field is an element
    /// of its own, which shows the text cursor itself.)
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        let frame = self.frame()?;
        if !frame.clip.contains(x, y) {
            return None;
        }
        Some(if self.tile_at(x, y).is_some() {
            crate::cursor::Affordance::Press
        } else {
            crate::cursor::Affordance::Inert
        })
    }

    fn tile_at(&self, x: f32, y: f32) -> Option<usize> {
        let frame = self.frame()?;
        if !frame.clip.contains(x, y) || !frame.grid.contains(x, y) {
            return None;
        }
        frame
            .tiles
            .iter()
            .find(|t| t.rect.contains(x, y))
            .map(|t| t.index)
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> StackOutcome {
        let mut outcome = StackOutcome::default();
        let Some(g) = self.geometry else {
            return outcome;
        };
        if let Some(start_y) = self.opener {
            outcome.consumed = true;
            if !self.open && g.inward(start_y, y) > OPEN_DRAG {
                return self.open();
            }
            return outcome;
        }
        if !self.open {
            return outcome;
        }
        if let Some(press) = self.press.as_mut() {
            outcome.consumed = true;
            if (x - press.x).hypot(y - press.y) > PRESS_SLOP {
                press.moved = true;
            }
            let press = *press;
            if press.moved
                && let Some(row) = press.tile.and_then(|i| self.tile_row(i)).cloned()
            {
                let mut closed = self.close();
                closed.drag = Some(row);
                return closed;
            }
            if -g.inward(press.y, y) > OPEN_DRAG {
                return self.close();
            }
        }
        let hovered = if self.press.is_some_and(|p| p.moved) {
            None
        } else {
            self.tile_at(x, y)
        };
        if hovered != self.hovered {
            self.hovered = hovered;
            outcome.changed = true;
        }
        outcome
    }

    /// Primary press only. A press on the Deck row is not the AppStack's:
    /// its cards and the search field take it.
    pub fn pointer_down(&mut self, x: f32, y: f32) -> StackOutcome {
        let mut outcome = StackOutcome::default();
        let Some(g) = self.geometry else {
            return outcome;
        };
        if !self.open {
            if g.opener().contains(x, y) {
                self.opener = Some(y);
                outcome.consumed = true;
            }
            return outcome;
        }
        let Some(frame) = self.frame() else {
            return self.close();
        };
        if frame.clip.contains(x, y) {
            // DECK-02: a tab shows its applications; a chip sets the
            // search's mode (the scene rewrites the field).
            if let Some((tab, _, _)) = frame.tabs.iter().find(|(_, r, _)| r.contains(x, y)) {
                outcome.consumed = true;
                outcome.changed = self.select_tab(*tab);
                return outcome;
            }
            if let Some((mode, _, _, _)) = frame.chips.iter().find(|(_, _, r, _)| r.contains(x, y))
            {
                outcome.consumed = true;
                outcome.mode = Some(*mode);
                return outcome;
            }
            self.press = Some(Press {
                x,
                y,
                tile: self.tile_at(x, y),
                moved: false,
            });
            outcome.consumed = true;
            return outcome;
        }
        if frame.row.contains(x, y) {
            return outcome;
        }
        // A press outside closes.
        self.close()
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> StackOutcome {
        let mut outcome = StackOutcome::default();
        if self.opener.take().is_some() {
            outcome.consumed = true;
            return outcome;
        }
        let Some(press) = self.press.take() else {
            return outcome;
        };
        outcome.consumed = true;
        if press.moved || !self.open {
            return outcome;
        }
        if let Some(index) = press.tile
            && self.tile_at(x, y) == Some(index)
        {
            self.current = Some(index);
            outcome.changed = true;
            outcome.launch = self.launch_current();
        }
        outcome
    }

    /// Wheel over the grid scrolls it by `delta_px` (positive scrolls
    /// toward the end); the current tile stays.
    pub fn wheel(&mut self, x: f32, y: f32, delta_px: f32) -> StackOutcome {
        let mut outcome = StackOutcome::default();
        if !self.open || !self.frame().is_some_and(|f| f.clip.contains(x, y)) {
            return outcome;
        }
        outcome.consumed = true;
        let before = self.scroll;
        if delta_px.is_finite() {
            self.scroll += delta_px;
        }
        self.clamp_scroll();
        outcome.changed = self.scroll != before;
        outcome
    }

    pub fn needs_frame(&self) -> bool {
        let Some(g) = self.geometry else {
            return false;
        };
        self.motion.needs_frame()
            || (self.open
                && (!self.extra.is_settled(self.target_extra(g), SETTLE_PX)
                    || !self.width.is_settled(self.open_width(g), SETTLE_PX)))
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
        if self.open
            && let Some(g) = self.geometry
        {
            self.extra.step(self.target_extra(g), dt, mode, SETTLE_PX);
            let width = self.open_width(g);
            self.width.step(width, dt, mode, SETTLE_PX);
        }
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// Input the scene must route here: the opening strip while closed; the
    /// whole output while open (a press outside closes).
    pub fn input_rects(&self) -> Vec<Rect> {
        let Some(g) = self.geometry else {
            return Vec::new();
        };
        if self.open || self.press.is_some() {
            return vec![Rect::new(0.0, 0.0, g.viewport.0, g.viewport.1)];
        }
        vec![g.opener()]
    }

    /// What to paint, or None while the AppStack is fully inside the Deck.
    pub fn frame(&self) -> Option<StackFrame> {
        let g = self.geometry?;
        if !self.motion.is_visible() {
            return None;
        }
        let phase = self.motion.phase().clamp(0.0, 1.0);
        let extra = self.extra.value.max(0.0) * phase;
        let width = g.rest.width + (self.width.value - g.rest.width) * phase;
        let centre = g.rest.x + g.rest.width / 2.0;
        let left = (centre - width / 2.0).clamp(g.span.0, (g.span.1 - width).max(g.span.0));
        let row = Rect::new(left, g.rest.y, width, g.rest.height);
        let (envelope, clip) = if g.grows_up {
            (
                Rect::new(left, g.rest.y - extra, width, g.rest.height + extra),
                Rect::new(left, g.rest.y - extra, width, extra),
            )
        } else {
            (
                Rect::new(left, g.rest.y, width, g.rest.height + extra),
                Rect::new(left, g.rest.y + g.rest.height, width, extra),
            )
        };
        let row_shift = left - g.rest.x;
        let control = Rect::new(
            g.rest_control.x + row_shift,
            g.rest_control.y,
            g.rest_control.width + (width - g.rest.width),
            g.rest_control.height,
        );
        // Content rides the far edge, laid out as when fully open: from the
        // far edge, the feedback, the tabs, the grid; the chips beside the
        // Deck row (mirrored when the Deck hangs from the top).
        let grid_width = self.grid_width();
        let gx = left + (width - grid_width) / 2.0;
        let inner = self.inner_width(g);
        let bx = left + GRID_PAD;
        let content = self.content_height(g);
        let feedback_height = self.feedback_height();
        let tabs_band = self.tabs_band(g);
        let (grid_y, tabs_y, chips_y, feedback) = if g.grows_up {
            let top = envelope.y + GRID_PAD;
            let grid_y = top + feedback_height + tabs_band;
            (
                grid_y,
                top + feedback_height,
                grid_y + content + BAND_GAP,
                Rect::new(gx, top, grid_width, FEEDBACK_HEIGHT),
            )
        } else {
            let chips_y = g.rest.y + g.rest.height + ROW_GAP;
            let grid_y = chips_y + self.chips_band(g);
            let bottom = envelope.y + envelope.height - GRID_PAD;
            (
                grid_y,
                grid_y + content + BAND_GAP,
                chips_y,
                Rect::new(gx, bottom - FEEDBACK_HEIGHT, grid_width, FEEDBACK_HEIGHT),
            )
        };
        let grid = Rect::new(gx, grid_y, grid_width, content);
        let columns = self.columns();
        let name_marks = self.searching() && self.mode == ApplicationSearchMode::Name;
        let mut tiles = Vec::new();
        let mut headings = Vec::new();
        let mut n = 0;
        for item in &self.layout.items {
            let (rect, tile) = match item {
                Laid::Tile { row, rect } => (*rect, Some(*row)),
                Laid::Heading { rect, .. } => (*rect, None),
            };
            let placed = Rect::new(
                grid.x + rect.x,
                grid.y + rect.y - self.scroll,
                rect.width,
                rect.height,
            );
            let visible = placed.y + placed.height > grid.y && placed.y < grid.y + grid.height;
            match (item, tile) {
                (_, Some(row)) => {
                    if visible && let Some(app) = self.rows.get(row) {
                        tiles.push(TilePaint {
                            index: n,
                            row: app.clone(),
                            rect: placed,
                            current: self.current == Some(n),
                            hovered: self.hovered == Some(n),
                            marked: if name_marks {
                                marked(&app.name, &self.text)
                            } else {
                                Vec::new()
                            },
                        });
                    }
                    n += 1;
                }
                (Laid::Heading { label, .. }, None) if visible => {
                    headings.push((label.clone(), placed));
                }
                _ => {}
            }
        }
        let tabs = if self.show_tabs() {
            tab_rects(&self.tabs, bx, tabs_y, inner)
                .0
                .into_iter()
                .map(|(tab, rect)| (tab, rect, tab == self.tab))
                .collect()
        } else {
            Vec::new()
        };
        let chips = chip_rects(bx, chips_y, inner)
            .0
            .into_iter()
            .map(|(mode, label, rect)| (mode, label, rect, mode == self.mode))
            .collect();
        Some(StackFrame {
            phase,
            envelope,
            radius: (HEIGHT / 2.0).min(envelope.height / 2.0),
            row,
            row_shift,
            control,
            clip,
            grid,
            columns,
            feedback: self.feedback.clone().map(|text| (feedback, text)),
            tiles,
            empty: self.rows.is_empty(),
            headings,
            tabs,
            chips,
        })
    }
}

/// The Deck row folded into the AppStack's envelope: its cards and
/// separators move with the row, its control slot is the stretched search
/// field, and its outline and bounds are the envelope's.
pub fn fold_deck(shape: &mut super::wave::DeckShape, frame: &StackFrame) {
    for card in &mut shape.cards {
        card.x += frame.row_shift;
    }
    for separator in &mut shape.separators {
        separator.x += frame.row_shift;
    }
    shape.control = frame.control;
    shape.base = frame.row;
    shape.tab = None;
    shape.bounds = frame.envelope;
    shape.outline = rounded_outline(frame.envelope, frame.radius);
}

/// A rounded rectangle as hull commands, clockwise.
pub fn rounded_outline(rect: Rect, radius: f32) -> Vec<HullCommand> {
    const KAPPA: f32 = 0.552_284_8;
    let r = radius.max(0.0).min(rect.width / 2.0).min(rect.height / 2.0);
    let k = KAPPA * r;
    let (l, t, rt, b) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    let p = Point2::new;
    vec![
        HullCommand::MoveTo(p(l + r, t)),
        HullCommand::LineTo(p(rt - r, t)),
        HullCommand::CubicTo {
            control1: p(rt - r + k, t),
            control2: p(rt, t + r - k),
            to: p(rt, t + r),
        },
        HullCommand::LineTo(p(rt, b - r)),
        HullCommand::CubicTo {
            control1: p(rt, b - r + k),
            control2: p(rt - r + k, b),
            to: p(rt - r, b),
        },
        HullCommand::LineTo(p(l + r, b)),
        HullCommand::CubicTo {
            control1: p(l + r - k, b),
            control2: p(l, b - r + k),
            to: p(l, b - r),
        },
        HullCommand::LineTo(p(l, t + r)),
        HullCommand::CubicTo {
            control1: p(l, t + r - k),
            control2: p(l + r - k, t),
            to: p(l + r, t),
        },
        HullCommand::Close,
    ]
}

/// The tile a hovered or current card rises to.
pub fn risen(rect: Rect) -> Rect {
    Rect::new(
        rect.x - TILE_RISE,
        rect.y - TILE_RISE,
        rect.width + 2.0 * TILE_RISE,
        rect.height + 2.0 * TILE_RISE,
    )
}

#[cfg(test)]
mod tests;
