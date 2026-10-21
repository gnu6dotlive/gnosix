//! The AppStack in the Deck, without a scene.

use super::*;
use std::time::Duration;

const VIEWPORT: (f32, f32) = (1280.0, 800.0);

fn generation() -> CatalogGeneration {
    gnosix_application_catalog::ApplicationCatalog::empty()
        .generation()
        .clone()
}

fn row(id: &str, name: &str) -> AppRow {
    AppRow {
        entry: DesktopEntryId::parse(format!("{id}.desktop")).expect("valid id"),
        generation: generation(),
        name: name.to_owned(),
        detail: name.to_owned(),
        icon: None,
        favourite: false,
        categories: Vec::new(),
        running: false,
        pinned: false,
    }
}

fn rows(n: usize) -> Vec<AppRow> {
    (0..n)
        .map(|i| row(&format!("app{i:02}"), &format!("App {i:02}")))
        .collect()
}

/// A 300 px Deck standing 16 px above the bottom border, right of a left
/// rail; or hanging 16 px under the top border.
fn geometry(grows_up: bool) -> StackGeometry {
    let y = if grows_up {
        800.0 - 16.0 - HEIGHT
    } else {
        16.0
    };
    let rest = Rect::new(490.0, y, 300.0, HEIGHT);
    StackGeometry {
        viewport: VIEWPORT,
        border: 10.0,
        rest,
        rest_control: Rect::new(rest.x + rest.width - 10.0 - 96.0, y + 10.0, 96.0, 48.0),
        grows_up,
        span: (76.0, 1254.0),
        limit: if grows_up { 26.0 } else { 774.0 },
    }
}

fn settle(host: &mut StackHost) {
    host.tick(Instant::now(), true);
}

fn open_host(n: usize, grows_up: bool) -> StackHost {
    let mut host = StackHost::new();
    host.set_geometry(geometry(grows_up));
    host.set_rows(rows(n), "");
    assert!(host.open().opened);
    settle(&mut host);
    host
}

#[test]
fn closed_it_paints_nothing_and_listens_on_the_border_under_the_deck() {
    let mut host = StackHost::new();
    host.set_geometry(geometry(true));
    host.set_rows(rows(3), "");
    assert!(host.frame().is_none());
    let opener = host.input_rects();
    assert_eq!(opener.len(), 1);
    assert!(
        opener[0].contains(640.0, 797.0),
        "the border under the Deck"
    );
    assert!(!opener[0].contains(640.0, 700.0));
}

#[test]
fn open_the_deck_widens_and_extrudes_the_grid_as_one_envelope() {
    let host = open_host(10, true);
    let g = geometry(true);
    let f = host.frame().expect("open");
    assert_eq!(f.phase, 1.0);
    assert_eq!(f.columns, 6);
    // Six 96 px tiles, five gaps, a 16 px pad on each side.
    assert_eq!(f.envelope.width, 6.0 * 96.0 + 5.0 * 8.0 + 32.0);
    // A letter heading and two rows of tiles above the Deck row, which
    // keeps its place (DECK-02: the A–Z section).
    assert_eq!(f.row.y, g.rest.y);
    assert_eq!(f.envelope.y + f.envelope.height, g.rest.y + HEIGHT);
    assert_eq!(
        f.grid.height,
        crate::deck::stack_content::HEADING + 4.0 + 2.0 * 96.0 + 8.0
    );
    assert_eq!(f.headings.len(), 1);
    assert_eq!(f.clip.y + f.clip.height, g.rest.y);
    assert!(f.grid.y >= f.clip.y && f.grid.y + f.grid.height <= f.clip.y + f.clip.height);
    // Centred on the Deck; the control stretched by exactly the widening.
    let centre = g.rest.x + g.rest.width / 2.0;
    assert!((f.envelope.x + f.envelope.width / 2.0 - centre).abs() < 1e-3);
    assert_eq!(f.control.width - 96.0, f.envelope.width - g.rest.width);
    assert_eq!(
        f.control.x + f.control.width,
        f.envelope.x + f.envelope.width - 10.0
    );
    assert_eq!(f.tiles.len(), 10);
    assert!(f.tiles[0].current);
    assert_eq!(
        f.tiles[6].rect.y,
        f.tiles[0].rect.y + 104.0,
        "reading order"
    );
}

#[test]
fn hanging_from_the_top_it_extrudes_downward() {
    let host = open_host(4, false);
    let g = geometry(false);
    let f = host.frame().expect("open");
    assert_eq!(f.envelope.y, g.rest.y);
    assert_eq!(f.clip.y, g.rest.y + HEIGHT);
    assert!(f.grid.y + f.grid.height <= f.envelope.y + f.envelope.height - GRID_PAD + 1e-3);
}

