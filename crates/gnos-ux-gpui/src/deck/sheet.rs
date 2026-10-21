//! DECK-01D: an appcard's action sheet (D04, D10).
//!
//! The hovered card's label tab grows into a sheet: the name stays in the
//! tab's crest and the card's open windows, its desktop actions and Pin or
//! Unpin are uncovered under it as the tab extrudes. It is the same tab of
//! the same material (`wave::deck_shape`); nothing pops over the Deck and
//! nothing fades.
//!
//! - **Opening.** The pointer resting on one card for the sheet's dwell
//!   (SET-03 `sheet_dwell_ms`, 450 ms by default) opens its
//!   sheet, so sweeping the Deck with the wave never flickers sheets open. A
//!   secondary click opens it at once (and closes it again).
//! - **Staying.** While the sheet is out, the pointer above the Deck's row
//!   (over the hump or the sheet) holds the wave still, so reaching for an
//!   item never hands the sheet to a neighbour. Along the row, another card
//!   takes over: the sheet retracts to its label and that card's dwell
//!   starts.
//! - **Closing.** Leaving the Deck, a press on a card, a drag, Escape or an
//!   item's activation closes it.
//!
//! The sheet only says what an item is; the scene carries it out through the
//! typed capabilities (focus, launch with a desktop action, the pins store).

use super::DeckCard;
use super::wave::TAB;
use crate::product_runtime::ProductDesktopAction;
use crate::product_scene::ProductApplicationContent;
use blobin_ui_core::layout::Rect;
use gnos_i18n::t;

/// How long the pointer rests on a card before its sheet opens.
/// One item row.
pub const ITEM: f32 = 30.0;

/// SET-03: a row's height now, `ITEM` by default.
fn item_height() -> f32 {
    crate::surface_prefs::advanced("sheet_row_px") as f32
}
/// Between sections (windows, actions, pinning).
pub const DIVIDER: f32 = 9.0;
/// Between the last row and the hump.
pub const SHEET_PAD: f32 = 8.0;
pub const SHEET_MIN: f32 = 200.0;
pub const SHEET_MAX: f32 = 320.0;
pub const ITEM_PX: f32 = 13.0;
/// The text inside a row, from the row's sides.
pub const ITEM_INSET: f32 = 12.0;
/// A row, from the sheet's sides.
pub const ITEM_MARGIN: f32 = 8.0;

#[derive(Clone, Debug, PartialEq)]
pub enum SheetItem {
    /// One of the card's windows, by its index in `DeckCard::windows`.
    Window {
        index: usize,
        title: String,
        focused: bool,
    },
    /// A desktop action of the card's entry.
    Action {
        id: String,
        name: String,
    },
    Pin,
    Unpin,
}

impl SheetItem {
    pub fn text(&self) -> &str {
        match self {
            Self::Window { title, .. } => title,
            Self::Action { name, .. } => name,
            Self::Pin => t("Pin to the Deck"),
            Self::Unpin => t("Unpin from the Deck"),
        }
    }

    /// The item as receipts name it.
    pub fn receipt(&self) -> String {
        match self {
            Self::Window { index, .. } => format!("window:{index}"),
            Self::Action { id, .. } => format!("action:{id}"),
            Self::Pin => "pin".to_owned(),
            Self::Unpin => "unpin".to_owned(),
        }
    }

    fn section(&self) -> u8 {
        match self {
            Self::Window { .. } => 0,
            Self::Action { .. } => 1,
            Self::Pin | Self::Unpin => 2,
        }
    }
}

/// What a card's sheet holds: its windows (a running card), the desktop
/// actions of its entry, then Pin or Unpin. A card no entry claims has only
/// its windows: it cannot be pinned and has no actions.
pub fn sheet_items(
    card: &DeckCard,
    actions: &[ProductDesktopAction],
    is_focused: impl Fn(&ProductApplicationContent) -> bool,
) -> Vec<SheetItem> {
    let mut items: Vec<SheetItem> = card
        .windows
        .iter()
        .enumerate()
        .map(|(index, window)| SheetItem::Window {
            index,
            title: if window.title.trim().is_empty() {
                card.name.clone()
            } else {
                window.title.clone()
            },
            focused: is_focused(window),
        })
        .collect();
    if card.entry.is_some() {
        items.extend(actions.iter().map(|action| SheetItem::Action {
            id: action.id.clone(),
            name: action.name.clone(),
        }));
        items.push(if card.pinned {
            SheetItem::Unpin
        } else {
            SheetItem::Pin
        });
    }
    items
}

fn dividers(items: &[SheetItem]) -> usize {
    items
        .windows(2)
        .filter(|pair| pair[0].section() != pair[1].section())
        .count()
}

/// The sheet's full height out of the hump: the label's crest, then the
/// rows.
pub fn sheet_height(items: &[SheetItem]) -> f32 {
    TAB + items.len() as f32 * item_height() + dividers(items) as f32 * DIVIDER + SHEET_PAD
}

