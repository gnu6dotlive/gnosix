//! SURF-02B: the session panel painted, on the popouts' material.
//!
//! The hull is the popout's (joined to the Bar), and the content slides with
//! the revealed edge: geometry only, no fade. A held action fills its tile
//! from the bottom as the hold goes on (size, not opacity); a blocked one is
//! ringed in the error colour; one logind does not offer is dimmed.

use crate::bar_popout_host::PopoutFrame;
use crate::bar_popout_view::paint_popout_hull;
use crate::notification_view::{line, with_alpha};
use crate::reference_bar::{
    COLOR_ERROR, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY,
    COLOR_SURFACE_CONTAINER_HIGHEST,
};
use crate::session_panel::{SessionRow, label};
use gnos_i18n::t;
use gnos_utilities::SessionAction;
use gpui::{
    Bounds, Div, ParentElement, Rgba, Styled, canvas, div, point, prelude::FluentBuilder, px, rgba,
    size,
};

const BOX: f32 = 44.0;

fn placed(rect: (f32, f32, f32, f32)) -> Div {
    let (x, y, w, h) = rect;
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}

fn glyph(action: SessionAction) -> &'static str {
    match action {
        SessionAction::Lock => "lock",
        SessionAction::Suspend => "bedtime",
        SessionAction::Logout => "logout",
        SessionAction::Reboot => "restart_alt",
        SessionAction::PowerOff => "power_settings_new",
    }
}

fn row_element(row: SessionRow, hint: Option<SessionAction>) -> Div {
    let rect = row.rect();
    match row {
        SessionRow::Header { text, .. } => {
            placed(rect)
                .flex()
                .items_center()
                .child(line(text, COLOR_ON_SURFACE, 13.0, 16.0))
        }
        SessionRow::Line { text, .. } => placed(rect)
            .flex()
            .items_center()
            .overflow_hidden()
            .child(line(&text, COLOR_ON_SURFACE_VARIANT, 11.0, 16.0)),
        SessionRow::Tile { tile, hold, .. } => {
            let offered = tile.available != Some(false);
            let fg = if offered {
                COLOR_ON_SURFACE
            } else {
                COLOR_OUTLINE_VARIANT
            };
            let bg = if offered {
                rgba(COLOR_SURFACE_CONTAINER_HIGHEST)
            } else {
                with_alpha(COLOR_SURFACE_CONTAINER_HIGHEST, 0x80)
            };
            let caption = match (&tile.note, tile.pending) {
                (_, true) => "…".to_owned(),
                (Some(note), false) => note.clone(),
                (None, false) if hint == Some(tile.action) => t("Hold").to_owned(),
                (None, false) if tile.blocked > 0 => t("Force").to_owned(),
                (None, false) => label(tile.action).to_owned(),
            };
            let (_, _, w, _) = rect;
            let fill = hold.map(|h| BOX * h);
            let mut tile_box = div()
                .relative()
                .w(px(w))
                .h(px(BOX))
                .rounded(px(16.0))
                .overflow_hidden()
                .bg(bg)
                .when(tile.blocked > 0, |d| {
                    d.border_1().border_color(rgba(COLOR_ERROR))
                });
            if let Some(fill) = fill {
                // SURF-02F: the fill is the tile's own rounded shape, seen
                // through a band that rises from the bottom; GPUI clips to
                // rectangles only, so a plain band overflowed the corners.
                tile_box = tile_box.child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(BOX - fill))
                        .w(px(w))
                        .h(px(fill))
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(fill - BOX))
                                .w(px(w))
                                .h(px(BOX))
                                .rounded(px(16.0))
                                .bg(rgba(if tile.blocked > 0 {
                                    COLOR_ERROR
                                } else {
                                    COLOR_PRIMARY
                                })),
                        ),
                );
            }
            tile_box = tile_box.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .w(px(w))
                    .h(px(BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(crate::icons::icon(glyph(tile.action), 22.0, fg)),
            );
            placed(rect)
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .child(tile_box)
                .child(line(&caption, COLOR_ON_SURFACE_VARIANT, 10.0, 14.0))
        }
    }
}

/// The panel's full-scene layer: hull, then the sliding content.
pub fn session_layer(
    viewport: (f32, f32),
    frame: PopoutFrame,
    rows: Vec<SessionRow>,
    hint: Option<SessionAction>,
    material: Rgba,
) -> Div {
    let scene = Bounds::new(
        point(px(0.0), px(0.0)),
        size(px(viewport.0), px(viewport.1)),
    );
    let hull = canvas(
        |_, _, _| (),
        move |_, _, window, _| paint_popout_hull(scene, &frame, material, window),
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w(px(viewport.0))
    .h(px(viewport.1));
    let (slide_x, slide_y) = match frame.side {
        crate::rail_topology::RailSide::Left => (frame.rect.width() - frame.visible.width(), 0.0),
        crate::rail_topology::RailSide::Top => (0.0, frame.rect.height() - frame.visible.height()),
        crate::rail_topology::RailSide::Right | crate::rail_topology::RailSide::Bottom => {
            (0.0, 0.0)
        }
    };
    let content = div()
        .absolute()
        .left(px(-slide_x))
        .top(px(-slide_y))
        .w(px(frame.rect.width()))
        .h(px(frame.rect.height()))
        .children(crate::icons::drawing(
            crate::icons::draw_on(frame.openness),
            || {
                rows.into_iter()
                    .map(|r| row_element(r, hint))
                    .collect::<Vec<_>>()
            },
        ));
    let clip = div()
        .absolute()
        .left(px(frame.visible.left()))
        .top(px(frame.visible.top()))
        .w(px(frame.visible.width()))
        .h(px(frame.visible.height()))
        .overflow_hidden()
        .child(content);
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(hull)
        .child(clip)
}
