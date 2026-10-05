//! GNOS.UX product design tokens and reusable GPUI primitives.
//!
//! Numeric/material values in this module are PRODUCT-TOOLING-01 candidate
//! defaults. Centralizing them does not ratify them as persistent user settings
//! or final release visuals.

use std::time::Duration;

use gpui::{
    Div, ElementId, FocusHandle, FontWeight, InteractiveElement, Rgba, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::*, px, rgba,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProductDesignContext {
    text_scale: f32,
    reduced_motion: bool,
}

impl Default for ProductDesignContext {
    fn default() -> Self {
        Self {
            text_scale: 1.0,
            reduced_motion: false,
        }
    }
}

impl ProductDesignContext {
    pub fn new(text_scale: f32, reduced_motion: bool) -> Option<Self> {
        (text_scale.is_finite() && (1.0..=2.0).contains(&text_scale)).then_some(Self {
            text_scale,
            reduced_motion,
        })
    }

    pub const fn text_scale(self) -> f32 {
        self.text_scale
    }

    pub const fn reduced_motion(self) -> bool {
        self.reduced_motion
    }

    pub fn scaled_px(self, base: f32) -> f32 {
        base * self.text_scale
    }
}

pub struct ProductPalette;

impl ProductPalette {
    pub fn canvas() -> Rgba {
        rgba(0x0d1218ff)
    }

    pub fn panel() -> Rgba {
        rgba(0x172029f7)
    }

    pub fn panel_elevated() -> Rgba {
        rgba(0x162733ee)
    }

    pub fn control() -> Rgba {
        rgba(0x162733cc)
    }

    pub fn control_hover() -> Rgba {
        rgba(0x1c3445e6)
    }

    pub fn control_pressed() -> Rgba {
        rgba(0x24465aff)
    }

    pub fn focus_ring() -> Rgba {
        rgba(0x8bdde7ff)
    }

    pub fn control_selected() -> Rgba {
        rgba(0x315f79ff)
    }

    pub fn border() -> Rgba {
        rgba(0xffffff33)
    }

    pub fn border_selected() -> Rgba {
        rgba(0xb7eef4aau32)
    }

    pub fn text_primary() -> Rgba {
        rgba(0xffffffff)
    }

    pub fn text_secondary() -> Rgba {
        rgba(0xd8e9efff)
    }

    pub fn text_muted() -> Rgba {
        rgba(0xa4b7c0ff)
    }

    pub fn accent() -> Rgba {
        rgba(0x8bdde7ff)
    }

    pub fn danger() -> Rgba {
        rgba(0xe47d7dff)
    }

    pub fn warning() -> Rgba {
        rgba(0xe7bf72ff)
    }
}

pub struct ProductSpacing;

impl ProductSpacing {
    pub const XXS: f32 = 4.0;
    pub const XS: f32 = 8.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 16.0;
    pub const LG: f32 = 20.0;
    pub const XL: f32 = 28.0;
}

pub struct ProductRadius;

impl ProductRadius {
    pub const CONTROL: f32 = 10.0;
    pub const PANEL: f32 = 14.0;
    pub const BADGE: f32 = 999.0;
}

pub struct ProductStroke;

impl ProductStroke {
    pub const STANDARD: f32 = 1.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductTextStyle {
    Caption,
    Body,
    Label,
    Title,
}

impl ProductTextStyle {
    const fn base_size(self) -> f32 {
        match self {
            Self::Caption => 11.0,
            Self::Body | Self::Label => 14.0,
            Self::Title => 20.0,
        }
    }

    const fn weight(self) -> FontWeight {
        match self {
            Self::Caption | Self::Body => FontWeight::NORMAL,
            Self::Label => FontWeight::SEMIBOLD,
            Self::Title => FontWeight::BOLD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductControlState {
    Default,
    Selected,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductStatusTone {
    Neutral,
    Accent,
    Warning,
    Danger,
}

pub struct ProductMotion;

impl ProductMotion {
    pub fn quick(context: ProductDesignContext) -> Duration {
        if context.reduced_motion() {
            Duration::ZERO
        } else {
            Duration::from_millis(120)
        }
    }

    pub fn standard(context: ProductDesignContext) -> Duration {
        if context.reduced_motion() {
            Duration::ZERO
        } else {
            Duration::from_millis(180)
        }
    }
}

pub struct ProductIconSize;

impl ProductIconSize {
    pub const SMALL: f32 = 14.0;
    pub const STANDARD: f32 = 18.0;
    pub const LARGE: f32 = 24.0;
}

pub fn text(
    content: impl Into<SharedString>,
    style: ProductTextStyle,
    context: ProductDesignContext,
) -> Div {
    div()
        .text_size(px(context.scaled_px(style.base_size())))
        .font_weight(style.weight())
        .text_color(match style {
            ProductTextStyle::Caption => ProductPalette::text_secondary(),
            ProductTextStyle::Body | ProductTextStyle::Label | ProductTextStyle::Title => {
                ProductPalette::text_primary()
            }
        })
        .child(content.into())
}

pub fn icon(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    accessibility_label: impl Into<SharedString>,
    context: ProductDesignContext,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(gpui::Role::Image)
        .aria_label(accessibility_label.into())
        .text_size(px(context.scaled_px(ProductIconSize::STANDARD)))
        .text_color(ProductPalette::text_secondary())
        .child(glyph.into())
}

/// The native accessibility semantics a control must publish.
///
/// This is the single decision point shared by the styling path and the
/// accessibility path, so a control cannot be visually one thing and
/// semantically another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProductControlSemantics {
    pub selected: Option<bool>,
    pub focusable: bool,
    pub tab_stop: bool,
}

pub const fn control_semantics(state: ProductControlState) -> ProductControlSemantics {
    let disabled = matches!(state, ProductControlState::Disabled);
    ProductControlSemantics {
        selected: if disabled {
            None
        } else {
            Some(matches!(state, ProductControlState::Selected))
        },
        focusable: !disabled,
        tab_stop: !disabled,
    }
}

/// The focus target a control must own to be genuinely keyboard reachable.
///
/// A control identity owns exactly one [`FocusHandle`] for its whole life.
/// The handle is keyed by stable product identity, never by render order or
/// display label, so an unrelated semantic generation cannot move keyboard
/// focus onto a different control.
///
/// A `None` handle means the control is not actionable, and the element must
/// not advertise `Focusable` at all rather than advertising a focus target
/// that does not exist.
#[derive(Debug, Default)]
pub struct ProductControlFocus<'a> {
    handle: Option<&'a FocusHandle>,
    tab_index: Option<isize>,
}

impl<'a> ProductControlFocus<'a> {
    pub const fn inert() -> Self {
        Self {
            handle: None,
            tab_index: None,
        }
    }

    pub const fn new(handle: &'a FocusHandle, tab_index: isize) -> Self {
        Self {
            handle: Some(handle),
            tab_index: Some(tab_index),
        }
    }

    pub const fn handle(&self) -> Option<&'a FocusHandle> {
        self.handle
    }

    pub const fn is_focusable(&self) -> bool {
        self.handle.is_some()
    }
}

/// Whether a product control is currently inert.
///
/// A control in this state must not be reachable by keyboard traversal or
/// assistive activation, and callers must not attach a click listener.
///
/// GPUI 0.3.5 exposes no `aria_disabled`, and AccessKit therefore always
/// reports `enabled`/`sensitive` for a `div`. The invariant is enforced by
/// removing the action and the tab stop, which is what assistive technology
/// actually uses to decide reachability. The residual `enabled` bit is a
/// platform limitation, recorded rather than papered over with a second
/// accessibility protocol.
pub const fn product_control_is_inert(state: ProductControlState) -> bool {
    matches!(state, ProductControlState::Disabled)
}

/// Describe a control identically for eyes, pointer, keyboard, assistive
/// technology and the typed product action path.
pub fn control_accessibility(
    control: Stateful<Div>,
    accessibility_label: SharedString,
    state: ProductControlState,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let semantics = control_semantics(state);
    let control = control
        .role(gpui::Role::Button)
        .aria_label(accessibility_label);
    let control = match semantics.selected {
        Some(selected) => control.aria_selected(selected),
        None => control,
    };

    // A control is keyboard reachable only when it is not inert, owns a
    // focus target, and binds it to this element.
    //
    // gpui-pre auto-allocates a handle for any `focusable()` element, keyed
    // by element id; `track_focus` overrides that with a product-owned
    // handle so focus identity follows the product's own stable identity
    // rather than a render id. Binding a handle that the caller owns is what
    // lets a control keep its focus identity across semantic generations.
    // A control with no handle at all must not claim `Focusable`.
    match (focus.handle(), semantics.focusable && semantics.tab_stop) {
        (Some(handle), true) => {
            let control = control.track_focus(handle);
            match focus.tab_index {
                Some(index) => control.tab_stop(true).tab_index(index),
                None => control.tab_stop(true),
            }
        }
        _ => control.tab_stop(false),
    }
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    accessibility_label: impl Into<SharedString>,
    state: ProductControlState,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let selected = state == ProductControlState::Selected;
    let disabled = state == ProductControlState::Disabled;
    let control = div()
        .id(id)
        .h(px(36.0))
        .px(px(ProductSpacing::SM))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ProductRadius::CONTROL))
        .border_1()
        .border_color(if selected {
            ProductPalette::border_selected()
        } else {
            ProductPalette::border()
        })
        .bg(if selected {
            ProductPalette::control_selected()
        } else {
            ProductPalette::control()
        })
        .when(!disabled, |control| {
            control
                .hover(|style| style.bg(ProductPalette::control_hover()))
                .active(|style| style.bg(ProductPalette::control_pressed()))
        })
        .when(disabled, |control| control.opacity(0.48))
        .child(
            text(label, ProductTextStyle::Label, context)
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis(),
        );
    control_accessibility(control, accessibility_label.into(), state, focus)
}

pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    accessibility_label: impl Into<SharedString>,
    state: ProductControlState,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let label = accessibility_label.into();
    button(id, glyph, label, state, context, focus)
        .w(px(36.0))
        .px(px(0.0))
}

