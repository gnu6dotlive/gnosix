//! UXE-02C: GPUI view of the notification centre.
//!
//! The drawer is one shell-material path on the frame's right edge:
//! - its right corners follow the frame cut-out radius;
//! - concave fillets join its left end to the top and bottom borders;
//! - it is masked at the frame's inner right edge, so while it slides it
//!   emerges from the border instead of covering it.
//!
//! Content is painted at the rectangles `CentreHost` hit-tests. Nothing
//! fades: the drawer slides, groups slide out.

use crate::notification_centre::{
    ACTION_H, AGE_W, BODY_LINE, BODY_PX, CentreFrame, ENTRY_RADIUS, EntryPaint, GROUP_RADIUS,
    GroupPaint, HEADER_H, IMAGE_SIZE, LINE, PAD, SUMMARY_PX,
};
use crate::notification_toasts::ButtonKind;
use crate::notification_view::{chevron_or_cross, line, roles, with_alpha, wrapped};
use crate::reference_bar::{
    COLOR_ERROR, COLOR_ON_PRIMARY, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT,
    COLOR_OUTLINE_VARIANT, COLOR_PRIMARY, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use blobin_ui_core::layout::Rect;
use gnos_notifications::Urgency;
use gpui::{
    Bounds, ContentMask, Div, IntoElement, ParentElement, PathBuilder, Pixels, Rgba, Styled,
    Window, canvas, div, point, px, rgba, size,
};

// Live scheme roles.
const COLOR_SURFACE_CONTAINER_LOW: u32 = 0x1c1b1fff;
const COLOR_SURFACE_CONTAINER_HIGH: u32 = 0x2a292eff;
const COLOR_OUTLINE: u32 = 0x918f9aff;
/// `layer(surfaceContainerHighest, 4)`, approximated one tone up.
const COLOR_ACTION: u32 = 0x403f44ff;

/// The full-height drawer: the envelope's frame-joint outline.
pub fn edge_drawer_path(panel: Rect, on_left: bool) -> Option<gpui::Path<Pixels>> {
    // Envelope grammar: flush with the top and bottom borders and the
    // drawer's own edge.
    crate::product_hull::frame_joint_path(
        panel,
        gnos_ux_surface::grammar::FlushSides {
            top: true,
            right: !on_left,
            bottom: true,
            left: on_left,
        },
    )
}

fn placed(rect: Rect, origin: (f32, f32)) -> Div {
    div()
        .absolute()
        .left(px(rect.x - origin.0))
        .top(px(rect.y - origin.1))
        .w(px(rect.width))
        .h(px(rect.height))
}

fn clear_glyph(colour: u32) -> impl IntoElement {
    canvas(
        |bounds, _, _| bounds,
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
            let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
            let mut path = PathBuilder::stroke(px(2.0));
            for (i, inset) in [0.0f32, 3.0, 6.0].into_iter().enumerate() {
                let y = cy - 6.0 + 6.0 * i as f32;
                path.move_to(point(px(cx - 8.0 + inset), px(y)));
                path.line_to(point(px(cx + 8.0 - inset), px(y)));
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgba(colour));
            }
        },
    )
    .size_full()
}

