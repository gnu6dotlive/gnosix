//! UXE-02B: GPUI view of the notification toast drawer.
//!
//! Paints exactly what `ToastHost` resolved. The drawer is shell material:
//! - it meets the frame's top and right inner edges;
//! - its top-right corner follows the frame's own cut-out radius, so drawer
//!   and frame read as one surface without painting the frame twice;
//! - concave fillets join its free ends to the frame (N09).
//!
//! Cards sit on it at the rectangles the router hit-tests. Motion is
//! geometric only; nothing fades.

use crate::notification_toasts::{
    APP_LINE, BODY_LINE, BODY_PX, ButtonKind, ButtonPaint, CARD_PADDING, CARD_RADIUS, CardPaint,
    IMAGE_SIZE, SUMMARY_LINE, SUMMARY_PX, TEXT_X, ToastFrame, text_width,
};
use crate::reference_bar::{
    COLOR_ERROR, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_PRIMARY,
    COLOR_SURFACE_CONTAINER, COLOR_SURFACE_CONTAINER_HIGHEST,
};
use blobin_ui_core::layout::Rect;
use gnos_notifications::Urgency;
use gpui::{
    Bounds, Div, IntoElement, ParentElement, PathBuilder, Pixels, Rgba, Styled, Window, canvas,
    div, point, px, rgba,
};

// Live scheme roles the Bar palette does not carry yet.
const COLOR_SECONDARY: u32 = 0xc6c4e0ff;
const COLOR_ON_SECONDARY: u32 = 0x2e2e44ff;
pub(crate) const COLOR_SECONDARY_CONTAINER: u32 = 0x45455cff;
pub(crate) const COLOR_ON_SECONDARY_CONTAINER: u32 = 0xb4b2ceff;
pub(crate) const COLOR_ON_ERROR: u32 = 0x690005ff;
const STROKE: f32 = 2.0;
const GLYPH: f32 = 10.0;

pub(crate) fn with_alpha(colour: u32, alpha: u8) -> Rgba {
    rgba((colour & 0xffff_ff00) | u32::from(alpha))
}

/// The drawer as one closed path, traversed clockwise.
pub fn corner_drawer_path(
    panel: Rect,
    on_left: bool,
    on_bottom: bool,
) -> Option<gpui::Path<Pixels>> {
    // Envelope grammar: flush with the anchor border (top, or bottom under a
    // top Bar) and the drawer's edge.
    crate::product_hull::frame_joint_path(
        panel,
        gnos_ux_surface::grammar::FlushSides {
            top: !on_bottom,
            bottom: on_bottom,
            right: !on_left,
            left: on_left,
        },
    )
}

/// Urgency roles: (card, badge, badge glyph, button, button label).
pub(crate) fn roles(urgency: Urgency) -> (u32, u32, u32, u32, u32) {
    match urgency {
        Urgency::Critical => (
            COLOR_SECONDARY_CONTAINER,
            COLOR_ERROR,
            COLOR_ON_ERROR,
            COLOR_SECONDARY,
            COLOR_ON_SECONDARY,
        ),
        Urgency::Low => (
            COLOR_SURFACE_CONTAINER,
            COLOR_SURFACE_CONTAINER_HIGHEST,
            COLOR_ON_SURFACE,
            COLOR_SURFACE_CONTAINER_HIGHEST,
            COLOR_ON_SURFACE_VARIANT,
        ),
        Urgency::Normal => (
            COLOR_SURFACE_CONTAINER,
            COLOR_SECONDARY_CONTAINER,
            COLOR_ON_SECONDARY_CONTAINER,
            COLOR_SURFACE_CONTAINER_HIGHEST,
            COLOR_ON_SURFACE_VARIANT,
        ),
    }
}

pub(crate) fn line(text: &str, colour: u32, size_px: f32, line_px: f32) -> Div {
    div()
        .min_w_0()
        .text_size(px(size_px))
        .line_height(px(line_px))
        .text_color(rgba(colour))
        .truncate()
        .child(text.to_owned())
}

pub(crate) fn wrapped(text: &str, colour: u32, size_px: f32, line_px: f32, lines: usize) -> Div {
    div()
        .w_full()
        .h(px(line_px * lines as f32))
        .overflow_hidden()
        .text_size(px(size_px))
        .line_height(px(line_px))
        .text_color(rgba(colour))
        .child(text.to_owned())
}

