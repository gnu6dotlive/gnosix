//! DECK-01A: the Deck at rest.
//!
//! The Deck is the designed Dock (DECK-01, D01): pinned appcards, a thin
//! separator, the running applications that are not pinned, then the AppStack
//! control. Its width follows its content, so the shared Dock envelope that
//! paints it is as wide as what it holds.
//!
//! Three rules decide what an appcard is.
//! - **Pins are catalog entries**, kept in pin order. A pin whose entry is no
//!   longer installed is not shown (it stays in the store, so reinstalling the
//!   application brings it back).
//! - **Running applications come from the compositor's windows.** A window's
//!   class resolves to a catalog entry with the rail's own lookup tiers
//!   (`reference_bar::resolve_window_entry`). Windows that resolve to a pinned
//!   entry join that pin; the others are grouped per entry (or per class when
//!   nothing resolves) in the order their first window was observed.
//! - **Identity is the key**, never the name: the entry id, or `class:<class>`
//!   for an unresolved window.

use crate::product_runtime::ProductInstalledApplicationContent;
use crate::product_scene::ProductApplicationContent;
use blobin_ui_core::layout::Rect;
use gnosix_application_catalog::{CatalogGeneration, DesktopEntryId};
use std::path::{Path, PathBuf};

pub mod drag;
pub mod sheet;
pub mod stack;
pub mod stack_content;
pub mod stack_view;
pub mod wave;

/// One appcard slot (the icon tile).
pub const CARD: f32 = 48.0;
/// The icon inside a slot.
pub const ICON: f32 = 36.0;
/// Between slots.
pub const GAP: f32 = 8.0;
/// Inside the Deck, around the row.
pub const PAD: f32 = 10.0;
/// The separator's painted width; it takes a gap on each side.
pub const SEPARATOR: f32 = 1.0;
/// The AppStack control's slot (DECK-01C turns it into the search field).
pub const CONTROL: f32 = 96.0;
/// DICTATION-01: the microphone's slot in the Deck's strip.
pub const DICTATION_SLOT: f32 = 32.0;
/// The Deck's height.
pub const HEIGHT: f32 = CARD + 2.0 * PAD;
/// Running indicator: at most this many dots per appcard.
pub const MAX_DOTS: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct DeckCard {
    /// Entry id, or `class:<class>` for a window no entry claims.
    pub key: String,
    /// The launch identity, for a card backed by a catalog entry.
    pub entry: Option<(DesktopEntryId, CatalogGeneration)>,
    pub name: String,
    /// Icon name, resolved against the icon theme by the scene.
    pub icon: Option<String>,
    pub pinned: bool,
    /// Open windows, in observed order.
    pub windows: Vec<ProductApplicationContent>,
    /// One of its windows has focus.
    pub focused: bool,
}

impl DeckCard {
    pub fn running(&self) -> bool {
        !self.windows.is_empty()
    }

    /// The letter a badge shows when no icon resolves.
    pub fn badge(&self) -> String {
        self.name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect())
    }
}

/// Window tags the Deck ignores, as the rail's workspace marks do.
fn ignored(window: &ProductApplicationContent) -> bool {
    window
        .tags
        .iter()
        .any(|tag| crate::reference_bar::WORKSPACE_IGNORED_TAGS.contains(&tag.as_str()))
}

