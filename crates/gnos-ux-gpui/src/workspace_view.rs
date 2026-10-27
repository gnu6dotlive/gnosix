//! WS-02: the workspace manager painted, on the popouts' material.
//!
//! The hull is the popout's (joined to the Bar), and the content slides with
//! the revealed edge: geometry only, no fade. The active workspace is
//! tinted with the primary colour, the focused window is marked, and the row
//! under the pointer takes a light tint.

use crate::bar_popout_host::PopoutFrame;
use crate::bar_popout_view::paint_popout_hull;
use crate::notification_view::{line, with_alpha};
use crate::reference_bar::{
    COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_PRIMARY, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use crate::workspace_panel::{Carried, WorkspaceEffect, WsRow};
use gpui::{
    Bounds, Div, ImageSource, ObjectFit, ParentElement, RenderImage, Rgba, Styled, StyledImage,
    canvas, div, img, point, prelude::FluentBuilder, px, rgba, size,
};
use std::collections::HashMap;
use std::sync::Arc;

/// WS-04: the live thumbnails taken, by window.
pub type Thumbnails = HashMap<gnosix_desktop_model::SurfaceId, Arc<RenderImage>>;

/// What the panel shows besides its rows: the row under the pointer, a
/// window carried, the thumbnails taken.
pub struct WorkspaceLive {
    pub hovered: Option<WorkspaceEffect>,
    pub carried: Option<Carried>,
    pub thumbs: Thumbnails,
}

fn placed(rect: (f32, f32, f32, f32)) -> Div {
    let (x, y, w, h) = rect;
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}

fn row_element(
    row: WsRow,
    hovered: Option<WorkspaceEffect>,
    target: Option<usize>,
    thumbs: &Thumbnails,
) -> Div {
    let rect = row.rect();
    match row {
        WsRow::Header { text, .. } => {
            placed(rect)
                .flex()
                .items_center()
                .child(line(text, COLOR_ON_SURFACE, 13.0, 16.0))
        }
        WsRow::Note { text, .. } => placed(rect)
            .flex()
            .items_center()
            .overflow_hidden()
            .child(line(&text, COLOR_ON_SURFACE_VARIANT, 11.0, 16.0)),
        WsRow::Card {
            index,
            label,
            active,
            count,
            ..
        } => {
            let hot = hovered == Some(WorkspaceEffect::Activate(index));
            let (bg, fg) = if active {
                (rgba(COLOR_PRIMARY), COLOR_SURFACE_CONTAINER_HIGHEST)
            } else if hot {
                (with_alpha(COLOR_ON_SURFACE, 0x1f), COLOR_ON_SURFACE)
            } else {
                (rgba(COLOR_SURFACE_CONTAINER_HIGHEST), COLOR_ON_SURFACE)
            };
            // WS-02B: where a carried window would land, outlined (the
            // current workspace's card is already in the accent colour).
            placed(rect)
                .rounded(px(10.0))
                .bg(bg)
                .when(target == Some(index), |d| {
                    d.border_2().border_color(rgba(COLOR_ON_SURFACE))
                })
                .flex()
                .items_center()
                .justify_between()
                .px(px(12.0))
                .child(line(&label, fg, 13.0, 16.0))
                .child(line(&count.to_string(), fg, 11.0, 14.0))
        }
        // WS-03: a workspace's miniature: its label above, its box below.
        WsRow::Tile {
            index,
            label,
            active,
            count,
            ..
        } => {
            let hot = hovered == Some(WorkspaceEffect::Activate(index));
            let (_, _, _, h) = rect;
            let fg = if active {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE
            };
            placed(rect)
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(2.0))
                        .child(line(&label, fg, 12.0, 14.0))
                        .child(line(
                            &count.to_string(),
                            COLOR_ON_SURFACE_VARIANT,
                            11.0,
                            14.0,
                        )),
                )
                .child(
                    div()
                        .h(px(h - 18.0))
                        .w_full()
                        .rounded(px(8.0))
                        .bg(if hot {
                            with_alpha(COLOR_ON_SURFACE, 0x1f)
                        } else {
                            rgba(COLOR_SURFACE_CONTAINER_HIGHEST)
                        })
                        .when(active, |d| d.border_2().border_color(rgba(COLOR_PRIMARY)))
                        .when(target == Some(index), |d| {
                            d.border_2().border_color(rgba(COLOR_ON_SURFACE))
                        }),
                )
        }
        // WS-03: a window where it stands on its output.
        WsRow::Mini {
            card,
            index,
            window,
            ..
        } => {
            let hot = hovered == Some(WorkspaceEffect::Focus(card, index));
            let (_, _, w, h) = rect;
            let radius = 4.0_f32.min(w.min(h) / 2.0);
            // WS-04: the window as it looks, its application's mark in a
            // corner; else its silhouette with the mark in the middle.
            if let Some(thumb) = window.surface.and_then(|s| thumbs.get(&s)) {
                let badge = 16.0_f32.min(w.min(h) / 3.0);
                return placed(rect)
                    .rounded(px(radius))
                    .overflow_hidden()
                    .bg(with_alpha(COLOR_ON_SURFACE, 0x22))
                    .child(
                        img(ImageSource::Render(thumb.clone()))
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .w(px(w))
                            .h(px(h))
                            .object_fit(ObjectFit::Cover)
                            .rounded(px(radius)),
                    )
                    .when(hot, |d| {
                        d.child(
                            div()
                                .absolute()
                                .size_full()
                                .bg(with_alpha(COLOR_ON_SURFACE, 0x24)),
                        )
                    })
                    .when(window.focused, |d| {
                        d.border_2().border_color(rgba(COLOR_PRIMARY))
                    })
                    .when(badge >= 10.0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(3.0))
                                .bottom(px(3.0))
                                .size(px(badge + 4.0))
                                .rounded(px(4.0))
                                .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(crate::icons::icon(
                                    window.symbol.clone(),
                                    badge,
                                    if window.focused {
                                        COLOR_PRIMARY
                                    } else {
                                        COLOR_ON_SURFACE
                                    },
                                )),
                        )
                    });
            }
            let fill = if window.focused {
                with_alpha(COLOR_PRIMARY, 0x48)
            } else if hot {
                with_alpha(COLOR_ON_SURFACE, 0x3a)
            } else {
                with_alpha(COLOR_ON_SURFACE, 0x22)
            };
            let glyph = (w.min(h) - 4.0).min(18.0);
            placed(rect)
                .rounded(px(radius))
                .bg(fill)
                .when(window.focused, |d| {
                    d.border_1().border_color(rgba(COLOR_PRIMARY))
                })
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .when(glyph >= 10.0, |d| {
                    d.child(crate::icons::icon(
                        window.symbol.clone(),
                        glyph,
                        if window.focused {
                            COLOR_PRIMARY
                        } else {
                            COLOR_ON_SURFACE
                        },
                    ))
                })
        }
        WsRow::Window {
            card,
            index,
            window,
            ..
        } => {
            let hot = hovered == Some(WorkspaceEffect::Focus(card, index));
            let fg = if window.focused {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE
            };
            placed(rect)
                .rounded(px(8.0))
                .when(hot, |d| d.bg(with_alpha(COLOR_ON_SURFACE, 0x14)))
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(8.0))
                .overflow_hidden()
                .child(crate::icons::icon(window.symbol.clone(), 18.0, fg))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(line(&window.title, fg, 12.0, 16.0)),
                )
        }
    }
}

