//! The wave's geometry and motion, without a scene.

use super::*;
use crate::deck::{DeckCard, deck_layout, deck_width};
use std::time::Duration;

fn cards(pinned: usize, running: usize) -> Vec<DeckCard> {
    (0..pinned + running)
        .map(|i| DeckCard {
            key: format!("card-{i}"),
            entry: None,
            name: format!("Card {i}"),
            icon: None,
            pinned: i < pinned,
            windows: Vec::new(),
            focused: false,
        })
        .collect()
}

const SPAN: (f32, f32) = (0.0, 4000.0);

fn wave(pointer: f32, amount: f32) -> WaveInput {
    WaveInput {
        pointer,
        amount,
        tab: None,
        tuning: WaveTuning::default(),
    }
}

#[test]
fn the_bell_is_one_at_the_pointer_and_flat_from_its_reach() {
    let tuning = WaveTuning::default();
    let reach = tuning.reach();
    assert_eq!(reach, 2.5 * (CARD + GAP));
    assert_eq!(tuning.bell(0.0), 1.0);
    assert_eq!(tuning.bell(reach), 0.0);
    assert_eq!(tuning.bell(reach * 2.0), 0.0);
    assert!((tuning.bell(reach / 2.0) - 0.5).abs() < 1e-6);
    assert_eq!(tuning.bell(-10.0), tuning.bell(10.0));
}

#[test]
fn at_rest_the_shape_is_the_rest_layout() {
    let cards = cards(2, 2);
    let rest = deck_layout(&cards, 100.0, 700.0);
    let shape = deck_shape(&cards, 100.0, 700.0, true, SPAN, &wave(80.0, 0.0));
    assert_eq!(shape.base, rest.deck);
    assert_eq!(shape.cards, rest.cards);
    assert_eq!(shape.separators, rest.separators);
    assert_eq!(shape.control, rest.control);
    assert_eq!(shape.bounds, rest.deck);
    assert!(shape.scales.iter().all(|s| *s == 1.0));
    assert!(shape.tab.is_none());
}

#[test]
fn the_card_under_the_pointer_peaks_and_stays_under_it() {
    let cards = cards(2, 3);
    let rest = deck_layout(&cards, 100.0, 700.0);
    // The pointer on card 2's rest centre.
    let at = rest.cards[2].x + CARD / 2.0;
    let shape = deck_shape(&cards, 100.0, 700.0, true, SPAN, &wave(at - 100.0, 1.0));
    assert!((shape.scales[2] - 1.5).abs() < 1e-5);
    assert!(shape.scales[1] < shape.scales[2] && shape.scales[0] < shape.scales[1]);
    assert!(shape.scales[3] < shape.scales[2] && shape.scales[4] < shape.scales[3]);
    let card = shape.cards[2];
    assert!(
        (card.x + card.width / 2.0 - at).abs() < 1e-3,
        "{card:?} vs {at}"
    );
    assert!(shape.base.width > deck_width(&cards));
    // Cards stand on the base row; the hump keeps the rest padding above
    // the peak.
    for card in &shape.cards {
        assert!((card.y + card.height - (shape.base.y + HEIGHT - PAD)).abs() < 1e-3);
    }
    assert!((shape.bounds.y - (shape.base.y - 24.0)).abs() < 0.5);
    assert!((card.y - PAD - shape.bounds.y).abs() < 0.5);
    assert_eq!(shape.card_at(at, card.y + 2.0), Some(2));
    assert_eq!(shape.card_at(at, shape.bounds.y - 1.0), None);
}

#[test]
fn a_moving_pointer_slides_cards_under_it_continuously() {
    let cards = cards(3, 0);
    let mut previous: Option<f32> = None;
    let mut x = PAD;
    while x < PAD + 3.0 * (CARD + GAP) {
        let shape = deck_shape(&cards, 0.0, 0.0, true, SPAN, &wave(x, 1.0));
        let left = shape.cards[1].x;
        if let Some(p) = previous {
            assert!((left - p).abs() < 2.0, "jump at {x}: {p} -> {left}");
        }
        previous = Some(left);
        x += 1.0;
    }
}

#[test]
fn a_deck_hanging_from_the_top_mirrors_its_hump_and_cards() {
    let cards = cards(1, 1);
    let rest = deck_layout(&cards, 0.0, 16.0);
    let at = rest.cards[0].x + CARD / 2.0;
    let up = deck_shape(&cards, 0.0, 16.0, true, SPAN, &wave(at, 1.0));
    let down = deck_shape(&cards, 0.0, 16.0, false, SPAN, &wave(at, 1.0));
    assert_eq!(down.base, up.base);
    assert!((down.cards[0].y - (16.0 + PAD)).abs() < 1e-3);
    assert!((down.bounds.y - 16.0).abs() < 1e-3);
    assert!((down.bounds.height - up.bounds.height).abs() < 1e-3);
}

