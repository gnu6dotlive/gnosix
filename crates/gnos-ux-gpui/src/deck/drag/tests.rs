//! Drag targets, gaps and the pins they leave.

use super::*;
use crate::deck::wave::{WaveInput, deck_shape_spaced};
use crate::deck::{DeckCard, deck_width, deck_width_spaced};
use std::time::Duration;

fn card(key: &str, pinned: bool) -> DeckCard {
    DeckCard {
        key: key.to_owned(),
        entry: None,
        name: key.to_owned(),
        icon: None,
        pinned,
        windows: Vec::new(),
        focused: false,
    }
}

/// Three pins and one running card.
fn cards() -> Vec<DeckCard> {
    vec![
        card("a", true),
        card("b", true),
        card("c", true),
        card("run", false),
    ]
}

fn drag(source: DragSource, key: &str) -> DeckDrag {
    DeckDrag::new(
        source,
        key.to_owned(),
        key.to_owned(),
        None,
        (0.0, 0.0),
        (24.0, 24.0),
        48.0,
    )
}

fn settle(drag: &mut DeckDrag) {
    let mut t = Instant::now();
    let mut n = 0;
    while drag.tick(t, false) {
        t += Duration::from_millis(16);
        n += 1;
        assert!(n < 400, "settles");
    }
}

fn shape(cards: &[DeckCard], spacing: &DeckSpacing) -> DeckShape {
    deck_shape_spaced(
        cards,
        spacing,
        100.0,
        700.0,
        true,
        (0.0, 4000.0),
        &WaveInput::default(),
    )
}

#[test]
fn a_carried_card_leaves_its_slot_and_a_gap_opens_where_it_would_land() {
    let cards = cards();
    let mut d = drag(DragSource::Card { pinned: true }, "a");
    assert!(d.retarget(DropTarget::Insert(2)));
    assert!(!d.retarget(DropTarget::Insert(2)));
    assert!(d.needs_frame());
    settle(&mut d);
    let spacing = d.spacing(&cards);
    assert_eq!(spacing.presence, [0.0, 1.0, 1.0, 1.0]);
    // After the other two pins: before the running card.
    assert_eq!(spacing.gaps, [(3, 1.0)]);
    // The row keeps its width: one slot left, one opened.
    assert!((deck_width_spaced(&cards, &spacing) - deck_width(&cards)).abs() < 1e-3);
    let s = shape(&cards, &spacing);
    assert_eq!(s.gaps.len(), 1);
    assert!(s.gaps[0].x > s.cards[2].x && s.gaps[0].x < s.cards[3].x);
}

#[test]
fn retargeting_closes_the_old_gap_while_the_new_one_opens() {
    let cards = cards();
    let mut d = drag(DragSource::Tile, "new");
    d.retarget(DropTarget::Insert(0));
    settle(&mut d);
    d.retarget(DropTarget::Insert(3));
    let t = Instant::now();
    d.tick(t, false);
    let spacing = d.spacing(&cards);
    assert_eq!(spacing.gaps.len(), 2, "both move at once");
    let (closing, opening) = (spacing.gaps[0], spacing.gaps[1]);
    assert_eq!((closing.0, opening.0), (0, 3));
    assert!(closing.1 < 1.0 && opening.1 > 0.0);
    settle(&mut d);
    assert_eq!(d.spacing(&cards).gaps, [(3, 1.0)]);
    d.retarget(DropTarget::Cancel);
    settle(&mut d);
    assert!(
        d.spacing(&cards).gaps.is_empty(),
        "a closed gap is forgotten"
    );
}

#[test]
fn reduced_motion_snaps_the_gaps() {
    let mut d = drag(DragSource::Card { pinned: true }, "b");
    d.retarget(DropTarget::Insert(0));
    assert!(!d.tick(Instant::now(), true));
    let spacing = d.spacing(&cards());
    assert_eq!(spacing.presence[1], 0.0);
    assert_eq!(spacing.gaps, [(0, 1.0)]);
}

