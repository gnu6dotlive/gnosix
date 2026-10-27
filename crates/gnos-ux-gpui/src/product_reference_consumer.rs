//! EC-03A reference-profile consumer: existing native sources in the real scene.
//! This is a configurable recipe, not the universal Gnosix Bar.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

use gnosix_icon_theme::IconThemeSearch;
use gpui::{TextSystem, WindowTextSystem};

use crate::{
    product_runtime::ProductRuntime,
    product_tray_icon::RealizedTrayIcon,
    reference_active_window::{self, ActiveWindowPlan, ActiveWindowState},
    reference_bar::{self, BarPresentation, TrayVisual},
};

/// Bounds of the reference workspace capsule in scene-local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkspaceHoverBounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl WorkspaceHoverBounds {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// The attached surface and bounded bridge remain one pointer-exit domain.
    /// It never permits unrelated parts of the full-screen scene to hold open.
    pub fn contains_attached(
        self,
        x: f32,
        y: f32,
        attached: Option<gnos_ux_surface::grammar::GeometryRect>,
    ) -> bool {
        let Some(rect) = attached else {
            return false;
        };
        let right = rect.right().max(self.right);
        let left = rect.left().min(self.right);
        let top = rect.top().min(self.top);
        let bottom = rect.bottom().max(self.bottom);
        (x >= left && x < right && y >= top && y < bottom) || rect.contains_point(x, y)
    }
}

/// Exact reference-layout workspace module; no hardcoded all-rail click hitbox.
/// Only entries before Workspaces affect its y-origin (no flexible spacer).
pub fn workspace_hover_bounds(presentation: &BarPresentation) -> Option<WorkspaceHoverBounds> {
    if presentation.workspaces.is_empty() {
        return None;
    }
    let layout = reference_bar::reference_module_layout(presentation);
    let mut top = reference_bar::LARGE_VERTICAL_PADDING;
    for (entry, height) in layout.entries {
        if entry == reference_bar::IdleBarEntry::Workspaces {
            return Some(WorkspaceHoverBounds {
                left: (60.0 - reference_bar::BAR_INNER_WIDTH) / 2.0,
                top,
                right: (60.0 + reference_bar::BAR_INNER_WIDTH) / 2.0,
                bottom: top + height,
            });
        }
        top += height + reference_bar::MEDIUM_SPACING;
    }
    None
}

/// Relative reference-wheel direction is only allowed to resolve to an
/// already-observed, output-local target. Out-of-range and zero delta refuse.
pub fn bounded_workspace_step_index(
    active_index: usize,
    target_count: usize,
    delta_y: f32,
) -> Option<usize> {
    if !delta_y.is_finite() || active_index >= target_count {
        return None;
    }
    if delta_y > 0.0 {
        active_index.checked_sub(1)
    } else if delta_y < 0.0 {
        active_index
            .checked_add(1)
            .filter(|next| *next < target_count)
    } else {
        None
    }
}

/// Hover owns only the surface it opened. Explicit keyboard/AppStack panels
/// are not stolen or closed by an unrelated pointer movement.
pub fn next_reference_workspace_hover(
    current: crate::product_scene::ProductPanelState,
    hover_owned: bool,
    source_hit: bool,
    attached_hit: bool,
    can_open: bool,
) -> (crate::product_scene::ProductPanelState, bool) {
    use crate::product_scene::ProductPanelState as Panel;
    if source_hit
        && can_open
        && (current == Panel::Closed || (current == Panel::WorkspaceSummary && hover_owned))
    {
        return (Panel::WorkspaceSummary, true);
    }
    if hover_owned
        && current == Panel::WorkspaceSummary
        && (!can_open || (!source_hit && !attached_hit))
    {
        return (Panel::Closed, false);
    }
    (current, hover_owned && current == Panel::WorkspaceSummary)
}

pub struct ReferenceVisualFrame {
    pub presentation: BarPresentation,
    pub tray_icons: Vec<RealizedTrayIcon>,
    pub active_window_plan: Option<ActiveWindowPlan>,
}

pub struct ReferenceVisualConsumer {
    tray_search: IconThemeSearch,
    tray_cache: Option<(Option<Vec<TrayVisual>>, Vec<RealizedTrayIcon>)>,
    window_state: ActiveWindowState,
    started: Instant,
    font_ready: bool,
    pub active_window_observed: Rc<RefCell<Option<[f32; 4]>>>,
}

/// Gnosix's icon-theme resolver consumes explicit XDG paths, not Qt objects.
pub fn reference_icon_search(
    home: Option<&Path>,
    data_home: Option<&Path>,
    data_dirs: Option<&str>,
    theme: &str,
) -> IconThemeSearch {
    let roots = IconThemeSearch::default_roots(home, data_home, data_dirs);
    IconThemeSearch::new(roots, if theme.is_empty() { "hicolor" } else { theme })
}

