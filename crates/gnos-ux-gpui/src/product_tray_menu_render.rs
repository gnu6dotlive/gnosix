//! GPUI rendering of the typed tray menu surface.
//!
//! Pure presentation: the widget never names a bus address, an interface, a
//! method, or a coordinate. An actionable row carries its immutable
//! [`TrayMenuEventBinding`]; clicking dispatches a typed `MenuEvent` and the
//! transport revalidates it against the current menu before any remote call.

use std::rc::Rc;

use gpui::{App, Div, Stateful, div, prelude::*, px};

use crate::product_design::{
    ProductDesignContext, ProductPalette, ProductRadius, ProductSpacing, ProductTextStyle, text,
};
use crate::product_tray_action::TrayIntent;
use crate::product_tray_menu::{TrayMenuIcon, TrayMenuToggleState, TrayMenuToggleType};
use crate::product_tray_menu_surface::{
    TrayMenuRowPresentation, TrayMenuRowRole, TrayMenuSurfacePresentation,
};

/// The sink a rendered row dispatches its typed intent through.
pub type MenuRowDispatch = Rc<dyn Fn(&mut App, TrayIntent)>;

/// Renders the one open surface.
pub fn render_tray_menu_surface(
    presentation: &TrayMenuSurfacePresentation,
    design: ProductDesignContext,
    dispatch: Option<&MenuRowDispatch>,
) -> Stateful<Div> {
    let rows: Vec<Stateful<Div>> = presentation
        .rows
        .iter()
        .map(|row| render_row(row, design, dispatch, presentation))
        .collect();

    div()
        .id("tray-menu-surface")
        .role(gpui::Role::Group)
        .aria_label("Menu du plateau")
        .flex()
        .flex_col()
        .min_w(px(180.0))
        .max_w(px(320.0))
        .rounded(px(ProductRadius::PANEL))
        .border_1()
        .border_color(ProductPalette::border())
        .bg(ProductPalette::panel_elevated())
        .p(px(ProductSpacing::XXS))
        .children(rows)
}

/// A menu label as read: dbusmenu marks the access key with `_` and writes a
/// literal underscore as `__`.
pub fn without_mnemonics(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
                out.push('_');
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn render_row(
    row: &TrayMenuRowPresentation,
    design: ProductDesignContext,
    dispatch: Option<&MenuRowDispatch>,
    surface: &TrayMenuSurfacePresentation,
) -> Stateful<Div> {
    let indent = row.depth as f32 * ProductSpacing::XS;
    let mut elem = div()
        .id(format!(
            "menu-row-{}-{}",
            surface.item.id.as_str(),
            match row.role {
                TrayMenuRowRole::Standard => row.binding.as_ref().map(|b| b.menu_id).unwrap_or(0),
                TrayMenuRowRole::Separator => -1,
                TrayMenuRowRole::Submenu => -2,
            }
        ))
        .flex()
        .items_center()
        .gap(px(ProductSpacing::XS))
        .px(px(ProductSpacing::XS + indent))
        .py(px(ProductSpacing::XXS))
        .rounded(px(ProductRadius::CONTROL));

    match row.role {
        TrayMenuRowRole::Separator => {
            // A hairline: no padding of its own, or the line grows into a bar.
            elem = elem
                .h(px(1.0))
                .py(px(0.0))
                .my(px(ProductSpacing::XXS))
                .bg(ProductPalette::border());
        }
        TrayMenuRowRole::Standard | TrayMenuRowRole::Submenu => {
            // Icon slot: aligned whether present or absent, so labels line up.
            // A named icon is its Tabler icon (ICONS-01); one Tabler lacks,
            // or a pixmap, leaves the slot empty rather than a placeholder.
            let slot = div().w(px(14.0)).flex_none();
            elem = elem.child(match &row.icon {
                TrayMenuIcon::Named(name) => match crate::icons::from_freedesktop(name) {
                    Some(tabler) => slot.child(crate::icons::icon(
                        tabler,
                        14.0,
                        crate::reference_bar::COLOR_ON_SURFACE_VARIANT,
                    )),
                    None => slot,
                },
                TrayMenuIcon::InlineData { .. } | TrayMenuIcon::None => slot,
            });

            // Toggle / radio state marker.
            if row.role == TrayMenuRowRole::Standard
                && row.toggle_type != TrayMenuToggleType::NotCheckable
            {
                let mark = match (row.toggle_type, row.toggle_state) {
                    (TrayMenuToggleType::Checkable, TrayMenuToggleState::Checked) => "✓",
                    (TrayMenuToggleType::Checkable, TrayMenuToggleState::Inconsistent) => "–",
                    (TrayMenuToggleType::Radio, TrayMenuToggleState::Checked) => "●",
                    _ => "○",
                };
                elem = elem.child(
                    div()
                        .w(px(10.0))
                        .text_color(ProductPalette::accent())
                        .child(mark),
                );
            }

            let label = without_mnemonics(row.label.as_deref().unwrap_or_default());
            elem = elem.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(if row.enabled {
                        ProductPalette::text_primary()
                    } else {
                        ProductPalette::text_muted()
                    })
                    .child(text(label, ProductTextStyle::Body, design)),
            );

            if row.role == TrayMenuRowRole::Submenu || row.has_submenu {
                elem = elem.child(div().text_color(ProductPalette::text_muted()).child("›"));
            }

            // Enabled and standard only: a disabled row does not dispatch, and
            // a hover affordance must not be offered on one.
            if row.role == TrayMenuRowRole::Standard && row.enabled {
                elem = elem.hover(|style| style.bg(ProductPalette::control_hover()));
            }
        }
    }

    // Only an actionable row becomes a real dispatch source. The row carries
    // its own immutable binding, so a later click can never be rebound to a
    // different generation merely because the layout moved.
    match dispatch {
        Some(dispatch) if row.is_actionable() => {
            let binding = row.binding.clone().expect("actionable rows bind");
            let sink = dispatch.clone();
            // CURSOR-01: a row a press acts on says so.
            elem.cursor(gpui::CursorStyle::PointingHand)
                .on_click(move |click, _window, cx| {
                    let intent = TrayIntent::MenuEvent {
                        binding: binding.clone(),
                        anchor:
                            crate::product_tray_action::TrayInteractionAnchor::from_local_point(
                                (0, 0),
                                (f32::from(click.position().x), f32::from(click.position().y)),
                            )
                            .unwrap_or(
                                crate::product_tray_action::TrayInteractionAnchor {
                                    output_origin: (0, 0),
                                    local: (0.0, 0.0),
                                    screen: (0, 0),
                                },
                            ),
                    };
                    sink(cx, intent);
                })
        }
        _ => elem,
    }
}

#[cfg(test)]
mod mnemonic_tests {
    use super::without_mnemonics;

    #[test]
    fn access_keys_are_not_shown_and_a_doubled_underscore_is_one() {
        assert_eq!(
            without_mnemonics("Turn Bluetooth _Off"),
            "Turn Bluetooth Off"
        );
        assert_eq!(without_mnemonics("_Devices..."), "Devices...");
        assert_eq!(without_mnemonics("snake__case"), "snake_case");
        assert_eq!(without_mnemonics("plain"), "plain");
    }
}