pub fn selectable_item(
    id: impl Into<ElementId>,
    primary: impl Into<SharedString>,
    secondary: Option<SharedString>,
    accessibility_label: impl Into<SharedString>,
    state: ProductControlState,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let selected = state == ProductControlState::Selected;
    let disabled = state == ProductControlState::Disabled;
    let control = div()
        .id(id)
        .min_h(px(44.0))
        .px(px(ProductSpacing::SM))
        .py(px(ProductSpacing::XS))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(ProductRadius::CONTROL))
        .border_1()
        .border_color(if selected {
            ProductPalette::border_selected()
        } else {
            ProductPalette::border()
        })
        .bg(if selected {
            ProductPalette::control_selected()
        } else {
            ProductPalette::control()
        })
        .when(!disabled, |item| {
            item.hover(|style| style.bg(ProductPalette::control_hover()))
                .active(|style| style.bg(ProductPalette::control_pressed()))
        })
        .when(disabled, |item| item.opacity(0.48))
        .child(
            text(primary, ProductTextStyle::Label, context)
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis(),
        )
        .when_some(secondary, |item, secondary| {
            item.child(
                text(secondary, ProductTextStyle::Caption, context)
                    .flex_shrink_0()
                    .ml(px(ProductSpacing::XS)),
            )
        });
    control_accessibility(control, accessibility_label.into(), state, focus)
}