/// The sheet's width for its widest text (measured by the scene) and its
/// label's width.
pub fn sheet_width(widest_text: f32, label_width: f32) -> f32 {
    (widest_text + 2.0 * (ITEM_INSET + ITEM_MARGIN))
        .max(label_width)
        .clamp(SHEET_MIN, SHEET_MAX)
}

/// One row, in scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetRow {
    pub item: usize,
    pub rect: Rect,
}

/// The rows inside the sheet's body (`DeckTab::body`), in screen order top
/// to bottom whichever way the Deck faces; the pad sits on the Deck's side.
/// Also returns the dividers' one-pixel lines.
pub fn sheet_rows(items: &[SheetItem], body: Rect, grows_up: bool) -> (Vec<SheetRow>, Vec<Rect>) {
    let mut y = if grows_up { body.y } else { body.y + SHEET_PAD };
    let width = (body.width - 2.0 * ITEM_MARGIN).max(0.0);
    let x = body.x + ITEM_MARGIN;
    let mut rows = Vec::with_capacity(items.len());
    let mut lines = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 && items[i - 1].section() != item.section() {
            lines.push(Rect::new(
                x + ITEM_INSET,
                y + DIVIDER / 2.0,
                width - 2.0 * ITEM_INSET,
                1.0,
            ));
            y += DIVIDER;
        }
        rows.push(SheetRow {
            item: i,
            rect: Rect::new(x, y, width, item_height()),
        });
        y += item_height();
    }
    (rows, lines)
}

/// The row under `(x, y)`, among the rows that have extruded (`clip`).
pub fn row_at(rows: &[SheetRow], clip: Rect, x: f32, y: f32) -> Option<usize> {
    if !clip.contains(x, y) {
        return None;
    }
    rows.iter().find(|r| r.rect.contains(x, y)).map(|r| r.item)
}

/// The dwell and the open sheet, by card key.
#[derive(Clone, Debug, Default)]
pub struct DeckSheet {
    /// The card under the pointer, whose dwell runs or whose sheet is out.
    card: Option<String>,
    /// The pending dwell's token; 0 when none runs.
    dwell: u64,
    tokens: u64,
    open: bool,
    hovered: Option<usize>,
    press: Option<usize>,
}

impl DeckSheet {
    /// The card under the pointer is now `key` (None: the pointer rests on
    /// no card). A new card closes any sheet and starts its dwell: the
    /// returned token is the one `dwell_elapsed` must be given after
    /// the dwell.
    pub fn hover(&mut self, key: Option<&str>) -> Option<u64> {
        if key == self.card.as_deref() {
            return None;
        }
        self.open = false;
        self.hovered = None;
        self.press = None;
        self.card = key.map(str::to_owned);
        self.tokens += 1;
        self.dwell = if key.is_some() { self.tokens } else { 0 };
        (self.dwell != 0).then_some(self.dwell)
    }

    /// The dwell timer fired. True when it opens the sheet: the pointer is
    /// still on the card it started on, and nothing cancelled it.
    pub fn dwell_elapsed(&mut self, token: u64) -> bool {
        if token == 0 || token != self.dwell || self.open {
            return false;
        }
        self.dwell = 0;
        self.open = true;
        true
    }

    /// A secondary click on a card: its sheet opens at once, or closes when
    /// it is already out. Returns whether it is open now.
    pub fn toggle(&mut self, key: &str) -> bool {
        if self.open && self.card.as_deref() == Some(key) {
            self.close();
            return false;
        }
        self.card = Some(key.to_owned());
        self.open = true;
        self.dwell = 0;
        self.hovered = None;
        self.press = None;
        true
    }

    /// Close and cancel the dwell; it starts again only on another card.
    /// Returns whether a sheet was open.
    pub fn close(&mut self) -> bool {
        let was = self.open;
        self.open = false;
        self.dwell = 0;
        self.hovered = None;
        self.press = None;
        was
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn card(&self) -> Option<&str> {
        self.card.as_deref()
    }

    pub fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    /// The row under the pointer. Returns whether it changed.
    pub fn set_hovered(&mut self, row: Option<usize>) -> bool {
        let row = row.filter(|_| self.open);
        let changed = row != self.hovered;
        self.hovered = row;
        changed
    }

    pub fn press(&mut self, row: Option<usize>) {
        self.press = row.filter(|_| self.open);
    }

    /// The release: the row to carry out, when it is the one pressed.
    pub fn release(&mut self, row: Option<usize>) -> Option<usize> {
        let pressed = self.press.take();
        (self.open && pressed.is_some() && pressed == row)
            .then_some(pressed)
            .flatten()
    }

    /// A press on a row is in progress.
    pub fn pressing(&self) -> bool {
        self.press.is_some()
    }
}

#[cfg(test)]
mod tests;