impl ReferenceVisualConsumer {
    pub fn new(text_system: &TextSystem) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let data_dirs = std::env::var("XDG_DATA_DIRS").ok();
        let theme =
            std::env::var("GNOS_UX_REFERENCE_ICON_THEME").unwrap_or_else(|_| "hicolor".to_owned());
        let font_ready = reference_active_window::reference_font_registered(text_system)
            || reference_active_window::register_reference_font(text_system).is_ok();
        if !font_ready {
            eprintln!(
                "GNOS_UX_REFERENCE_ACTIVE_WINDOW status=unavailable reason=private_font_not_registered"
            );
        }
        Self {
            tray_search: reference_icon_search(
                home.as_deref(),
                data_home.as_deref(),
                data_dirs.as_deref(),
                &theme,
            ),
            tray_cache: None,
            window_state: ActiveWindowState::default(),
            started: Instant::now(),
            font_ready,
            active_window_observed: Rc::default(),
        }
    }

    /// Source ordering and missing-image states are owned by reference_bar;
    /// active window title is planned only if the private font is registered.
    pub fn project(
        &mut self,
        runtime: &ProductRuntime,
        wts: &WindowTextSystem,
        height: f32,
        reduced_motion: bool,
    ) -> ReferenceVisualFrame {
        let presentation = BarPresentation::from_product_runtime(runtime);
        let tray_icons =
            cached_tray_visuals(&presentation, &self.tray_search, &mut self.tray_cache);
        let active_window_plan = if self.font_ready {
            match reference_bar::plan_active_window(
                &presentation,
                wts,
                height,
                &mut self.window_state,
                self.started.elapsed().as_secs_f32() * 1000.0,
                reduced_motion,
            ) {
                Ok(plan) => plan,
                Err(error) => {
                    eprintln!(
                        "GNOS_UX_REFERENCE_ACTIVE_WINDOW status=unavailable reason={error:?}"
                    );
                    None
                }
            }
        } else {
            None
        };
        ReferenceVisualFrame {
            presentation,
            tray_icons,
            active_window_plan,
        }
    }
}

/// A snapshot-bound cache avoids repeated icon-theme filesystem scans on
/// every motion/Clock repaint. Any SNI item, owner, status or imagery change
/// changes the typed presentation key and forces re-realization.
fn cached_tray_visuals(
    presentation: &BarPresentation,
    search: &IconThemeSearch,
    cache: &mut Option<(Option<Vec<TrayVisual>>, Vec<RealizedTrayIcon>)>,
) -> Vec<RealizedTrayIcon> {
    if let Some((key, icons)) = cache
        && *key == presentation.tray
    {
        return icons.clone();
    }
    let icons = reference_bar::realize_tray_visuals(presentation, search);
    *cache = Some((presentation.tray.clone(), icons.clone()));
    icons
}