/// The accessible name for a workspace control.
///
/// The name is the same whether or not the workspace is current, because
/// current state is published through the native `selected` property. A
/// screen reader therefore announces the workspace and its state as two
/// separate facts rather than parsing wording.
pub fn workspace_accessible_name(label: &str) -> SharedString {
    SharedString::from(format!("Espace de travail {label}"))
}

pub fn workspace_visible_label(
    label: &str,
    active: bool,
    mode: in_gnu_os_settings_read_model::WorkspaceDisplayMode,
) -> SharedString {
    use in_gnu_os_settings_read_model::WorkspaceDisplayMode;

    match mode {
        WorkspaceDisplayMode::Text => SharedString::from(label),
        // DONOR-12 deliberately does not import Caelestia workspaceIcons rules.
        // The donor itself falls back to text when no icon rule resolves, so the
        // target-owned no-icon-source behavior is truthful rather than synthetic.
        WorkspaceDisplayMode::Icons => SharedString::from(label),
        // Gnosix has no MaterialShape provider. Keep the semantic distinction with
        // a target-owned geometric marker while native selected state still carries
        // focus/current semantics separately.
        WorkspaceDisplayMode::Shapes => SharedString::from(if active { "◆" } else { "●" }),
    }
}

pub fn workspace_item_with_display_mode(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    mode: in_gnu_os_settings_read_model::WorkspaceDisplayMode,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let label = label.into();
    let state = if active {
        ProductControlState::Selected
    } else {
        ProductControlState::Default
    };
    let accessible_name = workspace_accessible_name(&label);
    let visible = workspace_visible_label(&label, active, mode);
    button(id, visible, accessible_name, state, context, focus)
        .min_w_0()
        .max_w_full()
}

