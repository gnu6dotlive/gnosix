//! SURF-03B: the dashboard's content, painted over the popout's hull (the
//! popout layer draws the hull; this layer the parts), clipped to what the
//! extrusion reveals and sliding with it. Pages slide by position; nothing
//! fades.

use crate::bar_popout_host::PopoutFrame;
use crate::dashboard_panel::{DashRow, Tone};
use crate::notification_view::{line, with_alpha};
use crate::reference_bar::{
    COLOR_ON_PRIMARY, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT,
    COLOR_PRIMARY, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use gpui::{
    Bounds, Div, ParentElement, PathBuilder, Styled, canvas, div, point, prelude::FluentBuilder,
    px, rgba,
};

fn placed(rect: (f32, f32, f32, f32)) -> Div {
    let (x, y, w, h) = rect;
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}

fn tone(t: Tone) -> u32 {
    match t {
        Tone::Strong => COLOR_ON_SURFACE,
        Tone::Normal => COLOR_ON_SURFACE,
        Tone::Muted => COLOR_ON_SURFACE_VARIANT,
        Tone::Accent => COLOR_PRIMARY,
    }
}

fn row_element(row: DashRow) -> Div {
    let rect = row.rect();
    match row {
        DashRow::Text {
            text,
            size,
            tone: t,
            right,
            ..
        } => placed(rect)
            .flex()
            .items_center()
            .overflow_hidden()
            .when(right, |d| d.justify_end())
            .child(line(&text, tone(t), size, size + 4.0)),
        DashRow::Button {
            text,
            active,
            enabled,
            ..
        } => {
            let (bg, fg) = match (active, enabled) {
                (true, _) => (rgba(COLOR_PRIMARY), COLOR_ON_PRIMARY),
                (false, true) => (rgba(COLOR_SURFACE_CONTAINER_HIGHEST), COLOR_ON_SURFACE),
                (false, false) => (
                    with_alpha(COLOR_SURFACE_CONTAINER_HIGHEST, 0x80),
                    COLOR_OUTLINE_VARIANT,
                ),
            };
            let (_, _, _, h) = rect;
            placed(rect)
                .rounded(px(h / 2.0))
                .bg(bg)
                .flex()
                .items_center()
                .justify_center()
                .child(line(&text, fg, 12.0, 16.0))
        }
        DashRow::Day {
            day,
            today,
            weekend,
            ..
        } => {
            let fg = if today {
                COLOR_ON_PRIMARY
            } else if weekend {
                COLOR_ON_SURFACE_VARIANT
            } else {
                COLOR_ON_SURFACE
            };
            let (_, _, w, h) = rect;
            placed(rect).flex().items_center().justify_center().child(
                div()
                    .w(px(w.min(h) - 2.0))
                    .h(px(w.min(h) - 2.0))
                    .rounded(px(w.min(h)))
                    .when(today, |d| d.bg(rgba(COLOR_PRIMARY)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(line(&day.to_string(), fg, 11.0, 14.0)),
            )
        }
        DashRow::Meter { fraction, .. } => {
            let (_, _, w, h) = rect;
            placed(rect)
                .rounded(px(h / 2.0))
                .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .h(px(h))
                        .w(px(w * fraction.clamp(0.0, 1.0)))
                        .rounded(px(h / 2.0))
                        .bg(rgba(COLOR_PRIMARY)),
                )
        }
        DashRow::Spark { values, .. } => {
            let (_, _, w, h) = rect;
            placed(rect).child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                        if values.len() < 2 {
                            return;
                        }
                        let n = (crate::dashboard_panel::CPU_HISTORY.max(values.len()) - 1) as f32;
                        let start =
                            crate::dashboard_panel::CPU_HISTORY.saturating_sub(values.len()) as f32;
                        let mut path = PathBuilder::stroke(px(1.5));
                        for (i, v) in values.iter().enumerate() {
                            let x = bounds.origin.x + px((start + i as f32) / n * w);
                            let y = bounds.origin.y + px(h - (v / 100.0).clamp(0.0, 1.0) * h);
                            if i == 0 {
                                path.move_to(point(x, y));
                            } else {
                                path.line_to(point(x, y));
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgba(COLOR_PRIMARY));
                        }
                    },
                )
                .size_full(),
            )
        }
    }
}

/// The dashboard's parts over the popout's hull.
/// The dashboard's content on the popout's surface: its own `size`, at
/// `offset` along the Bar (FOCUS-01B: passing along during a handoff).
pub fn dashboard_layer(
    frame: PopoutFrame,
    rows: Vec<DashRow>,
    size: (f32, f32),
    offset: f32,
) -> Div {
    let (x, y) = crate::bar_popout_view::content_origin(&frame, size, offset);
    let content = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(size.0))
        .h(px(size.1))
        .children(rows.into_iter().map(row_element));
    div().absolute().left(px(0.0)).top(px(0.0)).child(
        div()
            .absolute()
            .left(px(frame.visible.left()))
            .top(px(frame.visible.top()))
            .w(px(frame.visible.width()))
            .h(px(frame.visible.height()))
            .overflow_hidden()
            .child(content),
    )
}
