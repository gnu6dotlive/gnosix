//! DECK-01D: pinning, unpinning and reordering by dragging (D04, D11).
//!
//! A drag carries one application, by its catalog entry, from an appcard in
//! the Deck or from a tile in the AppStack's grid. The carried app follows
//! the pointer as a ghost (direct manipulation, no spring); the Deck answers
//! by opening a gap where it would land and closing the slot it left, both
//! on presence springs, so the cards slide apart and together.
//!
//! - **Over the Deck**, the drop target is the insertion point among the
//!   pins: how many of the other pinned cards stand left of the pointer. A
//!   card that is not pinned has to be carried into the pins (left of the
//!   separator) to land; dropped back among the running cards it stays as
//!   it was.
//! - **On the unpin target** (DECK-04), shown beyond the Deck while a pinned
//!   card is carried, the card is unpinned (it stays in the Deck if it
//!   runs).
//! - **Anywhere else off the Deck** (DECK-04), the application launches: a
//!   new instance, as from the AppStack. A card no catalog entry backs has
//!   nothing to launch and is cancelled.
//! - **Pulling a tile out of the grid retracts the AppStack**, so the Deck it
//!   is carried to is the Deck at rest.
//!
//! Pins stay entry ids; `pins_after_drop` maps the insertion point among
//! the pins the Deck shows onto the stored list, which may also hold pins
//! that are not installed now (they keep their place).

use super::wave::DeckShape;
use super::{CARD, DeckCard, DeckSpacing, GAP, PAD};
use blobin_ui_core::layout::Rect;
use gnos_ux_surface::grammar::{Follow, TemporalResponseMode};
use std::path::PathBuf;
use std::time::Instant;

/// A press that travels further starts a drag.
pub const DRAG_SLOP: f32 = 6.0;
/// The Deck accepts a drop this far around its shape.
pub const DROP_REACH: f32 = 24.0;
/// DECK-04: the unpin target's diameter, its distance beyond the Deck's
/// reach, and how far around it a drop still lands on it.
pub const UNPIN_SIZE: f32 = 56.0;
const UNPIN_GAP: f32 = 16.0;
const UNPIN_REACH: f32 = 16.0;
/// Presence springs settle to this, in pixels of a card.
const SETTLE_PX: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragSource {
    /// An appcard of the Deck.
    Card { pinned: bool },
    /// A tile of the AppStack's grid.
    Tile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// Pin at this place among the other pins the Deck shows.
    Insert(usize),
    /// Unpin.
    Remove,
    /// DECK-04: launch an instance.
    Launch,
    Cancel,
}

