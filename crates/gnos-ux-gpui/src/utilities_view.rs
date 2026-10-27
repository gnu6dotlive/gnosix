//! SURF-01B: the Utilities panel painted, on the popouts' material.
//!
//! The hull is the popout's (joined to the rail), and the content slides
//! with the revealed edge: geometry only, no fade. Toggles are filled tiles
//! when on, outlined when off, dimmed when unavailable; "…" marks a request
//! in flight and a refusal shows as the toggle's caption.

use crate::bar_popout_host::PopoutFrame;
use crate::bar_popout_view::paint_popout_hull;
use crate::notification_view::{line, with_alpha};
use crate::reference_bar::{
    COLOR_ON_PRIMARY, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT,
    COLOR_PRIMARY, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use crate::utilities_panel::{ToggleKind, UtilitiesRow};
use gpui::{
    Bounds, Div, ParentElement, Rgba, Styled, canvas, div, point, prelude::FluentBuilder, px, rgba,
    size,
};

const TEXT: f32 = 12.0;
const LINE: f32 = 16.0;
const TRACK: f32 = 6.0;

fn placed(rect: (f32, f32, f32, f32)) -> Div {
    let (x, y, w, h) = rect;
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}

/// The toggle's Tabler icon (ICONS-01).
fn mark(kind: ToggleKind) -> &'static str {
    match kind {
        ToggleKind::Wifi => "wifi",
        ToggleKind::Bluetooth => "bluetooth",
        ToggleKind::Microphone => "microphone",
        ToggleKind::DoNotDisturb => "bell-off",
        ToggleKind::KeepAwake => "coffee",
    }
}

fn row_element(row: UtilitiesRow) -> Div {
    let rect = row.rect();
    match row {
        UtilitiesRow::Header { text, .. } => {
            placed(rect)
                .flex()
                .items_center()
                .child(line(text, COLOR_ON_SURFACE, 13.0, LINE))
        }
        UtilitiesRow::Label { text, .. } => placed(rect).flex().items_center().child(line(
            &text,
            COLOR_ON_SURFACE_VARIANT,
            TEXT,
            LINE,
        )),
        UtilitiesRow::Caption { text, .. } => placed(rect).flex().items_center().child(line(
            &text,
            COLOR_ON_SURFACE_VARIANT,
            TEXT,
            LINE,
        )),
        UtilitiesRow::Toggle { state, .. } => {
            let on = state.on == Some(true);
            let (bg, fg) = match state.on {
                Some(true) => (rgba(COLOR_PRIMARY), COLOR_ON_PRIMARY),
                Some(false) => (rgba(COLOR_SURFACE_CONTAINER_HIGHEST), COLOR_ON_SURFACE),
                None => (
                    with_alpha(COLOR_SURFACE_CONTAINER_HIGHEST, 0x80),
                    COLOR_OUTLINE_VARIANT,
                ),
            };
            let caption = match (&state.note, state.pending) {
                (_, true) => "…".to_owned(),
                (Some(note), false) => note.clone(),
                (None, false) => state.kind.label().to_owned(),
            };
            let (_, _, w, _) = rect;
            placed(rect)
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .size(px(w))
                        .h(px(44.0))
                        .rounded(px(16.0))
                        .bg(bg)
                        .when(!on && state.on.is_some(), |d| {
                            d.border_1().border_color(rgba(COLOR_OUTLINE_VARIANT))
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::icons::icon(mark(state.kind), 20.0, fg)),
                )
                .child(line(&caption, COLOR_ON_SURFACE_VARIANT, 10.0, 14.0))
        }
        UtilitiesRow::Slider {
            percent,
            preview,
            pending,
            ..
        } => {
            let (_, _, w, h) = rect;
            let shown = preview.or(percent);
            let fill = shown.map_or(0.0, |p| w * f32::from(p) / 100.0);
            let mut track = div()
                .absolute()
                .left(px(0.0))
                .top(px((h - TRACK) / 2.0))
                .w(px(w))
                .h(px(TRACK))
                .rounded(px(TRACK / 2.0))
                .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST));
            if shown.is_some() {
                track = track.child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .w(px(fill))
                        .h(px(TRACK))
                        .rounded(px(TRACK / 2.0))
                        .bg(rgba(if pending {
                            COLOR_OUTLINE_VARIANT
                        } else {
                            COLOR_PRIMARY
                        })),
                );
            }
            let label = shown.map_or("—".to_owned(), |p| format!("{p}%"));
            placed(rect).child(track).child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(0.0))
                    .text_size(px(10.0))
                    .text_color(rgba(COLOR_ON_SURFACE_VARIANT))
                    .child(label),
            )
        }
    }
}

/// The panel's full-scene layer: hull, then the sliding content.
pub fn utilities_layer(
    viewport: (f32, f32),
    frame: PopoutFrame,
    rows: Vec<UtilitiesRow>,
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
            || rows.into_iter().map(row_element).collect::<Vec<_>>(),
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