pub fn workspace_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    workspace_item_with_display_mode(
        id,
        label,
        active,
        in_gnu_os_settings_read_model::WorkspaceDisplayMode::Text,
        context,
        focus,
    )
}

pub fn application_item(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    class: impl Into<SharedString>,
    focused: bool,
    visible: bool,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let title = title.into();
    let class = class.into();
    let secondary = if visible {
        class
    } else {
        SharedString::from(format!("{class} · masquée"))
    };
    let accessibility_label = match (focused, visible) {
        (true, true) => SharedString::from(format!("Application {title}, focalisée")),
        (true, false) => SharedString::from(format!("Application {title}, focalisée, masquée")),
        (false, true) => SharedString::from(format!("Focaliser l’application {title}")),
        (false, false) => SharedString::from(format!("Focaliser l’application {title}, masquée")),
    };
    selectable_item(
        id,
        title,
        Some(secondary),
        accessibility_label,
        if focused {
            ProductControlState::Selected
        } else {
            ProductControlState::Default
        },
        context,
        focus,
    )
}

/// One installed application offered by the AppStack.
///
/// Distinct from [`application_item`], which describes an application *surface*
/// the compositor has already told us about. This describes an application that
/// is *installed* and could be launched, which is a different product concept
/// with a different identity and a different action, and conflating the two
/// would make "focus this window" and "start this program" indistinguishable in
/// the accessibility tree.
///
/// The secondary line carries the desktop entry id. Two installed applications
/// may legitimately share a visible title, and without the id a user
/// distinguishing them by spoken name has nothing to tell them apart. The
/// visible name is what a user recognises; the id is what the product and the
/// screen reader both need to be unambiguous.
pub fn installed_application_item(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    entry_id: &str,
    context: ProductDesignContext,
    focus: &ProductControlFocus<'_>,
) -> Stateful<Div> {
    let name = name.into();
    let accessible_name = installed_application_accessible_name(&name);
    let secondary = SharedString::from(entry_id);
    // Never `Selected`: selection in this surface is a focus concern, published
    // through the focused state, and marking every AppStack row as a selectable
    // list option would suggest a selection model the AppStack does not have.
    selectable_item(
        id,
        name,
        Some(secondary),
        accessible_name,
        ProductControlState::Default,
        context,
        focus,
    )
    // AppStack lists remain compact at normal scale and grow with the text
    // line box at larger scales. No fixed height or shrinking glyphs.
    .min_h_0()
    .py(px(ProductSpacing::XXS))
    // Refine the existing border, so keyboard focus is visible without
    // changing row allocation or the AppStack envelope.
    .focus(|style| style.border_color(ProductPalette::focus_ring()))
    .flex_shrink_0()
    .w_full()
    .min_w_0()
    .max_w_full()
}

fn installed_application_accessible_name(name: &str) -> SharedString {
    SharedString::from(format!("AppStack · {name}"))
}

/// The empty state for a AppStack that has nothing to offer.
///
/// Rendered rather than leaving the surface blank, because a AppStack that
/// opened and showed nothing is indistinguishable from a AppStack that failed
/// to open.
pub fn appstack_empty_state(
    message: impl Into<SharedString>,
    context: ProductDesignContext,
) -> Div {
    text(message, ProductTextStyle::Caption, context)
}

pub fn panel() -> Div {
    div()
        .p(px(ProductSpacing::MD))
        .rounded(px(ProductRadius::PANEL))
        .border_1()
        .border_color(ProductPalette::border())
        .bg(ProductPalette::panel())
}

