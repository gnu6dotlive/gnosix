//! The action sheet's items, layout and dwell.

use super::*;
use crate::deck::{DeckCard, wave::TAB};
use gnos_ux_product_intent::ObservedSurfaceReference;
use gnosix_application_catalog::DesktopEntryId;
use gnosix_desktop_model::{DesktopContextGeneration, SurfaceFocus, SurfaceId, SurfaceVisibility};

fn window(byte: u8, title: &str) -> ProductApplicationContent {
    ProductApplicationContent {
        bounds: None,
        reference: ObservedSurfaceReference::new(
            SurfaceId::from_bytes([byte; 16]),
            DesktopContextGeneration(1),
        ),
        workspace: None,
        title: title.to_owned(),
        class: "term".to_owned(),
        initial_class: "term".to_owned(),
        tags: Vec::new(),
        process_id: 1000 + u32::from(byte),
        xwayland: false,
        visibility: SurfaceVisibility::Visible,
        focus: SurfaceFocus::Unfocused,
    }
}

fn card(entry: bool, pinned: bool, windows: Vec<ProductApplicationContent>) -> DeckCard {
    DeckCard {
        key: "term.desktop".to_owned(),
        entry: entry.then(|| {
            (
                DesktopEntryId::parse("term.desktop".to_owned()).unwrap(),
                gnosix_application_catalog::ApplicationCatalog::empty()
                    .generation()
                    .clone(),
            )
        }),
        name: "Term".to_owned(),
        icon: None,
        pinned,
        windows,
        focused: false,
    }
}

fn actions() -> Vec<ProductDesktopAction> {
    vec![ProductDesktopAction {
        id: "new-window".to_owned(),
        name: "New Window".to_owned(),
        icon: None,
    }]
}

#[test]
fn a_running_pinned_card_lists_windows_then_actions_then_unpin() {
    let windows = vec![window(1, "~/src"), window(2, " ")];
    let focused = windows[0].reference;
    let items = sheet_items(&card(true, true, windows), &actions(), |w| {
        w.reference == focused
    });
    let receipts: Vec<String> = items.iter().map(SheetItem::receipt).collect();
    assert_eq!(
        receipts,
        ["window:0", "window:1", "action:new-window", "unpin"]
    );
    assert_eq!(items[1].text(), "Term", "an untitled window shows the name");
    assert!(matches!(items[0], SheetItem::Window { focused: true, .. }));
    // Two section changes: two dividers.
    assert_eq!(
        sheet_height(&items),
        TAB + 4.0 * ITEM + 2.0 * DIVIDER + SHEET_PAD
    );
}

#[test]
fn a_card_without_an_entry_has_only_its_windows_and_a_running_one_can_be_pinned() {
    let items = sheet_items(
        &card(false, false, vec![window(1, "x")]),
        &actions(),
        |_| false,
    );
    assert_eq!(items.len(), 1);
    let items = sheet_items(&card(true, false, vec![window(1, "x")]), &[], |_| false);
    assert_eq!(items.last(), Some(&SheetItem::Pin));
}

#[test]
fn rows_read_top_down_and_keep_the_pad_on_the_decks_side() {
    let items = sheet_items(&card(true, true, vec![window(1, "a")]), &actions(), |_| {
        false
    });
    let height = sheet_height(&items) - TAB;
    let body = Rect::new(100.0, 400.0, 240.0, height);
    let (rows, lines) = sheet_rows(&items, body, true);
    assert_eq!(rows.len(), 3);
    assert_eq!(lines.len(), 2);
    assert_eq!(rows[0].rect.y, 400.0);
    assert!(rows.windows(2).all(|w| w[1].rect.y > w[0].rect.y));
    let last = rows.last().unwrap().rect;
    assert!((body.y + body.height - (last.y + last.height) - SHEET_PAD).abs() < 1e-3);
    let (hanging, _) = sheet_rows(&items, body, false);
    assert_eq!(hanging[0].rect.y, 400.0 + SHEET_PAD);
    assert_eq!(rows[0].rect.x, 100.0 + ITEM_MARGIN);
    let clip = Rect::new(100.0, 400.0, 240.0, ITEM + 1.0);
    let inside = (rows[0].rect.x + 10.0, rows[0].rect.y + 10.0);
    assert_eq!(row_at(&rows, clip, inside.0, inside.1), Some(0));
    let below = (last.x + 10.0, last.y + 10.0);
    assert_eq!(row_at(&rows, clip, below.0, below.1), None, "not out yet");
}

#[test]
fn the_width_fits_the_text_within_bounds() {
    assert_eq!(sheet_width(10.0, 60.0), SHEET_MIN);
    assert_eq!(sheet_width(500.0, 60.0), SHEET_MAX);
    assert_eq!(
        sheet_width(180.0, 60.0),
        180.0 + 2.0 * (ITEM_INSET + ITEM_MARGIN)
    );
}

#[test]
fn a_dwell_opens_only_on_the_card_it_started_on() {
    let mut sheet = DeckSheet::default();
    let first = sheet.hover(Some("a")).expect("a dwell starts");
    assert_eq!(
        sheet.hover(Some("a")),
        None,
        "the same card keeps its dwell"
    );
    let second = sheet.hover(Some("b")).expect("another card restarts it");
    assert!(!sheet.dwell_elapsed(first), "a stale timer is ignored");
    assert!(sheet.dwell_elapsed(second));
    assert!(sheet.is_open());
    assert_eq!(sheet.card(), Some("b"));
    assert!(!sheet.dwell_elapsed(second), "once");
    sheet.hover(Some("c"));
    assert!(!sheet.is_open(), "another card closes it");
    let token = sheet.hover(None);
    assert!(token.is_none());
}

#[test]
fn a_press_or_escape_cancels_the_dwell_until_another_card() {
    let mut sheet = DeckSheet::default();
    let token = sheet.hover(Some("a")).unwrap();
    assert!(!sheet.close());
    assert!(!sheet.dwell_elapsed(token));
    assert_eq!(
        sheet.hover(Some("a")),
        None,
        "resting on it again does not reopen"
    );
    assert!(sheet.hover(Some("b")).is_some());
}

#[test]
fn a_secondary_click_toggles_and_rows_activate_on_release() {
    let mut sheet = DeckSheet::default();
    assert!(sheet.toggle("a"));
    assert!(sheet.set_hovered(Some(1)));
    assert!(!sheet.set_hovered(Some(1)));
    sheet.press(Some(1));
    assert!(sheet.pressing());
    assert_eq!(sheet.release(Some(2)), None, "released elsewhere");
    sheet.press(Some(1));
    assert_eq!(sheet.release(Some(1)), Some(1));
    assert!(!sheet.toggle("a"), "closes");
    assert!(!sheet.set_hovered(Some(0)), "no rows while closed");
    assert!(sheet.toggle("b"));
    assert_eq!(sheet.card(), Some("b"));
}
