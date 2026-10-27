//! The Deck's model, tested directly with an in-memory catalog.

use super::*;
use crate::product_runtime::ProductRuntime;
use gnos_ux_product_intent::ObservedSurfaceReference;
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
};
use gnosix_desktop_model::{DesktopContextGeneration, SurfaceFocus, SurfaceId, SurfaceVisibility};
use std::sync::Arc;

fn catalog(id: &str) -> (ApplicationCatalog, Vec<ProductInstalledApplicationContent>) {
    let root = std::env::temp_dir().join(format!("gnosix-deck-{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (file, name, extra) in [
        ("alpha-notes.desktop", "Alpha Notes", "Icon=notes\n"),
        (
            "beta-paint.desktop",
            "Beta Paint",
            "StartupWMClass=BetaPaintWindow\n",
        ),
        ("gamma-term.desktop", "Gamma Terminal", ""),
    ] {
        std::fs::write(
            root.join(file),
            format!("[Desktop Entry]\nType=Application\nName={name}\nExec=/bin/true\n{extra}"),
        )
        .unwrap();
    }
    let catalog = ApplicationCatalog::discover(
        &ApplicationCatalogRoots::from_application_dirs(vec![root]),
        &LocalePreferences::neutral(),
        DesktopEnvironment::from_names(["GNOSIX".to_owned()]),
    );
    let mut runtime = ProductRuntime::default();
    runtime.set_application_catalog(Arc::new(catalog.clone()));
    (catalog, runtime.installed_applications())
}

fn window(byte: u8, class: &str) -> ProductApplicationContent {
    ProductApplicationContent {
        bounds: None,
        reference: ObservedSurfaceReference::new(
            SurfaceId::from_bytes([byte; 16]),
            DesktopContextGeneration(1),
        ),
        workspace: None,
        title: format!("{class} window"),
        class: class.to_owned(),
        initial_class: class.to_owned(),
        tags: Vec::new(),
        process_id: 1000 + u32::from(byte),
        xwayland: false,
        visibility: SurfaceVisibility::Visible,
        focus: SurfaceFocus::Unfocused,
    }
}

#[test]
fn pins_come_first_running_windows_join_their_pin_and_the_rest_follow() {
    let (catalog, installed) = catalog("order");
    let resolve = |class: &str| crate::reference_bar::resolve_window_entry(&catalog, class);
    let pins = vec![
        "gamma-term.desktop".to_owned(),
        "missing.desktop".to_owned(),
        "alpha-notes.desktop".to_owned(),
    ];
    let windows = vec![
        window(1, "BetaPaintWindow"),
        window(2, "gamma-term"),
        window(3, "unknown-tool"),
        window(4, "BetaPaintWindow"),
    ];
    let cards = deck_cards(&pins, &installed, &windows, resolve, |w| {
        w.reference == windows[3].reference
    });
    let keys: Vec<&str> = cards.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "gamma-term.desktop",
            "alpha-notes.desktop",
            "beta-paint.desktop",
            "class:unknown-tool"
        ],
        "pins in pin order (missing skipped), then running in observed order"
    );
    assert!(
        cards[0].pinned && cards[0].running(),
        "the window joined its pin"
    );
    assert!(cards[1].pinned && !cards[1].running());
    assert_eq!(
        cards[2].windows.len(),
        2,
        "grouped per entry via StartupWMClass"
    );
    assert!(cards[2].focused && !cards[0].focused);
    assert_eq!(cards[3].entry, None);
    assert_eq!(cards[3].badge(), "U");
    assert_eq!(cards[1].icon.as_deref(), Some("notes"));
}

#[test]
fn ignored_tags_never_make_an_appcard() {
    let (_, installed) = catalog("tags");
    let mut popup = window(9, "popup");
    popup.tags = vec!["xwl_popup".to_owned()];
    assert!(deck_cards(&[], &installed, &[popup], |_| None, |_| false).is_empty());
}

#[test]
fn the_layout_separates_pins_from_running_and_ends_with_the_control() {
    let (_, installed) = catalog("layout");
    let pins = vec!["alpha-notes.desktop".to_owned()];
    let cards = deck_cards(&pins, &installed, &[window(1, "x")], |_| None, |_| false);
    let layout = deck_layout(&cards, 100.0, 700.0);
    assert_eq!(layout.cards.len(), 2);
    assert_eq!(layout.separators.len(), 2, "pins | running | control");
    assert_eq!(layout.cards[0].x, 100.0 + PAD);
    assert!(layout.separators[0].x > layout.cards[0].x + CARD);
    assert!(layout.cards[1].x > layout.separators[0].x);
    assert!(layout.control.x > layout.cards[1].x + CARD);
    assert_eq!(
        layout.control.x + CONTROL + PAD,
        layout.deck.x + layout.deck.width,
        "the Deck ends one pad after the control"
    );
    assert_eq!(layout.deck.height, HEIGHT);
    // DICTATION-01: the microphone stands just before the control.
    assert!(layout.dictation.x > layout.cards[1].x + CARD);
    assert_eq!(layout.dictation.x + DICTATION_SLOT + GAP, layout.control.x);
    // No cards: the microphone and the control, no separator.
    let alone = deck_layout(&[], 0.0, 0.0);
    assert!(alone.separators.is_empty());
    assert_eq!(alone.deck.width, 2.0 * PAD + DICTATION_SLOT + GAP + CONTROL);
}

#[test]
fn pins_round_trip_and_a_malformed_file_is_refused() {
    let dir = std::env::temp_dir().join(format!("gnosix-deck-pins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("gnosix/deck.json");
    assert_eq!(
        DeckPins::load(&path).unwrap(),
        DeckPins::default(),
        "missing = none"
    );
    let pins = DeckPins {
        pinned: vec!["a.desktop".into(), "b.desktop".into()],
    };
    pins.save(&path).unwrap();
    assert_eq!(DeckPins::load(&path).unwrap(), pins);
    std::fs::write(&path, "{\"schema\":\"other\",\"pinned\":[]}").unwrap();
    assert!(DeckPins::load(&path).is_err());
}

#[test]
fn running_cards_keep_their_first_observed_order_across_snapshots() {
    let (_, installed) = catalog("observed");
    let mut seen = Vec::new();
    let snapshot = |classes: &[&str]| {
        let windows: Vec<_> = classes
            .iter()
            .enumerate()
            .map(|(i, c)| window(i as u8 + 1, c))
            .collect();
        deck_cards(&[], &installed, &windows, |_| None, |_| false)
    };
    let keys = |cards: &[DeckCard]| cards.iter().map(|c| c.key.clone()).collect::<Vec<_>>();
    let mut first = snapshot(&["b"]);
    keep_observed_order(&mut first, &mut seen);
    // The compositor now lists the newer window first.
    let mut second = snapshot(&["c", "b"]);
    keep_observed_order(&mut second, &mut seen);
    assert_eq!(keys(&second), ["class:b", "class:c"]);
    // b closes, then reopens: it is newer than c now.
    let mut third = snapshot(&["c"]);
    keep_observed_order(&mut third, &mut seen);
    let mut fourth = snapshot(&["b", "c"]);
    keep_observed_order(&mut fourth, &mut seen);
    assert_eq!(keys(&fourth), ["class:c", "class:b"]);
}