/// The panel's full-scene layer: hull, then the sliding content.
pub fn workspace_layer(
    viewport: (f32, f32),
    frame: PopoutFrame,
    rows: Vec<WsRow>,
    live: WorkspaceLive,
    material: Rgba,
    own_hull: bool,
) -> Div {
    let WorkspaceLive {
        hovered,
        carried,
        thumbs,
    } = live;
    let scene = Bounds::new(
        point(px(0.0), px(0.0)),
        size(px(viewport.0), px(viewport.1)),
    );
    let hull = canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            if own_hull {
                crate::bar_popout_view::paint_own_hull(scene, &frame, material, window)
            } else {
                paint_popout_hull(scene, &frame, material, window)
            }
        },
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
    let target = carried.as_ref().and_then(|c| c.over);
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
                    .map(|r| row_element(r, hovered, target, &thumbs))
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
    // WS-02B: the carried window follows the pointer, above the panel.
    let ghost = carried.map(|c| {
        div()
            .absolute()
            .left(px(c.at.0 + 12.0))
            .top(px(c.at.1 + 8.0))
            .w(px(220.0))
            .h(px(32.0))
            .rounded(px(8.0))
            .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .overflow_hidden()
            .child(crate::icons::icon(
                c.window.symbol.clone(),
                18.0,
                COLOR_PRIMARY,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(line(&c.window.title, COLOR_ON_SURFACE, 12.0, 16.0)),
            )
    });
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(hull)
        .child(clip)
        .children(ghost)
}
