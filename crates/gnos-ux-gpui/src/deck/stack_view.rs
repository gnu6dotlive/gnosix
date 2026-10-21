//! DECK-01C: the AppStack's tiles, painted inside the part of the Deck's
//! envelope that has extruded.
//!
//! The envelope itself is painted with the Deck (one material), and the
//! search field sits in the Deck row's control slot; this layer holds the
//! grid. Tiles (D05) are full rounded tiles in surface-container, the icon
//! centred and the name below. A hovered or current tile rises: it grows by
//! `TILE_RISE` and takes a primary tint; the current one also gets a primary
//! ring for the keyboard. Nothing fades.
//!
//! DECK-02: the tabs at the far edge (the current one filled), letter
//! headings in the grid, the search modes as chips by the Deck row (the
//! active one filled), and tiles with a two-line name, the letters a search
//! matched in the primary colour, a dot when the application runs and a
//! mark when it is pinned.

use super::stack::{StackFrame, TILE_ICON, TILE_RADIUS, TilePaint, risen};
use crate::notification_view::{line, with_alpha};
use crate::reference_bar::{
    COLOR_ERROR, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_PRIMARY,
    COLOR_SURFACE_CONTAINER, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use blobin_ui_core::layout::Rect;
use gpui::{
    Div, FontWeight, HighlightStyle, ParentElement, Styled, StyledText, div, img, px, rgba,
};

const COLOR_OUTLINE: u32 = 0x918f9aff;
const NAME_PX: f32 = 12.0;
const NAME_LINE: f32 = 15.0;
const LABEL_PX: f32 = 12.0;
/// The running dot and the pinned mark, inset from the tile's corners.
const STATE_DOT: f32 = 6.0;
const STATE_INSET: f32 = 9.0;
const TINT_ALPHA: u8 = 0x29;

fn placed(rect: Rect, origin: (f32, f32)) -> Div {
    div()
        .absolute()
        .left(px(rect.x - origin.0))
        .top(px(rect.y - origin.1))
        .w(px(rect.width))
        .h(px(rect.height))
}

fn badge(name: &str) -> Div {
    let letter = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .size(px(TILE_ICON))
        .flex_none()
        .rounded(px(14.0))
        .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(20.0))
        .text_color(rgba(COLOR_ON_SURFACE_VARIANT))
        .child(letter)
}