fn entry_element(entry: EntryPaint, origin: (f32, f32)) -> Div {
    let e = &entry.entry;
    if !entry.expanded {
        // One line: summary, then the body on the same line, elided.
        let body = e.body.replace('\n', " ");
        return placed(entry.rect, origin)
            .flex()
            .items_center()
            .gap(px(8.0))
            .overflow_hidden()
            .child(
                line(&e.summary, COLOR_ON_SURFACE, SUMMARY_PX, LINE)
                    .flex_none()
                    .max_w(px(entry.rect.width * 0.6)),
            )
            .child(line(&body, COLOR_OUTLINE, BODY_PX, LINE).flex_1());
    }
    let inner = entry.rect.width - 2.0 * PAD;
    let summary_h = LINE * entry.summary_lines as f32;
    let mut element = placed(entry.rect, origin)
        .rounded(px(ENTRY_RADIUS))
        .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
        .child(
            div()
                .absolute()
                .left(px(PAD))
                .top(px(PAD))
                .w(px(inner - AGE_W - 8.0))
                .child(wrapped(
                    &e.summary,
                    COLOR_ON_SURFACE,
                    SUMMARY_PX,
                    LINE,
                    entry.summary_lines,
                )),
        )
        .child(
            div()
                .absolute()
                .right(px(PAD))
                .top(px(PAD))
                .w(px(AGE_W))
                .flex()
                .justify_end()
                .child(line(&e.age, COLOR_OUTLINE, BODY_PX, LINE)),
        );
    if entry.body_lines > 0 {
        element = element.child(
            div()
                .absolute()
                .left(px(PAD))
                .top(px(PAD + summary_h + 4.0))
                .w(px(inner))
                .child(wrapped(
                    &e.body,
                    COLOR_OUTLINE,
                    BODY_PX,
                    BODY_LINE,
                    entry.body_lines,
                )),
        );
    }
    let entry_origin = (entry.rect.x, entry.rect.y);
    element.children(entry.buttons.iter().map(|b| {
        let el = placed(b.rect, entry_origin)
            .h(px(ACTION_H))
            .rounded(px(ENTRY_RADIUS))
            .bg(rgba(COLOR_ACTION))
            .flex()
            .items_center()
            .justify_center();
        match &b.kind {
            ButtonKind::Action { label } => {
                el.px(px(PAD))
                    .child(line(label, COLOR_ON_SURFACE_VARIANT, BODY_PX, BODY_LINE))
            }
            other => el.child(chevron_or_cross(other, COLOR_ON_SURFACE_VARIANT)),
        }
    }))
}

fn group_element(group: GroupPaint, origin: (f32, f32), alpha: u8) -> Div {
    let g = &group.group;
    let (_, badge, badge_glyph, _, _) = roles(g.urgency);
    let critical = g.urgency == Urgency::Critical;
    let (pill_bg, pill_fg) = if critical {
        (COLOR_ERROR, crate::notification_view::COLOR_ON_ERROR)
    } else {
        (COLOR_SURFACE_CONTAINER_HIGHEST, COLOR_ON_SURFACE_VARIANT)
    };
    let rect = group.rect;
    let group_origin = (rect.x, rect.y);
    let newest_age = g.entries.first().map(|e| e.age.clone()).unwrap_or_default();
    let header_w = group.pill.x - rect.x - 3.0 * PAD - IMAGE_SIZE;
    let pill = placed(group.pill, group_origin)
        .rounded_full()
        .bg(rgba(pill_bg))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .child(line(&g.entries.len().to_string(), pill_fg, BODY_PX, HEADER_H).flex_none())
        .child(div().w(px(14.0)).h(px(HEADER_H)).child(chevron_or_cross(
            &ButtonKind::Expand {
                expanded: group.expanded,
            },
            pill_fg,
        )));
    placed(rect, origin)
        .rounded(px(GROUP_RADIUS))
        .overflow_hidden()
        .bg(with_alpha(COLOR_SURFACE_CONTAINER_HIGH, alpha))
        .child(
            div()
                .absolute()
                .left(px(PAD))
                .top(px(PAD))
                .w(px(IMAGE_SIZE))
                .h(px(IMAGE_SIZE))
                .rounded_full()
                .bg(rgba(badge))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(18.0))
                .text_color(rgba(badge_glyph))
                .child(g.badge.to_string()),
        )
        .child(
            div()
                .absolute()
                .left(px(2.0 * PAD + IMAGE_SIZE))
                .top(px(PAD))
                .w(px(header_w.max(0.0)))
                .h(px(HEADER_H))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(line(&g.app_name, COLOR_ON_SURFACE_VARIANT, BODY_PX, HEADER_H).flex_1())
                .child(line(&newest_age, COLOR_OUTLINE, BODY_PX, HEADER_H).flex_none()),
        )
        .child(pill)
        .children(
            group
                .entries
                .into_iter()
                .map(|e| entry_element(e, group_origin)),
        )
}