impl DropTarget {
    pub fn receipt(&self) -> String {
        match self {
            Self::Insert(at) => format!("insert:{at}"),
            Self::Remove => "remove".to_owned(),
            Self::Launch => "launch".to_owned(),
            Self::Cancel => "cancel".to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
struct Gap {
    /// The insertion point it opens at.
    at: usize,
    /// How much of a slot it holds, in pixels of a card.
    width: Follow,
    open: bool,
}

#[derive(Clone, Debug)]
pub struct DeckDrag {
    pub source: DragSource,
    /// The pin identity: the catalog entry id.
    pub key: String,
    pub name: String,
    /// The resolved icon file; None paints a letter badge.
    pub icon: Option<PathBuf>,
    pub pointer: (f32, f32),
    /// Where the pointer holds the ghost, from its top-left.
    pub grab: (f32, f32),
    pub size: f32,
    target: DropTarget,
    gaps: Vec<Gap>,
    /// How much of its slot a card being carried still holds.
    source_width: Follow,
    last_tick: Option<Instant>,
}

impl DeckDrag {
    pub fn new(
        source: DragSource,
        key: String,
        name: String,
        icon: Option<PathBuf>,
        pointer: (f32, f32),
        grab: (f32, f32),
        size: f32,
    ) -> Self {
        Self {
            source,
            key,
            name,
            icon,
            pointer,
            grab,
            size,
            target: DropTarget::Cancel,
            gaps: Vec::new(),
            source_width: Follow::at(CARD),
            last_tick: None,
        }
    }

    /// The ghost, in scene coordinates.
    pub fn ghost(&self) -> Rect {
        Rect::new(
            self.pointer.0 - self.grab.0,
            self.pointer.1 - self.grab.1,
            self.size,
            self.size,
        )
    }

    pub fn target(&self) -> DropTarget {
        self.target
    }

    /// Aim at a new target. Returns whether it changed. The gap of the
    /// previous insertion point closes, the new one opens.
    pub fn retarget(&mut self, target: DropTarget) -> bool {
        if target == self.target {
            return false;
        }
        self.target = target;
        for gap in &mut self.gaps {
            gap.open = false;
        }
        if let DropTarget::Insert(at) = target {
            match self.gaps.iter_mut().find(|g| g.at == at) {
                Some(gap) => gap.open = true,
                None => self.gaps.push(Gap {
                    at,
                    width: Follow::at(0.0),
                    open: true,
                }),
            }
        }
        true
    }

    /// The carried card's index in `cards`, for a drag out of the Deck.
    pub fn source_index(&self, cards: &[DeckCard]) -> Option<usize> {
        matches!(self.source, DragSource::Card { .. })
            .then(|| cards.iter().position(|c| c.key == self.key))
            .flatten()
    }

    /// The row's spacing for this frame.
    pub fn spacing(&self, cards: &[DeckCard]) -> DeckSpacing {
        let source = self.source_index(cards);
        let mut presence = vec![1.0; cards.len()];
        if let Some(i) = source {
            presence[i] = (self.source_width.value / CARD).clamp(0.0, 1.0);
        }
        let pinned = cards.iter().filter(|c| c.pinned).count();
        let others: Vec<usize> = (0..pinned).filter(|i| Some(*i) != source).collect();
        let gaps = self
            .gaps
            .iter()
            .filter(|g| g.width.value > 0.0)
            .map(|g| {
                let before = others.get(g.at).copied().unwrap_or(pinned);
                (before, (g.width.value / CARD).clamp(0.0, 1.0))
            })
            .collect();
        DeckSpacing { presence, gaps }
    }

    fn source_target(&self) -> f32 {
        // A card leaves its slot as soon as it is carried; it comes back
        // only by being dropped.
        0.0
    }

    pub fn needs_frame(&self) -> bool {
        let source_moving = matches!(self.source, DragSource::Card { .. })
            && !self
                .source_width
                .is_settled(self.source_target(), SETTLE_PX);
        source_moving
            || self.gaps.iter().any(|g| {
                !g.width
                    .is_settled(if g.open { CARD } else { 0.0 }, SETTLE_PX)
            })
    }

    /// Advances the presence springs; true while any still moves.
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
        let target = self.source_target();
        self.source_width.step(target, dt, mode, SETTLE_PX);
        for gap in &mut self.gaps {
            let target = if gap.open { CARD } else { 0.0 };
            gap.width.step(target, dt, mode, SETTLE_PX);
        }
        self.gaps
            .retain(|g| g.open || !g.width.is_settled(0.0, SETTLE_PX));
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }
}

/// DECK-04: the unpin target, centred beyond the Deck's far edge (above a
/// Deck on the bottom border, below one hanging from the top).
pub fn unpin_target(shape: &DeckShape) -> Rect {
    let b = shape.bounds;
    let x = shape.base.x + shape.base.width / 2.0 - UNPIN_SIZE / 2.0;
    let y = if shape.grows_up {
        b.y - DROP_REACH - UNPIN_GAP - UNPIN_SIZE
    } else {
        b.y + b.height + DROP_REACH + UNPIN_GAP
    };
    Rect::new(x, y, UNPIN_SIZE, UNPIN_SIZE)
}

/// Where a drop at `pointer` lands, against the Deck as painted.
pub fn drop_target(
    shape: &DeckShape,
    cards: &[DeckCard],
    drag: &DeckDrag,
    pointer: (f32, f32),
) -> DropTarget {
    let b = shape.bounds;
    let reach = Rect::new(
        b.x - DROP_REACH,
        b.y - DROP_REACH,
        b.width + 2.0 * DROP_REACH,
        b.height + 2.0 * DROP_REACH,
    );
    if !reach.contains(pointer.0, pointer.1) {
        let unpin = unpin_target(shape);
        let on_unpin = Rect::new(
            unpin.x - UNPIN_REACH,
            unpin.y - UNPIN_REACH,
            unpin.width + 2.0 * UNPIN_REACH,
            unpin.height + 2.0 * UNPIN_REACH,
        )
        .contains(pointer.0, pointer.1);
        return match drag.source {
            DragSource::Card { pinned: true } if on_unpin => DropTarget::Remove,
            _ if drag.key.starts_with("class:") => DropTarget::Cancel,
            _ => DropTarget::Launch,
        };
    }
    let source = drag.source_index(cards);
    let others: Vec<Rect> = cards
        .iter()
        .zip(&shape.cards)
        .enumerate()
        .filter(|(i, (card, _))| card.pinned && Some(*i) != source)
        .map(|(_, (_, rect))| *rect)
        .collect();
    if drag.source == (DragSource::Card { pinned: false }) {
        // It lands only once carried into the pins.
        let end = others
            .iter()
            .chain(&shape.gaps)
            .map(|r| r.x + r.width + GAP / 2.0)
            .fold(shape.base.x + PAD + CARD / 2.0, f32::max);
        if pointer.0 > end {
            return DropTarget::Cancel;
        }
    }
    DropTarget::Insert(
        others
            .iter()
            .filter(|r| r.x + r.width / 2.0 < pointer.0)
            .count(),
    )
}

/// The stored pins after `key` is dropped on `target`. `visible` are the
/// pins the Deck shows, in order; the insertion point counts those, without
/// `key`.
pub fn pins_after_drop(
    pins: &[String],
    visible: &[String],
    key: &str,
    target: DropTarget,
) -> Vec<String> {
    let mut out: Vec<String> = pins.iter().filter(|p| *p != key).cloned().collect();
    match target {
        DropTarget::Cancel | DropTarget::Launch => pins.to_vec(),
        DropTarget::Remove => out,
        DropTarget::Insert(at) => {
            let others: Vec<&String> = visible.iter().filter(|v| *v != key).collect();
            let index = match others.get(at) {
                Some(next) => out.iter().position(|p| p == *next).unwrap_or(out.len()),
                None => others
                    .last()
                    .and_then(|last| out.iter().position(|p| p == *last))
                    .map_or(out.len(), |i| i + 1),
            };
            out.insert(index, key.to_owned());
            out
        }
    }
}

#[cfg(test)]
mod tests;
