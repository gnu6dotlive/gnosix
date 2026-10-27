//! Development-only frozen endpoints. Uses production geometry, painter and content.
use crate::{
    lab_topology_view::{ComparisonContent, content_fixture},
    product_design::{ProductControlFocus, installed_application_item},
    product_focus::traversable_focus_handle,
    product_scene::{ProductHit, ProductLayout},
    product_shared_envelope::{SharedDockEnvelope, SharedMotion},
};
use gnos_ux_surface::{
    envelope::SurfaceEdge,
    grammar::{
        BarPlacementTransition, GeometryRect, ResolvedBarPlacement, SurfaceViewport, UnitInterval,
    },
    scene::{MonitorSceneId, SceneEpoch},
};
use gpui::{
    AnyElement, Context, FocusHandle, Render, Window, canvas, div, point, prelude::*, px, rgba,
};
use serde_json::json;
use std::{cell::RefCell, path::PathBuf, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureState {
    Closed,
    Workspace,
    AppStack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarLabRecipe {
    Generic,
    ReferenceIdle,
}

pub struct BarPlacementLab {
    pub edge: SurfaceEdge,
    pub state: FixtureState,
    content: ComparisonContent,
    pub bar_focus: FocusHandle,
    pub dock_focus: FocusHandle,
    pub row_focus: Vec<FocusHandle>,
    ack: Option<PathBuf>,
    observed: Rc<RefCell<Option<[f32; 4]>>>,
    pub last_layout: Option<ProductLayout>,
    pub clicks: usize,
    scroll: gpui::ScrollHandle,
    warmup_frames: u8,
    transition: Option<(SurfaceEdge, UnitInterval)>,
    recipe: BarLabRecipe,
    active_window_case: crate::reference_bar::ActiveWindowCase,
    reference_font_registered: bool,
    active_window_observed: Rc<RefCell<Option<[f32; 4]>>>,
    tray_search: gnosix_icon_theme::IconThemeSearch,
    tray_fixture_root: Option<PathBuf>,
    reference_status: ReferenceStatus,
    shell_material: ShellMaterialRequest,
}

/// How the reference Bar's shell is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellMaterialRequest {
    /// The frozen opaque composite calibrated against the retained raster.
    OpaqueComposite,
    /// The native translucent material from `product_shell_material`.
    Native(NativeShell),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeShell {
    pub transparency: crate::product_shell_material::Transparency,
    /// Wallpaper-derived observation; the lab never reads a wallpaper.
    pub wallpaper_luminance: f32,
    pub light: bool,
    /// Paint capture locator markers in the uncovered area.
    pub locator: bool,
    /// False models an output without a Bar (excluded or hidden): the shell
    /// keeps uniform borders and no Bar content is drawn.
    pub bar_present: bool,
}

/// The reference palette roles (opaque, 8-bit) the material is derived from.
pub const REFERENCE_MATERIAL_PALETTE: crate::product_shell_material::MaterialPalette =
    crate::product_shell_material::MaterialPalette {
        surface: [0x13, 0x13, 0x17],
        surface_container: [0x20, 0x1f, 0x23],
        shadow: [0, 0, 0],
    };

impl ShellMaterialRequest {
    pub fn material(
        &self,
        output: (f32, f32),
    ) -> Option<crate::product_shell_material::ShellMaterial> {
        let Self::Native(native) = self else {
            return None;
        };
        Some(crate::product_shell_material::ShellMaterial::derive(
            crate::product_shell_material::MaterialInputs {
                border: crate::reference_bar::reference_shell_border_for(native.bar_present),
                output,
                palette: REFERENCE_MATERIAL_PALETTE,
                transparency: native.transparency,
                wallpaper_luminance: native.wallpaper_luminance,
                light: native.light,
                shadow_blur_max: crate::reference_bar::REFERENCE_SHADOW_BLUR_MAX,
                shadow_alpha: crate::reference_bar::REFERENCE_SHADOW_ALPHA,
            },
        ))
    }
}

/// Where the reference Bar's Clock and Status facts come from.
#[derive(Clone, Default)]
pub enum ReferenceStatus {
    /// The frozen oracle presentation, unchanged.
    #[default]
    Frozen,
    /// A controlled typed-snapshot case through the production adapter.
    Case(crate::reference_status_fixture::StatusCase),
    /// The real read-only observers of this host, through the same sessions
    /// and runtime adapter as the product scene.
    Live(LiveStatusSessions),
}

#[derive(Clone)]
pub struct LiveStatusSessions {
    clock: gpui::Entity<
        crate::product_status_session::StatusSession<crate::product_status_session::ClockSource>,
    >,
    locks: gpui::Entity<
        crate::product_status_session::StatusSession<
            crate::product_status_session::KeyboardLockSource,
        >,
    >,
    bluetooth: gpui::Entity<
        crate::product_status_session::StatusSession<
            crate::product_status_session::BluetoothSource,
        >,
    >,
    power: gpui::Entity<
        crate::product_status_session::StatusSession<crate::product_status_session::PowerSource>,
    >,
    network: gpui::Entity<crate::product_network_session::NetworkSession>,
}

impl LiveStatusSessions {
    fn start(cx: &mut Context<BarPlacementLab>) -> Self {
        use crate::product_status_session::{StatusSession, ensure_observation};
        let sessions = Self {
            clock: StatusSession::shared(cx),
            locks: StatusSession::shared(cx),
            bluetooth: StatusSession::shared(cx),
            power: StatusSession::shared(cx),
            network: crate::product_network_session::NetworkSession::shared(cx),
        };
        ensure_observation(&sessions.clock, cx);
        ensure_observation(&sessions.locks, cx);
        ensure_observation(&sessions.bluetooth, cx);
        ensure_observation(&sessions.power, cx);
        crate::product_network_session::ensure_observation(&sessions.network, cx);
        cx.observe(&sessions.clock, |_, _, cx| cx.notify()).detach();
        cx.observe(&sessions.locks, |_, _, cx| cx.notify()).detach();
        cx.observe(&sessions.bluetooth, |_, _, cx| cx.notify())
            .detach();
        cx.observe(&sessions.power, |_, _, cx| cx.notify()).detach();
        cx.observe(&sessions.network, |_, _, cx| cx.notify())
            .detach();
        sessions
    }

    fn runtime(&self, cx: &gpui::App) -> crate::product_runtime::ProductRuntime {
        let mut runtime = crate::product_runtime::ProductRuntime::default();
        runtime.set_clock(*self.clock.read(cx).value());
        runtime.set_keyboard_locks(*self.locks.read(cx).value());
        runtime.set_bluetooth(self.bluetooth.read(cx).value().clone());
        runtime.set_power(*self.power.read(cx).value());
        runtime.set_network(self.network.read(cx).snapshot().clone());
        runtime
    }
}
fn rect(r: GeometryRect) -> [f32; 4] {
    [r.left(), r.top(), r.right(), r.bottom()]
}
impl BarPlacementLab {
    pub fn new(
        edge: SurfaceEdge,
        state: FixtureState,
        ack: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let content = content_fixture("appstack-basic").expect("static local fixture");
        let count = crate::product_appstack::appstack_rows(&content.runtime).len();
        Self {
            edge,
            state,
            content,
            bar_focus: traversable_focus_handle(cx),
            dock_focus: traversable_focus_handle(cx),
            row_focus: (0..count).map(|_| traversable_focus_handle(cx)).collect(),
            ack,
            observed: Default::default(),
            last_layout: None,
            clicks: 0,
            scroll: gpui::ScrollHandle::new(),
            warmup_frames: 0,
            transition: None,
            recipe: BarLabRecipe::Generic,
            active_window_case: crate::reference_bar::ActiveWindowCase::Live,
            reference_font_registered: false,
            active_window_observed: Default::default(),
            tray_search: gnosix_icon_theme::IconThemeSearch::new(Vec::new(), "hicolor"),
            tray_fixture_root: None,
            reference_status: ReferenceStatus::Frozen,
            shell_material: ShellMaterialRequest::OpaqueComposite,
        }
    }

    pub fn new_reference_idle(
        ack: Option<PathBuf>,
        case: crate::reference_bar::ActiveWindowCase,
        tray_fixture_root: Option<PathBuf>,
        status: ReferenceStatusRequest,
        shell_material: ShellMaterialRequest,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut lab = Self::new(SurfaceEdge::Left, FixtureState::Closed, ack, cx);
        lab.shell_material = shell_material;
        lab.reference_status = match status {
            ReferenceStatusRequest::Frozen => ReferenceStatus::Frozen,
            ReferenceStatusRequest::Case(case) => ReferenceStatus::Case(case),
            ReferenceStatusRequest::Live => ReferenceStatus::Live(LiveStatusSessions::start(cx)),
        };
        lab.recipe = BarLabRecipe::ReferenceIdle;
        lab.active_window_case = case;
        if let Some(root) = tray_fixture_root {
            lab.tray_search = gnosix_icon_theme::IconThemeSearch {
                roots: vec![root.join("icons")],
                fallback_dirs: Vec::new(),
                theme: "GnosixTrayFixture".into(),
            };
            lab.tray_fixture_root = Some(root);
        }
        // The reference oracle (Qt on a translucent surface) uses grayscale
        // coverage; GPUI would default to LCD sub-pixel fringes on an opaque
        // surface. Lab process only.
        cx.set_text_rendering_mode(gpui::TextRenderingMode::Grayscale);
        // Process-private registration only: no fontconfig, no host state.
        lab.reference_font_registered =
            crate::reference_active_window::register_reference_font(cx.text_system()).is_ok();
        if !lab.reference_font_registered {
            eprintln!("reference font registration failed; ActiveWindow title is not drawn");
        }
        lab
    }

    pub fn new_transition(
        source: SurfaceEdge,
        target: SurfaceEdge,
        progress: UnitInterval,
        state: FixtureState,
        ack: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut lab = Self::new(target, state, ack, cx);
        lab.transition = Some((source, progress));
        lab
    }
}
/// Command-line choice of Clock/Status facts for the reference recipe.
#[derive(Clone, Copy, Debug, Default)]
pub enum ReferenceStatusRequest {
    #[default]
    Frozen,
    Case(crate::reference_status_fixture::StatusCase),
    Live,
}

impl Render for BarPlacementLab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The debug accessibility tree describes the last completed frame.
        // Allow bounded startup redraws before freezing the capture endpoint.
        if self.ack.is_some() && self.warmup_frames < 3 {
            self.warmup_frames += 1;
            window.request_animation_frame();
        }
        let size = window.viewport_size();
        let viewport = SurfaceViewport::try_new(f32::from(size.width), f32::from(size.height))
            .expect("viewport");
        let workspace_open = self.state == FixtureState::Workspace
            && self
                .transition
                .is_none_or(|(_, progress)| progress == UnitInterval::ONE);
        let mut layout = match self.recipe {
            BarLabRecipe::Generic => ProductLayout::resolve_reactive_at_edge(
                viewport,
                &mut None,
                workspace_open,
                true,
                None,
                self.edge,
                14.,
            )
            .expect("valid cardinal endpoint"),
            BarLabRecipe::ReferenceIdle => {
                let placement = crate::reference_bar::idle_bar_placement(viewport)
                    .expect("reference Bar oracle fits the viewport");
                ProductLayout::resolve_with_bar_placement(viewport, 1.0, placement)
                    .expect("reference idle placement resolves in the Gnosix engine")
            }
        };
        let transition_frame = self.transition.map(|(source_edge, progress)| {
            let source_layout = ProductLayout::resolve_reactive_at_edge(
                viewport,
                &mut None,
                false,
                true,
                None,
                source_edge,
                14.,
            )
            .expect("source endpoint");
            let transition = BarPlacementTransition::new(
                viewport,
                source_layout.bar_placement,
                layout.bar_placement,
            )
            .expect("valid transition endpoints");
            transition
                .evaluate(progress)
                .expect("valid transition progress")
        });
        let active_placement: ResolvedBarPlacement = if let Some(ref motion) = transition_frame {
            if motion.active_edge == layout.bar_placement.edge() {
                layout.bar_placement
            } else {
                ProductLayout::resolve_reactive_at_edge(
                    viewport,
                    &mut None,
                    false,
                    true,
                    None,
                    motion.active_edge,
                    14.,
                )
                .expect("active edge placement")
                .bar_placement
            }
        } else {
            layout.bar_placement
        };
        let shared = (self.state == FixtureState::AppStack).then(|| {
            SharedDockEnvelope::resolve_with_safe(
                viewport,
                layout.dock,
                layout.dock,
                SharedMotion::endpoint(true),
                layout.dock_safe_bounds().unwrap(),
            )
            .expect("safe bottom shared outer for visible AppStack")
        });
        if !workspace_open {
            layout.shared_dock = shared;
        }
        if let Some(ref motion) = transition_frame {
            layout.bar = motion.active_allocation;
        }
        let frame = if let Some(ref motion) = transition_frame {
            layout.frame_with_bar_transition(MonitorSceneId::new(6), SceneEpoch::new(1), motion)
        } else {
            layout.frame(MonitorSceneId::new(6), SceneEpoch::new(1))
        };
        let input_regions = match self.recipe {
            BarLabRecipe::Generic => frame.input_regions.clone(),
            BarLabRecipe::ReferenceIdle => {
                gnos_ux_surface::pointer_regions::project_resolved_bar_pointer_regions(
                    viewport,
                    active_placement,
                )
                .expect("parity Bar input projection")
                .surface_local_rects()
                .collect()
            }
        };
        let input = input_regions
            .iter()
            .map(|r| gpui::Bounds {
                origin: point(px(r.left()), px(r.top())),
                size: gpui::size(px(r.width()), px(r.height())),
            })
            .collect::<Vec<_>>();
        window.set_input_region(Some(&input));
        let semantic = self.content.runtime.semantic();
        let reference_presentation = (self.recipe == BarLabRecipe::ReferenceIdle).then(|| {
            let mut presentation = self.active_window_case.presentation();
            if let Some(root) = &self.tray_fixture_root {
                presentation.tray = Some(crate::reference_tray_fixture::controlled_tray(root));
            }
            match &self.reference_status {
                ReferenceStatus::Frozen => {}
                ReferenceStatus::Case(case) => case.apply(&mut presentation),
                ReferenceStatus::Live(sessions) => {
                    let derived = crate::reference_bar::BarPresentation::from_product_runtime(
                        &sessions.runtime(cx),
                    );
                    presentation.clock = derived.clock;
                    presentation.status = derived.status;
                }
            }
            presentation
        });
        let reference_tray_icons = reference_presentation.as_ref().map(|presentation| {
            crate::reference_bar::realize_tray_visuals(presentation, &self.tray_search)
        });
        let tray_fixture_summary = self.tray_fixture_root.as_ref().and_then(|_| {
            reference_presentation.as_ref().and_then(|presentation| {
                presentation
                    .tray
                    .as_ref()
                    .zip(reference_tray_icons.as_ref())
                    .map(|(visuals, realized)| {
                        crate::reference_tray_fixture::describe_realized(visuals, realized)
                    })
            })
        });
        let active_window_plan = if let Some(presentation) = &reference_presentation {
            if self.tray_fixture_root.is_some() {
                let mut state = crate::reference_active_window::ActiveWindowState::default();
                let mut shown = None;
                for now_ms in [0.0, 10_000.0] {
                    shown = crate::reference_bar::plan_active_window(
                        presentation,
                        window.text_system(),
                        f32::from(size.height),
                        &mut state,
                        now_ms,
                        false,
                    )
                    .unwrap_or_else(|error| {
                        eprintln!("ActiveWindow not planned: {error:?}");
                        None
                    });
                }
                shown
            } else {
                self.active_window_case
                    .plan(window.text_system(), f32::from(size.height))
                    .unwrap_or_else(|error| {
                        eprintln!("ActiveWindow not planned: {error:?}");
                        None
                    })
            }
        } else {
            None
        };
        let draw_frame = frame.clone();
        let draw_transition = transition_frame.clone();
        let reactive = layout.reactive_frame();
        let draw_shared = layout.shared_dock;
        let recipe = self.recipe;
        let shell_material = self
            .shell_material
            .material((f32::from(size.width), f32::from(size.height)));
        let locator = matches!(
            self.shell_material,
            ShellMaterialRequest::Native(NativeShell { locator: true, .. })
        );
        let bar_present = !matches!(
            self.shell_material,
            ShellMaterialRequest::Native(NativeShell {
                bar_present: false,
                ..
            })
        );
        let capsule_fill = shell_material
            .as_ref()
            .map_or(crate::reference_bar::COLOR_SURFACE_CONTAINER, |m| {
                u32::from_be_bytes(m.container.to_u8())
            });
        let paint = canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| match recipe {
                BarLabRecipe::Generic => {
                    crate::scene_runtime::paint_product_scene_with_transition(
                        bounds,
                        &draw_frame,
                        0.,
                        reactive,
                        draw_shared.as_ref(),
                        draw_transition.as_ref(),
                        true,
                        window,
                    );
                }
                BarLabRecipe::ReferenceIdle => match &shell_material {
                    Some(material) => crate::reference_bar::paint_native_shell_material(
                        bounds, material, locator, window,
                    ),
                    None => crate::reference_bar::paint_idle_shell_border(bounds, window),
                },
            },
        )
        .absolute()
        .inset_0();
        let observed = self.observed.clone();
        let active_window_observed = self.active_window_observed.clone();
        let bar_geometry = layout.clone();
        let bar = match self.recipe {
            BarLabRecipe::Generic => crate::product_bar::content(semantic, active_placement, 14.),
            BarLabRecipe::ReferenceIdle => {
                let presentation = reference_presentation
                    .as_ref()
                    .expect("reference recipe owns a presentation");
                let tray_icons = reference_tray_icons
                    .as_ref()
                    .expect("reference recipe owns realised Tray icons");
                crate::reference_bar::idle_bar_fixture_content(
                    presentation,
                    active_window_plan.as_ref(),
                    self.active_window_observed.clone(),
                    tray_icons,
                    capsule_fill,
                )
            }
        }
        .absolute()
        .left(px(layout.bar.left()))
        .top(px(layout.bar.top()))
        .w(px(layout.bar.width()))
        .h(px(layout.bar.height()))
        .id("product-bar")
        .role(gpui::Role::Button)
        .aria_label(
            semantic
                .bar_accessibility_action_label(workspace_open)
                .unwrap(),
        )
        .track_focus(&self.bar_focus)
        .tab_stop(true)
        .tab_index(0)
        .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
            if this.ack.is_some() {
                return;
            }
            let p = event.position();
            if matches!(event, gpui::ClickEvent::Keyboard(_))
                || bar_geometry.hit(f32::from(p.x), f32::from(p.y)) == Some(ProductHit::Bar)
            {
                this.clicks += 1;
                this.state = if this.state == FixtureState::Workspace {
                    FixtureState::Closed
                } else {
                    FixtureState::Workspace
                };
                cx.notify();
            }
        }))
        .child(
            canvas(
                move |bounds, _, _| {
                    *observed.borrow_mut() = Some([
                        f32::from(bounds.left()),
                        f32::from(bounds.top()),
                        f32::from(bounds.right()),
                        f32::from(bounds.bottom()),
                    ]);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        let dock = div()
            .absolute()
            .left(px(layout.dock.left()))
            .top(px(layout.dock.top()))
            .w(px(layout.dock.width()))
            .h(px(layout.dock.height()))
            .flex()
            .items_center()
            .justify_center()
            .text_color(gpui::white())
            .text_size(px(14.))
            .child(semantic.dock_label())
            .id("product-dock")
            .role(gpui::Role::Button)
            .aria_label("Ouvrir AppStack")
            .track_focus(&self.dock_focus)
            .tab_stop(true)
            .tab_index(1)
            .on_click(cx.listener(|this, _, _, cx| {
                if this.ack.is_some() {
                    return;
                }
                this.state = FixtureState::AppStack;
                cx.notify();
            }));
        let workspace = layout.workspace.map(|r| {
            crate::product_bar::workspace_content(semantic, layout.bar_placement, 14.)
                .absolute()
                .left(px(r.left()))
                .top(px(r.top()))
                .w(px(r.width()))
                .h(px(r.height()))
                .id("product-workspace")
                .role(gpui::Role::Button)
                .aria_label("Fermer le résumé de l’espace de travail")
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.ack.is_some() {
                        return;
                    }
                    this.state = FixtureState::Closed;
                    cx.notify();
                }))
        });
        let mut appstack: Option<AnyElement> = None;
        if self.state == FixtureState::AppStack {
            let r = shared
                .as_ref()
                .expect("AppStack state builds shared envelope")
                .content_clip;
            let rows = crate::product_appstack::appstack_rows(&self.content.runtime);
            let children = rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    installed_application_item(
                        ("appstack-row", i),
                        row.name.clone(),
                        row.entry.as_str(),
                        self.content.design,
                        &ProductControlFocus::new(&self.row_focus[i], i as isize + 2),
                    )
                })
                .collect::<Vec<_>>();
            appstack = Some(
                div()
                    .id("product-appstack")
                    .role(gpui::Role::Group)
                    .aria_label("AppStack")
                    .absolute()
                    .left(px(r.left()))
                    .top(px(r.top()))
                    .w(px(r.width()))
                    .h(px(r.height()))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .overflow_hidden()
                    .child(
                        div()
                            .id("product-appstack-list")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(children),
                    )
                    .into_any_element(),
            );
        }
        let mut snapshot = json!({"schema":"gnosix.donor07.transition-fixture.v1","recipe":format!("{:?}",self.recipe),"edge":format!("{:?}",self.edge),"transition":transition_frame.as_ref().map(|t|json!({"progress":t.progress.value(),"phase":format!("{:?}",t.phase),"active_edge":format!("{:?}",t.active_edge),"reservation_edge":format!("{:?}",t.reservation_edge),"arms":t.arms.iter().map(|a|json!({"edge":format!("{:?}",a.edge),"bounds":rect(a.bounds)})).collect::<Vec<_>>()})),"state":format!("{:?}",self.state),"viewport":[viewport.width,viewport.height],"scene_id":6,"epoch":1,"bar":rect(layout.bar),"orientation":format!("{:?}",active_placement.orientation()),"dock":rect(layout.dock),"dock_safe":layout.dock_safe_bounds().map(rect),"workspace":layout.workspace.map(rect),"shared_outer":layout.shared_dock.map(|s|rect(s.geometry.outer)),"appstack":layout.shared_dock.filter(|s|s.appstack.visible).map(|s|rect(s.content_clip)),"input":input_regions.iter().copied().map(rect).collect::<Vec<_>>(),"reservation":[frame.reservation.top,frame.reservation.right,frame.reservation.bottom,frame.reservation.left],"bar_identity":"SceneNodeId::BAR / SceneNodeRole::Bar","logical_groups":semantic.bar_groups(),"bar_accessibility_label":semantic.bar_accessibility_action_label(workspace_open),"focus_sequence":if self.recipe == BarLabRecipe::ReferenceIdle { vec!["product-bar".to_string()] } else { std::iter::once("product-bar".to_string()).chain(std::iter::once("product-dock".to_string())).chain(crate::product_appstack::appstack_rows(&self.content.runtime).into_iter().map(|row|row.entry.as_str().to_string())).collect::<Vec<_>>() },"execution":"DISABLED_FIXTURE"});
        if let Some(material) = self
            .shell_material
            .material((f32::from(size.width), f32::from(size.height)))
        {
            snapshot["shell_material"] = json!({
                "mode": "native",
                "border": [material.border.left, material.border.top, material.border.right, material.border.bottom],
                "radius": material.border.radius,
                "surface_rgba": material.surface.to_u8(),
                "container_rgba": material.container.to_u8(),
                "shadow_sigma": material.shadow.map(|s| s.sigma),
                "shadow_alpha": material.shadow.map(|s| s.alpha),
                "blur_ignore_alpha": material.blur.as_ref().map(|b| b.ignore_alpha),
                "blur_regions": material.blur.as_ref().map(|b| b.regions.iter().map(|r| [r.x0, r.y0, r.x1, r.y1]).collect::<Vec<_>>()),
                "blur_requested_from_compositor": false,
                "locator": locator,
                "locator_markers": crate::reference_bar::locator_marker_origins((f32::from(size.width), f32::from(size.height))),
                "bar_present": bar_present,
            });
        } else if self.recipe == BarLabRecipe::ReferenceIdle {
            snapshot["shell_material"] = json!({"mode": "opaque_composite"});
        }
        if let Some(presentation) = reference_presentation.as_ref() {
            snapshot["clock_status"] = json!(crate::reference_bar::describe_clock_and_status(
                presentation
            ));
            snapshot["reference_status"] = json!(match &self.reference_status {
                ReferenceStatus::Frozen => "frozen".to_owned(),
                ReferenceStatus::Case(case) => format!("case:{}", case.name()),
                ReferenceStatus::Live(_) => "live".to_owned(),
            });
            snapshot["module_layout"] = json!(
                crate::reference_bar::reference_module_layout(presentation)
                    .entries
                    .iter()
                    .map(|(entry, height)| json!([format!("{entry:?}"), height]))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(summary) = tray_fixture_summary {
            snapshot["tray_fixture"] = json!(summary);
            snapshot["tray_fixture_root"] = json!(
                self.tray_fixture_root
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            );
        }
        if let Some(plan) = active_window_plan.as_ref() {
            let predicted_presentation = reference_presentation
                .as_ref()
                .cloned()
                .unwrap_or_else(|| self.active_window_case.presentation());
            snapshot["active_window_predicted_top"] =
                json!(crate::reference_bar::active_window_top(
                    &predicted_presentation,
                    f32::from(size.height),
                    plan.implicit_height,
                ));
            snapshot["active_window"] = plan.to_json(
                self.active_window_case.name(),
                self.reference_font_registered,
            );
        }
        self.last_layout = Some(layout);
        let ack = self.ack.clone();
        let observed = self.observed.clone();
        div()
            .relative()
            .size_full()
            .bg(rgba(
                if matches!(self.shell_material, ShellMaterialRequest::Native(_)) {
                    0x0000_0000
                } else {
                    0x0d1218ff
                },
            ))
            .child(paint)
            .when(bar_present, |root| root.child(bar))
            .when(self.recipe == BarLabRecipe::Generic, |root| {
                root.child(dock)
            })
            .when_some(workspace, |d, w| d.child(w))
            .when_some(appstack, |d, a| d.child(a))
            .child(
                canvas(
                    move |_, _, _| (),
                    move |_, _, window, _| {
                        if let Some(path) = &ack {
                            let mut snapshot = snapshot.clone();
                            snapshot["native_bar_bounds"] = json!(*observed.borrow());
                            snapshot["a11y_tree"] = json!(window.debug_a11y_tree_json());
                            snapshot["active_window_bounds"] =
                                json!(*active_window_observed.borrow());
                            std::fs::write(path, snapshot.to_string()).expect("fixture ack");
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext, size};
    /// The material is derived from the active palette's opaque roles, not
    /// from the calibrated composite colours: `surface` is the shell, and
    /// `surface_container` the module container.
    #[test]
    fn reference_material_uses_the_active_palette_roles() {
        use crate::reference_bar::{COLOR_SURFACE, COLOR_SURFACE_CONTAINER};
        let palette = REFERENCE_MATERIAL_PALETTE;
        let hex = |rgb: [u8; 3]| u32::from_be_bytes([rgb[0], rgb[1], rgb[2], 0xff]);
        assert_eq!(hex(palette.surface), COLOR_SURFACE);
        assert_eq!(hex(palette.surface_container), COLOR_SURFACE_CONTAINER);
        assert_eq!(palette.shadow, [0, 0, 0]);
        assert_ne!(palette.surface, palette.surface_container);
        let material = ShellMaterialRequest::Native(NativeShell {
            transparency: crate::product_shell_material::Transparency::OBSERVED,
            wallpaper_luminance: 0.1305,
            light: false,
            locator: false,
            bar_present: true,
        })
        .material((2560.0, 1440.0))
        .unwrap();
        assert_eq!(material.surface.to_u8(), [0x13, 0x13, 0x17, 217]);
        let shadow = material.shadow.expect("the shell casts a shadow");
        assert_eq!(shadow.alpha, 0.7);
        assert!((shadow.sigma - 15.0 / 4.6).abs() < 1e-6);
        assert!(
            ShellMaterialRequest::OpaqueComposite
                .material((2560.0, 1440.0))
                .is_none()
        );
    }

    #[test]
    fn native_bar_bounds_and_focus_sequence_survive_all_cardinal_placements() {
        for edge in [
            SurfaceEdge::Top,
            SurfaceEdge::Right,
            SurfaceEdge::Bottom,
            SurfaceEdge::Left,
        ] {
            let mut app = TestAppContext::single();
            let window = app
                .add_window(|_, cx| BarPlacementLab::new(edge, FixtureState::AppStack, None, cx));
            let context = VisualTestContext::from_window(window.into(), &app).into_mut();
            context.simulate_resize(size(px(1280.), px(800.)));
            context.run_until_parked();
            let (handles, observed, layout) = {
                let cx = &app.app.borrow();
                let view = window.read(cx).unwrap();
                (
                    std::iter::once(view.bar_focus.clone())
                        .chain(std::iter::once(view.dock_focus.clone()))
                        .chain(view.row_focus.iter().cloned())
                        .collect::<Vec<_>>(),
                    *view.observed.borrow(),
                    view.last_layout.clone().unwrap(),
                )
            };
            let observed = observed.expect("actual Bar prepaint allocation");
            for (a, b) in observed.into_iter().zip(rect(layout.bar)) {
                assert!(
                    (a - b).abs() <= 0.5,
                    "{edge:?} native Bar allocation observed={observed:?} expected={:?}",
                    rect(layout.bar)
                );
            }
            context.update(|window, cx| {
                window.focus(&handles[0], cx);
                for handle in handles.iter().skip(1) {
                    window.focus_next(cx);
                    assert!(handle.is_focused(window), "{edge:?} forward logical order");
                }
                for handle in handles.iter().rev().skip(1) {
                    window.focus_prev(cx);
                    assert!(handle.is_focused(window), "{edge:?} backward logical order");
                }
                // TestPlatform does not activate an accessibility adapter.
                // Native focus traversal above is independent of that adapter.
            });
        }
    }
    #[test]
    fn native_bar_clicks_respect_rounded_input_on_every_edge() {
        for edge in [
            SurfaceEdge::Top,
            SurfaceEdge::Right,
            SurfaceEdge::Bottom,
            SurfaceEdge::Left,
        ] {
            let mut app = TestAppContext::single();
            let window =
                app.add_window(|_, cx| BarPlacementLab::new(edge, FixtureState::Closed, None, cx));
            let context = VisualTestContext::from_window(window.into(), &app).into_mut();
            context.simulate_resize(size(px(1280.), px(800.)));
            context.run_until_parked();
            let bar = window
                .read(&app.app.borrow())
                .unwrap()
                .last_layout
                .as_ref()
                .unwrap()
                .bar;
            for p in [
                point(px(bar.left() + 1.), px(bar.top() + 1.)),
                point(px(bar.left() - 2.), px(bar.top() + bar.height() / 2.)),
            ] {
                context.simulate_click(p, gpui::Modifiers::default());
                assert_eq!(
                    window.read(&app.app.borrow()).unwrap().clicks,
                    0,
                    "{edge:?} corner/outside pass through"
                );
            }
            context.simulate_click(
                point(
                    px((bar.left() + bar.right()) / 2.),
                    px((bar.top() + bar.bottom()) / 2.),
                ),
                gpui::Modifiers::default(),
            );
            assert_eq!(
                window.read(&app.app.borrow()).unwrap().clicks,
                1,
                "{edge:?} visible center activates"
            );
        }
    }
}