/// Pure read model → native icon pipeline, shared by scene and causal tests.
pub fn project_tray_visuals(
    runtime: &ProductRuntime,
    search: &IconThemeSearch,
) -> (BarPresentation, Vec<RealizedTrayIcon>) {
    let presentation = BarPresentation::from_product_runtime(runtime);
    let icons = reference_bar::realize_tray_visuals(&presentation, search);
    (presentation, icons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::{
        TrayCategory, TrayIcon, TrayIconFacts, TrayItemId, TrayItemSnapshot, TraySnapshot,
        TrayStatus,
    };

    fn item(id: &str, status: TrayStatus, icon: TrayIcon) -> TrayItemSnapshot {
        TrayItemSnapshot {
            id: TrayItemId::new(id),
            service: format!("org.example.{id}"),
            path: format!("/org/example/{id}"),
            title: id.into(),
            status,
            category: TrayCategory::ApplicationStatus,
            icons: TrayIconFacts::from(icon),
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.100".into(),
        }
    }

    #[test]
    fn explicit_icon_roots_are_pure_and_ordered() {
        let search = reference_icon_search(
            Some(Path::new("/home/example")),
            Some(Path::new("/tmp/xdg")),
            Some("/opt/xdg:/usr/share"),
            "ExampleTheme",
        );
        assert_eq!(search.theme, "ExampleTheme");
        assert_eq!(
            search.roots,
            vec![
                PathBuf::from("/home/example/.icons"),
                PathBuf::from("/tmp/xdg/icons"),
                PathBuf::from("/opt/xdg/icons"),
                PathBuf::from("/usr/share/icons"),
            ]
        );
    }

    #[test]
    fn real_runtime_tray_pixmap_reaches_native_visuals_without_passive_or_fake_icon() {
        let mut runtime = ProductRuntime::default();
        runtime.set_tray(TraySnapshot::from_items(vec![
            item(
                "real",
                TrayStatus::Active,
                TrayIcon::Pixmap {
                    width: 1,
                    height: 1,
                    argb_bytes: vec![255, 13, 37, 91],
                },
            ),
            item(
                "passive",
                TrayStatus::Passive,
                TrayIcon::Named("ignored".into()),
            ),
        ]));
        let search = reference_icon_search(None, None, Some("/nonexistent"), "hicolor");
        let (presentation, icons) = project_tray_visuals(&runtime, &search);
        assert_eq!(presentation.tray.as_ref().unwrap().len(), 1);
        assert_eq!(icons.len(), 1);
        assert!(matches!(icons[0], RealizedTrayIcon::Image { .. }));
        runtime.set_tray(TraySnapshot::from_items(vec![item(
            "missing",
            TrayStatus::Active,
            TrayIcon::Named("ec03-unknown-unavailable-imagery".into()),
        )]));
        let (_, icons) = project_tray_visuals(&runtime, &search);
        assert!(matches!(icons[0], RealizedTrayIcon::Missing(_)));
    }

    #[test]
    fn cache_tracks_typed_tray_lifetime_not_widget_rerenders() {
        let mut runtime = ProductRuntime::default();
        let search = reference_icon_search(None, None, Some("/nonexistent"), "hicolor");
        let mut cache = None;
        let initial = BarPresentation::from_product_runtime(&runtime);
        assert!(cached_tray_visuals(&initial, &search, &mut cache).is_empty());
        let no_change = cached_tray_visuals(&initial, &search, &mut cache);
        assert!(no_change.is_empty());
        assert_eq!(cache.as_ref().unwrap().0, initial.tray);
        runtime.set_tray(TraySnapshot::from_items(vec![item(
            "real",
            TrayStatus::Active,
            TrayIcon::Pixmap {
                width: 1,
                height: 1,
                argb_bytes: vec![255, 4, 5, 6],
            },
        )]));
        let active = BarPresentation::from_product_runtime(&runtime);
        let actual = cached_tray_visuals(&active, &search, &mut cache);
        assert!(matches!(
            actual.as_slice(),
            [RealizedTrayIcon::Image { .. }]
        ));
        assert_eq!(cache.as_ref().unwrap().0, active.tray);
        runtime.set_tray(TraySnapshot::from_items(vec![item(
            "real",
            TrayStatus::Passive,
            TrayIcon::None,
        )]));
        let passive = BarPresentation::from_product_runtime(&runtime);
        assert!(cached_tray_visuals(&passive, &search, &mut cache).is_empty());
        assert_eq!(cache.as_ref().unwrap().0, passive.tray);
    }

    #[test]
    fn workspace_hit_zone_follows_reference_layout_not_full_rail() {
        let mut bar = BarPresentation::captured_oracle_2026_10_07();
        let hit = workspace_hover_bounds(&bar).expect("reference has workspaces");
        assert_eq!(hit.left, 10.0);
        assert_eq!(hit.top, 16.0 + 19.0 + 12.0);
        assert_eq!(hit.right, 50.0);
        assert!(hit.bottom > hit.top);
        assert!(hit.contains(30.0, hit.top + 1.0));
        assert!(!hit.contains(30.0, 3.0));
        assert!(!hit.contains(30.0, hit.bottom + 4.0));
        bar.workspaces.clear();
        assert!(workspace_hover_bounds(&bar).is_none());
    }

    #[test]
    fn workspace_wheel_is_bounded_and_no_relative_dispatch_is_created() {
        assert_eq!(bounded_workspace_step_index(2, 5, 1.0), Some(1));
        assert_eq!(bounded_workspace_step_index(2, 5, -1.0), Some(3));
        assert_eq!(bounded_workspace_step_index(0, 5, 1.0), None);
        assert_eq!(bounded_workspace_step_index(4, 5, -1.0), None);
        assert_eq!(bounded_workspace_step_index(0, 1, -1.0), None);
        assert_eq!(bounded_workspace_step_index(0, 5, 0.0), None);
        assert_eq!(bounded_workspace_step_index(5, 5, -1.0), None);
        assert_eq!(bounded_workspace_step_index(2, 5, f32::NAN), None);
    }

    #[test]
    fn workspace_hover_opens_holds_retracts_without_stealing_keyboard_or_appstack() {
        use crate::product_scene::ProductPanelState as Panel;
        let (panel, owned) =
            next_reference_workspace_hover(Panel::Closed, false, true, false, true);
        assert_eq!((panel, owned), (Panel::WorkspaceSummary, true));
        assert_eq!(
            next_reference_workspace_hover(panel, owned, false, true, true),
            (Panel::WorkspaceSummary, true)
        );
        assert_eq!(
            next_reference_workspace_hover(panel, owned, false, false, true),
            (Panel::Closed, false)
        );
        assert_eq!(
            next_reference_workspace_hover(Panel::AppStack, false, true, false, true),
            (Panel::AppStack, false)
        );
        assert_eq!(
            next_reference_workspace_hover(Panel::WorkspaceSummary, false, false, false, true),
            (Panel::WorkspaceSummary, false)
        );
        assert_eq!(
            next_reference_workspace_hover(panel, owned, true, true, false),
            (Panel::Closed, false)
        );
    }

    #[test]
    fn unavailable_product_tray_never_materializes_illustrative_slots() {
        let runtime = ProductRuntime::default();
        let search = reference_icon_search(None, None, Some("/nonexistent"), "");
        assert_eq!(search.theme, "hicolor");
        let (presentation, icons) = project_tray_visuals(&runtime, &search);
        assert!(presentation.tray.is_none());
        assert!(icons.is_empty());
    }
}