/// The name on up to two lines, the matched letters marked (A02).
fn name_element(name: &str, marked: &[usize]) -> Div {
    let starts: Vec<(usize, usize)> = name
        .char_indices()
        .enumerate()
        .filter(|(i, _)| marked.contains(i))
        .map(|(_, (at, c))| (at, at + c.len_utf8()))
        .collect();
    let text =
        StyledText::new(name.to_owned()).with_highlights(starts.into_iter().map(|(a, b)| {
            (
                a..b,
                HighlightStyle {
                    color: Some(rgba(COLOR_PRIMARY).into()),
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
            )
        }));
    div()
        .w_full()
        .h(px(2.0 * NAME_LINE))
        .overflow_hidden()
        .text_center()
        .text_size(px(NAME_PX))
        .line_height(px(NAME_LINE))
        .line_clamp(2)
        .text_color(rgba(COLOR_ON_SURFACE))
        .child(text)
}

fn tile_element(tile: TilePaint, origin: (f32, f32)) -> Div {
    let raised = tile.current || tile.hovered;
    let rect = if raised { risen(tile.rect) } else { tile.rect };
    let icon = match &tile.row.icon {
        Some(path) => div()
            .size(px(TILE_ICON))
            .flex_none()
            .child(img(path.clone()).size(px(TILE_ICON))),
        None => badge(&tile.row.name),
    };
    let mut element = placed(rect, origin)
        .rounded(px(TILE_RADIUS))
        .bg(rgba(COLOR_SURFACE_CONTAINER))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .px(px(6.0))
        .child(icon)
        .child(name_element(&tile.row.name, &tile.marked));
    // A04: running, a dot top-left; pinned, a mark top-right.
    if tile.row.running {
        element = element.child(
            div()
                .absolute()
                .left(px(STATE_INSET))
                .top(px(STATE_INSET))
                .size(px(STATE_DOT))
                .rounded(px(STATE_DOT / 2.0))
                .bg(rgba(COLOR_ON_SURFACE)),
        );
    }
    if tile.row.pinned {
        element = element.child(
            div()
                .absolute()
                .right(px(STATE_INSET))
                .top(px(STATE_INSET - 1.0))
                .w(px(STATE_DOT))
                .h(px(STATE_DOT + 2.0))
                .rounded(px(2.0))
                .bg(rgba(COLOR_PRIMARY)),
        );
    }
    if raised {
        element = element.bg(with_alpha(COLOR_PRIMARY, TINT_ALPHA));
    }
    if tile.current {
        element = element.border_2().border_color(rgba(COLOR_PRIMARY));
    }
    element
}

/// A tab or a chip: a pill, filled when it is the current one.
fn pill(rect: Rect, origin: (f32, f32), label: &str, on: bool, outlined: bool) -> Div {
    let mut pill = placed(rect, origin)
        .rounded(px(rect.height / 2.0))
        .flex()
        .items_center()
        .justify_center()
        .child(line(
            label,
            if on {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE_VARIANT
            },
            LABEL_PX,
            NAME_LINE,
        ));
    if on {
        pill = pill.bg(with_alpha(COLOR_PRIMARY, TINT_ALPHA));
    }
    if outlined || on {
        pill = pill
            .border_1()
            .border_color(rgba(if on { COLOR_PRIMARY } else { COLOR_OUTLINE }));
    }
    pill
}

fn heading(label: &str, rect: Rect, origin: (f32, f32)) -> Div {
    placed(rect, origin).flex().items_end().px(px(4.0)).child(
        div()
            .text_size(px(LABEL_PX))
            .line_height(px(NAME_LINE))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(COLOR_ON_SURFACE_VARIANT))
            .child(label.to_owned()),
    )
}

fn empty_state(grid: Rect, origin: (f32, f32)) -> Div {
    placed(grid, origin)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(line(
            gnos_i18n::t("No results"),
            COLOR_ON_SURFACE_VARIANT,
            16.0,
            22.0,
        ))
        .child(line(
            gnos_i18n::t("Try searching for something else"),
            COLOR_OUTLINE,
            NAME_PX,
            NAME_LINE,
        ))
}

/// The grid layer, in scene coordinates: the extruded part of the envelope
/// clips everything, the grid's viewport clips the tiles.
pub fn stack_layer(frame: StackFrame) -> Div {
    let clip = frame.clip;
    let origin = (clip.x, clip.y);
    let grid = frame.grid;
    // The viewport is grown by the rise so a risen edge tile is not cut.
    let viewport = risen(grid);
    let at = (viewport.x, viewport.y);
    let content = placed(viewport, origin)
        .overflow_hidden()
        .children(
            frame
                .headings
                .iter()
                .map(|(label, rect)| heading(label, *rect, at)),
        )
        .children(frame.tiles.into_iter().map(|tile| tile_element(tile, at)));
    let mut layer = placed(clip, (0.0, 0.0)).overflow_hidden();
    if let Some((rect, text)) = frame.feedback {
        layer = layer.child(
            placed(rect, origin)
                .flex()
                .items_center()
                .justify_center()
                .child(line(&text, COLOR_ERROR, NAME_PX, NAME_LINE)),
        );
    }
    layer = layer
        .children(
            frame
                .tabs
                .iter()
                .map(|(tab, rect, current)| pill(*rect, origin, tab.label(), *current, false)),
        )
        .children(
            frame
                .chips
                .iter()
                .map(|(_, label, rect, active)| pill(*rect, origin, label, *active, true)),
        );
    if frame.empty {
        layer.child(empty_state(grid, origin))
    } else {
        layer.child(content)
    }
}