/// Full-scene layer: drawer, container, header, list and clear button.
pub fn centre_layer(viewport: (f32, f32), frame: CentreFrame, material: Rgba) -> Div {
    let panel = frame.panel;
    let clip_right = frame.clip_right;
    let clip_left = frame.clip_left;
    let on_left = frame.on_left;
    let drawer = canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let mask = ContentMask {
                bounds: Bounds::new(
                    point(px(clip_left), px(0.0)),
                    size(px(clip_right - clip_left), px(viewport.1)),
                ),
            };
            window.with_content_mask(Some(mask), |window| {
                // SHELL-FIELD-01C: the shell's field paints the drawer.
                #[cfg(feature = "component-lab")]
                if crate::shell_field_paint::paints(material) {
                    return;
                }
                if let Some(path) = edge_drawer_path(panel, on_left) {
                    window.paint_path(path, material);
                }
            });
        },
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w(px(viewport.0))
    .h(px(viewport.1));
    let alpha = (material.a.clamp(0.0, 1.0) * 255.0).round() as u8;
    let title = match frame.count {
        0 => "Notifications".to_owned(),
        1 => "1 notification".to_owned(),
        n => format!("{n} notifications"),
    };
    let (dnd_bg, dnd_fg) = if frame.do_not_disturb {
        (COLOR_PRIMARY, COLOR_ON_PRIMARY)
    } else {
        (COLOR_SURFACE_CONTAINER_HIGHEST, COLOR_ON_SURFACE_VARIANT)
    };
    let list_origin = (frame.list.x, frame.list.y);
    let list = placed(frame.list, (0.0, 0.0))
        .overflow_hidden()
        .when_empty(frame.count == 0)
        .children(
            frame
                .groups
                .into_iter()
                .map(|g| group_element(g, list_origin, alpha)),
        );
    // Everything beyond the frame's inner edge stays hidden while sliding.
    let visible_lo = panel.x.max(clip_left);
    let visible_w = ((panel.x + panel.width).min(clip_right) - visible_lo).max(0.0);
    let content = div()
        .absolute()
        .left(px(visible_lo))
        .top(px(0.0))
        .w(px(visible_w))
        .h(px(viewport.1))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(px(-visible_lo))
                .top(px(0.0))
                .w(px(viewport.0))
                .h(px(viewport.1))
                .child(
                    placed(frame.container, (0.0, 0.0))
                        .rounded(px(GROUP_RADIUS))
                        .bg(with_alpha(COLOR_SURFACE_CONTAINER_LOW, alpha)),
                )
                .child(
                    placed(frame.title, (0.0, 0.0))
                        .flex()
                        .items_center()
                        .px(px(4.0))
                        .child(line(&title, COLOR_OUTLINE, SUMMARY_PX, LINE)),
                )
                .child(
                    placed(frame.dnd, (0.0, 0.0))
                        .rounded_full()
                        .bg(rgba(dnd_bg))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(line("DND", dnd_fg, BODY_PX, BODY_LINE)),
                )
                .child(list)
                .when_some_clear(frame.clear),
        );
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(drawer)
        .child(content)
}

trait CentreParts {
    fn when_empty(self, empty: bool) -> Self;
    fn when_some_clear(self, clear: Option<Rect>) -> Self;
}

impl CentreParts for Div {
    fn when_empty(self, empty: bool) -> Self {
        if !empty {
            return self;
        }
        self.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(20.0))
                .text_color(rgba(COLOR_OUTLINE_VARIANT))
                .child("All up to date"),
        )
    }

    fn when_some_clear(self, clear: Option<Rect>) -> Self {
        match clear {
            Some(rect) => self.child(
                placed(rect, (0.0, 0.0))
                    .rounded_full()
                    .bg(rgba(COLOR_PRIMARY))
                    .child(clear_glyph(COLOR_ON_PRIMARY)),
            ),
            None => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_drawer_builds_for_full_and_tiny_heights() {
        assert!(edge_drawer_path(Rect::new(840.0, 10.0, 430.0, 780.0), false).is_some());
        assert!(edge_drawer_path(Rect::new(840.0, 10.0, 430.0, 6.0), true).is_some());
        assert!(edge_drawer_path(Rect::new(840.0, 10.0, 430.0, 0.0), false).is_none());
    }
}