/// Summary or app name, then "•" and the age, on one elided line.
fn title_row(title: &str, title_px: f32, age: &str, line_px: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .w_full()
        .h(px(line_px))
        .child(line(title, COLOR_ON_SURFACE, title_px, line_px).flex_shrink(1.0))
        .child(line("•", COLOR_ON_SURFACE_VARIANT, BODY_PX, line_px).flex_none())
        .child(line(age, COLOR_ON_SURFACE_VARIANT, BODY_PX, line_px).flex_none())
}

pub(crate) fn chevron_or_cross(kind: &ButtonKind, colour: u32) -> impl IntoElement {
    let kind = kind.clone();
    canvas(
        |bounds, _, _| bounds,
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
            let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
            let h = GLYPH / 2.0;
            let mut path = PathBuilder::stroke(px(STROKE));
            match kind {
                ButtonKind::Expand { expanded } => {
                    let dy = if expanded { -h / 2.0 } else { h / 2.0 };
                    path.move_to(point(px(cx - h), px(cy - dy)));
                    path.line_to(point(px(cx), px(cy + dy)));
                    path.line_to(point(px(cx + h), px(cy - dy)));
                }
                _ => {
                    path.move_to(point(px(cx - h * 0.8), px(cy - h * 0.8)));
                    path.line_to(point(px(cx + h * 0.8), px(cy + h * 0.8)));
                    path.move_to(point(px(cx + h * 0.8), px(cy - h * 0.8)));
                    path.line_to(point(px(cx - h * 0.8), px(cy + h * 0.8)));
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgba(colour));
            }
        },
    )
    .size_full()
}

fn progress_ring(progress: u8) -> impl IntoElement {
    canvas(
        |bounds, _, _| bounds,
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let r = f32::from(bounds.size.width) / 2.0 + STROKE;
            let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
            let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
            let sweep = f32::from(progress.min(100)) / 100.0 * std::f32::consts::TAU;
            let steps = ((sweep / std::f32::consts::TAU) * 64.0).ceil().max(1.0) as usize;
            let mut path = PathBuilder::stroke(px(STROKE));
            for step in 0..=steps {
                let a = -std::f32::consts::FRAC_PI_2 + sweep * step as f32 / steps as f32;
                let at = point(px(cx + r * a.cos()), px(cy + r * a.sin()));
                if step == 0 {
                    path.move_to(at);
                } else {
                    path.line_to(at);
                }
            }
            if progress > 0
                && let Ok(path) = path.build()
            {
                window.paint_path(path, rgba(COLOR_PRIMARY));
            }
        },
    )
    .absolute()
    .left(px(CARD_PADDING))
    .top(px(CARD_PADDING))
    .w(px(IMAGE_SIZE))
    .h(px(IMAGE_SIZE))
}

fn button(paint: &ButtonPaint, origin: (f32, f32), urgency: Urgency) -> Div {
    let (_, _, _, fill, on_fill) = roles(urgency);
    let placed = div()
        .absolute()
        .left(px(paint.rect.x - origin.0))
        .top(px(paint.rect.y - origin.1))
        .w(px(paint.rect.width))
        .h(px(paint.rect.height));
    match &paint.kind {
        ButtonKind::Expand { .. } => placed
            .rounded_full()
            .child(chevron_or_cross(&paint.kind, COLOR_ON_SURFACE)),
        ButtonKind::Close => placed
            .rounded_full()
            .bg(rgba(fill))
            .child(chevron_or_cross(&paint.kind, on_fill)),
        ButtonKind::Action { label } => placed
            .rounded_full()
            .bg(rgba(fill))
            .flex()
            .items_center()
            .justify_center()
            .px(px(CARD_PADDING))
            .child(line(label, on_fill, BODY_PX, BODY_LINE)),
    }
}

