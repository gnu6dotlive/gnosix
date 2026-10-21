//! UXE-05B: GPUI view of the Settings surface.
//!
//! The panel is the envelope's fused hull: its revealed part joined to the
//! top border (or a top or bottom Bar) with concave joints, painted in shell
//! material and masked at that inner edge so it extrudes out of it. Navigation,
//! title, close control and page lines are painted at the rectangles
//! `SettingsHost` hit-tests. Lines shared with the Bar popouts reuse their
//! row painter.

use crate::bar_status_popout_view::info_row_element;
use crate::bar_status_popouts::{InfoRow, InfoRowKind};
use crate::notification_toasts::ButtonKind;
use crate::notification_view::{chevron_or_cross, line, with_alpha};
use crate::reference_bar::{
    COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY,
};
use crate::settings_pages::SettingsLine;
use crate::settings_panel::{LinePaint, SettingsFrame, Target};
use blobin_ui_core::layout::Rect;
use gnos_i18n::t;
use gpui::prelude::FluentBuilder;
use gpui::{
    Bounds, ContentMask, Div, InteractiveElement, ParentElement, Rgba, Styled, canvas, div, point,
    px, rgba, size,
};

const COLOR_OUTLINE: u32 = 0x918f9aff;
/// SET-04: the state layer over a hovered control, and over a held one.
const HOVER_ALPHA: u8 = 0x14;
const PRESSED_ALPHA: u8 = 0x29;

/// SET-04: the state layer for the control under the pointer.
fn state_layer(rect: Rect, origin: (f32, f32), pressed: bool) -> Div {
    placed(rect, origin).rounded_full().bg(with_alpha(
        COLOR_ON_SURFACE,
        if pressed { PRESSED_ALPHA } else { HOVER_ALPHA },
    ))
}

fn placed(rect: Rect, origin: (f32, f32)) -> Div {
    div()
        .absolute()
        .left(px(rect.x - origin.0))
        .top(px(rect.y - origin.1))
        .w(px(rect.width))
        .h(px(rect.height))
}

fn line_element(paint: LinePaint, origin: (f32, f32)) -> Div {
    let rect = paint.rect;
    let as_row = |kind: InfoRowKind, label: String| {
        info_row_element(InfoRow {
            kind,
            label,
            x: rect.x - origin.0,
            y: rect.y - origin.1,
            width: rect.width,
            height: rect.height,
        })
    };
    match paint.line {
        SettingsLine::Header(text) => as_row(InfoRowKind::Header, text),
        SettingsLine::Caption(text) => as_row(InfoRowKind::Caption, text),
        SettingsLine::State { label, on } => as_row(InfoRowKind::State { on }, label),
        SettingsLine::Info { kind, label } => as_row(kind, label),
        SettingsLine::Value { label, value } => placed(rect, origin)
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(line(&label, COLOR_ON_SURFACE, 13.0, 18.0).flex_1())
            .child(line(&value, COLOR_ON_SURFACE_VARIANT, 13.0, 18.0).flex_none()),
        SettingsLine::Choice { label, value, .. } => {
            let (button, _) = paint.buttons[0];
            placed(rect, origin)
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .h(px(rect.height))
                        .w(px(rect.width - button.width - 12.0))
                        .flex()
                        .items_center()
                        .child(line(&label, COLOR_ON_SURFACE, 13.0, 18.0)),
                )
                .child(
                    placed(button, (rect.x, rect.y))
                        .rounded_full()
                        .bg(with_alpha(COLOR_ON_SURFACE, 0x14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(line(&value, COLOR_PRIMARY, 13.0, 18.0)),
                )
        }
        SettingsLine::Stepper { label, value, .. } => {
            let (down, _) = paint.buttons[0];
            let (up, _) = paint.buttons[1];
            let step = |r: Rect, glyph: &str| {
                placed(r, (rect.x, rect.y))
                    .rounded_full()
                    .bg(with_alpha(COLOR_ON_SURFACE, 0x14))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(line(glyph, COLOR_PRIMARY, 15.0, 18.0))
            };
            placed(rect, origin)
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .h(px(rect.height))
                        .w(px(down.x - rect.x - 12.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(line(&label, COLOR_ON_SURFACE, 13.0, 18.0).flex_1())
                        .child(line(&value, COLOR_PRIMARY, 13.0, 18.0).flex_none()),
                )
                .child(step(down, "−"))
                .child(step(up, "+"))
        }
        SettingsLine::Module {
            icon,
            label,
            detail,
            place,
            ..
        } => {
            // MODS-01B: icon, name over what it is; place and arrows, or the
            // place alone when the profile cannot be written.
            let text_right = paint
                .buttons
                .first()
                .map_or(rect.width, |(b, _)| b.x - rect.x - 12.0);
            let mut row = placed(rect, origin).child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .h(px(rect.height))
                    .w(px(text_right))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(crate::icons::icon(icon, 20.0, COLOR_ON_SURFACE_VARIANT))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(line(&label, COLOR_ON_SURFACE, 13.0, 18.0))
                            .child(line(&detail, COLOR_ON_SURFACE_VARIANT, 11.0, 15.0)),
                    )
                    .when(paint.buttons.is_empty(), |d| {
                        d.child(line(&place, COLOR_ON_SURFACE_VARIANT, 13.0, 18.0).flex_none())
                    }),
            );
            let button = |r: Rect| {
                placed(r, (rect.x, rect.y))
                    .rounded_full()
                    .bg(with_alpha(COLOR_ON_SURFACE, 0x14))
                    .flex()
                    .items_center()
                    .justify_center()
            };
            if let [(place_rect, _), (earlier, _), (later, _)] = paint.buttons[..] {
                row = row
                    .child(button(place_rect).child(line(&place, COLOR_PRIMARY, 13.0, 18.0)))
                    .child(button(earlier).child(crate::icons::icon(
                        "chevron-up",
                        16.0,
                        COLOR_PRIMARY,
                    )))
                    .child(button(later).child(crate::icons::icon(
                        "chevron-down",
                        16.0,
                        COLOR_PRIMARY,
                    )));
            }
            row
        }
        SettingsLine::UnderConstruction { reason } => placed(rect, origin)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .child(line(
                t("Page under construction"),
                COLOR_OUTLINE_VARIANT,
                20.0,
                26.0,
            ))
            .child(line(&reason, COLOR_OUTLINE, 13.0, 18.0)),
    }
}