/// The Deck's appcards: pins first (pin order), then running applications
/// that are not pinned (first-observed order).
pub fn deck_cards(
    pins: &[String],
    installed: &[ProductInstalledApplicationContent],
    windows: &[ProductApplicationContent],
    resolve: impl Fn(&str) -> Option<DesktopEntryId>,
    is_focused: impl Fn(&ProductApplicationContent) -> bool,
) -> Vec<DeckCard> {
    let from_entry = |entry: &ProductInstalledApplicationContent, pinned: bool| DeckCard {
        key: entry.reference.entry().as_str().to_owned(),
        entry: Some((
            entry.reference.entry().clone(),
            entry.reference.generation().clone(),
        )),
        name: entry.name.clone(),
        icon: entry.icon.clone(),
        pinned,
        windows: Vec::new(),
        focused: false,
    };
    let mut cards: Vec<DeckCard> = Vec::new();
    for pin in pins {
        if cards.iter().any(|c| &c.key == pin) {
            continue;
        }
        if let Some(entry) = installed
            .iter()
            .find(|e| e.reference.entry().as_str() == pin)
        {
            cards.push(from_entry(entry, true));
        }
    }
    for window in windows.iter().filter(|w| !ignored(w)) {
        let class = if window.class.is_empty() {
            &window.initial_class
        } else {
            &window.class
        };
        let entry = resolve(class);
        let key = entry
            .as_ref()
            .map_or_else(|| format!("class:{class}"), |e| e.as_str().to_owned());
        let index = match cards.iter().position(|c| c.key == key) {
            Some(index) => index,
            None => {
                let card = entry
                    .as_ref()
                    .and_then(|e| installed.iter().find(|i| i.reference.entry() == e))
                    .map(|i| from_entry(i, false))
                    .unwrap_or_else(|| DeckCard {
                        key: key.clone(),
                        entry: None,
                        name: if class.is_empty() {
                            window.title.clone()
                        } else {
                            class.clone()
                        },
                        icon: None,
                        pinned: false,
                        windows: Vec::new(),
                        focused: false,
                    });
                cards.push(card);
                cards.len() - 1
            }
        };
        let card = &mut cards[index];
        card.focused |= is_focused(window);
        card.windows.push(window.clone());
    }
    cards
}

/// Keeps the running cards that are not pinned in the order they were first
/// observed across snapshots: the compositor's list order is not observation
/// order. `seen` is the caller's memory; keys no longer running leave it.
pub fn keep_observed_order(cards: &mut [DeckCard], seen: &mut Vec<String>) {
    let pinned = cards.iter().take_while(|c| c.pinned).count();
    let running: Vec<String> = cards[pinned..].iter().map(|c| c.key.clone()).collect();
    seen.retain(|key| running.contains(key));
    for key in running {
        if !seen.contains(&key) {
            seen.push(key);
        }
    }
    cards[pinned..].sort_by_key(|card| seen.iter().position(|key| key == &card.key));
}

/// Where everything sits, in scene coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckLayout {
    pub deck: Rect,
    /// One rect per card, in card order.
    pub cards: Vec<Rect>,
    pub separators: Vec<Rect>,
    /// DICTATION-01: the microphone's slot in the Deck's strip.
    pub dictation: Rect,
    pub control: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Card(usize),
    /// An open slot where a dragged appcard would land (DECK-01D).
    Gap,
    Separator,
    /// DICTATION-01: the microphone's slot, in the Deck's own strip.
    Dictation,
    Control,
}

/// How a drag reshapes the row (DECK-01D): a card being carried away keeps
/// part of its slot, and gaps open where it would land. Presence is how
/// much of a slot (and the gap after it) stands, 0..=1; it is animated, so
/// the cards slide apart and together instead of jumping.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckSpacing {
    /// Per card, by index; a missing value is 1.
    pub presence: Vec<f32>,
    /// Open slots: (before this card index, presence).
    pub gaps: Vec<(usize, f32)>,
}

impl DeckSpacing {
    fn card(&self, index: usize) -> f32 {
        self.presence
            .get(index)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }
}

/// Slots in row order, each with its presence.
fn slots(cards: &[DeckCard], spacing: &DeckSpacing) -> Vec<(Slot, f32)> {
    let mut out = Vec::new();
    let pinned = cards.iter().filter(|c| c.pinned).count();
    let gaps_at = |i: usize, out: &mut Vec<(Slot, f32)>| {
        for (_, p) in spacing.gaps.iter().filter(|(at, _)| *at == i) {
            out.push((Slot::Gap, p.clamp(0.0, 1.0)));
        }
    };
    let has_pins = pinned > 0 || !spacing.gaps.is_empty();
    for (i, card) in cards.iter().enumerate() {
        if i == pinned {
            gaps_at(i, &mut out);
            if has_pins && !card.pinned {
                out.push((Slot::Separator, 1.0));
            }
        } else {
            gaps_at(i, &mut out);
        }
        out.push((Slot::Card(i), spacing.card(i)));
    }
    if pinned == cards.len() {
        gaps_at(cards.len(), &mut out);
    }
    if !cards.is_empty() || !spacing.gaps.is_empty() {
        out.push((Slot::Separator, 1.0));
    }
    // DICTATION-01: the microphone sits in the Deck's strip, before the
    // control. The Deck is the surface that already has the right material and
    // the right extrusion.
    out.push((Slot::Dictation, 1.0));
    out.push((Slot::Control, 1.0));
    out
}

