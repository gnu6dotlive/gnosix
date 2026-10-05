#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

pub mod geometry_lab;
pub mod hyprland_live;
#[cfg(feature = "component-lab")]
pub mod lab_appstack_surface;
#[cfg(feature = "component-lab")]
pub mod lab_topology_compare;
#[cfg(feature = "component-lab")]
pub mod lab_topology_view;
pub mod lifecycle;
mod motion_preference;
pub mod product_appstack;
pub mod product_appstack_view;
pub mod product_bar;
pub mod product_bar_interaction;
pub mod product_bar_settings;
#[cfg(feature = "r13-production-client")]
pub mod product_controller;
pub mod product_design;
pub mod product_diagnostics;
pub mod product_focus;
pub mod product_focus_coordinator;
pub mod product_hull;
pub mod product_keyboard;
#[cfg(feature = "component-lab")]
pub mod product_lab;
pub mod product_launch;
pub mod product_preferences;
#[cfg(feature = "r12-production-client")]
pub mod product_profile;
pub mod product_runtime;
pub mod product_scene;
pub mod product_shared_envelope;
mod product_typography;
pub mod product_workspace_presentation;
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

#[cfg(feature = "component-lab")]
pub mod bar_placement_lab;

mod topology_wake;