pub fn badge(
    label: impl Into<SharedString>,
    tone: ProductStatusTone,
    context: ProductDesignContext,
) -> Div {
    let foreground = match tone {
        ProductStatusTone::Neutral => ProductPalette::text_secondary(),
        ProductStatusTone::Accent => ProductPalette::accent(),
        ProductStatusTone::Warning => ProductPalette::warning(),
        ProductStatusTone::Danger => ProductPalette::danger(),
    };
    div()
        .px(px(ProductSpacing::XS))
        .py(px(ProductSpacing::XXS))
        .rounded(px(ProductRadius::BADGE))
        .border_1()
        .border_color(foreground)
        .text_color(foreground)
        .child(text(label, ProductTextStyle::Caption, context))
}

pub fn notification_card(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    tone: ProductStatusTone,
    context: ProductDesignContext,
) -> Stateful<Div> {
    let title = title.into();
    let message = message.into();
    let accessibility_label = SharedString::from(format!("{title}. {message}"));

    panel()
        .id(id)
        .w_full()
        .min_w_0()
        // Deliberately no fixed/max height and no overflow clipping here.
        // Notification content owns its intrinsic vertical size; viewport/list
        // parents are responsible for scrolling if the card becomes tall.
        .flex()
        .flex_col()
        .gap(px(ProductSpacing::XS))
        .role(match tone {
            ProductStatusTone::Warning | ProductStatusTone::Danger => gpui::Role::Alert,
            ProductStatusTone::Neutral | ProductStatusTone::Accent => gpui::Role::Status,
        })
        .aria_label(accessibility_label)
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_start()
                .gap(px(ProductSpacing::XS))
                .child(
                    text(title, ProductTextStyle::Label, context)
                        .flex_1()
                        .min_w_0(),
                )
                .child(
                    badge(
                        match tone {
                            ProductStatusTone::Neutral => "info",
                            ProductStatusTone::Accent => "nouveau",
                            ProductStatusTone::Warning => "attention",
                            ProductStatusTone::Danger => "erreur",
                        },
                        tone,
                        context,
                    )
                    .flex_shrink_0(),
                ),
        )
        .child(
            text(message, ProductTextStyle::Body, context)
                .w_full()
                .min_w_0(),
        )
}

pub fn separator() -> Div {
    div()
        .h(px(ProductStroke::STANDARD))
        .w_full()
        .bg(ProductPalette::border())
}