#[test]
fn a_wide_deck_keeps_the_search_field_wide_enough() {
    // A Deck full of applications: the search field, not the grid, sets
    // the open width.
    let mut host = StackHost::new();
    let mut g = geometry(true);
    g.rest = Rect::new(390.0, g.rest.y, 500.0, HEIGHT);
    g.rest_control = Rect::new(g.rest.x + 500.0 - 10.0 - 96.0, g.rest.y + 10.0, 96.0, 48.0);
    host.set_geometry(g);
    host.set_rows(rows(1), "x");
    host.open();
    settle(&mut host);
    let f = host.frame().unwrap();
    // DECK-02 (A02): the grid fits its one result.
    assert_eq!(f.columns, 1);
    assert!(f.envelope.width > 6.0 * 96.0 + 5.0 * 8.0 + 32.0);
    assert!((f.control.width - SEARCH_MIN).abs() < 1e-3);
    // A narrow span still holds the envelope; the field gets what fits.
    g.span = (420.0, 860.0);
    host.set_geometry(g);
    settle(&mut host);
    let f = host.frame().unwrap();
    assert!(f.envelope.x >= 420.0 - 1e-3 && f.envelope.x + f.envelope.width <= 860.0 + 1e-3);
}

#[test]
fn four_rows_show_and_the_rest_scroll_to_the_current_tile() {
    let mut host = open_host(40, true);
    let f = host.frame().unwrap();
    assert_eq!(f.grid.height, MAX_CONTENT);
    for _ in 0..6 {
        host.key(StackKey::Down);
    }
    let f = host.frame().unwrap();
    let current = f.tiles.iter().find(|t| t.current).expect("current painted");
    assert_eq!(current.index, 36);
    assert!(
        current.rect.y + TILE <= f.grid.y + f.grid.height + 1e-3,
        "scrolled into view"
    );
}

#[test]
fn a_room_too_short_caps_the_grid() {
    let mut host = StackHost::new();
    let mut g = geometry(true);
    g.limit = g.rest.y - 200.0;
    host.set_geometry(g);
    host.set_rows(rows(30), "");
    host.open();
    settle(&mut host);
    let f = host.frame().unwrap();
    assert!(f.envelope.y >= g.limit - 1e-3);
}

#[test]
fn arrows_move_in_two_dimensions_and_clamp() {
    let mut host = open_host(10, true);
    let at = |h: &StackHost| h.current().unwrap().name.clone();
    host.key(StackKey::Right);
    assert_eq!(at(&host), "App 01");
    host.key(StackKey::Down);
    assert_eq!(at(&host), "App 07");
    host.key(StackKey::Down);
    assert_eq!(at(&host), "App 07", "no row below");
    host.key(StackKey::Up);
    host.key(StackKey::Up);
    assert_eq!(at(&host), "App 01", "Up clamps on the first row");
    host.key(StackKey::Left);
    host.key(StackKey::Left);
    assert_eq!(at(&host), "App 00");
    host.key(StackKey::Tab);
    host.key(StackKey::BackTab);
    assert_eq!(at(&host), "App 00");
    host.key(StackKey::PageDown);
    assert_eq!(
        at(&host),
        "App 09",
        "a page past the end lands on the last tile"
    );
    // From the row above the last, Down reaches the last tile.
    let mut host = open_host(10, true);
    for _ in 0..5 {
        host.key(StackKey::Right);
    }
    host.key(StackKey::Down);
    assert_eq!(at(&host), "App 09");
}

#[test]
fn enter_launches_the_current_tile_and_escape_closes() {
    let mut host = open_host(3, true);
    host.key(StackKey::Right);
    let outcome = host.key(StackKey::Enter);
    assert_eq!(
        outcome.launch.map(|(e, _)| e.as_str().to_owned()),
        Some("app01.desktop".into())
    );
    let outcome = host.key(StackKey::Escape);
    assert!(outcome.closed && !host.is_open());
    // No results: Enter launches nothing.
    let mut host = open_host(3, true);
    host.set_rows(Vec::new(), "zzz");
    assert!(host.key(StackKey::Enter).launch.is_none());
    assert!(host.frame().unwrap().empty);
}

#[test]
fn a_new_query_resets_the_current_tile_but_an_identical_refresh_does_not() {
    let mut host = open_host(5, true);
    host.key(StackKey::Right);
    assert!(!host.set_rows(rows(5), ""));
    assert_eq!(host.current().unwrap().name, "App 01");
    assert!(host.set_rows(rows(5), "app"));
    assert_eq!(host.current().unwrap().name, "App 00");
}