#[test]
fn the_label_tab_extrudes_out_of_the_hump_by_its_height() {
    let cards = cards(2, 0);
    let rest = deck_layout(&cards, 0.0, 700.0);
    let at = rest.cards[0].x + CARD / 2.0;
    let mut input = wave(at, 1.0);
    input.tab = Some(TabInput::label(at, 90.0, TAB));
    let shape = deck_shape(&cards, 0.0, 700.0, true, SPAN, &input);
    let tab = shape.tab.clone().expect("a tab");
    assert!((shape.bounds.y - (700.0 - 24.0 - TAB)).abs() < 0.5);
    assert!((tab.clip.height - TAB).abs() < 0.5);
    assert!((tab.text_box.x + tab.text_box.width / 2.0 - at).abs() < 1e-3);
    // Half out: the tab is shorter, the text box rides its top.
    input.tab.as_mut().unwrap().extrusion = TAB / 2.0;
    let half = deck_shape(&cards, 0.0, 700.0, true, SPAN, &input);
    let half_tab = half.tab.expect("a tab");
    assert!(half_tab.clip.height > 0.0 && half_tab.clip.height < TAB);
    assert_eq!(half_tab.text_box.y, half_tab.clip.y);
}

#[test]
fn the_deck_stays_inside_its_span() {
    let cards = cards(4, 0);
    let rest = deck_layout(&cards, 10.0, 0.0);
    let shape = deck_shape(
        &cards,
        10.0,
        0.0,
        true,
        (10.0, 10.0 + rest.deck.width + 20.0),
        &wave(PAD + CARD / 2.0, 1.0),
    );
    assert!(shape.base.x >= 10.0);
    assert!(shape.base.x + shape.base.width <= 10.0 + rest.deck.width + 20.0 + 1e-3);
}

#[test]
fn the_wave_rises_settles_and_falls_back() {
    let mut wave = DeckWave::default();
    let t0 = Instant::now();
    assert!(!wave.needs_frame());
    assert!(wave.pointer(Some(40.0)));
    wave.hover(Some((0, "Card 0".into())));
    wave.aim_tab(40.0, 70.0, false);
    let mut t = t0;
    let mut previous = 0.0;
    let mut ticks = 0;
    while wave.tick(t, false) {
        let amount = wave.input().amount;
        assert!(amount + 1e-4 >= previous, "the rise does not dip");
        previous = amount;
        t += Duration::from_millis(16);
        ticks += 1;
        assert!(ticks < 200, "settles");
    }
    assert!(ticks > 3, "it animates");
    let input = wave.input();
    assert_eq!(input.amount, 1.0);
    assert_eq!(input.tab.map(|t| t.extrusion), Some(TAB));
    assert!(wave.pointer(None));
    wave.hover(None);
    while wave.tick(t, false) {
        t += Duration::from_millis(16);
    }
    let input = wave.input();
    assert_eq!(input.amount, 0.0);
    assert!(input.tab.is_none());
    assert!(wave.label().is_none());
}

#[test]
fn reduced_motion_snaps() {
    let mut wave = DeckWave::default();
    wave.pointer(Some(40.0));
    wave.hover(Some((0, "Card 0".into())));
    wave.aim_tab(40.0, 70.0, false);
    assert!(!wave.tick(Instant::now(), true));
    assert_eq!(wave.input().amount, 1.0);
    wave.pointer(None);
    wave.hover(None);
    assert!(!wave.tick(Instant::now(), true));
    assert_eq!(wave.input().amount, 0.0);
}

#[test]
fn tuning_is_bounded_to_a_coherent_range() {
    let t = WaveTuning::bounded(9.0, 0.1, 50.0, 1.0);
    assert_eq!(t.magnify(), WaveTuning::MAGNIFY.1);
    assert_eq!(t.reach(), WaveTuning::REACH_SLOTS.0 * (CARD + GAP));
    assert_eq!(t.tempo(), WaveTuning::TEMPO.1);
    let nan = WaveTuning::bounded(f32::NAN, f32::INFINITY, f32::NAN, f32::NAN);
    assert_eq!(nan, WaveTuning::default());
    assert_eq!(
        WaveTuning::bounded(1.5, 2.5, 1.0, 24.0),
        WaveTuning::default()
    );
}

