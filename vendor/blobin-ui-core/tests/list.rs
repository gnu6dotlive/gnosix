use blobin_ui_core::list::{CursorStep, Ends, ListCursor, UniformRows};

#[test]
fn empty_list_has_no_current_row_and_ignores_steps() {
    let mut cursor = ListCursor::new(Ends::Clamp);
    assert_eq!(cursor.current(), None);
    assert!(!cursor.step(CursorStep::Next, 1));
    assert!(!cursor.select(0));
    cursor.reset(0);
    assert!(cursor.is_empty());
    assert_eq!(cursor.current(), None);
}

#[test]
fn reset_makes_the_first_row_current() {
    let mut cursor = ListCursor::new(Ends::Clamp);
    cursor.reset(5);
    assert!(cursor.step(CursorStep::Last, 1));
    assert_eq!(cursor.current(), Some(4));
    cursor.reset(3);
    assert_eq!(cursor.current(), Some(0));
    assert_eq!(cursor.len(), 3);
}

#[test]
fn clamped_steps_stop_at_both_ends() {
    let mut cursor = ListCursor::new(Ends::Clamp);
    cursor.reset(3);
    assert!(!cursor.step(CursorStep::Previous, 1));
    assert!(cursor.step(CursorStep::Next, 1));
    assert!(cursor.step(CursorStep::Next, 1));
    assert!(!cursor.step(CursorStep::Next, 1));
    assert_eq!(cursor.current(), Some(2));
    assert!(cursor.step(CursorStep::First, 1));
    assert_eq!(cursor.current(), Some(0));
}

#[test]
fn wrapping_applies_to_single_steps_only() {
    let mut cursor = ListCursor::new(Ends::Wrap);
    cursor.reset(10);
    assert!(cursor.step(CursorStep::Previous, 1));
    assert_eq!(cursor.current(), Some(9));
    assert!(cursor.step(CursorStep::Next, 1));
    assert_eq!(cursor.current(), Some(0));
    // Page steps clamp even when single steps wrap.
    assert!(!cursor.step(CursorStep::PageUp, 4));
    assert!(cursor.step(CursorStep::PageDown, 4));
    assert!(cursor.step(CursorStep::PageDown, 4));
    assert!(cursor.step(CursorStep::PageDown, 4));
    assert_eq!(cursor.current(), Some(9));
    assert!(!cursor.step(CursorStep::PageDown, 0));
    assert!(cursor.step(CursorStep::PageUp, 0));
    assert_eq!(cursor.current(), Some(8), "a zero page moves one row");
}

#[test]
fn select_refuses_out_of_range_and_reports_change() {
    let mut cursor = ListCursor::new(Ends::Clamp);
    cursor.reset(4);
    assert!(!cursor.select(4));
    assert!(cursor.select(3));
    assert!(!cursor.select(3));
}

#[test]
fn content_extent_has_no_trailing_gap() {
    let rows = UniformRows::new(57.0, 8.0);
    assert_eq!(rows.content_extent(0), 0.0);
    assert_eq!(rows.content_extent(1), 57.0);
    assert_eq!(rows.content_extent(7), 7.0 * 65.0 - 8.0);
    assert_eq!(rows.viewport_for(7, 3), 3.0 * 65.0 - 8.0);
    assert_eq!(rows.viewport_for(7, 30), 447.0);
}

#[test]
fn keep_visible_moves_the_least_and_clamps() {
    let rows = UniformRows::new(57.0, 8.0);
    let viewport = rows.viewport_for(7, 20);
    // Already visible: unchanged.
    assert_eq!(rows.keep_visible(0.0, 6, 20, viewport), 0.0);
    // Below: the row's bottom meets the viewport's bottom.
    let s = rows.keep_visible(0.0, 7, 20, viewport);
    assert_eq!(s, 7.0 * 65.0 + 57.0 - viewport);
    // Above: the row's top meets the viewport's top.
    assert_eq!(rows.keep_visible(s, 0, 20, viewport), 0.0);
    // The last row never scrolls past the end.
    let end = rows.keep_visible(0.0, 19, 20, viewport);
    assert_eq!(end, rows.max_scroll(20, viewport));
    // Out of range or garbage scroll: clamped, not moved.
    assert_eq!(rows.keep_visible(f32::NAN, 99, 20, viewport), 0.0);
    assert_eq!(
        rows.keep_visible(1.0e9, 99, 20, viewport),
        rows.max_scroll(20, viewport)
    );
}

#[test]
fn row_taller_than_the_viewport_aligns_to_its_top() {
    let rows = UniformRows::new(100.0, 0.0);
    assert_eq!(rows.keep_visible(0.0, 2, 5, 40.0), 200.0);
}

#[test]
fn row_at_skips_gaps_and_ends() {
    let rows = UniformRows::new(57.0, 8.0);
    assert_eq!(rows.row_at(0.0, 0.0, 3), Some(0));
    assert_eq!(rows.row_at(0.0, 56.9, 3), Some(0));
    assert_eq!(rows.row_at(0.0, 60.0, 3), None, "gap");
    assert_eq!(rows.row_at(0.0, 65.0, 3), Some(1));
    assert_eq!(rows.row_at(65.0, 0.0, 3), Some(1), "scrolled by one pitch");
    assert_eq!(rows.row_at(0.0, 200.0, 3), None, "past the last row");
    assert_eq!(rows.row_at(0.0, -1.0, 3), None);
    assert_eq!(rows.row_at(0.0, f32::INFINITY, 3), None);
}

#[test]
fn page_counts_whole_rows_and_is_never_zero() {
    let rows = UniformRows::new(57.0, 8.0);
    assert_eq!(rows.page(rows.viewport_for(7, 20)), 7);
    assert_eq!(rows.page(10.0), 1);
    assert_eq!(UniformRows::new(0.0, 0.0).page(100.0), 1);
}

#[test]
fn invalid_dimensions_count_as_zero() {
    let rows = UniformRows::new(-5.0, f32::NAN);
    assert_eq!(rows.pitch(), 0.0);
    assert_eq!(rows.content_extent(4), 0.0);
    assert_eq!(rows.row_at(0.0, 0.0, 4), None);
    let raw = UniformRows {
        extent: f32::INFINITY,
        gap: -1.0,
    };
    assert_eq!(raw.pitch(), 0.0);
}
