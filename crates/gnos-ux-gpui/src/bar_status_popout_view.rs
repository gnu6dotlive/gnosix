//! UXE-04: GPUI rows of the read-only Bar popouts.
//!
//! Painted at the rectangles `InfoPopout` laid out, inside the same sliding
//! clip as the audio popout. Nothing here reacts to input: a switch shows an
//! observed state, and no mutating button is drawn.

use crate::bar_status_popouts::{InfoRow, InfoRowKind};
use crate::product_status_glyph::{PowerProfileGlyph, StatusGlyph, status_glyph_paths_drawn};
use crate::reference_bar::{
    COLOR_ON_PRIMARY, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT,
    COLOR_PRIMARY, COLOR_SURFACE_CONTAINER, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use gpui::{Div, ParentElement, Styled, canvas, div, px, rgba};

const TEXT_SMALL: f32 = 12.0;
const TEXT_BODY: f32 = 13.0;
const GLYPH: f32 = 18.0;
const SWITCH_W: f32 = 36.0;
const SWITCH_H: f32 = 20.0;

fn placed(row: &InfoRow) -> Div {
    div()
        .absolute()
        .left(px(row.x))
        .top(px(row.y))
        .w(px(row.width))
        .h(px(row.height))
}

fn text(label: &str, colour: u32, size_px: f32) -> Div {
    div()
        .min_w_0()
        .flex_1()
        .text_size(px(size_px))
        .text_color(rgba(colour))
        .truncate()
        .child(label.to_owned())
}

fn glyph(glyph: StatusGlyph, colour: u32, size_px: f32) -> Div {
    let drawn = crate::icons::current_drawn();
    div().size(px(size_px)).flex_none().child(
        canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let ox =
                    f32::from(bounds.origin.x) + (f32::from(bounds.size.width) - size_px) / 2.0;
                let oy =
                    f32::from(bounds.origin.y) + (f32::from(bounds.size.height) - size_px) / 2.0;
                for path in status_glyph_paths_drawn(glyph, (ox, oy), size_px, drawn) {
                    window.paint_path(path, rgba(colour));
                }
            },
        )
        .size_full(),
    )
}

/// A small padlock: shackle over a body.
fn lock_mark(colour: u32) -> Div {
    div()
        .w(px(10.0))
        .h(px(12.0))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .w(px(6.0))
                .h(px(5.0))
                .border_2()
                .border_b_0()
                .rounded_t(px(3.0))
                .border_color(rgba(colour)),
        )
        .child(
            div()
                .w(px(10.0))
                .h(px(7.0))
                .rounded(px(1.5))
                .bg(rgba(colour)),
        )
}

/// The observed state as a switch that never moves under the pointer.
fn switch(on: Option<bool>) -> Div {
    let track = div()
        .w(px(SWITCH_W))
        .h(px(SWITCH_H))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .px(px(3.0));
    match on {
        Some(true) => track.bg(rgba(COLOR_PRIMARY)).justify_end().child(
            div()
                .size(px(SWITCH_H - 6.0))
                .rounded_full()
                .bg(rgba(COLOR_ON_PRIMARY)),
        ),
        Some(false) => track
            .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
            .border_2()
            .border_color(rgba(COLOR_OUTLINE_VARIANT))
            .child(
                div()
                    .size(px(SWITCH_H - 10.0))
                    .rounded_full()
                    .bg(rgba(COLOR_OUTLINE_VARIANT)),
            ),
        // Unobserved: an empty outline, neither on nor off.
        None => track.border_2().border_color(rgba(COLOR_OUTLINE_VARIANT)),
    }
}

fn profiles(current: Option<PowerProfileGlyph>) -> Div {
    let cell = |profile: PowerProfileGlyph| {
        let selected = current == Some(profile);
        let base = div()
            .size(px(32.0))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center();
        let base = if selected {
            base.bg(rgba(COLOR_PRIMARY))
        } else {
            base
        };
        base.child(glyph(
            StatusGlyph::Profile(profile),
            if selected {
                COLOR_ON_PRIMARY
            } else {
                COLOR_ON_SURFACE_VARIANT
            },
            20.0,
        ))
    };
    div()
        .flex()
        .items_center()
        .gap(px(20.0))
        .px(px(4.0))
        .py(px(4.0))
        .rounded_full()
        .bg(rgba(COLOR_SURFACE_CONTAINER))
        .child(cell(PowerProfileGlyph::PowerSaver))
        .child(cell(PowerProfileGlyph::Balanced))
        .child(cell(PowerProfileGlyph::Performance))
}

pub fn info_row_element(row: InfoRow) -> Div {
    match row.kind {
        InfoRowKind::Header => placed(&row).flex().items_end().child(
            text(&row.label, COLOR_ON_SURFACE, TEXT_BODY).font_weight(gpui::FontWeight::MEDIUM),
        ),
        InfoRowKind::Caption => placed(&row).flex().items_center().child(text(
            &row.label,
            COLOR_ON_SURFACE_VARIANT,
            TEXT_SMALL,
        )),
        InfoRowKind::Text => {
            placed(&row)
                .flex()
                .items_center()
                .child(text(&row.label, COLOR_ON_SURFACE, TEXT_BODY))
        }
        InfoRowKind::State { on } => placed(&row)
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(text(&row.label, COLOR_ON_SURFACE, TEXT_BODY))
            .child(switch(on)),
        InfoRowKind::Wifi {
            glyph: g,
            secure,
            active,
        } => {
            let colour = if active {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE_VARIANT
            };
            let mut element = placed(&row)
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(glyph(g, colour, GLYPH));
            if secure {
                element = element.child(lock_mark(COLOR_ON_SURFACE_VARIANT));
            }
            element.child(text(
                &row.label,
                if active {
                    COLOR_PRIMARY
                } else {
                    COLOR_ON_SURFACE
                },
                TEXT_BODY,
            ))
        }
        InfoRowKind::Wired { connected } => {
            let colour = if connected {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE_VARIANT
            };
            placed(&row)
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(glyph(StatusGlyph::Wired, colour, GLYPH))
                .child(text(
                    &row.label,
                    if connected {
                        COLOR_PRIMARY
                    } else {
                        COLOR_ON_SURFACE
                    },
                    TEXT_BODY,
                ))
        }
        InfoRowKind::BluetoothDevice {
            glyph: g,
            connected,
        } => {
            let mut element = placed(&row)
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(glyph(
                    g,
                    if connected {
                        COLOR_PRIMARY
                    } else {
                        COLOR_ON_SURFACE_VARIANT
                    },
                    GLYPH,
                ))
                .child(text(
                    &row.label,
                    if connected {
                        COLOR_PRIMARY
                    } else {
                        COLOR_ON_SURFACE
                    },
                    TEXT_BODY,
                ));
            if connected {
                element = element.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_SMALL))
                        .text_color(rgba(COLOR_ON_SURFACE_VARIANT))
                        .child("Connected"),
                );
            }
            element
        }
        InfoRowKind::OpenSettings => placed(&row)
            .rounded_full()
            .border_1()
            .border_color(rgba(COLOR_OUTLINE_VARIANT))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(px(TEXT_BODY))
                    .text_color(rgba(COLOR_PRIMARY))
                    .child(row.label.clone()),
            ),
        InfoRowKind::Profiles { current } => placed(&row)
            .flex()
            .items_center()
            .justify_center()
            .child(profiles(current)),
    }
}