#[test]
fn a_magnification_of_one_turns_the_wave_off_but_keeps_the_label() {
    let cards = cards(2, 0);
    let rest = deck_layout(&cards, 0.0, 700.0);
    let at = rest.cards[0].x + CARD / 2.0;
    let mut input = wave(at, 1.0);
    input.tuning = WaveTuning::bounded(1.0, 2.5, 1.0, 24.0);
    input.tab = Some(TabInput::label(at, 90.0, TAB));
    let shape = deck_shape(&cards, 0.0, 700.0, true, SPAN, &input);
    assert_eq!(shape.base, rest.deck);
    assert_eq!(shape.cards, rest.cards);
    assert!((shape.bounds.y - (700.0 - TAB)).abs() < 0.5, "only the tab");
}

#[test]
fn a_larger_peak_and_reach_grow_the_deck_more() {
    let cards = cards(4, 0);
    let rest = deck_layout(&cards, 0.0, 0.0);
    let at = rest.cards[1].x + CARD / 2.0;
    let mut small = wave(at, 1.0);
    small.tuning = WaveTuning::bounded(1.25, 1.5, 1.0, 24.0);
    let mut large = wave(at, 1.0);
    large.tuning = WaveTuning::bounded(2.0, 4.0, 1.0, 24.0);
    let a = deck_shape(&cards, 0.0, 0.0, true, SPAN, &small);
    let b = deck_shape(&cards, 0.0, 0.0, true, SPAN, &large);
    assert!((a.scales[1] - 1.25).abs() < 1e-5 && (b.scales[1] - 2.0).abs() < 1e-5);
    assert!(b.base.width > a.base.width);
    assert!(b.bounds.height > a.bounds.height);
    assert!(
        b.scales[3] > a.scales[3],
        "the wider reach touches the far card"
    );
}

#[test]
fn a_faster_tempo_settles_sooner_without_overshoot() {
    let frames = |tempo: f32| {
        let mut wave = DeckWave::default();
        wave.set_tuning(WaveTuning::bounded(1.5, 2.5, tempo, 24.0));
        wave.pointer(Some(40.0));
        let mut t = Instant::now();
        let mut n = 0;
        while wave.tick(t, false) {
            assert!(wave.input().amount <= 1.0 + 1e-4, "no overshoot");
            t += Duration::from_millis(16);
            n += 1;
        }
        n
    };
    let (slow, normal, fast) = (frames(0.5), frames(1.0), frames(2.0));
    assert!(fast < normal && normal < slow, "{fast} {normal} {slow}");
}

#[test]
fn the_tab_grows_on_into_a_sheet_and_its_body_rides_the_crest() {
    let cards = cards(3, 0);
    let rest = deck_layout(&cards, 0.0, 700.0);
    let at = rest.cards[1].x + CARD / 2.0;
    let height = TAB + 100.0;
    let mut input = wave(at, 1.0);
    input.tab = Some(TabInput {
        center: at,
        width: 160.0,
        extrusion: height,
        height,
    });
    let shape = deck_shape(&cards, 0.0, 700.0, true, SPAN, &input);
    let tab = shape.tab.clone().expect("a sheet");
    // The sheet is the crest: it stands `height` out of the hump.
    assert!((shape.bounds.y - tab.clip.y).abs() < 0.5);
    assert!((tab.clip.height - height).abs() < 0.5);
    assert_eq!(tab.body.y, tab.text_box.y + TAB);
    assert_eq!(tab.body.height, 100.0);
    // Half out: the body keeps its layout and rides the crest.
    input.tab.as_mut().unwrap().extrusion = TAB + 50.0;
    let half = deck_shape(&cards, 0.0, 700.0, true, SPAN, &input)
        .tab
        .unwrap();
    assert_eq!(half.body.height, 100.0);
    assert!((half.body.y - (half.clip.y + TAB)).abs() < 1e-3);
    assert!((half.clip.height - (TAB + 50.0)).abs() < 0.5);
    // Hanging from the top, the crest is the bottom edge.
    input.tab.as_mut().unwrap().extrusion = height;
    let down = deck_shape(&cards, 0.0, 16.0, false, SPAN, &input)
        .tab
        .unwrap();
    assert_eq!(down.text_box.y + TAB, down.clip.y + down.clip.height);
    assert_eq!(down.body.y + down.body.height, down.text_box.y);
}

