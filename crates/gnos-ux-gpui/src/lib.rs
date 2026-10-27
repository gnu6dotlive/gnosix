#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

#[cfg(feature = "component-lab")]
pub mod attention;
pub mod cursor;
pub mod geometry_lab;
#[cfg(feature = "qualification-surfaces")]
pub mod hull_input_qualification;
pub mod hyprland_live;
pub mod interaction_lease;
#[cfg(feature = "component-lab")]
pub mod lab_appstack_surface;
#[cfg(feature = "component-lab")]
pub mod lab_topology_compare;
#[cfg(feature = "component-lab")]
pub mod lab_topology_view;
pub mod lifecycle;
mod motion_preference;
pub mod product_anchor;
pub mod product_application_search;
pub mod product_appstack;
pub mod product_appstack_search_input;
pub mod product_appstack_view;
pub mod product_audio;
pub mod product_audio_action;
pub mod product_audio_osd;
pub mod product_audio_session;
pub mod product_bar;
pub mod product_bar_interaction;
pub mod product_bar_settings;
pub mod product_clock;
#[cfg(feature = "r13-production-client")]
pub mod product_controller;
pub mod product_design;
pub mod product_diagnostics;
pub mod product_focus;
pub mod product_focus_coordinator;
pub mod product_hull;
pub mod product_indicator;
pub mod product_keyboard;
#[cfg(feature = "component-lab")]
pub mod product_lab;
pub mod product_launch;
pub mod product_mpris;
pub mod product_mpris_ownership;
pub mod product_mpris_relay;
pub mod product_mpris_session;
pub mod product_mpris_transport;
pub mod product_network;
pub mod product_network_session;
pub mod product_now_playing;
pub mod product_preferences;
#[cfg(feature = "r12-production-client")]
pub mod product_profile;
#[cfg(feature = "component-lab")]
pub mod product_reference_consumer;
pub mod product_runtime;
pub mod product_scene;
pub mod product_shared_envelope;
pub mod product_shell_blur;
pub mod product_status_session;
pub mod product_tray;
pub mod product_tray_action;
pub mod product_tray_controller;
pub mod product_tray_geometry;
pub mod product_tray_icon;
pub mod product_tray_menu;
pub mod product_tray_menu_render;
pub mod product_tray_menu_surface;
pub mod product_tray_menu_wire;
pub mod product_tray_ownership;
pub mod product_tray_presentation;
pub mod product_tray_relay;
pub mod product_tray_transport;
mod product_typography;
pub mod product_workspace_presentation;
#[cfg(feature = "component-lab")]
pub mod reference_status_fixture;
#[cfg(feature = "component-lab")]
pub mod reference_tray_fixture;
pub mod scene_runtime;

use gnos_ux_surface::SurfaceRole;
use gpui::{
    App, Bounds, Context, FontWeight, IntoElement, Pixels, Render, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, layer_shell::*,
    point, prelude::*, px, rems, rgba, white,
};

// These values make the first product surfaces observable without ratifying product geometry.
const QUALIFICATION_WIDTH: f32 = 500.0;
const QUALIFICATION_BAR_HEIGHT: f32 = 72.0;
const QUALIFICATION_DOCK_HEIGHT: f32 = 72.0;
const QUALIFICATION_BAR_EXCLUSIVE_ZONE: f32 = 36.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProductSurfacePlan {
    pub role: SurfaceRole,
    pub identity: &'static str,
    pub namespace: &'static str,
    pub layer: Layer,
    pub anchors: Anchor,
    pub keyboard_interactivity: KeyboardInteractivity,
    pub qualification_size: Size<Pixels>,
    pub qualification_exclusive_zone: Option<Pixels>,
}

pub fn try_surface_plan(role: SurfaceRole) -> Option<ProductSurfacePlan> {
    let (layer, anchors, qualification_size, qualification_exclusive_zone) = match role {
        SurfaceRole::Bar => (
            Layer::Top,
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            Size::new(px(QUALIFICATION_WIDTH), px(QUALIFICATION_BAR_HEIGHT)),
            Some(px(QUALIFICATION_BAR_EXCLUSIVE_ZONE)),
        ),
        SurfaceRole::Dock => (
            Layer::Top,
            Anchor::BOTTOM,
            Size::new(px(QUALIFICATION_WIDTH), px(QUALIFICATION_DOCK_HEIGHT)),
            None,
        ),
        SurfaceRole::Rail | SurfaceRole::Overlay => return None,
    };

    Some(ProductSurfacePlan {
        role,
        identity: role.product_identity(),
        namespace: role.namespace(),
        layer,
        anchors,
        keyboard_interactivity: KeyboardInteractivity::None,
        qualification_size,
        qualification_exclusive_zone,
    })
}