pub fn list_container(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap(px(ProductSpacing::XS))
        .overflow_y_scroll()
        .scrollbar_width(px(8.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductStateKind {
    Empty,
    Loading,
    Unavailable,
    Error,
}

pub fn state_presentation(
    id: impl Into<ElementId>,
    kind: ProductStateKind,
    message: impl Into<SharedString>,
    context: ProductDesignContext,
) -> Stateful<Div> {
    let message = message.into();
    let tone = match kind {
        ProductStateKind::Empty | ProductStateKind::Loading => ProductStatusTone::Neutral,
        ProductStateKind::Unavailable => ProductStatusTone::Warning,
        ProductStateKind::Error => ProductStatusTone::Danger,
    };
    panel()
        .id(id)
        .flex()
        .flex_col()
        .gap(px(ProductSpacing::XS))
        .role(match kind {
            ProductStateKind::Error => gpui::Role::Alert,
            ProductStateKind::Empty | ProductStateKind::Loading | ProductStateKind::Unavailable => {
                gpui::Role::Status
            }
        })
        .aria_label(message.clone())
        .child(badge(
            match kind {
                ProductStateKind::Empty => "Vide",
                ProductStateKind::Loading => "Chargement",
                ProductStateKind::Unavailable => "Indisponible",
                ProductStateKind::Error => "Erreur",
            },
            tone,
            context,
        ))
        .child(text(message, ProductTextStyle::Body, context))
}

pub struct ProductTooltip {
    text: SharedString,
    context: ProductDesignContext,
}

impl ProductTooltip {
    pub fn new(text: impl Into<SharedString>, context: ProductDesignContext) -> Self {
        Self {
            text: text.into(),
            context,
        }
    }
}

impl gpui::Render for ProductTooltip {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        div()
            .px(px(ProductSpacing::XS))
            .py(px(ProductSpacing::XXS))
            .rounded(px(ProductRadius::CONTROL))
            .bg(ProductPalette::panel_elevated())
            .border_1()
            .border_color(ProductPalette::border())
            .child(text(
                self.text.clone(),
                ProductTextStyle::Caption,
                self.context,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_application_semantic_name_is_complete_despite_visible_ellipsis() {
        let name = "Gestionnaire de périphériques réseau avancés et_material";
        assert_eq!(
            installed_application_accessible_name(name).as_ref(),
            format!("AppStack · {name}")
        );
    }

    #[test]
    fn design_context_rejects_unbounded_text_scale() {
        assert!(ProductDesignContext::new(1.0, false).is_some());
        assert!(ProductDesignContext::new(2.0, true).is_some());
        assert!(ProductDesignContext::new(0.99, false).is_none());
        assert!(ProductDesignContext::new(2.01, false).is_none());
        assert!(ProductDesignContext::new(f32::NAN, false).is_none());
    }

    #[test]
    fn reduced_motion_collapses_toolkit_durations_without_changing_semantics() {
        let normal = ProductDesignContext::new(1.0, false).unwrap();
        let reduced = ProductDesignContext::new(1.0, true).unwrap();
        assert_eq!(ProductMotion::quick(normal), Duration::from_millis(120));
        assert_eq!(ProductMotion::standard(normal), Duration::from_millis(180));
        assert_eq!(ProductMotion::quick(reduced), Duration::ZERO);
        assert_eq!(ProductMotion::standard(reduced), Duration::ZERO);
    }

    #[test]
    fn type_scale_is_applied_at_the_token_level() {
        let normal = ProductDesignContext::new(1.0, false).unwrap();
        let doubled = ProductDesignContext::new(2.0, false).unwrap();
        assert_eq!(normal.scaled_px(14.0), 14.0);
        assert_eq!(doubled.scaled_px(14.0), 28.0);
    }

    #[test]
    fn candidate_palette_keeps_selected_and_default_states_distinct() {
        assert_ne!(
            ProductPalette::control(),
            ProductPalette::control_selected()
        );
        assert_ne!(ProductPalette::border(), ProductPalette::border_selected());
    }

    #[test]
    fn product_control_is_inert_only_in_the_disabled_state() {
        assert!(product_control_is_inert(ProductControlState::Disabled));
        assert!(!product_control_is_inert(ProductControlState::Default));
        assert!(!product_control_is_inert(ProductControlState::Selected));
    }

    #[test]
    fn disabled_control_is_not_focusable_and_claims_no_selection() {
        let semantics = control_semantics(ProductControlState::Disabled);
        assert!(!semantics.focusable);
        assert!(!semantics.tab_stop);
        assert_eq!(semantics.selected, None);
    }

    #[test]
    fn enabled_control_is_focusable_and_exposes_native_selection() {
        for state in [ProductControlState::Default, ProductControlState::Selected] {
            let semantics = control_semantics(state);
            assert!(semantics.focusable, "{state:?} must be focusable");
            assert!(semantics.tab_stop, "{state:?} must be in the tab order");
        }
        assert_eq!(
            control_semantics(ProductControlState::Selected).selected,
            Some(true)
        );
        assert_eq!(
            control_semantics(ProductControlState::Default).selected,
            Some(false)
        );
    }

    #[test]
    fn workspace_accessible_name_carries_the_label_and_not_the_state() {
        // Current state travels in the native selected property, so the
        // accessible name is the plain label for every workspace.
        assert_eq!(
            workspace_accessible_name("3").as_ref(),
            "Espace de travail 3"
        );
    }

    #[test]
    fn workspace_display_mode_changes_only_visible_presentation() {
        use in_gnu_os_settings_read_model::WorkspaceDisplayMode;

        assert_eq!(
            workspace_visible_label("3", false, WorkspaceDisplayMode::Text).as_ref(),
            "3"
        );
        assert_eq!(
            workspace_visible_label("3", false, WorkspaceDisplayMode::Icons).as_ref(),
            "3"
        );
        assert_eq!(
            workspace_visible_label("3", false, WorkspaceDisplayMode::Shapes).as_ref(),
            "●"
        );
        assert_eq!(
            workspace_visible_label("3", true, WorkspaceDisplayMode::Shapes).as_ref(),
            "◆"
        );
        assert_eq!(
            workspace_accessible_name("3").as_ref(),
            "Espace de travail 3"
        );
    }

    #[test]
    fn workspace_state_is_carried_by_the_native_selected_property() {
        assert_eq!(
            control_semantics(ProductControlState::Selected).selected,
            Some(true)
        );
        assert_eq!(
            control_semantics(ProductControlState::Default).selected,
            Some(false)
        );
    }
}