#[test]
fn the_sheet_rises_from_the_label_and_retracts_to_it() {
    let mut wave = DeckWave::default();
    let mut t = Instant::now();
    wave.pointer(Some(40.0));
    wave.hover(Some((0, "Card 0".into())));
    wave.aim_tab(40.0, 70.0, false);
    while wave.tick(t, false) {
        t += Duration::from_millis(16);
    }
    assert!(wave.set_sheet(Some(TAB + 80.0)));
    assert!(!wave.set_sheet(Some(TAB + 80.0)));
    assert_eq!(wave.sheet_shown(), Some(TAB + 80.0));
    while wave.tick(t, false) {
        assert!(wave.input().tab.unwrap().extrusion <= TAB + 80.0 + 1e-3);
        t += Duration::from_millis(16);
    }
    let tab = wave.input().tab.unwrap();
    assert_eq!((tab.extrusion, tab.height), (TAB + 80.0, TAB + 80.0));
    wave.set_sheet(None);
    let mut seen_body = false;
    while wave.tick(t, false) {
        seen_body |= wave.sheet_shown().is_some();
        t += Duration::from_millis(16);
    }
    assert!(seen_body, "the content stays while it retracts");
    assert_eq!(wave.sheet_shown(), None);
    assert_eq!(
        wave.input().tab.unwrap().extrusion,
        TAB,
        "back to the label"
    );
    // Another card takes the tab: its sheet is gone.
    wave.set_sheet(Some(TAB + 80.0));
    wave.hover(Some((1, "Card 1".into())));
    assert!(!wave.sheet_open());
}

#[test]
fn spacing_opens_a_gap_and_closes_a_carried_cards_slot() {
    use crate::deck::{DeckSpacing, deck_layout_spaced};
    let cards = cards(2, 1);
    let spacing = DeckSpacing {
        presence: vec![1.0, 0.0, 1.0],
        gaps: vec![(0, 0.5)],
    };
    let rest = deck_layout(&cards, 0.0, 0.0);
    let spaced = deck_layout_spaced(&cards, &spacing, 0.0, 0.0);
    assert!((spaced.deck.width - (rest.deck.width - (CARD + GAP) * 0.5)).abs() < 1e-3);
    assert!((spaced.cards[0].x - (rest.cards[0].x + (CARD + GAP) * 0.5)).abs() < 1e-3);
    let shape = super::deck_shape_spaced(&cards, &spacing, 0.0, 0.0, true, SPAN, &wave(0.0, 0.0));
    assert_eq!(shape.base.width, spaced.deck.width);
    assert_eq!(shape.gaps.len(), 1);
    assert_eq!(shape.cards[1].width, 0.0);
    assert_eq!(shape.cards[0].x, spaced.cards[0].x);
}

#[test]
fn a_sheet_stands_on_vertical_walls_with_round_corners_and_fillets() {
    let tuning = WaveTuning::default();
    let bell: Vec<(f32, f32)> = (0..200).map(|i| (i as f32 * 2.0, 0.0)).collect();
    let tab = TabInput {
        center: 200.0,
        width: 160.0,
        extrusion: TAB + 100.0,
        height: TAB + 100.0,
    };
    let top = tab.extrusion;
    let profile = tab_profile(&bell, tab, top, HEIGHT / 2.0, &tuning);
    assert!(
        profile.windows(2).all(|w| w[1].0 >= w[0].0 - 1e-3),
        "x never runs back"
    );
    for edge in [120.0, 280.0] {
        let wall: Vec<f32> = profile
            .iter()
            .filter(|(x, _)| (x - edge).abs() < 1e-3)
            .map(|p| p.1)
            .collect();
        let (lo, hi) = wall
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
        // The wall runs from the fillet's top to the corner's foot.
        assert!((lo - tuning.fillet()).abs() < 1e-3, "{edge}: {lo}");
        assert!((hi - (top - HEIGHT / 2.0)).abs() < 1e-3, "{edge}: {hi}");
    }
    let at = |x: f32| profile.iter().find(|p| p.0 >= x).unwrap().1;
    assert_eq!(at(200.0), top, "a flat top");
    assert_eq!(at(10.0), 0.0, "the hump away from it");
    // Both sides mirror each other.
    for &(x, h) in profile.iter().filter(|p| p.0 > 100.0 && p.0 < 200.0) {
        assert!(
            profile
                .iter()
                .any(|q| (q.0 - (400.0 - x)).abs() < 1e-2 && (q.1 - h).abs() < 1e-2),
            "({x}, {h}) has no mirror"
        );
    }
}