pub fn surface_plan(role: SurfaceRole) -> ProductSurfacePlan {
    try_surface_plan(role).expect("unsupported GNOS.UX product runtime role")
}

struct ProductView {
    label: &'static str,
}

impl ProductView {
    fn new(label: &'static str, _cx: &mut Context<Self>) -> Self {
        Self { label }
    }
}

impl Render for ProductView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(rems(2.0))
            .font_weight(FontWeight::EXTRA_BOLD)
            .text_color(white())
            .bg(rgba(0x183b4dff))
            .child(self.label)
    }
}

pub fn run_surface(role: SurfaceRole) {
    let plan = surface_plan(role);

    eprintln!(
        "GNOS_UX_START identity={} namespace={}",
        plan.identity, plan.namespace
    );
    gpui_platform::application().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: plan.qualification_size,
                })),
                app_id: Some(plan.identity.to_string()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: plan.namespace.to_string(),
                    layer: plan.layer,
                    anchor: plan.anchors,
                    exclusive_zone: plan.qualification_exclusive_zone,
                    keyboard_interactivity: plan.keyboard_interactivity,
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| ProductView::new(plan.identity, cx)),
        )
        .expect("opening GNOS.UX product layer-shell window");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_plan_is_canonical_and_qualification_bounded() {
        let plan = surface_plan(SurfaceRole::Bar);

        assert_eq!(plan.identity, "gnos.ux.bar");
        assert_eq!(plan.namespace, "gnos.ux:bar");
        assert_eq!(plan.layer, Layer::Top);
        assert_eq!(plan.anchors, Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        assert_eq!(plan.keyboard_interactivity, KeyboardInteractivity::None);
        assert_eq!(plan.qualification_exclusive_zone, Some(px(36.0)));
    }

    #[test]
    fn dock_plan_is_canonical_without_product_geometry_or_exclusive_zone() {
        let plan = surface_plan(SurfaceRole::Dock);

        assert_eq!(plan.identity, "gnos.ux.dock");
        assert_eq!(plan.namespace, "gnos.ux:dock");
        assert_eq!(plan.layer, Layer::Top);
        assert_eq!(plan.anchors, Anchor::BOTTOM);
        assert_eq!(plan.keyboard_interactivity, KeyboardInteractivity::None);
        assert_eq!(plan.qualification_exclusive_zone, None);
    }

    #[test]
    fn bar_and_dock_plans_are_distinct() {
        assert_ne!(
            surface_plan(SurfaceRole::Bar),
            surface_plan(SurfaceRole::Dock)
        );
    }

    #[test]
    fn unsupported_product_roles_are_queryable_without_panicking() {
        assert_eq!(try_surface_plan(SurfaceRole::Rail), None);
        assert_eq!(try_surface_plan(SurfaceRole::Overlay), None);
    }
}

pub mod bar_audio_popout;
pub mod bar_osd;
pub mod bar_osd_view;
#[cfg(feature = "component-lab")]
pub mod bar_placement_lab;
pub mod bar_pointer_routing;
pub mod bar_popout_host;
pub mod bar_popout_view;
pub mod bar_status_popout_view;
pub mod bar_status_popouts;
pub mod bar_strip;
pub mod compositor_blur;
pub mod dashboard_panel;
#[cfg(feature = "component-lab")]
pub mod dashboard_view;
#[cfg(feature = "component-lab")]
pub mod deck;
#[cfg(feature = "component-lab")]
pub mod desktop_layer;
pub mod desktop_modules;
pub mod icons;
pub mod identity;
pub mod modules;
pub mod notification_centre;
pub mod notification_centre_view;
pub mod notification_toasts;
pub mod notification_view;
pub mod overflow_panel;
#[cfg(feature = "component-lab")]
pub mod overflow_view;
pub mod product_icon;
pub mod product_notification_session;
pub mod product_shell_material;
pub mod product_status_glyph;
#[cfg(feature = "component-lab")]
pub mod product_thumbnails;
pub mod rail_topology;
pub mod receipt_hold;
pub mod reference_active_window;
pub mod reference_bar;
pub mod session_panel;
#[cfg(feature = "component-lab")]
pub mod session_view;
pub mod settings_pages;
pub mod settings_panel;
pub mod settings_view;
#[cfg(feature = "component-lab")]
pub mod shell_field_paint;
#[cfg(feature = "component-lab")]
pub mod surface_prefs;
#[cfg(feature = "component-lab")]
pub mod tray_menu_extrusion;
pub mod utilities_panel;
pub mod utilities_view;
#[cfg(feature = "component-lab")]
pub mod wallpaper;
pub mod workspace_panel;
#[cfg(feature = "component-lab")]
pub mod workspace_view;

mod topology_wake;