#[test]
fn a_drag_inward_from_the_border_opens_past_the_threshold() {
    for grows_up in [true, false] {
        let mut host = StackHost::new();
        host.set_geometry(geometry(grows_up));
        host.set_rows(rows(3), "");
        let (start, short, far) = if grows_up {
            (797.0, 760.0, 740.0)
        } else {
            (3.0, 40.0, 60.0)
        };
        assert!(host.pointer_down(640.0, start).consumed);
        assert!(!host.pointer_moved(640.0, short).opened);
        assert!(host.pointer_moved(640.0, far).opened);
        assert!(host.pointer_up(640.0, far).consumed);
        assert!(host.is_open());
    }
    // Away from the Deck the border is not the AppStack's.
    let mut host = StackHost::new();
    host.set_geometry(geometry(true));
    assert!(!host.pointer_down(100.0, 797.0).consumed);
}

#[test]
fn a_click_on_a_tile_launches_it_but_a_drag_does_not() {
    let mut host = open_host(3, true);
    let tile = host.frame().unwrap().tiles[2].rect;
    let (x, y) = (tile.x + 40.0, tile.y + 40.0);
    assert!(host.pointer_down(x, y).consumed);
    let outcome = host.pointer_up(x, y);
    assert_eq!(
        outcome.launch.map(|(e, _)| e.as_str().to_owned()),
        Some("app02.desktop".into())
    );
    host.pointer_down(x, y);
    host.pointer_moved(x + 3.0, y);
    assert!(host.is_open(), "within the slop");
    let pulled = host.pointer_moved(x + 20.0, y);
    assert!(pulled.closed, "pulling a tile out retracts the AppStack");
    assert_eq!(
        pulled.drag.map(|row| row.entry.as_str().to_owned()),
        Some("app02.desktop".into())
    );
    assert!(host.pointer_up(x + 20.0, y).launch.is_none());
}

#[test]
fn a_drag_that_starts_off_the_tiles_is_not_a_tile_drag() {
    let mut host = open_host(3, true);
    let grid = host.frame().unwrap().grid;
    // The grid's far end, past the three tiles of its first row.
    let (x, y) = (grid.x + grid.width - 4.0, grid.y + 10.0);
    assert!(host.pointer_down(x, y).consumed);
    let moved = host.pointer_moved(x - 30.0, y);
    assert!(moved.drag.is_none() && !moved.closed);
    assert!(
        host.pointer_moved(x - 30.0, y + 70.0).closed,
        "the close gesture"
    );
}

#[test]
fn the_deck_row_keeps_its_presses_a_drag_toward_the_border_or_a_press_outside_closes() {
    let mut host = open_host(3, true);
    let f = host.frame().unwrap();
    let row = f.row;
    assert!(
        !host.pointer_down(row.x + 40.0, row.y + 30.0).consumed,
        "Deck cards"
    );
    let tile = f.tiles[0].rect;
    host.pointer_down(tile.x + 10.0, tile.y + 10.0);
    assert!(host.pointer_moved(tile.x + 10.0, tile.y + 70.0).closed);
    let mut host = open_host(3, true);
    assert!(host.pointer_down(100.0, 100.0).closed);
}

#[test]
fn opening_extrudes_in_full_motion_and_closing_retracts_into_the_deck() {
    let mut host = StackHost::new();
    host.set_geometry(geometry(true));
    host.set_rows(rows(8), "");
    host.open();
    let mut t = Instant::now();
    let mut heights = Vec::new();
    while host.tick(t, false) {
        heights.push(host.frame().map_or(0.0, |f| f.envelope.height));
        t += Duration::from_millis(16);
        assert!(heights.len() < 400);
    }
    assert!(heights.len() > 5, "it animates");
    assert!(heights.windows(2).take(5).all(|w| w[1] >= w[0]), "it grows");
    host.close();
    while host.tick(t, false) {
        t += Duration::from_millis(16);
    }
    assert!(host.frame().is_none(), "back inside the Deck");
}

#[test]
fn wheel_scrolls_without_moving_the_current_tile() {
    let mut host = open_host(40, true);
    let f = host.frame().unwrap();
    let first = f.tiles[0].index;
    let at = (f.grid.x + 10.0, f.grid.y + 10.0);
    assert!(host.wheel(at.0, at.1, 104.0).changed);
    let f = host.frame().unwrap();
    assert_eq!(
        f.tiles.iter().find(|t| t.rect.y >= f.grid.y).unwrap().index,
        first + 6
    );
    assert_eq!(host.current().unwrap().name, "App 00");
}