fn card(paint: CardPaint, clip_origin: (f32, f32), alpha: u8) -> Div {
    let c = &paint.card;
    let (card_colour, badge, badge_glyph, _, _) = roles(c.urgency);
    let rect = paint.rect;
    let text_w = text_width(rect.width);
    let badge_el = div()
        .absolute()
        .left(px(CARD_PADDING))
        .top(px(CARD_PADDING))
        .w(px(IMAGE_SIZE))
        .h(px(IMAGE_SIZE))
        .rounded_full()
        .bg(rgba(badge))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(18.0))
        .text_color(rgba(badge_glyph))
        .child(c.badge.to_string());
    let text = if paint.expanded {
        let summary_h = SUMMARY_LINE * paint.summary_lines as f32;
        div()
            .absolute()
            .left(px(TEXT_X))
            .top(px(CARD_PADDING))
            .w(px(text_w))
            .flex()
            .flex_col()
            .child(title_row(&c.app_name, BODY_PX, &c.age, APP_LINE))
            .child(div().h(px(4.0)))
            .child(
                wrapped(
                    &c.summary,
                    COLOR_ON_SURFACE,
                    SUMMARY_PX,
                    SUMMARY_LINE,
                    paint.summary_lines,
                )
                .h(px(summary_h)),
            )
            .child(div().h(px(4.0)))
            .when_some_body(paint.body_lines, &c.body)
    } else {
        div()
            .absolute()
            .left(px(TEXT_X))
            .top(px(CARD_PADDING))
            .w(px(text_w))
            .flex()
            .flex_col()
            .child(title_row(&c.summary, SUMMARY_PX, &c.age, SUMMARY_LINE))
            .child(line(&c.body, COLOR_ON_SURFACE_VARIANT, BODY_PX, BODY_LINE))
    };
    let origin = (rect.x, rect.y);
    let mut element = div()
        .absolute()
        .left(px(rect.x - clip_origin.0))
        .top(px(rect.y - clip_origin.1))
        .w(px(rect.width))
        .h(px(rect.height))
        .rounded(px(CARD_RADIUS))
        .overflow_hidden()
        .bg(with_alpha(card_colour, alpha))
        .child(badge_el)
        .child(text);
    if let Some(progress) = c.progress {
        element = element.child(progress_ring(progress));
    }
    element.children(paint.buttons.iter().map(|b| button(b, origin, c.urgency)))
}

trait BodyBlock {
    fn when_some_body(self, lines: usize, body: &str) -> Self;
}

impl BodyBlock for Div {
    fn when_some_body(self, lines: usize, body: &str) -> Self {
        if lines == 0 {
            self
        } else {
            self.child(wrapped(
                body,
                COLOR_ON_SURFACE_VARIANT,
                BODY_PX,
                BODY_LINE,
                lines,
            ))
        }
    }
}

/// Full-scene layer: drawer canvas plus clipped cards. `material` is the
/// shell surface colour (its alpha is the shell transparency).
pub fn toast_layer(viewport: (f32, f32), frame: ToastFrame, material: Rgba) -> Div {
    let panel = frame.panel;
    let on_left = frame.on_left;
    let on_bottom = frame.on_bottom;
    let drawer = canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            // SHELL-FIELD-01C: the shell's field paints the drawer.
            #[cfg(feature = "component-lab")]
            if crate::shell_field_paint::paints(material) {
                return;
            }
            if let Some(path) = corner_drawer_path(panel, on_left, on_bottom) {
                window.paint_path(path, material);
            }
        },
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w(px(viewport.0))
    .h(px(viewport.1));
    let alpha = (material.a.clamp(0.0, 1.0) * 255.0).round() as u8;
    let clip_origin = (frame.clip.x, frame.clip.y);
    let clip = div()
        .absolute()
        .left(px(frame.clip.x))
        .top(px(frame.clip.y))
        .w(px(frame.clip.width))
        .h(px(frame.clip.height))
        .overflow_hidden()
        .children(frame.cards.into_iter().map(|c| card(c, clip_origin, alpha)));
    // Zero-size anchor: the layer owns no hitbox over the scene.
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(drawer)
        .child(clip)
}

/// GPUI's own line wrapper, so measured heights match painted text.
pub struct WindowTextMeasure<'a>(pub &'a Window);

impl crate::notification_toasts::TextMeasure for WindowTextMeasure<'_> {
    fn lines(&mut self, text: &str, font_px: f32, width: f32) -> usize {
        let font = self.0.text_style().font();
        let mut wrapper = self.0.text_system().line_wrapper(font, px(font_px));
        text.split('\n')
            .map(|paragraph| {
                1 + wrapper
                    .wrap_line(&[gpui::LineFragment::text(paragraph)], px(width))
                    .count()
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_path_exists_only_with_area() {
        assert!(corner_drawer_path(Rect::new(840.0, 10.0, 430.0, 0.5), false, false).is_none());
        assert!(corner_drawer_path(Rect::new(840.0, 10.0, 430.0, 94.0), true, false).is_some());
        assert!(corner_drawer_path(Rect::new(840.0, 696.0, 430.0, 94.0), false, true).is_some());
        // A very short drawer still builds (radii clamp to its height).
        assert!(corner_drawer_path(Rect::new(840.0, 10.0, 430.0, 8.0), false, false).is_some());
    }

    #[test]
    fn critical_cards_use_the_secondary_container() {
        assert_eq!(roles(Urgency::Critical).0, COLOR_SECONDARY_CONTAINER);
        assert_eq!(roles(Urgency::Normal).0, COLOR_SURFACE_CONTAINER);
        assert_eq!(roles(Urgency::Critical).1, COLOR_ERROR);
    }
}