#[test]
fn the_insertion_point_counts_the_other_pins_left_of_the_pointer() {
    let cards = cards();
    let d = drag(DragSource::Card { pinned: true }, "a");
    let s = shape(&cards, &DeckSpacing::default());
    let centre = |i: usize| s.cards[i].x + s.cards[i].width / 2.0;
    let y = s.base.y + 30.0;
    assert_eq!(
        drop_target(&s, &cards, &d, (s.base.x + 2.0, y)),
        DropTarget::Insert(0)
    );
    assert_eq!(
        drop_target(&s, &cards, &d, (centre(1) + 1.0, y)),
        DropTarget::Insert(1)
    );
    assert_eq!(
        drop_target(&s, &cards, &d, (centre(2) + 1.0, y)),
        DropTarget::Insert(2)
    );
    // Over the running card: after the last pin.
    assert_eq!(
        drop_target(&s, &cards, &d, (centre(3), y)),
        DropTarget::Insert(2)
    );
    // DECK-04: off the Deck, past its reach, the app launches; a pin is
    // removed only on the unpin target.
    let off = (centre(1), s.bounds.y - DROP_REACH - 5.0);
    assert_eq!(drop_target(&s, &cards, &d, off), DropTarget::Launch);
    let unpin = unpin_target(&s);
    let on = (unpin.x + unpin.width / 2.0, unpin.y + unpin.height / 2.0);
    assert!(
        unpin.y + unpin.height < s.bounds.y - DROP_REACH,
        "beyond the reach"
    );
    assert_eq!(drop_target(&s, &cards, &d, on), DropTarget::Remove);
    let tile = drag(DragSource::Tile, "new");
    assert_eq!(drop_target(&s, &cards, &tile, off), DropTarget::Launch);
    assert_eq!(
        drop_target(&s, &cards, &tile, on),
        DropTarget::Launch,
        "only a pin is unpinned"
    );
    let window = drag(DragSource::Card { pinned: false }, "class:foot");
    assert_eq!(
        drop_target(&s, &cards, &window, off),
        DropTarget::Cancel,
        "nothing to launch"
    );
    assert_eq!(
        drop_target(&s, &cards, &tile, (centre(3), y)),
        DropTarget::Insert(3)
    );
}

#[test]
fn a_running_card_lands_only_once_carried_into_the_pins() {
    let cards = cards();
    let d = drag(DragSource::Card { pinned: false }, "run");
    let s = shape(&cards, &DeckSpacing::default());
    let y = s.base.y + 30.0;
    let run = s.cards[3];
    assert_eq!(
        drop_target(&s, &cards, &d, (run.x + 24.0, y)),
        DropTarget::Cancel,
        "dropped back among the running cards"
    );
    let c = s.cards[2];
    assert_eq!(
        drop_target(&s, &cards, &d, (c.x + c.width / 2.0 - 1.0, y)),
        DropTarget::Insert(2)
    );
    assert_eq!(
        drop_target(&s, &cards, &d, (s.cards[0].x + 2.0, s.bounds.y - 100.0)),
        DropTarget::Launch,
        "a card that is not pinned is never removed"
    );
}

#[test]
fn pins_after_a_drop_keep_the_pins_the_deck_does_not_show() {
    let pins: Vec<String> = ["a", "gone", "b", "c"].map(String::from).to_vec();
    let visible: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
    let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // Reorder: "c" before "a".
    assert_eq!(
        pins_after_drop(&pins, &visible, "c", DropTarget::Insert(0)),
        v(&["c", "a", "gone", "b"])
    );
    // "a" after "b" (the others are b, c).
    assert_eq!(
        pins_after_drop(&pins, &visible, "a", DropTarget::Insert(1)),
        v(&["gone", "b", "a", "c"])
    );
    // A new pin at the end, after the last shown pin.
    assert_eq!(
        pins_after_drop(&pins, &visible, "new", DropTarget::Insert(3)),
        v(&["a", "gone", "b", "c", "new"])
    );
    assert_eq!(
        pins_after_drop(&pins, &visible, "b", DropTarget::Remove),
        v(&["a", "gone", "c"])
    );
    assert_eq!(
        pins_after_drop(&pins, &visible, "b", DropTarget::Cancel),
        pins
    );
    assert_eq!(
        pins_after_drop(&[], &[], "new", DropTarget::Insert(0)),
        v(&["new"])
    );
}