#[test]
fn feedback_takes_a_line_at_the_far_edge_and_a_new_query_clears_it() {
    let mut host = open_host(3, true);
    let before = host.frame().unwrap().grid;
    host.set_feedback(Some("Could not launch".into()));
    let mut t = Instant::now();
    while host.tick(t, false) {
        t += Duration::from_millis(16);
    }
    let f = host.frame().unwrap();
    let (rect, _) = f.feedback.clone().expect("feedback");
    assert!(rect.y < f.grid.y, "at the far edge, above the grid");
    assert!((f.grid.y - before.y).abs() < 1e-3 || f.envelope.height > 0.0);
    host.set_rows(rows(3), "a");
    assert!(host.frame().unwrap().feedback.is_none());
}

#[test]
fn empty_query_orders_favourites_then_frequency_then_name() {
    let mut freq = LaunchFrequency::default();
    let a = row("a", "Alpha");
    let b = row("b", "Beta");
    let c = row("c", "Gamma");
    let mut d = row("d", "Delta");
    d.favourite = true;
    freq.record(&c.entry);
    freq.record(&c.entry);
    freq.record(&b.entry);
    let ordered = order_rows(
        vec![d.clone(), a.clone(), b.clone(), c.clone()],
        &freq,
        true,
    );
    let names: Vec<_> = ordered.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Delta", "Gamma", "Beta", "Alpha"]);
    let kept = order_rows(vec![a, b, c, d], &freq, false);
    let names: Vec<_> = kept.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["Alpha", "Beta", "Gamma", "Delta"],
        "a query keeps search order"
    );
}

fn categorised(id: &str, name: &str, categories: &[&str]) -> AppRow {
    let mut r = row(id, name);
    r.categories = categories.iter().map(|c| (*c).to_owned()).collect();
    r
}

#[test]
fn tabs_show_a_category_and_chips_ask_for_a_mode() {
    use crate::deck::stack_content::{AppCategory, StackTab};
    let mut host = StackHost::new();
    host.set_geometry(geometry(true));
    let mut running = categorised("term", "Terminal", &["System", "TerminalEmulator"]);
    running.running = true;
    running.pinned = true;
    host.set_rows(
        vec![
            categorised("web", "Browser", &["Network", "WebBrowser"]),
            running,
            categorised("paint", "Paint", &["Graphics"]),
        ],
        "",
    );
    host.open();
    settle(&mut host);
    let f = host.frame().unwrap();
    let labels: Vec<_> = f.tabs.iter().map(|(t, _, _)| t.label()).collect();
    assert_eq!(labels, ["All", "Internet", "Graphics", "System"]);
    assert!(f.tabs[0].2, "All is current");
    assert_eq!(f.chips.len(), 9);
    assert!(f.chips[0].3, "Name is the active mode");
    // The tabs ride the far edge, the chips stand by the Deck row.
    assert!(f.tabs[0].1.y < f.grid.y);
    assert!(f.chips[0].2.y > f.grid.y + f.grid.height);
    // A press on System shows only the terminal, its state on the tile.
    let system = f
        .tabs
        .iter()
        .find(|(t, _, _)| *t == StackTab::Category(AppCategory::System))
        .unwrap()
        .1;
    let out = host.pointer_down(system.x + 4.0, system.y + 4.0);
    assert!(out.consumed && out.changed);
    host.pointer_up(system.x + 4.0, system.y + 4.0);
    settle(&mut host);
    let f = host.frame().unwrap();
    assert_eq!(host.tab(), StackTab::Category(AppCategory::System));
    assert_eq!(f.tiles.len(), 1);
    assert!(f.tiles[0].row.running && f.tiles[0].row.pinned);
    // A press on the Terminal chip asks the scene for that mode.
    let chip = f
        .chips
        .iter()
        .find(|(_, l, _, _)| *l == "Terminal")
        .unwrap()
        .2;
    let out = host.pointer_down(chip.x + 3.0, chip.y + 3.0);
    assert_eq!(out.mode, Some(ApplicationSearchMode::TerminalOnly));
    host.pointer_up(chip.x + 3.0, chip.y + 3.0);
    // Searching hides the tabs, marks the letters and fits the results.
    host.set_rows(vec![categorised("paint", "Paint", &["Graphics"])], "pnt");
    settle(&mut host);
    let f = host.frame().unwrap();
    assert!(f.tabs.is_empty());
    assert!(f.headings.is_empty());
    assert_eq!(f.columns, 1);
    assert_eq!(f.tiles[0].marked, vec![0, 3, 4]);
    let chip = f.chips.iter().find(|(_, _, _, active)| *active).unwrap();
    assert_eq!(chip.1, "Name");
}