fn slot_width(slot: Slot) -> f32 {
    match slot {
        Slot::Card(_) | Slot::Gap => CARD,
        Slot::Separator => SEPARATOR,
        Slot::Dictation => DICTATION_SLOT,
        Slot::Control => CONTROL,
    }
}

/// The Deck's width for these cards.
pub fn deck_width(cards: &[DeckCard]) -> f32 {
    deck_width_spaced(cards, &DeckSpacing::default())
}

/// The Deck's width while a drag reshapes it. Every slot but the last takes
/// a gap after it, scaled by its presence.
pub fn deck_width_spaced(cards: &[DeckCard], spacing: &DeckSpacing) -> f32 {
    let slots = slots(cards, spacing);
    let last = slots.len().saturating_sub(1);
    2.0 * PAD
        + slots
            .iter()
            .enumerate()
            .map(|(i, (s, p))| (slot_width(*s) + if i < last { GAP } else { 0.0 }) * p)
            .sum::<f32>()
}

/// Lays the cards out in a Deck whose origin is `(x, y)`.
pub fn deck_layout(cards: &[DeckCard], x: f32, y: f32) -> DeckLayout {
    deck_layout_spaced(cards, &DeckSpacing::default(), x, y)
}

pub fn deck_layout_spaced(cards: &[DeckCard], spacing: &DeckSpacing, x: f32, y: f32) -> DeckLayout {
    let mut layout = DeckLayout {
        deck: Rect::new(x, y, deck_width_spaced(cards, spacing), HEIGHT),
        cards: Vec::with_capacity(cards.len()),
        separators: Vec::new(),
        dictation: Rect::new(0.0, 0.0, 0.0, 0.0),
        control: Rect::new(0.0, 0.0, 0.0, 0.0),
    };
    let mut at = x + PAD;
    let top = y + PAD;
    for (slot, presence) in slots(cards, spacing) {
        let width = slot_width(slot);
        match slot {
            Slot::Card(_) => layout.cards.push(Rect::new(at, top, CARD, CARD)),
            Slot::Gap => {}
            Slot::Separator => {
                layout
                    .separators
                    .push(Rect::new(at, top + CARD * 0.2, SEPARATOR, CARD * 0.6))
            }
            Slot::Control => layout.control = Rect::new(at, top, CONTROL, CARD),
            Slot::Dictation => {
                layout.dictation = Rect::new(at, top, DICTATION_SLOT, DICTATION_SLOT)
            }
        }
        at += (width + GAP) * presence;
    }
    layout
}

/// The pins store: a user-owned state file (DECK-01, P04).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckPins {
    pub pinned: Vec<String>,
}

const SCHEMA: &str = "gnosix.deck.v1";

impl DeckPins {
    /// `$XDG_STATE_HOME/gnosix/deck.json`, else `~/.local/state/gnosix/deck.json`.
    pub fn default_path() -> Option<PathBuf> {
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })?;
        Some(state.join("gnosix").join("deck.json"))
    }

    /// A missing file is no pins; a malformed one is refused, never guessed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("read: {error}")),
        };
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("parse: {e}"))?;
        if value.get("schema").and_then(|s| s.as_str()) != Some(SCHEMA) {
            return Err("schema: expected gnosix.deck.v1".into());
        }
        let pinned = value
            .get("pinned")
            .and_then(|p| p.as_array())
            .ok_or("pinned: expected an array")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("pinned: expected strings")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { pinned })
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "schema": SCHEMA,
            "pinned": self.pinned,
        }))
        .map_err(|e| format!("encode: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text + "\n").map_err(|e| format!("write: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))
    }
}

#[cfg(test)]
mod tests;