/// Full-scene layer: hull, title, close control, navigation and page.
pub fn settings_layer(
    viewport: (f32, f32),
    frame: SettingsFrame,
    material: Rgba,
) -> gpui::Stateful<Div> {
    let hull = frame.hull;
    let clip_top = frame.clip_top;
    let clip_bottom = frame.clip_bottom;
    let scene = Bounds::new(
        point(px(0.0), px(0.0)),
        size(px(viewport.0), px(viewport.1)),
    );
    let drawer = canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let mask = ContentMask {
                bounds: Bounds::new(
                    point(px(0.0), px(clip_top)),
                    size(px(viewport.0), px((clip_bottom - clip_top).max(0.0))),
                ),
            };
            window.with_content_mask(Some(mask), |window| {
                // SHELL-FIELD-01C: the shell's field paints the panel.
                #[cfg(feature = "component-lab")]
                if crate::shell_field_paint::paints(material) {
                    return;
                }
                if let Some(hull) = hull {
                    window.paint_path(
                        crate::product_hull::connective_hull_path(scene, hull),
                        material,
                    );
                }
            });
        },
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w(px(viewport.0))
    .h(px(viewport.1));
    let nav = frame.nav.into_iter().map(|item| {
        if let Some(category) = item.category {
            return placed(item.rect, (0.0, 0.0))
                .flex()
                .items_end()
                .px(px(12.0))
                .child(line(category.label(), COLOR_OUTLINE, 12.0, 16.0));
        }
        let page = item.page.expect("a nav item is a category or a page");
        let element = placed(item.rect, (0.0, 0.0))
            .rounded_full()
            .flex()
            .items_center()
            .px(px(12.0))
            .child(line(
                page.title(),
                if item.current {
                    COLOR_PRIMARY
                } else {
                    COLOR_ON_SURFACE
                },
                13.0,
                18.0,
            ));
        if item.current {
            element.bg(with_alpha(COLOR_ON_SURFACE, 0x14))
        } else {
            element
        }
    });
    let focus_ring = frame.focused.map(|(rect, _)| {
        placed(
            Rect::new(
                rect.x - 3.0,
                rect.y - 3.0,
                rect.width + 6.0,
                rect.height + 6.0,
            ),
            (0.0, 0.0),
        )
        .rounded_full()
        .border_2()
        .border_color(rgba(COLOR_PRIMARY))
    });
    let area = frame.page_area;
    let pressed = frame.pressed;
    // A page control's layer is clipped with the page; the others ride the
    // panel.
    let (page_state, panel_state) = match frame.hovered {
        Some(Target::Button(r, _)) => (Some(state_layer(r, (area.x, area.y), pressed)), None),
        Some(t) => (None, Some(state_layer(t.rect(), (0.0, 0.0), pressed))),
        None => (None, None),
    };
    let page = placed(area, (0.0, 0.0))
        .overflow_hidden()
        .children(
            frame
                .lines
                .into_iter()
                .map(|line| line_element(line, (area.x, area.y))),
        )
        .children(page_state);
    let content = div()
        .absolute()
        .left(px(0.0))
        .top(px(clip_top))
        .w(px(viewport.0))
        .h(px((clip_bottom - clip_top).max(0.0)))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(-clip_top))
                .w(px(viewport.0))
                .h(px(viewport.1))
                .child(
                    placed(frame.title, (0.0, 0.0))
                        .flex()
                        .items_center()
                        .child(line(frame.page.title(), COLOR_ON_SURFACE, 18.0, 24.0)),
                )
                .child(
                    placed(frame.close, (0.0, 0.0))
                        .rounded_full()
                        .bg(with_alpha(COLOR_ON_SURFACE, 0x14))
                        .child(chevron_or_cross(
                            &ButtonKind::Close,
                            COLOR_ON_SURFACE_VARIANT,
                        )),
                )
                .children(nav)
                .children(panel_state)
                .child(page)
                .children(focus_ring),
        );
    div()
        .id("settings-surface")
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(drawer)
        .child(content)
}
