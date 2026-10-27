//! BAR-02A: the overflow panel painted, on the popouts' material.
//!
//! The hull is the popout's (joined to the Bar), and the content slides with
//! the revealed edge: geometry only, no fade. The active workspace is
//! tinted with the primary colour, the focused window is marked, and the row
//! under the pointer takes a light tint.

use crate::bar_popout_host::PopoutFrame;
use crate::bar_popout_view::paint_popout_hull;
use crate::notification_view::{line, with_alpha};
use crate::overflow_panel::{OverflowPick, OverflowRow};
use crate::reference_bar::{COLOR_ON_SURFACE, COLOR_PRIMARY};
use gpui::{
    Bounds, Div, ParentElement, Rgba, Styled, canvas, div, point, prelude::FluentBuilder, px, size,
};

fn placed(rect: (f32, f32, f32, f32)) -> Div {
    let (x, y, w, h) = rect;
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}

fn row_element(row: OverflowRow, hovered: Option<OverflowPick>) -> Div {
    let rect = row.rect();
    match row {
        OverflowRow::Header { text, .. } => {
            placed(rect)
                .flex()
                .items_center()
                .child(line(text, COLOR_ON_SURFACE, 13.0, 16.0))
        }
        OverflowRow::Item {
            pick,
            ref label,
            symbol,
            ..
        } => {
            let hot = hovered == Some(pick);
            placed(rect)
                .rounded(px(10.0))
                .when(hot, |d| d.bg(with_alpha(COLOR_ON_SURFACE, 0x1f)))
                .flex()
                .items_center()
                .gap(px(12.0))
                .px(px(10.0))
                .child(
                    if pick == OverflowPick::Module(crate::reference_bar::IdleBarEntry::Power) {
                        crate::identity::session_mark(symbol, 18.0, COLOR_PRIMARY)
                    } else {
                        crate::icons::icon(symbol, 18.0, COLOR_PRIMARY)
                    },
                )
                .child(line(label, COLOR_ON_SURFACE, 13.0, 16.0))
        }
    }
}

/// The panel's full-scene layer: hull, then the sliding content.
pub fn overflow_layer(
    viewport: (f32, f32),
    frame: PopoutFrame,
    rows: Vec<OverflowRow>,
    hovered: Option<OverflowPick>,
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
                    .map(|r| row_element(r, hovered))
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
