//! Persistent GPUI host for one GNOS.UX monitor scene.
//!
//! Bar, Body and Dock are scene nodes inside one visible layer-shell surface.

use std::{
    collections::{BTreeMap, HashSet},
    env,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use ashpd::desktop::settings::{ColorScheme as PortalColorScheme, Settings};
use futures::{StreamExt, channel::mpsc as async_mpsc};
use gnos_ux_surface::{
    compositor::{CompositorIntentSource, ResolvedCompositorIntent},
    envelope::{RuntimeId, SurfaceEdge},
    grammar::{
        BarReservationPolicy, BarSensingPolicy, GeometryRect, Point2, SurfaceViewport,
        reactive_envelope::ReactiveEnvelopeSolver,
    },
    scene::{
        MonitorScene, MonitorSceneId, SCENE_GRAMMAR_ID, SCENE_GRAMMAR_REVISION, SceneCycleStep,
        SceneEpoch, SceneFrame, SceneNodeId, SceneNodeRole,
    },
};
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
};
use gnosix_desktop_model::{
    ConnectorName, DesktopContextGeneration, DesktopOutputProjection, DesktopSurfaceRole,
    SurfaceFocus, SurfaceVisibility, WorkspaceFocus, WorkspaceVisibility,
    project_all_output_contexts,
};
use gnosix_hyprland::{
    ApplicationOutcome, FenceDecision, GlobalProjectionIntent, HyprctlLuaBackend,
    HyprlandGlobalMutation, HyprlandRuntimeBackend, IntentFence, ObservedRuntimeOutput,
    ProjectionBoundary, ProjectionKey, ProjectionSlot, RollbackOutcome, RuntimeOutputIdentity,
    semantic::{DesktopSemanticReceipt, DesktopSemanticRuntime, SemanticObservationError},
    surface_reservation_plan,
    topology::{
        SessionTopologyPlan, SessionTopologyReconciler, TopologyAction, TopologyEventSubscription,
        TopologyWakeBatch, TopologyWakeHint,
    },
    validate_runtime_output_target,
};
use gpui::{
    AnyWindowHandle, App, Bounds, Context, DisplayId, IntoElement, PathBuilder, QuitMode, Render,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div,
    layer_shell::*, point, prelude::*, px, rgba, white,
};
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    iterator::{Handle as SignalHandle, Signals},
};
use uuid::Uuid;

#[cfg(feature = "component-lab")]
mod attention;
#[cfg(feature = "component-lab")]
mod bar_drag;
#[cfg(feature = "component-lab")]
mod bar_keys;
#[cfg(feature = "component-lab")]
mod bar_travel;
#[cfg(feature = "component-lab")]
mod cursor;
#[cfg(feature = "component-lab")]
mod dashboard;
#[cfg(feature = "component-lab")]
mod deck_actions;
#[cfg(feature = "component-lab")]
mod desktop;
#[cfg(feature = "component-lab")]
mod launch_feedback;
#[cfg(feature = "component-lab")]
mod osd;
#[cfg(feature = "component-lab")]
mod overflow;
#[cfg(feature = "component-lab")]
mod rail;
#[cfg(feature = "component-lab")]
mod session;
mod shell_prefs;
#[cfg(feature = "component-lab")]
mod utilities;
#[cfg(feature = "component-lab")]
mod workspaces;
// ARCH-01A: parts of the scene, each in its module; their free items are
// named here as before.
mod appstack;
mod deck_dock;
mod focus;
mod frame;
mod lifecycle;
mod media;
mod motion;
mod notifications_host;
mod popouts;
mod projection;
mod qualification;
mod settings_host;
mod topology;
mod tray_host;
mod windows;

use deck_dock::*;
use focus::*;
use frame::*;
use lifecycle::*;
use media::*;
use motion::*;
use projection::*;
use qualification::*;
use topology::*;
use tray_host::*;
use windows::*;

#[cfg(feature = "r13-production-client")]
use crate::product_keyboard::dispatch_scene_key;
#[cfg(feature = "r13-production-client")]
use crate::product_workspace_presentation::workspace_bar_presentation;
use gpui::Stateful;

#[cfg(feature = "r13-production-client")]
use crate::{
    product_controller::ProductController,
    product_design::{ProductControlState, button, workspace_item_with_display_mode},
    product_focus::{
        ProductControlId, ProductFocusState, installed_application_control_identity,
        panel_control_identity, traversable_focus_handle, workspace_control_identity,
    },
    product_runtime::ProductActionTerminal,
};

use crate::{
    geometry_lab::viewport_from_size,
    lifecycle::{EventStreamState, RuntimeHealthReceipt, RuntimeLifecycle, RuntimeLifecyclePhase},
    motion_preference::{MotionPreference, MotionPreferenceState},
    product_bar_interaction::{
        BarInteractionPolicy, BarPointerZone, BarRevealMode, BarRevealState, next_state,
        pointer_zone, project_frame as project_bar_reveal_frame,
    },
    product_design::{
        ProductControlFocus, ProductDesignContext, ProductSpacing, ProductTextStyle,
        installed_application_item, panel, text as design_text,
    },
    product_diagnostics::ProductDiagnosticSnapshot,
    product_focus_coordinator::{ProductFocusCoordinator, ProductHostId, ProductHostWindows},
    product_preferences::{
        ProductColorScheme, ProductPreferenceDefaults, SystemProductPreferences,
        resolve_without_profile_authority,
    },
    product_runtime::ProductRuntime,
    product_scene::{ProductLayout, ProductPanelState, ProductSemanticState},
    product_typography::ProductTextScale,
};

pub const SCENE_NAMESPACE: &str = "gnos.ux:scene";
pub const SCENE_IDENTITY: &str = "gnos.ux.scene";

/// The Dock affordance's visible label. Kept as a constant because the
/// qualification harness and the accessibility tree both address this control,
/// and a label that exists in two spellings is two controls.
#[cfg(feature = "r13-production-client")]
/// The Dock control that opens the AppStack.
///
/// "AppStack", not "Applications" and not a localized "Lanceur". The canonical
/// component identity is what a qualification has to be able to see, and a
/// localized presentation label is a separate, explicitly ratified decision that
/// this tranche does not make.
const APPSTACK_AFFORDANCE_LABEL: &str = "AppStack";

/// Keyboard interactivity for the persistent product scene surface.
///
/// The scene owns interactive controls, so it must be able to take keyboard
/// focus when the user or an assistive technology asks for it, and give it
/// back through ordinary desktop interaction.
///
/// - `None` prohibited focus entirely, which left the Workspace Manager
///   reachable only by pointer and AT-SPI, and never by keyboard.
/// - `Exclusive` would make a persistent bar/dock a keyboard monopolist.
/// - `OnDemand` is the compositor-managed middle: the surface can be focused
///   like a normal window, and merely existing never steals focus.
///
/// This applies to the production scene only. Qualification-only and
/// display-only layer surfaces keep `None`; `Exclusive` is reserved for a
/// future bounded modal or security-sensitive surface that genuinely
/// requires exclusive keyboard ownership.
///
/// Qualified on Hyprland 0.56.2. Layer-shell focus semantics are partly
/// compositor-defined, so this is not a compositor-independent claim.
pub const PRODUCT_SCENE_KEYBOARD_INTERACTIVITY: KeyboardInteractivity =
    KeyboardInteractivity::OnDemand;

type SharedLifecycle = Arc<Mutex<RuntimeLifecycle>>;

/// LIVE-02: the motion's step period.
const MOTION_STEP: Duration = Duration::from_millis(4);
/// The height of the bands a shaped surface asks its blur as, where its
/// outline curves (`outline_bands`).
#[cfg(feature = "component-lab")]
const BLUR_BAND: f32 = 2.0;

struct SceneHost {
    scene_id: MonitorSceneId,
    runtime_output: Option<RuntimeId>,
    /// Bounded tray qualification request, when one was supplied.
    tray_qualification: Option<TrayQualificationRequest>,
    /// Shared tray action controller, stored so the scene can re-render its
    /// open menu surface when the menu projection changes.
    tray_controller: gpui::Entity<crate::product_tray_controller::TrayActionController>,
    /// The single output whose Now Playing controls are dispatch sources.
    ///
    /// Whether this process holds the interaction-owner bus name.
    ///
    /// Resolved asynchronously, because arbitrating across processes needs a bus
    /// round trip. Consulted at render time rather than cached at construction,
    /// so a surface built before the claim resolved still renders and simply
    /// installs no dispatch sink until ownership is known.
    mpris_process_owns: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Whether this process holds the tray interaction-owner bus name.
    ///
    /// Also consulted by the tray qualification gate, which is what makes
    /// "one remote tray call across three outputs" a property of the process
    /// rather than of whichever host happened to start first.
    tray_process_owns: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Watches the controller for menu changes to repaint the open surface.
    _menu_subscription: Option<gpui::Subscription>,
    /// Bounded attempts, so a request for an item that never arrives does not
    /// retry for the life of the session.
    tray_qualification_attempts: u8,
    /// Item whose menu row click should follow a successful menu open.
    tray_menu_row_target: Option<String>,
    #[cfg(feature = "component-lab")]
    /// RAIL-01: where a tray menu was opened from the rail (scene coords).
    rail_tray_menu_at: Option<(f32, f32)>,
    /// Logical origin of the output this scene was routed to.
    ///
    /// Carried from the topology route so tray coordinate hints are derived
    /// from the output's real position, including negative origins.
    output_origin: (i32, i32),
    semantic_consumer: SceneSemanticConsumer,
    last_projection_revision: u64,
    product_mode: bool,
    product_diagnostics: bool,
    /// Explicit review-only native rail; never a default product setting.
    review_native_rail: bool,
    #[cfg(feature = "component-lab")]
    reference_visual: Option<crate::product_reference_consumer::ReferenceVisualConsumer>,
    /// Ownership of the WorkspaceSummary opened by this reference hover recipe.
    #[cfg(feature = "component-lab")]
    reference_workspace_hover_owned: bool,
    /// UXE-01: Bar-attached popout (routing, motion, geometry) on the native rail.
    #[cfg(feature = "component-lab")]
    bar_popout: crate::bar_popout_host::BarPopoutHost,
    /// SURF-01B: the Utilities panel out of the status icons, and the
    /// scene's side of its powers (pending requests, the idle inhibitor,
    /// the brightness targets).
    #[cfg(feature = "component-lab")]
    utilities: crate::utilities_panel::UtilitiesPanel,
    #[cfg(feature = "component-lab")]
    utilities_state: utilities::UtilitiesState,
    /// SURF-01C: the OSD out of the Bar beside the status icons, and the
    /// last observed levels it compares against.
    #[cfg(feature = "component-lab")]
    bar_osd: crate::bar_osd::BarOsd,
    #[cfg(feature = "component-lab")]
    osd_state: osd::OsdState,
    /// SURF-02B: the session panel out of the power button, and what
    /// logind reports for it.
    #[cfg(feature = "component-lab")]
    session: crate::session_panel::SessionPanel,
    #[cfg(feature = "component-lab")]
    session_state: session::SessionState,
    /// WS-02: the workspace manager out of the workspaces module.
    #[cfg(feature = "component-lab")]
    workspaces: crate::workspace_panel::WorkspacePanel,
    /// FOCUS-01: when each surface out of the shell comes out.
    #[cfg(feature = "component-lab")]
    attention: crate::attention::Attention,
    #[cfg(feature = "component-lab")]
    attention_state: attention::AttentionState,
    /// CURSOR-01: where the pointer is on the scene, and the shape last
    /// reported.
    #[cfg(feature = "component-lab")]
    cursor_at: Option<(f32, f32)>,
    #[cfg(feature = "component-lab")]
    cursor_reported: Option<gpui::CursorStyle>,
    /// CURSOR-01B: the applications starting.
    #[cfg(feature = "component-lab")]
    launching: Vec<launch_feedback::Launching>,
    #[cfg(feature = "component-lab")]
    workspaces_state: workspaces::WorkspacesState,
    /// BAR-02A: the overflow panel out of the "…" module.
    #[cfg(feature = "component-lab")]
    overflow: crate::overflow_panel::OverflowPanel,
    /// SURF-03B: the dashboard's content (the popout host shows it), and
    /// its facts.
    #[cfg(feature = "component-lab")]
    dashboard: crate::dashboard_panel::DashboardPanel,
    #[cfg(feature = "component-lab")]
    dashboard_state: dashboard::DashboardState,
    /// A press outside closed a panel out of the Bar: that press, and its
    /// release, act on nothing beneath (no Dock click, no Deck press).
    #[cfg(feature = "component-lab")]
    swallow_click: bool,
    /// SURF-04: this output's desktop surface (modules on the background
    /// layer).
    #[cfg(feature = "component-lab")]
    desktop_state: desktop::DesktopState,
    /// UXE-02: notification toasts on the native rail.
    #[cfg(feature = "component-lab")]
    notification_toasts: crate::notification_toasts::ToastHost,
    /// UXE-02C: notification centre on the native rail.
    #[cfg(feature = "component-lab")]
    notification_centre: crate::notification_centre::CentreHost,
    /// DECK-01C: the AppStack in the Deck on the native rail (it retired
    /// the UXE-03 launcher drawer, D03).
    #[cfg(feature = "component-lab")]
    stack: crate::deck::stack::StackHost,
    /// UXE-05B: the single Settings shell surface.
    #[cfg(feature = "component-lab")]
    settings: crate::settings_panel::SettingsHost,
    #[cfg(feature = "component-lab")]
    settings_focus: gpui::FocusHandle,
    /// UXE-03: resolved application icons by name; None is a known miss.
    #[cfg(feature = "component-lab")]
    app_icons: std::collections::BTreeMap<String, Option<std::path::PathBuf>>,
    #[cfg(feature = "component-lab")]
    app_icon_search: Option<gnosix_icon_theme::IconThemeSearch>,
    /// DECK-01A: pinned entries, the current appcards and the last Deck
    /// receipt.
    #[cfg(feature = "component-lab")]
    deck_pins: crate::deck::DeckPins,
    #[cfg(feature = "component-lab")]
    deck_cards: Vec<crate::deck::DeckCard>,
    #[cfg(feature = "component-lab")]
    deck_reported: Option<String>,
    /// Running appcard keys in first-observed order.
    #[cfg(feature = "component-lab")]
    deck_seen: Vec<String>,
    /// DECK-01B: the magnification wave, the shape built from it this frame,
    /// the rest Deck's left edge (the wave's coordinate origin), frames
    /// stepped since the last receipt and the last settled receipt.
    #[cfg(feature = "component-lab")]
    deck_wave: crate::deck::wave::DeckWave,
    #[cfg(feature = "component-lab")]
    deck_shape: Option<crate::deck::wave::DeckShape>,
    #[cfg(feature = "component-lab")]
    deck_rest_left: f32,
    #[cfg(feature = "component-lab")]
    deck_wave_frames: u32,
    #[cfg(feature = "component-lab")]
    deck_wave_reported: Option<String>,
    /// DECK-01D: whether the pins store may be written (it was read, or is
    /// absent), the action sheet and its rows, a press on an appcard, a
    /// drag and the last pointer over the scene.
    #[cfg(feature = "component-lab")]
    deck_pins_writable: bool,
    /// DECK-01F (B10): whether the pins of `deck.json` were offered to the
    /// profile once, and pins sent to the profile that it has not reflected
    /// yet (shown meanwhile, so nothing snaps back).
    #[cfg(all(feature = "component-lab", feature = "r12-production-client"))]
    deck_pins_imported: bool,
    #[cfg(all(feature = "component-lab", feature = "r12-production-client"))]
    deck_pins_pending: Option<(Vec<String>, Instant)>,
    #[cfg(feature = "component-lab")]
    deck_sheet: crate::deck::sheet::DeckSheet,
    #[cfg(feature = "component-lab")]
    deck_sheet_items: Vec<crate::deck::sheet::SheetItem>,
    #[cfg(feature = "component-lab")]
    deck_sheet_reported: Option<String>,
    #[cfg(feature = "component-lab")]
    deck_press: Option<deck_actions::DeckPress>,
    #[cfg(feature = "component-lab")]
    deck_drag: Option<crate::deck::drag::DeckDrag>,
    #[cfg(feature = "component-lab")]
    deck_last_pointer: Option<(f32, f32)>,
    #[cfg(feature = "component-lab")]
    deck_drag_reported: Option<String>,
    /// BAR-01B: the route of the travel in progress (for the conveyor), the
    /// qualification trace switch and its sequence, and whether the settled
    /// trace line was printed.
    #[cfg(feature = "component-lab")]
    bar_travel_route: Option<Vec<gnos_ux_surface::envelope::SurfaceEdge>>,
    /// BAR-02B: the Bar driven from the keyboard, and what the next frame
    /// does with the layer's keyboard.
    #[cfg(feature = "component-lab")]
    bar_keys: Option<bar_keys::BarKeys>,
    #[cfg(feature = "component-lab")]
    bar_keys_want: Option<bar_keys::BarKeysWant>,
    /// BAR-03: the module order the travel in hand started with, and
    /// whether it leaves the edge the profile named before (an operator's
    /// edge change; the reveal at start-up is not one and keeps the order).
    #[cfg(feature = "component-lab")]
    bar_travel_order: Option<(crate::reference_bar::BarModuleOrder, bool)>,
    /// BAR-03: the edge the profile named, and the one it named before:
    /// only a travel from that previous edge is the operator's edge change
    /// (the first one read at start-up is not).
    #[cfg(feature = "r12-production-client")]
    profile_edge: Option<SurfaceEdge>,
    #[cfg(feature = "r12-production-client")]
    profile_edge_before: Option<SurfaceEdge>,
    /// BAR-01J (B09): the relocation of the travel's last frame, the one a
    /// retarget interrupted, and the modules as painted then (and now).
    #[cfg(feature = "component-lab")]
    bar_last_relocation: Option<crate::rail_topology::Relocation>,
    #[cfg(feature = "component-lab")]
    bar_relocation_prior: Option<crate::rail_topology::Relocation>,
    #[cfg(feature = "component-lab")]
    bar_travel_handoff: Option<Vec<crate::bar_strip::ModulePlacement>>,
    #[cfg(feature = "component-lab")]
    bar_last_travel_placements: Option<Vec<crate::bar_strip::ModulePlacement>>,
    #[cfg(feature = "component-lab")]
    continuity_trace: bool,
    #[cfg(feature = "component-lab")]
    continuity_seq: u64,
    #[cfg(feature = "component-lab")]
    continuity_settled: bool,
    #[cfg(feature = "component-lab")]
    bar_travel_popout_sent: bool,
    /// The rail travels: Bar popouts take no pointer until it settles.
    #[cfg(feature = "component-lab")]
    bar_travelling: bool,
    /// BAR-01D: the motion clock (real time scaled by the profile's motion
    /// speed) and the rail metrics easing toward the profile's.
    motion_real_last: Option<Instant>,
    motion_virtual: Instant,
    rail_metrics_follow: [gnos_ux_surface::grammar::Follow; 3],
    rail_metrics_last: Option<Instant>,
    rail_metrics_frames: u32,
    /// BAR-01H: the toast drawer's eased offset off its edge (B07).
    toast_base: gnos_ux_surface::grammar::Follow,
    toast_base_target_value: f32,
    toast_base_last: Option<Instant>,
    /// BAR-01D: the rest layout last painted (with its side), and a settings
    /// reflow in progress (the layout it started from, and when).
    #[cfg(feature = "component-lab")]
    bar_rest_previous: Option<(
        crate::rail_topology::RailSide,
        Vec<crate::bar_strip::ModulePlacement>,
    )>,
    #[cfg(feature = "component-lab")]
    bar_relayout: Option<(Vec<crate::bar_strip::ModulePlacement>, Instant)>,
    /// BAR-01G: while the rail travels, the modules where they are painted
    /// and where the pointer router still sees them, and the last module
    /// the pointer was carried to.
    #[cfg(feature = "component-lab")]
    bar_travel_map: Option<(
        Vec<crate::bar_strip::ModulePlacement>,
        Vec<crate::bar_strip::ModulePlacement>,
    )>,
    #[cfg(feature = "component-lab")]
    bar_travel_pointer: Option<crate::reference_bar::IdleBarEntry>,
    /// The Dock node's region is part of this frame's input (BAR-01G trace).
    #[cfg(feature = "component-lab")]
    bar_dock_input: bool,
    /// BAR-01I (B08): a press waiting to lift a module, a carried module,
    /// a dropped order the profile has not reflected yet, the order shown
    /// before a lift, the others laid out without the carried one, the
    /// placements last painted through the travel layer, and the viewport.
    #[cfg(feature = "component-lab")]
    bar_press: Option<bar_drag::BarPress>,
    #[cfg(feature = "component-lab")]
    bar_press_tokens: u64,
    #[cfg(feature = "component-lab")]
    bar_drag: Option<bar_drag::BarDrag>,
    #[cfg(feature = "component-lab")]
    bar_order_pending: Option<(crate::reference_bar::BarModuleOrder, Instant)>,
    #[cfg(feature = "component-lab")]
    bar_rest_order: Option<crate::reference_bar::BarModuleOrder>,
    /// SET-02C: the modules hidden on a short edge, as last reported.
    #[cfg(feature = "component-lab")]
    bar_fit_hidden: Vec<crate::reference_bar::IdleBarEntry>,
    /// BAR-02A: the focused window's title, for the "…" panel.
    #[cfg(feature = "component-lab")]
    bar_active_title: Option<String>,
    /// SHELL-FIELD-01: the compositor's blur rule, as last asserted.
    #[cfg(feature = "component-lab")]
    compositor_blur: crate::compositor_blur::CompositorBlur,
    /// SHELL-FIELD-01B: the shell's shape, traced when the surfaces move.
    #[cfg(feature = "component-lab")]
    shell_field: crate::shell_field_paint::ShellFieldCache,
    /// SHELL-FIELD-01C: the open tray menu's extrusion, and its laid-out size.
    #[cfg(feature = "component-lab")]
    tray_menu: crate::tray_menu_extrusion::TrayMenuExtrusion,
    #[cfg(feature = "component-lab")]
    tray_menu_size: std::rc::Rc<std::cell::Cell<Option<(f32, f32)>>>,
    /// The rows each tray menu on the surface shows, by item: kept while
    /// it passes along (FOCUS-01B) or retracts, inert.
    #[cfg(feature = "component-lab")]
    tray_menu_rows: Vec<(
        usize,
        crate::product_tray_menu_surface::TrayMenuSurfacePresentation,
    )>,
    /// The item whose menu is asked for (FOCUS-01C).
    #[cfg(feature = "component-lab")]
    rail_tray_menu_key: Option<usize>,
    #[cfg(feature = "component-lab")]
    bar_rest_others: Option<(
        crate::rail_topology::RailSide,
        Vec<crate::bar_strip::ModulePlacement>,
    )>,
    #[cfg(feature = "component-lab")]
    bar_painted: Option<Vec<crate::bar_strip::ModulePlacement>>,
    #[cfg(feature = "component-lab")]
    bar_viewport: (f32, f32),
    /// UXE-04: last read-only popout receipt line.
    #[cfg(feature = "component-lab")]
    status_popout_reported: Option<String>,
    /// TOPO-01A: the rail side last reported.
    #[cfg(feature = "component-lab")]
    topology_reported: Option<crate::rail_topology::RailSide>,
    /// Last (phase, active edge) reported by `GNOS_UX_TOPO_TRANSITION`.
    topo_transition_reported: Option<(
        gnos_ux_surface::grammar::BarPlacementTransitionPhase,
        SurfaceEdge,
    )>,
    /// UXE-02: the process-wide notification service, when requested.
    _notification_subscription: Option<gpui::Subscription>,
    /// Which product surface is open.
    ///
    /// A bounded enum rather than a boolean, so "workspace summary and AppStack
    /// both open" is not a state the product can be in. See
    /// [`ProductPanelState`].
    product_panel: ProductPanelState,
    /// The AppStack's list scroll position, owned by the host so it survives the
    /// rerenders that a live product generates constantly.
    appstack_scroll: gpui::ScrollHandle,
    /// IME-aware search state. The parent observes it only to re-project rows;
    /// the text itself carries no launch authority.
    appstack_search: gpui::Entity<crate::product_appstack_search_input::AppStackSearchInput>,
    _appstack_search_subscription: gpui::Subscription,
    /// Wall-clock channels for the shared Dock/AppStack outer envelope.
    appstack_motion: crate::product_shared_envelope::SharedMotion,
    appstack_motion_previous: Option<Instant>,

    product_runtime: ProductRuntime,
    system_product_preferences: SystemProductPreferences,
    #[cfg(feature = "r13-production-client")]
    product_controller: ProductController,
    #[cfg(feature = "r12-production-client")]
    _profile_subscription: gpui::Subscription,
    #[cfg(feature = "r12-production-client")]
    pending_profile_edge: Option<SurfaceEdge>,
    _tray_subscription: Option<gpui::Subscription>,
    /// Read-only native PipeWire projection shared by every output host.
    _audio_subscription: Option<gpui::Subscription>,
    _network_subscription: Option<gpui::Subscription>,
    _status_subscriptions: Vec<gpui::Subscription>,
    product_pointer: Option<Point2>,
    bar_pointer_zone: BarPointerZone,
    bar_reveal_state: BarRevealState,
    product_bar_edge: SurfaceEdge,
    bar_transition_qualification: Option<(crate::product_bar::BarTransitionQualification, Instant)>,
    /// A qualified travel waiting for `--qualify-bar-delay-ms` (BAR-01F).
    bar_transition_pending: Option<crate::product_bar::BarTransitionQualification>,
    bar_transition_completion_notified: bool,
    /// BAR-01J (B09): the travel's last frame, where a new target rebases it,
    /// and its leg (the transition, its progress and that progress's rate per
    /// second), which slows to a stop before the Bar turns.
    bar_last_travel_frame: Option<gnos_ux_surface::grammar::ResolvedBarTransitionFrame>,
    bar_last_leg: Option<(
        Box<gnos_ux_surface::grammar::BarPlacementTransition>,
        f32,
        f32,
    )>,
    bar_transition_reduced_logged: bool,
    bar_transition_driver_started: Option<Instant>,
    product_text_scale: ProductTextScale,
    motion_preference: MotionPreferenceState,
    product_hull: Option<ReactiveEnvelopeSolver>,
    motion_driver: ProductMotionDriver,
    perf_counters: Option<Arc<ProductPerfCounters>>,
    perf_last_settled: Option<bool>,
    reactive_qualification_diagnostics: bool,
    a11y_tree_qualification_diagnostics: bool,
    a11y_tree_qualification_redraw_requests: u8,
    a11y_tree_qualification_dumped: bool,
    last_reactive_qualification_line: Option<String>,
    appstack_motion_trace: bool,
    appstack_motion_trace_frame: u64,
    appstack_motion_trace_target: Option<bool>,
    appstack_motion_trace_started: Option<Instant>,
    appstack_motion_trace_previous: Option<Instant>,
    motion_tick_previous: Option<Instant>,
    /// DICTATION-01: the last element rectangle reported, so the receipt is
    /// written when it moves and not on every frame.
    #[cfg(feature = "component-lab")]
    dictation_reported: Option<(f32, f32, f32, f32)>,
    /// DICTATION-01: the element's surface, so the compositor blurs behind it
    /// like it blurs behind the deck.
    #[cfg(feature = "component-lab")]
    dictation_surface: Option<(f32, f32, f32, f32)>,
    /// DICTATION-01: the element's motion — the extrusion, and the two lengths
    /// that follow the content.
    #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
    dictation_motion: crate::product_indicator::DictationMotion,
    /// DICTATION-01: when the motion was last stepped.
    #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
    dictation_previous: Option<Instant>,
    product_epoch: u64,
    /// Stable focus handles for the interactive product controls, keyed by
    /// durable product identity so focus survives unrelated semantic
    /// generations.
    #[cfg(feature = "r13-production-client")]
    product_focus: ProductFocusState,
    /// Opt-in, read-only projection of the scene's own focus ownership, used to
    /// qualify the production keyboard path against an AT-SPI observer. Enabled
    /// only by `GNOS_UX_QUALIFY_FOCUS`; it emits stdout lines and changes no
    /// product behaviour.
    /// Shared across every output host, so at most one product control in
    /// the whole session can own focus. Owned by the topology runtime and
    /// handed down; deliberately not a process-global.
    #[cfg(feature = "r13-production-client")]
    focus_coordinator: ProductFocusCoordinator,
    /// Where every host's window lives, so focus arbitration can reach the
    /// host that lost it.
    #[cfg(feature = "r13-production-client")]
    host_windows: ProductHostWindows,
    /// This host's own identity within the session.
    #[cfg(feature = "r13-production-client")]
    host_id: ProductHostId,
    #[cfg(feature = "r13-production-client")]
    key_qualification_diagnostics: bool,
    #[cfg(feature = "r13-production-client")]
    focus_qualification_diagnostics: bool,
    /// Last reported focus state per control, so the diagnostic prints on
    /// transitions only rather than once per frame.
    #[cfg(feature = "r13-production-client")]
    focus_report: std::collections::BTreeMap<String, bool>,
    scene: Option<MonitorScene>,
    frame: Option<SceneFrame>,
    projection_qualification_enabled: bool,
    projection_worker_started: bool,
    completion_logged: bool,
    host_token: u64,
    previous_step: Option<SceneCycleStep>,
    previous_viewport: Option<SurfaceViewport>,
    surface_fence: IntentFence,
    applied_reservation: Option<(SurfaceEdge, i32)>,
    initial_reservation: Option<(SurfaceEdge, i32)>,
    reservation_changed_from_initial: bool,
    reservation_return_logged: bool,
}

impl SceneHost {
    #[allow(clippy::too_many_arguments)]
    fn new(
        scene_id: MonitorSceneId,
        runtime_output: Option<RuntimeId>,
        output_origin: (i32, i32),
        tray_qualification: Option<TrayQualificationRequest>,
        semantic_consumer: SceneSemanticConsumer,
        application_catalog: Arc<ApplicationCatalog>,
        #[cfg_attr(not(feature = "r13-production-client"), allow(unused_variables))]
        focus_coordinator: ProductFocusCoordinator,
        #[cfg_attr(not(feature = "r13-production-client"), allow(unused_variables))]
        host_windows: ProductHostWindows,
        cx: &mut Context<Self>,
    ) -> Self {
        let args = env::args().collect::<Vec<_>>();
        let cycle_enabled = args.iter().any(|arg| arg == "--scene-cycle");
        let product_mode = args.iter().any(|arg| arg == "--product-scene");
        let product_diagnostics =
            product_mode && args.iter().any(|arg| arg == "--product-diagnostics");
        let bar_transition_qualification = crate::product_bar::qualification_transition(&args)
            .expect("validated Bar transition qualification selector")
            .map(|plan| (plan, Instant::now()));
        // BAR-01F (development builds): the qualified travel can wait until
        // the scene's content has settled, so every module is there before
        // it starts and a popout can be opened first.
        let travel_delay = cfg!(debug_assertions)
            .then(|| {
                args.iter()
                    .find_map(|arg| arg.strip_prefix("--qualify-bar-delay-ms="))
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|ms| (1..=60_000).contains(ms))
            })
            .flatten()
            .map(Duration::from_millis);
        let (bar_transition_qualification, bar_transition_pending) =
            match (bar_transition_qualification, travel_delay) {
                (Some((plan, _)), Some(delay)) => {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(delay).await;
                        let _ = this.update(cx, |view, cx| {
                            if let Some(plan) = view.bar_transition_pending.take() {
                                println!(
                                    "GNOS_UX_BAR_TRAVEL state=start delayed_ms={}",
                                    delay.as_millis()
                                );
                                view.bar_transition_qualification = Some((plan, Instant::now()));
                                #[cfg(feature = "component-lab")]
                                {
                                    view.continuity_settled = false;
                                }
                                view.start_motion_driver(cx);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                    (None, Some(plan))
                }
                (qualification, _) => (qualification, None),
            };
        let cli_reduced_motion = args.iter().any(|arg| arg == "--reduced-motion");
        let motion_preference = MotionPreferenceState::new(cli_reduced_motion);
        let (product_text_scale, product_text_scale_source) =
            match ProductTextScale::from_args(&args) {
                Ok(scale) => (
                    scale,
                    if args
                        .iter()
                        .any(|arg| arg.starts_with("--product-text-scale="))
                    {
                        "qualification_cli"
                    } else {
                        "product_candidate"
                    },
                ),
                Err(error) => {
                    eprintln!("GNOS_UX_PRODUCT_TEXT_SCALE status=invalid reason={error}");
                    (ProductTextScale::default(), "invalid_fallback")
                }
            };
        let perf_counters =
            (product_mode && env::var_os("GNOS_UX_QUALIFY_PERF").is_some()).then(|| {
                Arc::new(ProductPerfCounters {
                    started: Some(Instant::now()),
                    ..ProductPerfCounters::default()
                })
            });
        let reactive_qualification_diagnostics =
            env::var_os("GNOS_UX_QUALIFY_REACTIVE_DIAGNOSTICS").is_some();
        let appstack_motion_trace =
            product_mode && env::var_os("GNOS_UX_QUALIFY_APPSTACK_MOTION_TRACE").is_some();
        let a11y_tree_qualification_diagnostics =
            env::var_os("GNOS_UX_QUALIFY_A11Y_TREE").is_some();
        #[cfg(feature = "r13-production-client")]
        let key_qualification_diagnostics = env::var_os("GNOS_UX_QUALIFY_FOCUS").is_some();
        #[cfg(feature = "r13-production-client")]
        let focus_qualification_diagnostics = key_qualification_diagnostics;
        let projection_qualification_enabled = args
            .iter()
            .any(|arg| arg == "--sdk6-projection-qualification");
        let pid = std::process::id();
        let host_token = (u64::from(pid) << 32) | scene_id.value();
        let output_label = runtime_output
            .as_ref()
            .map(RuntimeId::as_str)
            .unwrap_or("COMPOSITOR_SELECTED");
        let watcher_output = output_label.to_owned();
        let mut semantic_revisions = semantic_consumer.is_enabled().then(|| {
            semantic_consumer
                .subscribe()
                .expect("subscribe to scene semantics")
        });
        let last_projection_revision = semantic_consumer.revision();
        let initial_projection = runtime_output
            .as_ref()
            .and_then(|output| semantic_consumer.get(Some(output)));
        let mut product_runtime =
            ProductRuntime::from_projection(runtime_output.as_ref(), initial_projection.as_ref());
        product_runtime.set_application_catalog(application_catalog);
        let system_product_preferences = SystemProductPreferences::default();
        let effective_product_preferences = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            system_product_preferences,
        );
        #[cfg(feature = "r12-production-client")]
        let profile_session = crate::product_profile::ProfileSession::shared(cx);
        #[cfg(feature = "r12-production-client")]
        let effective_product_preferences = {
            let mut effective = effective_product_preferences;
            crate::product_profile::apply_bar_projection(
                &mut effective,
                profile_session.read(cx).snapshot.as_ref(),
            );
            effective
        };
        #[cfg(feature = "r12-production-client")]
        let profile_subscription = cx.observe(&profile_session, |this, session, cx| {
            let snapshot = session.read(cx).snapshot.clone();
            let mut preferences = this.product_runtime.preferences().clone();
            crate::product_profile::apply_bar_projection(&mut preferences, snapshot.as_ref());
            let edge = if this.review_native_rail {
                crate::product_preferences::native_rail_edge(&preferences)
            } else {
                crate::product_preferences::bar_surface_edge(&preferences)
            };
            this.product_runtime.set_preferences(preferences);
            // An unavailable projection cannot manufacture a normal/default target.
            this.pending_profile_edge = edge;
            if edge != this.profile_edge {
                this.profile_edge_before = this.profile_edge;
                this.profile_edge = edge;
            }
            #[cfg(feature = "component-lab")]
            {
                let session = session.read(cx);
                println!(
                    "GNOS_UX_UXE05_PROFILE status={:?} revision={:?} edge={:?} overrides={:?}",
                    session.status,
                    this.product_runtime.preferences().profile_revision,
                    this.product_runtime.preferences().bar_edge,
                    this.product_runtime.preferences().bar_overrides
                );
            }
            this.consume_profile_edge();
            this.start_motion_driver(cx);
            cx.notify();
        });
        let native_rail_requested = product_mode
            && cfg!(feature = "component-lab")
            && args.iter().any(|arg| arg == "--review-native-rail");
        let projected_bar_edge = if native_rail_requested {
            crate::product_preferences::native_rail_edge(&effective_product_preferences)
        } else {
            crate::product_preferences::bar_surface_edge(&effective_product_preferences)
        };
        product_runtime.set_preferences(effective_product_preferences);
        let appstack_search =
            cx.new(crate::product_appstack_search_input::AppStackSearchInput::new);
        let appstack_search_subscription = cx.observe(&appstack_search, |this, _, cx| {
            this.appstack_scroll.scroll_to_item(0);
            #[cfg(feature = "component-lab")]
            if this.review_native_rail && this.stack.is_open() && this.refresh_stack_rows(cx) {
                this.report_stack("query");
                this.start_motion_driver(cx);
            }
            cx.notify();
        });
        let tray_controller = crate::product_tray_controller::shared_controller(cx);
        // SET-02: a setting changed from any output's Settings reaches every
        // output at once.
        #[cfg(feature = "component-lab")]
        {
            // BAR-02B: SIGUSR1 asks the focused output to take the keyboard.
            let bar_keys_signal = bar_keys::BarKeysSignal::shared(cx);
            cx.observe(&bar_keys_signal, |this, signal, cx| {
                if this.review_native_rail {
                    let output = signal.read(cx).output.clone();
                    this.bar_keys_signal(output.as_deref(), cx);
                }
            })
            .detach();
            let settings_changed = crate::surface_prefs::SettingsChanged::shared(cx);
            cx.observe(&settings_changed, |this, _, cx| {
                if this.review_native_rail {
                    this.sync_desktop(cx);
                    // The desktop modules' sizes are read when it draws.
                    if let Some(model) = this.desktop_state.model.clone() {
                        model.update(cx, |_, cx| cx.notify());
                    }
                }
                cx.notify();
            })
            .detach();
        }
        let menu_subscription = cx.observe(&tray_controller, |this, controller, cx| {
            let menu_open = controller.read(cx).menu().is_some();
            // Only the output that opened the menu judges it, against the
            // shared tray session (the source of truth). Every output used to
            // judge it against its own copy, and an output still holding the
            // startup snapshot (owners not resolved yet) closed the first
            // menu opened after a restart for all of them.
            let here = this
                .runtime_output
                .as_ref()
                .map(|output| output.as_str().to_owned());
            // A menu no output claims is judged by every output, as before.
            let owned_here = controller
                .read(cx)
                .open_requested_by()
                .is_none_or(|by| Some(by) == here.as_deref());
            if menu_open && owned_here {
                let live = crate::product_tray::TraySession::shared(cx)
                    .read(cx)
                    .snapshot()
                    .clone();
                let open_item = controller.read(cx).open_item().cloned();
                let verdict = match open_item {
                    None => Some("no_open_item".to_owned()),
                    Some(item) => {
                        let present = live.items().iter().any(|live| {
                            live.id == item.id
                                && live.service == item.service
                                && live.path == item.path
                        });
                        let live_owner = live
                            .items()
                            .iter()
                            .find(|live| live.service == item.service)
                            .map(|live| live.owner.clone());
                        let owner_ok = live_owner.as_deref() == Some(item.owner.as_str());
                        (!present || !owner_ok).then(|| {
                            format!(
                                "present={present} owner_ok={owner_ok} item={} service={} path={} owner={} live_owner={} live_items={}",
                                item.id.as_str(),
                                item.service,
                                item.path,
                                item.owner,
                                live_owner.as_deref().unwrap_or("none"),
                                live.items().len()
                            )
                        })
                    }
                };
                if let Some(detail) = verdict {
                    // Item disappeared or owner changed: retain no authority.
                    controller.update(cx, |this, _cx| {
                        this.clear_menu();
                    });
                    println!(
                        "GNOS_UX_TRAY_MENU_DISMISS reason=item_gone_or_owner_changed {detail}"
                    );
                }
            }
            cx.notify();
        });
        let tray_subscription = if product_mode {
            let session = crate::product_tray::TraySession::shared(cx);
            product_runtime.set_tray(session.read(cx).snapshot().clone());
            Some(cx.observe(&session, |this, session, cx| {
                let snapshot = session.read(cx).snapshot().clone();
                if this.product_runtime.set_tray(snapshot) {
                    let items_count = match this.product_runtime.tray() {
                        crate::product_tray::TraySnapshot::Active { items } => items.len(),
                        crate::product_tray::TraySnapshot::Unavailable => 0,
                    };
                    println!(
                        "GNOS_UX_TRAY_RECEIPT available={} items={} output={}",
                        this.product_runtime.tray().is_available(),
                        items_count,
                        this.runtime_output
                            .as_ref()
                            .map_or("UNKNOWN", RuntimeId::as_str)
                    );
                    cx.notify();
                }
                // A bounded qualification request is served against the
                // snapshot the provider just published.
                this.serve_tray_qualification(cx);
            }))
        } else {
            None
        };
        // UXE-02: opt-in native notification service. It never displaces a
        // running service (G01); `NameTaken` simply shows no toasts.
        #[cfg(feature = "component-lab")]
        let notification_subscription = (product_mode
            && args.iter().any(|arg| arg == "--review-native-rail")
            && args.iter().any(|arg| arg == "--notification-service"))
        .then(|| {
            let session = crate::product_notification_session::NotificationSession::shared(cx);
            crate::product_notification_session::ensure_service(&session, cx);
            cx.observe(&session, |this, _, cx| {
                // SETTINGS-03B: the service gets the stored preferences
                // as soon as it serves.
                this.notification_prefs_sync(cx);
                this.start_motion_driver(cx);
                cx.notify();
            })
        });
        #[cfg(not(feature = "component-lab"))]
        let notification_subscription: Option<gpui::Subscription> = None;
        let audio_subscription = if product_mode {
            let session = crate::product_audio_session::AudioSession::shared(cx);
            crate::product_audio_session::ensure_observation(&session, cx);
            product_runtime.set_audio(session.read(cx).snapshot().clone());
            Some(cx.observe(&session, |this, session, cx| {
                let snapshot = session.read(cx).snapshot().clone();
                if this.product_runtime.set_audio(snapshot.clone()) {
                    // AUDIO-01 Phase 5. The OSD consumes the observation that
                    // ProductRuntime has just accepted, so it presents what
                    // PipeWire reported and nothing else. There is deliberately
                    // no reference to any intent or dispatch here: an OSD that
                    // only followed Gnosix's own button would be a side effect
                    // of the Bar rather than a projection of audio truth, and
                    // would need rework the moment a keybinding mutates volume.
                    let osd = crate::product_audio_osd::AudioOsd::shared(cx);
                    let design = this.product_design_context();
                    if let Some(event) = osd.update(cx, |osd, cx| osd.observe(&snapshot, design, cx))
                    {
                        println!(
                            "GNOS_UX_AUDIO_OSD event cause={} observed_level={} observed_muted={} generation={} scene_output={}",
                            event.cause.as_str(),
                            event
                                .truth
                                .level
                                .map(|level| format!("{level:.3}"))
                                .unwrap_or_else(|| "unknown".to_owned()),
                            event.truth.muted,
                            event.truth.generation,
                            this.runtime_output
                                .as_ref()
                                .map_or("UNKNOWN", RuntimeId::as_str),
                        );
                    }
                    // SURF-01C: the OSD from the Bar presents the same
                    // accepted observation (output and microphone).
                    #[cfg(feature = "component-lab")]
                    this.observe_osd_audio(cx);
                    let presentation = this.product_runtime.audio_presentation();
                    let output = match presentation.output {
                        crate::product_audio::ProductAudioEndpointState::ProviderUnavailable => {
                            "provider_unavailable".to_owned()
                        }
                        crate::product_audio::ProductAudioEndpointState::DefaultUnresolved {
                            ..
                        } => "default_unresolved".to_owned(),
                        crate::product_audio::ProductAudioEndpointState::Ready(output) => format!(
                            "ready level={} muted={}",
                            output
                                .level
                                .map(|level| format!("{level:.3}"))
                                .unwrap_or_else(|| "unknown".to_owned()),
                            output.muted
                        ),
                    };
                    println!(
                        "GNOS_UX_AUDIO_RECEIPT generation={} output={} scene_output={}",
                        this.product_runtime.audio().generation,
                        output,
                        this.runtime_output
                            .as_ref()
                            .map_or("UNKNOWN", RuntimeId::as_str)
                    );
                    cx.notify();
                }
            }))
        } else {
            None
        };
        let network_subscription = if product_mode {
            // NETWORK-01 Phase 3. Read-only: the session observes
            // NetworkManager and publishes typed snapshots; the runtime accepts
            // them; the Bar derives status from the runtime. There is no
            // dispatch sink and no intent anywhere on this path.
            let session = crate::product_network_session::NetworkSession::shared(cx);
            crate::product_network_session::ensure_observation(&session, cx);
            product_runtime.set_network(session.read(cx).snapshot().clone());
            Some(cx.observe(&session, |this, session, cx| {
                let snapshot = session.read(cx).snapshot().clone();
                if this.product_runtime.set_network(snapshot) {
                    let presentation = this.product_runtime.network_presentation();
                    println!(
                        "GNOS_UX_NETWORK_RECEIPT generation={} provider={} primary={} transport={} \
connectivity={} checked={} overlay={} scene_output={}",
                        this.product_runtime.network().generation,
                        presentation.provider.as_str(),
                        match &presentation.primary {
                            crate::product_network::ProductNetworkPrimary::Unavailable =>
                                "unavailable",
                            crate::product_network::ProductNetworkPrimary::Disconnected =>
                                "disconnected",
                            crate::product_network::ProductNetworkPrimary::Connecting =>
                                "connecting",
                            crate::product_network::ProductNetworkPrimary::Ready(_) => "ready",
                        },
                        match &presentation.primary {
                            crate::product_network::ProductNetworkPrimary::Ready(endpoint) =>
                                endpoint.transport.as_str(),
                            _ => "none",
                        },
                        presentation.connectivity.as_str(),
                        presentation.connectivity_checked,
                        presentation.overlay_active,
                        this.runtime_output
                            .as_ref()
                            .map_or("UNKNOWN", RuntimeId::as_str)
                    );
                    cx.notify();
                }
            }))
        } else {
            None
        };
        // PARITY-01H. Read-only Clock and Status facts follow the network
        // pattern: one observer per process, typed values into ProductRuntime,
        // one receipt line per accepted change. No intent exists on this path.
        let status_subscriptions = if product_mode {
            use crate::product_status_session::{
                BluetoothSource, ClockSource, KeyboardLockSource, PowerSource, StatusSession,
                ensure_observation,
            };
            let clock = StatusSession::<ClockSource>::shared(cx);
            let locks = StatusSession::<KeyboardLockSource>::shared(cx);
            let bluetooth = StatusSession::<BluetoothSource>::shared(cx);
            let power = StatusSession::<PowerSource>::shared(cx);
            ensure_observation(&clock, cx);
            ensure_observation(&locks, cx);
            ensure_observation(&bluetooth, cx);
            ensure_observation(&power, cx);
            product_runtime.set_clock(*clock.read(cx).value());
            product_runtime.set_keyboard_locks(*locks.read(cx).value());
            product_runtime.set_bluetooth(bluetooth.read(cx).value().clone());
            product_runtime.set_power(*power.read(cx).value());
            vec![
                cx.observe(&clock, |this, session, cx| {
                    let value = *session.read(cx).value();
                    if this.product_runtime.set_clock(value) {
                        this.print_status_receipt("clock", cx);
                    }
                }),
                cx.observe(&locks, |this, session, cx| {
                    let value = *session.read(cx).value();
                    if this.product_runtime.set_keyboard_locks(value) {
                        this.print_status_receipt("locks", cx);
                    }
                }),
                cx.observe(&bluetooth, |this, session, cx| {
                    let value = session.read(cx).value().clone();
                    if this.product_runtime.set_bluetooth(value) {
                        this.print_status_receipt("bluetooth", cx);
                    }
                }),
                cx.observe(&power, |this, session, cx| {
                    let value = *session.read(cx).value();
                    if this.product_runtime.set_power(value) {
                        this.print_status_receipt("power", cx);
                    }
                }),
            ]
        } else {
            Vec::new()
        };
        let bar_reveal_state =
            BarInteractionPolicy::from_preferences(product_runtime.preferences()).initial_state();

        println!(
            "GNOS_UX_SCENE_START pid={} identity={} namespace={} host_token={} scene_id={} output={} scene_grammar={}@{} cycle={}",
            pid,
            SCENE_IDENTITY,
            SCENE_NAMESPACE,
            host_token,
            scene_id.value(),
            output_label,
            SCENE_GRAMMAR_ID,
            SCENE_GRAMMAR_REVISION,
            cycle_enabled,
        );

        if product_mode {
            println!(
                "GNOS_UX_PRODUCT_TEXT_SCALE factor={:.2} font_size_px={:.2} source={}",
                product_text_scale.factor(),
                product_text_scale.font_size_px(),
                product_text_scale_source
            );
            if product_diagnostics {
                println!(
                    "GNOS_UX_PRODUCT_DIAGNOSTICS output={} status=enabled source=in_process_product_runtime",
                    output_label
                );
            }
        }

        if product_mode {
            if cli_reduced_motion {
                println!(
                    "GNOS_UX_MOTION_PREFERENCE output={} source=cli value=reduce effective=reduce",
                    output_label
                );
            } else {
                println!(
                    "GNOS_UX_MOTION_PREFERENCE output={} source=portal status=starting",
                    output_label
                );
                let watcher_output = watcher_output.clone();
                cx.spawn(async move |this, cx| {
                    let settings = match Settings::new().await {
                        Ok(settings) => settings,
                        Err(_) => {
                            eprintln!(
                                "GNOS_UX_MOTION_PREFERENCE output={} source=portal value=no_preference reason=portal_unavailable",
                                watcher_output
                            );
                            return Some(());
                        }
                    };
                    let portal_version = settings.version();
                    if portal_version < 2 {
                        eprintln!(
                            "GNOS_UX_MOTION_PREFERENCE output={} source=portal value=no_preference reason=portal_version_below_2",
                            watcher_output
                        );
                        return Some(());
                    }
                    let mut changes = match settings
                        .receive_setting_changed_with_args::<u32>(
                            "org.freedesktop.appearance",
                            "reduced-motion",
                        )
                        .await
                    {
                        Ok(changes) => changes,
                        Err(_) => {
                            eprintln!(
                                "GNOS_UX_MOTION_PREFERENCE output={} source=portal value=no_preference reason=subscribe_failed",
                                watcher_output
                            );
                            return Some(());
                        }
                    };
                    let initial = settings
                        .read::<u32>("org.freedesktop.appearance", "reduced-motion")
                        .await
                        .ok();
                    let initial =
                        MotionPreference::from_portal_setting(Some(portal_version), initial);
                    if this
                        .update(cx, |view, cx| view.set_system_motion_preference(initial, cx))
                        .is_err()
                    {
                        return Some(());
                    }
                    while let Some(value) = changes.next().await {
                        let preference = MotionPreference::from_portal_setting(
                            Some(portal_version),
                            value.ok(),
                        );
                        if this
                            .update(cx, |view, cx| {
                                view.set_system_motion_preference(preference, cx)
                            })
                            .is_err()
                        {
                            return Some(());
                        }
                    }
                    let _ = this.update(cx, |view, cx| {
                        view.set_system_motion_preference(MotionPreference::NoPreference, cx)
                    });
                    Some(())
                })
                .detach();
            }
        }

        if product_mode {
            let watcher_output = watcher_output.clone();
            cx.spawn(async move |this, cx| {
                let settings = match Settings::new().await {
                    Ok(settings) => settings,
                    Err(_) => {
                        eprintln!(
                            "GNOS_UX_EFFECTIVE_PREFERENCES output={} field=color_scheme source=portal reason=portal_unavailable",
                            watcher_output
                        );
                        return Some(());
                    }
                };
                let map_scheme = |scheme: PortalColorScheme| match scheme {
                    PortalColorScheme::PreferDark => Some(ProductColorScheme::Dark),
                    PortalColorScheme::PreferLight => Some(ProductColorScheme::Light),
                    PortalColorScheme::NoPreference => None,
                };
                let initial = settings.color_scheme().await.ok().and_then(map_scheme);
                if this
                    .update(cx, |view, cx| view.set_system_color_scheme(initial, cx))
                    .is_err()
                {
                    return Some(());
                }
                let mut changes = match settings.receive_color_scheme_changed().await {
                    Ok(changes) => changes,
                    Err(_) => {
                        eprintln!(
                            "GNOS_UX_EFFECTIVE_PREFERENCES output={} field=color_scheme source=portal reason=subscribe_failed",
                            watcher_output
                        );
                        return Some(());
                    }
                };
                while let Some(scheme) = changes.next().await {
                    if this
                        .update(cx, |view, cx| {
                            view.set_system_color_scheme(map_scheme(scheme), cx)
                        })
                        .is_err()
                    {
                        return Some(());
                    }
                }
                let _ = this.update(cx, |view, cx| view.set_system_color_scheme(None, cx));
                Some(())
            })
            .detach();
        }

        if let Some(mut revisions) = semantic_revisions.take() {
            cx.spawn(async move |this, cx| {
                while revisions.next().await.is_some() {
                    if this
                        .update(cx, |view, cx| {
                            view.notify_projection_revision(cx);
                            if view.product_mode && view.advance_product_motion() {
                                view.start_motion_driver(cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }

        // The indicator's state comes from publishers on the session bus. A
        // change there is a frame to draw, not a new semantic generation.
        #[cfg(feature = "indicator-production-client")]
        if let Ok(mut revisions) = crate::product_indicator::registry().subscribe() {
            cx.spawn(async move |this, cx| {
                while revisions.next().await.is_some() {
                    if this
                        .update(cx, |_view, cx| {
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }

        // DICTATION-01: the element's motion is only visible if the scene
        // redraws while it moves. The scene redraws when the state changes,
        // which is once a second — the followers would jump instead of moving.
        #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let moving = this
                    .update(cx, |view, _| {
                        view.dictation_motion.extrusion.needs_frame()
                            || view.dictation_motion.height.velocity.abs() > 0.5
                            || view.dictation_motion.reveal.velocity.abs() > 0.5
                    })
                    .unwrap_or(false);
                if moving {
                    let _ = this.update(cx, |_view, cx| cx.notify());
                } else {
                    // Nothing moves: do not wake the scene every frame for
                    // nothing.
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                }
            }
        })
        .detach();

        if cycle_enabled {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this
                        .update(cx, |view, cx| {
                            view.advance_scene();
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }

        if perf_counters.is_some() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    if this.update(cx, |view, _| view.log_perf_sample()).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }

        if a11y_tree_qualification_diagnostics {
            cx.spawn(async move |this, cx| {
                for _ in 0..3 {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let Ok(continue_requests) = this.update(cx, |view, cx| {
                        if view.a11y_tree_qualification_dumped {
                            return false;
                        }
                        view.a11y_tree_qualification_redraw_requests += 1;
                        cx.notify();
                        true
                    }) else {
                        break;
                    };
                    if !continue_requests {
                        break;
                    }
                }
            })
            .detach();
        }

        #[cfg(feature = "component-lab")]
        let deck_pins =
            load_deck_pins(product_mode && args.iter().any(|arg| arg == "--review-native-rail"));
        Self {
            #[cfg(feature = "r13-production-client")]
            product_focus: ProductFocusState::new(),
            #[cfg(feature = "r13-production-client")]
            focus_coordinator,
            #[cfg(feature = "r13-production-client")]
            host_windows,
            #[cfg(feature = "r13-production-client")]
            host_id: ProductHostId(scene_id.value()),
            #[cfg(feature = "r13-production-client")]
            key_qualification_diagnostics,
            #[cfg(feature = "r13-production-client")]
            focus_qualification_diagnostics,
            #[cfg(feature = "r13-production-client")]
            focus_report: std::collections::BTreeMap::new(),
            scene_id,
            mpris_process_owns: {
                claim_mpris_interaction_owner(cx);
                MPRIS_PROCESS_OWNS.get_or_init(Default::default).clone()
            },
            tray_process_owns: {
                claim_tray_interaction_owner(cx);
                TRAY_PROCESS_OWNS.get_or_init(Default::default).clone()
            },
            runtime_output,
            output_origin,
            tray_qualification,
            tray_qualification_attempts: 0,
            tray_menu_row_target: None,
            #[cfg(feature = "component-lab")]
            rail_tray_menu_at: None,
            tray_controller: crate::product_tray_controller::shared_controller(cx),
            semantic_consumer,
            last_projection_revision,
            product_mode,
            product_diagnostics,
            review_native_rail: product_mode
                && cfg!(feature = "component-lab")
                && args.iter().any(|arg| arg == "--review-native-rail"),
            #[cfg(feature = "component-lab")]
            reference_visual: (product_mode
                && args.iter().any(|arg| arg == "--review-native-rail"))
            .then(|| {
                crate::product_reference_consumer::ReferenceVisualConsumer::new(cx.text_system())
            }),
            product_panel: ProductPanelState::Closed,
            #[cfg(feature = "component-lab")]
            reference_workspace_hover_owned: false,
            #[cfg(feature = "component-lab")]
            bar_popout: crate::bar_popout_host::BarPopoutHost::new(),
            #[cfg(feature = "component-lab")]
            utilities: crate::utilities_panel::UtilitiesPanel::new(),
            #[cfg(feature = "component-lab")]
            utilities_state: Default::default(),
            #[cfg(feature = "component-lab")]
            bar_osd: crate::bar_osd::BarOsd::new(),
            #[cfg(feature = "component-lab")]
            osd_state: Default::default(),
            #[cfg(feature = "component-lab")]
            session: crate::session_panel::SessionPanel::new(),
            #[cfg(feature = "component-lab")]
            session_state: Default::default(),
            #[cfg(feature = "component-lab")]
            workspaces: crate::workspace_panel::WorkspacePanel::new(),
            #[cfg(feature = "component-lab")]
            attention: crate::attention::Attention::new(),
            #[cfg(feature = "component-lab")]
            attention_state: Default::default(),
            #[cfg(feature = "component-lab")]
            cursor_at: None,
            #[cfg(feature = "component-lab")]
            cursor_reported: None,
            #[cfg(feature = "component-lab")]
            launching: Vec::new(),
            #[cfg(feature = "component-lab")]
            workspaces_state: Default::default(),
            #[cfg(feature = "component-lab")]
            overflow: crate::overflow_panel::OverflowPanel::new(),
            #[cfg(feature = "component-lab")]
            dashboard: crate::dashboard_panel::DashboardPanel::new(),
            #[cfg(feature = "component-lab")]
            dashboard_state: Default::default(),
            #[cfg(feature = "component-lab")]
            swallow_click: false,
            #[cfg(feature = "component-lab")]
            desktop_state: Default::default(),
            #[cfg(feature = "component-lab")]
            notification_toasts: crate::notification_toasts::ToastHost::new(),
            #[cfg(feature = "component-lab")]
            notification_centre: crate::notification_centre::CentreHost::new(),
            #[cfg(feature = "component-lab")]
            stack: crate::deck::stack::StackHost::new(),
            #[cfg(feature = "component-lab")]
            settings: crate::settings_panel::SettingsHost::new(),
            #[cfg(feature = "component-lab")]
            settings_focus: cx.focus_handle(),
            #[cfg(feature = "component-lab")]
            app_icons: std::collections::BTreeMap::new(),
            #[cfg(feature = "component-lab")]
            app_icon_search: None,
            #[cfg(feature = "component-lab")]
            deck_pins: deck_pins.0,
            #[cfg(feature = "component-lab")]
            deck_pins_writable: deck_pins.1,
            #[cfg(all(feature = "component-lab", feature = "r12-production-client"))]
            deck_pins_imported: false,
            #[cfg(all(feature = "component-lab", feature = "r12-production-client"))]
            deck_pins_pending: None,
            #[cfg(feature = "component-lab")]
            deck_sheet: Default::default(),
            #[cfg(feature = "component-lab")]
            deck_sheet_items: Vec::new(),
            #[cfg(feature = "component-lab")]
            deck_sheet_reported: None,
            #[cfg(feature = "component-lab")]
            deck_press: None,
            #[cfg(feature = "component-lab")]
            deck_drag: None,
            #[cfg(feature = "component-lab")]
            deck_last_pointer: None,
            #[cfg(feature = "component-lab")]
            deck_drag_reported: None,
            #[cfg(feature = "component-lab")]
            bar_travel_route: None,
            #[cfg(feature = "component-lab")]
            bar_travel_order: None,
            #[cfg(feature = "component-lab")]
            bar_keys: None,
            #[cfg(feature = "component-lab")]
            bar_keys_want: None,
            #[cfg(feature = "r12-production-client")]
            profile_edge: None,
            #[cfg(feature = "r12-production-client")]
            profile_edge_before: None,
            #[cfg(feature = "component-lab")]
            bar_last_relocation: None,
            #[cfg(feature = "component-lab")]
            bar_relocation_prior: None,
            #[cfg(feature = "component-lab")]
            bar_travel_handoff: None,
            #[cfg(feature = "component-lab")]
            bar_last_travel_placements: None,
            #[cfg(feature = "component-lab")]
            continuity_trace: cfg!(debug_assertions)
                && args.iter().any(|arg| arg == "--qualify-continuity-trace"),
            #[cfg(feature = "component-lab")]
            continuity_seq: 0,
            #[cfg(feature = "component-lab")]
            continuity_settled: true,
            #[cfg(feature = "component-lab")]
            bar_travel_popout_sent: false,
            #[cfg(feature = "component-lab")]
            bar_travelling: false,
            motion_real_last: None,
            motion_virtual: Instant::now(),
            rail_metrics_follow: shell_prefs::default_metrics_follow(),
            rail_metrics_last: None,
            rail_metrics_frames: 0,
            toast_base: gnos_ux_surface::grammar::Follow::at(0.0),
            toast_base_target_value: 0.0,
            toast_base_last: None,
            #[cfg(feature = "component-lab")]
            bar_rest_previous: None,
            #[cfg(feature = "component-lab")]
            bar_relayout: None,
            #[cfg(feature = "component-lab")]
            bar_travel_map: None,
            #[cfg(feature = "component-lab")]
            bar_travel_pointer: None,
            #[cfg(feature = "component-lab")]
            bar_dock_input: false,
            #[cfg(feature = "component-lab")]
            bar_press: None,
            #[cfg(feature = "component-lab")]
            bar_press_tokens: 0,
            #[cfg(feature = "component-lab")]
            bar_drag: None,
            #[cfg(feature = "component-lab")]
            bar_order_pending: None,
            #[cfg(feature = "component-lab")]
            bar_rest_order: None,
            #[cfg(feature = "component-lab")]
            bar_fit_hidden: Vec::new(),
            #[cfg(feature = "component-lab")]
            bar_active_title: None,
            #[cfg(feature = "component-lab")]
            compositor_blur: Default::default(),
            #[cfg(feature = "component-lab")]
            shell_field: Default::default(),
            #[cfg(feature = "component-lab")]
            tray_menu: Default::default(),
            #[cfg(feature = "component-lab")]
            tray_menu_size: Default::default(),
            #[cfg(feature = "component-lab")]
            tray_menu_rows: Vec::new(),
            #[cfg(feature = "component-lab")]
            rail_tray_menu_key: None,
            #[cfg(feature = "component-lab")]
            bar_rest_others: None,
            #[cfg(feature = "component-lab")]
            bar_painted: None,
            #[cfg(feature = "component-lab")]
            bar_viewport: (1.0, 1.0),
            #[cfg(feature = "component-lab")]
            deck_cards: Vec::new(),
            #[cfg(feature = "component-lab")]
            deck_reported: None,
            #[cfg(feature = "component-lab")]
            deck_seen: Vec::new(),
            #[cfg(feature = "component-lab")]
            deck_wave: Default::default(),
            #[cfg(feature = "component-lab")]
            deck_shape: None,
            #[cfg(feature = "component-lab")]
            deck_rest_left: 0.0,
            #[cfg(feature = "component-lab")]
            deck_wave_frames: 0,
            #[cfg(feature = "component-lab")]
            deck_wave_reported: None,
            #[cfg(feature = "component-lab")]
            status_popout_reported: None,
            #[cfg(feature = "component-lab")]
            topology_reported: None,
            topo_transition_reported: None,
            _notification_subscription: notification_subscription,
            appstack_scroll: gpui::ScrollHandle::new(),
            appstack_search,
            _appstack_search_subscription: appstack_search_subscription,
            appstack_motion: Default::default(),
            appstack_motion_previous: None,
            product_runtime,
            system_product_preferences,
            #[cfg(feature = "r13-production-client")]
            product_controller: ProductController,
            #[cfg(feature = "r12-production-client")]
            _profile_subscription: profile_subscription,
            #[cfg(feature = "r12-production-client")]
            pending_profile_edge: None,
            _tray_subscription: tray_subscription,
            _audio_subscription: audio_subscription,
            _network_subscription: network_subscription,
            _status_subscriptions: status_subscriptions,
            _menu_subscription: Some(menu_subscription),
            product_pointer: None,
            bar_pointer_zone: BarPointerZone::Outside,
            bar_reveal_state,
            product_bar_edge: match &bar_transition_pending {
                Some(plan) => plan.first_edge(),
                None => bar_transition_qualification.as_ref().map_or_else(
                    || {
                        crate::product_bar::qualification_edge(&args)
                            .expect("validated qualification edge")
                            .or(projected_bar_edge)
                            .unwrap_or(
                                if product_mode
                                    && cfg!(feature = "component-lab")
                                    && args.iter().any(|arg| arg == "--review-native-rail")
                                {
                                    SurfaceEdge::Left
                                } else {
                                    SurfaceEdge::Top
                                },
                            )
                    },
                    |(plan, _)| plan.final_target(),
                ),
            },
            bar_transition_qualification,
            bar_transition_pending,
            bar_transition_completion_notified: false,
            bar_last_travel_frame: None,
            bar_last_leg: None,
            bar_transition_reduced_logged: false,
            bar_transition_driver_started: None,
            product_text_scale,
            motion_preference,
            product_hull: None,
            motion_driver: ProductMotionDriver::default(),
            perf_counters,
            perf_last_settled: None,
            reactive_qualification_diagnostics,
            a11y_tree_qualification_diagnostics,
            a11y_tree_qualification_redraw_requests: 0,
            a11y_tree_qualification_dumped: false,
            last_reactive_qualification_line: None,
            appstack_motion_trace,
            appstack_motion_trace_frame: 0,
            appstack_motion_trace_target: None,
            appstack_motion_trace_started: None,
            appstack_motion_trace_previous: None,
            motion_tick_previous: None,
            #[cfg(feature = "component-lab")]
            dictation_reported: None,
            #[cfg(feature = "component-lab")]
            dictation_surface: None,
            #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
            dictation_motion: Default::default(),
            #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
            dictation_previous: None,
            product_epoch: 0,
            scene: None,
            frame: None,
            projection_qualification_enabled,
            projection_worker_started: false,
            completion_logged: false,
            host_token,
            previous_step: None,
            previous_viewport: None,
            surface_fence: IntentFence::default(),
            applied_reservation: None,
            initial_reservation: None,
            reservation_changed_from_initial: false,
            reservation_return_logged: false,
        }
    }

    #[cfg(feature = "r12-production-client")]
    fn consume_profile_edge(&mut self) -> bool {
        let active = !self.effective_reduced_motion()
            && self
                .bar_transition_qualification
                .as_ref()
                .is_some_and(|(plan, started)| started.elapsed() < plan.total_duration());
        let source = self
            .bar_transition_qualification
            .as_ref()
            .map_or(self.product_bar_edge, |(plan, _)| plan.final_target());
        // BAR-01J (B09): a new edge while the Bar travels turns the travel
        // from where it is, instead of waiting for it to arrive.
        if active
            && let Some(target) = self.pending_profile_edge.filter(|t| *t != source)
            && self.retarget_bar_travel(
                target,
                crate::product_bar::BarTransitionQualification::between_edges_at(
                    source,
                    target,
                    self.travel_tempo(),
                )
                .total_duration(),
            )
        {
            self.pending_profile_edge = None;
            return true;
        }
        let Some(target) = crate::product_profile::take_relocation_target(
            &mut self.pending_profile_edge,
            source,
            active,
        ) else {
            return false;
        };
        // BAR-01D: the profile's travel speed sets the leg's duration.
        self.bar_transition_qualification = Some((
            crate::product_bar::BarTransitionQualification::between_edges_at(
                source,
                target,
                self.travel_tempo(),
            ),
            Instant::now(),
        ));
        self.bar_transition_completion_notified = false;
        self.bar_transition_reduced_logged = false;
        self.bar_transition_driver_started = None;
        true
    }

    /// BAR-01J (B09): a new target while the Bar travels. The travel turns
    /// from its live frame: the material, the modules and the relocating
    /// surfaces all go on from where they are.
    fn retarget_bar_travel(&mut self, target: SurfaceEdge, leg_duration: Duration) -> bool {
        if self.bar_last_travel_frame.is_none() {
            return false;
        }
        // The leg in hand slows to a stop first, so the material's speed
        // never jumps (a short overrun, then the turn).
        if let Some((leg, progress, rate)) = self.bar_last_leg.clone()
            && rate > 1.0e-3
            && !self.effective_reduced_motion()
        {
            let span = leg_duration
                .mul_f32(DECELERATION_SHARE)
                .clamp(Duration::from_millis(60), Duration::from_millis(200));
            println!(
                "GNOS_UX_BAR_TRAVEL state=decelerate progress={progress:.3} span_ms={} to={target:?}",
                span.as_millis()
            );
            self.bar_transition_qualification = Some((
                crate::product_bar::BarTransitionQualification::Decelerate {
                    leg,
                    progress,
                    rate,
                    span,
                    rail: self.product_bar_edge,
                    target,
                    leg_duration,
                },
                Instant::now(),
            ));
            self.bar_transition_completion_notified = false;
            return true;
        }
        self.rebase_bar_travel(target, leg_duration)
    }

    /// BAR-01J: the turn itself, from the travel's last frame.
    fn rebase_bar_travel(&mut self, target: SurfaceEdge, leg_duration: Duration) -> bool {
        let Some(origin) = self.bar_last_travel_frame.clone() else {
            return false;
        };
        println!(
            "GNOS_UX_BAR_TRAVEL state=retarget from={:?} phase={:?} progress={:.3} to={target:?}",
            origin.active_edge,
            origin.phase,
            origin.progress.value()
        );
        // The handoff comes on top of the leg: the route itself keeps a whole
        // leg, so the Bar never travels faster than on any other leg.
        let leg_duration = leg_duration.div_f32(1.0 - gnos_ux_surface::grammar::RETARGET_HANDOFF);
        self.bar_transition_qualification = Some((
            crate::product_bar::BarTransitionQualification::Rebased {
                origin: Box::new(origin),
                rail: self.product_bar_edge,
                target,
                leg_duration,
            },
            Instant::now(),
        ));
        #[cfg(feature = "component-lab")]
        {
            self.bar_relocation_prior = self.bar_last_relocation.clone();
            self.bar_travel_handoff = self.bar_last_travel_placements.clone();
        }
        self.bar_transition_completion_notified = false;
        true
    }

    fn refresh_product_runtime(&mut self) -> bool {
        let projection = self
            .runtime_output
            .as_ref()
            .and_then(|output| self.semantic_consumer.get(Some(output)));
        self.product_runtime
            .refresh(self.runtime_output.as_ref(), projection.as_ref())
    }

    #[cfg(feature = "r13-production-client")]
    /// The one place a workspace activation intent is carried out.
    ///
    /// Pointer, keyboard and assistive activation all converge here, so
    /// the workspace identity, the freshness fence and the typed R13
    /// request exist once rather than once per input device.
    fn activate_workspace_intent(
        &mut self,
        selected: crate::product_scene::ProductWorkspaceContent,
        cx: &mut Context<Self>,
    ) {
        self.refresh_product_runtime();
        self.product_controller
            .activate_workspace(&mut self.product_runtime, selected);
        if let Some(perf) = &self.perf_counters {
            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
        }
        cx.notify();
    }

    /// TOPO-01A/C: the native rail's side, from the Bar edge (durable
    /// preference or qualification flag).
    fn rail_side(&self) -> crate::rail_topology::RailSide {
        crate::rail_topology::RailSide::from_edge(self.product_bar_edge)
    }

    fn effective_reduced_motion(&self) -> bool {
        self.motion_preference.effective_reduced_motion()
    }

    fn product_design_context(&self) -> ProductDesignContext {
        ProductDesignContext::new(
            self.product_text_scale.factor(),
            self.effective_reduced_motion(),
        )
        .expect("qualified product text scale must fit design context")
    }
}

impl SceneHost {
    /// SURF-01B/SURF-02B: while a panel out of the Bar is open it takes
    /// the whole output; the product surfaces beneath it (the Dock's
    /// buttons, on a narrow output) take no click.
    fn product_click_blocked(&self) -> bool {
        #[cfg(feature = "component-lab")]
        if self.review_native_rail && (self.bar_panel_open() || self.swallow_click) {
            return true;
        }
        false
    }

    /// One machine-readable line per accepted Clock/Status change: what the
    /// runtime now holds, projected exactly as the reference Bar presents it.
    fn print_status_receipt(&self, source: &str, cx: &mut Context<Self>) {
        let presentation =
            crate::reference_bar::BarPresentation::from_product_runtime(&self.product_runtime);
        println!(
            "GNOS_UX_STATUS_RECEIPT source={source} {} scene_output={}",
            crate::reference_bar::describe_clock_and_status(&presentation),
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str)
        );
        cx.notify();
    }
}

#[cfg(any(feature = "r13-production-client", test))]
pub(super) fn focused_appstack_row_index(
    identities: &[crate::product_focus::ProductControlId],
    handles: &[gpui::FocusHandle],
    window: &Window,
) -> Option<usize> {
    identities
        .iter()
        .zip(handles)
        .filter(|(identity, _)| identity.as_str().starts_with("app:"))
        .enumerate()
        .find_map(|(index, (_, handle))| handle.is_focused(window).then_some(index))
}

impl Render for SceneHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(perf) = &self.perf_counters {
            perf.renders.fetch_add(1, Ordering::Relaxed);
        }
        let viewport =
            viewport_from_size(window.viewport_size()).unwrap_or_else(SurfaceViewport::unit);
        if self.product_mode {
            self.refresh_product_runtime();
            // BAR-01D: the profile's shell preferences, eased.
            self.sync_shell_preferences(cx);
            let state = self.product_runtime.semantic().clone();
            let design = self.product_design_context();
            if self.a11y_tree_qualification_diagnostics
                && !self.a11y_tree_qualification_dumped
                && !state.can_open_workspace()
                && let Some(tree_json) = window.debug_a11y_tree_json()
                && tree_json.contains("product-bar")
            {
                println!(
                    "GNOS_UX_GPUI_A11Y_TREE output={} current_semantic_can_open=false tree={tree_json}",
                    self.runtime_output
                        .as_ref()
                        .map(RuntimeId::as_str)
                        .unwrap_or("UNKNOWN"),
                );
                self.a11y_tree_qualification_dumped = true;
            }
            // Constrain before laying out, so a panel whose capability was lost
            // is not given geometry this frame.
            self.product_panel = self
                .product_panel
                .constrained_by(state.can_open_workspace(), state.can_open_appstack());
            let appstack_target_open = self.product_panel.is_appstack();
            let appstack_needs_motion =
                !self.appstack_motion.is_settled_for(appstack_target_open) && !appstack_target_open;
            let workspace_open_intent = self.product_panel.is_workspace_summary();
            // BAR-01J: a development retarget turns the same way a
            // production one does, from the live frame.
            if let Some((
                crate::product_bar::BarTransitionQualification::Retarget {
                    target,
                    leg_duration,
                    at,
                    ..
                },
                started,
            )) = &self.bar_transition_qualification
                && started.elapsed()
                    >= crate::product_bar::BarTransitionQualification::retarget_at(
                        *leg_duration,
                        *at,
                    )
                && !self.effective_reduced_motion()
            {
                let (target, leg_duration) = (*target, *leg_duration);
                self.retarget_bar_travel(target, leg_duration);
            }
            if let Some((
                crate::product_bar::BarTransitionQualification::Decelerate {
                    span,
                    target,
                    leg_duration,
                    ..
                },
                started,
            )) = &self.bar_transition_qualification
                && started.elapsed() >= *span
            {
                let (target, leg_duration) = (*target, *leg_duration);
                self.rebase_bar_travel(target, leg_duration);
            }
            let transition_spec = self.bar_transition_qualification.clone();
            let transition_elapsed = transition_spec
                .as_ref()
                .map(|(_, started)| started.elapsed());
            if let Some((plan, _)) = &transition_spec {
                self.product_bar_edge = if self.effective_reduced_motion() {
                    plan.final_target()
                } else {
                    plan.target_at(transition_elapsed.expect("transition start has elapsed time"))
                };
            }
            let transition_active = transition_spec
                .as_ref()
                .zip(transition_elapsed)
                .is_some_and(|((plan, _), elapsed)| {
                    elapsed < plan.total_duration() && !self.effective_reduced_motion()
                });
            // BAR-01B: an open Bar popout retracts into the rail when it
            // starts to travel, instead of being cut off.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                self.bar_travelling = transition_active;
                if !transition_active {
                    self.bar_travel_popout_sent = false;
                } else {
                    if self.bar_osd.close() {
                        // SURF-01C: the OSD retracts as the Bar leaves.
                        self.report_osd("closing");
                    }
                    if self.session.close() {
                        // SURF-02B: so does the session panel.
                        self.report_session("closing");
                    }
                    if self.overflow.close() {
                        self.report_overflow("closing");
                    }
                    if !self.bar_travel_popout_sent && self.utilities.is_open() {
                        // SURF-01B: Utilities retracts into the rail as a
                        // popout does when the Bar starts to travel.
                        self.bar_travel_popout_sent = true;
                        self.utilities.close();
                        self.report_utilities("closing");
                    } else if !self.bar_travel_popout_sent && self.bar_popout.is_open() {
                        self.bar_travel_popout_sent = true;
                        let outcome = self.bar_popout.retract();
                        println!("GNOS_UX_BAR_TRAVEL_POPOUT state=retracting");
                        self.apply_bar_popout_outcome(outcome, cx);
                    }
                    // WS-02 / FOCUS-01: the workspace manager and the
                    // popouts go back in with it.
                    let mut all = vec![crate::attention::Attendant::Workspaces];
                    all.extend(crate::attention::Attendant::POPOUTS);
                    self.attention_close(&all, cx);
                }
            }
            let bar_policy =
                BarInteractionPolicy::from_preferences(self.product_runtime.preferences());
            self.bar_reveal_state = if transition_active {
                BarRevealState::Visible
            } else {
                next_state(
                    bar_policy,
                    self.bar_reveal_state,
                    self.bar_pointer_zone,
                    self.product_panel.is_open(),
                )
            };
            let bar_visible = self.bar_reveal_state.is_visible();
            let sensing_policy = if bar_policy.mode() == BarRevealMode::HoverReveal {
                BarSensingPolicy::EdgeStrip {
                    thickness: crate::product_bar::PRODUCT_BAR_INSET,
                }
            } else {
                BarSensingPolicy::None
            };
            let reservation_policy = if bar_visible {
                BarReservationPolicy::ReserveOccupiedExtent
            } else {
                BarReservationPolicy::Overlay
            };
            // Collapse the derived Workspace surface during edge travel, while
            // retaining product_panel as the restore intent for the target.
            let target_open = workspace_open_intent && !transition_active;
            if let Some(perf) = &self.perf_counters {
                perf.solver_resolves.fetch_add(1, Ordering::Relaxed);
            }
            let reduced_motion = self.effective_reduced_motion();
            if reduced_motion && transition_spec.is_some() && !self.bar_transition_reduced_logged {
                self.bar_transition_reduced_logged = true;
                println!(
                    "GNOS_UX_BAR_TRANSITION_REDUCED scene_id={} output={} target={:?} animation=none",
                    self.scene_id.value(),
                    self.runtime_output
                        .as_ref()
                        .map_or("COMPOSITOR_SELECTED", RuntimeId::as_str),
                    self.product_bar_edge,
                );
            }
            let resolved_layout = if self.review_native_rail {
                // The production solver and all its input/space owners see the
                // same actual full-height 60 px rail as the native Bar renderer.
                crate::reference_bar::idle_bar_placement_on(viewport, self.rail_side()).and_then(
                    |placement| {
                        ProductLayout::resolve_reactive_with_exact_bar_placement(
                            viewport,
                            &mut self.product_hull,
                            target_open,
                            reduced_motion,
                            self.product_pointer,
                            placement,
                        )
                    },
                )
            } else {
                ProductLayout::resolve_reactive_at_edge_with_policies(
                    viewport,
                    &mut self.product_hull,
                    target_open,
                    reduced_motion,
                    self.product_pointer,
                    self.product_bar_edge,
                    self.product_text_scale.font_size_px(),
                    reservation_policy,
                    sensing_policy,
                )
            };
            let Some(layout) = resolved_layout else {
                return div().size_full();
            };
            let mut layout = layout;
            let mut transition_frame = None;
            let mut active_bar_placement = layout.bar_placement;
            if transition_active {
                let (plan, _) = transition_spec
                    .as_ref()
                    .expect("active transition has selector");
                let elapsed = transition_elapsed.expect("active transition has elapsed time");
                let font_px = self.product_text_scale.font_size_px();
                let native = self.review_native_rail;
                let resolve_placement = |edge| {
                    // TOPO-01B: the native rail travels between full-span,
                    // rail-thick endpoints on the frame.
                    if native
                        && let Some(placement) =
                            crate::reference_bar::native_bar_placement(viewport, edge)
                                .and_then(|p| p.resolve(viewport).ok())
                    {
                        return placement;
                    }
                    ProductLayout::resolve_reactive_at_edge(
                        viewport, &mut None, false, true, None, edge, font_px,
                    )
                    .expect("valid qualified transition endpoint")
                    .bar_placement
                };
                // Each plan gives the leg's transition, its progress and how
                // fast that progress runs (per second): BAR-01J keeps the
                // leg in hand to slow it down when the travel turns.
                let per_second = |leg: &Duration| 1.0 / leg.as_secs_f32().max(1.0e-3);
                let (transition, progress_value, rate) = match plan {
                    crate::product_bar::BarTransitionQualification::Path {
                        edges,
                        leg_duration,
                    } => {
                        let leg_index = ((elapsed.as_millis() / leg_duration.as_millis()) as usize)
                            .min(edges.len() - 2);
                        let leg_elapsed = elapsed.saturating_sub(*leg_duration * leg_index as u32);
                        let transition = gnos_ux_surface::grammar::BarPlacementTransition::new(
                            viewport,
                            resolve_placement(edges[leg_index]),
                            resolve_placement(edges[leg_index + 1]),
                        )
                        .expect("valid path endpoints");
                        (
                            transition,
                            leg_elapsed.as_secs_f32() / leg_duration.as_secs_f32(),
                            per_second(leg_duration),
                        )
                    }
                    crate::product_bar::BarTransitionQualification::Retarget {
                        source,
                        via,
                        target,
                        leg_duration,
                        at,
                    } => {
                        let first_leg = gnos_ux_surface::grammar::BarPlacementTransition::new(
                            viewport,
                            resolve_placement(*source),
                            resolve_placement(*via),
                        )
                        .expect("valid retarget source endpoints");
                        let retarget_at =
                            crate::product_bar::BarTransitionQualification::retarget_at(
                                *leg_duration,
                                *at,
                            );
                        let at_fraction = f32::from(*at) / 100.0;
                        if elapsed < retarget_at {
                            (
                                first_leg,
                                (elapsed.as_secs_f32() / leg_duration.as_secs_f32())
                                    .min(at_fraction),
                                per_second(leg_duration),
                            )
                        } else {
                            // Only without a live frame to turn from.
                            let current = first_leg
                                .evaluate(gnos_ux_surface::grammar::UnitInterval::clamped(
                                    at_fraction,
                                ))
                                .expect("retarget origin resolves");
                            let source_semantics = resolve_placement(current.active_edge);
                            let transition =
                                gnos_ux_surface::grammar::BarPlacementTransition::retarget_from_frame(
                                    viewport,
                                    &current,
                                    source_semantics,
                                    resolve_placement(*target),
                                )
                                .expect("retarget rebases from live transition frame");
                            (
                                transition,
                                elapsed.saturating_sub(retarget_at).as_secs_f32()
                                    / leg_duration.as_secs_f32(),
                                per_second(leg_duration),
                            )
                        }
                    }
                    crate::product_bar::BarTransitionQualification::Decelerate {
                        leg,
                        progress,
                        rate,
                        span,
                        ..
                    } => {
                        // BAR-01J: the interrupted leg goes on, slowing to a
                        // stop, before the Bar turns: its speed never jumps.
                        let tau = span.as_secs_f32().max(1.0e-3);
                        let e = elapsed.as_secs_f32().min(tau);
                        (
                            (**leg).clone(),
                            progress + rate * (e - e * e / (2.0 * tau)),
                            rate * (1.0 - e / tau),
                        )
                    }
                    crate::product_bar::BarTransitionQualification::Rebased {
                        origin,
                        target,
                        leg_duration,
                        ..
                    } => {
                        let transition =
                            gnos_ux_surface::grammar::BarPlacementTransition::retarget_from_frame(
                                viewport,
                                origin,
                                resolve_placement(origin.active_edge),
                                resolve_placement(*target),
                            )
                            .expect("a travel rebases from its live frame");
                        (
                            transition,
                            elapsed.as_secs_f32() / leg_duration.as_secs_f32(),
                            per_second(leg_duration),
                        )
                    }
                };
                let progress = gnos_ux_surface::grammar::UnitInterval::clamped(
                    progress_value.clamp(0.0, 0.999_999),
                );
                let evaluated = transition
                    .evaluate(progress)
                    .expect("travel frame resolves");
                let route = transition.route();
                self.bar_last_leg = Some((Box::new(transition), progress.value(), rate));
                self.bar_last_travel_frame = Some(evaluated.clone());
                #[cfg(feature = "component-lab")]
                {
                    self.bar_travel_route = Some(route);
                }
                #[cfg(not(feature = "component-lab"))]
                let _ = route;
                active_bar_placement = resolve_placement(evaluated.active_edge);
                let key = (evaluated.phase, evaluated.active_edge);
                if self.topo_transition_reported != Some(key) {
                    self.topo_transition_reported = Some(key);
                    let arms = evaluated
                        .arms
                        .iter()
                        .map(|arm| {
                            format!(
                                "{:?}:[{:.1},{:.1},{:.1},{:.1}]",
                                arm.edge,
                                arm.bounds.left(),
                                arm.bounds.top(),
                                arm.bounds.right(),
                                arm.bounds.bottom()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    println!(
                        "GNOS_UX_TOPO_TRANSITION phase={:?} active_edge={:?} progress={:.3} arms=[{arms}]",
                        evaluated.phase,
                        evaluated.active_edge,
                        evaluated.progress.value(),
                    );
                }
                transition_frame = Some(evaluated);
                self.start_motion_driver(cx);
            }
            // BAR-01C (B06): the surfaces whose place depends on the rail's
            // side follow each leg: those the new side moves retract into
            // their edge, then come out of the new one.
            #[cfg(feature = "component-lab")]
            let relocation = if self.review_native_rail && transition_active {
                transition_frame
                    .as_ref()
                    .zip(self.bar_travel_route.as_ref())
                    .and_then(|(frame, route)| {
                        let to = crate::rail_topology::RailSide::from_edge(*route.last()?);
                        // BAR-01J: a rebased leg goes on from where the
                        // interrupted one left each surface.
                        Some(match &self.bar_relocation_prior {
                            Some(prior) => crate::rail_topology::Relocation::rebased(
                                prior.clone(),
                                to,
                                frame.progress.value(),
                            ),
                            None => crate::rail_topology::Relocation::new(
                                crate::rail_topology::RailSide::from_edge(*route.first()?),
                                to,
                                frame.progress.value(),
                            ),
                        })
                    })
            } else {
                None
            };
            #[cfg(feature = "component-lab")]
            {
                self.bar_last_relocation = relocation.clone();
                if !transition_active {
                    self.bar_relocation_prior = None;
                    self.bar_travel_handoff = None;
                    self.bar_last_travel_placements = None;
                }
            }
            if !transition_active {
                self.bar_last_travel_frame = None;
                self.bar_last_leg = None;
            }
            let now = Instant::now();
            let dt = self
                .appstack_motion_previous
                .replace(now)
                .map_or(0.0, |p| now.duration_since(p).as_secs_f32());
            self.appstack_motion
                .advance_observed(appstack_target_open, reduced_motion, dt)
                .expect("monotonic wall-clock dt");
            if let Some(perf) = &self.perf_counters {
                perf.appstack_solver_resolves
                    .fetch_add(1, Ordering::Relaxed);
            }
            // DECK-01A: the Deck holds its appcards and is as wide as they
            // are, centred between the frame's insets.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                let runtime = &self.product_runtime;
                let installed = runtime.installed_applications();
                let cards = crate::deck::deck_cards(
                    &self.deck_pins.pinned,
                    &installed,
                    runtime.application_targets(),
                    |class| {
                        crate::reference_bar::resolve_window_entry(
                            runtime.application_catalog(),
                            class,
                        )
                    },
                    |window| runtime.application_is_focused(window),
                );
                self.deck_cards = cards;
                crate::deck::keep_observed_order(&mut self.deck_cards, &mut self.deck_seen);
                // CURSOR-01B: a launch whose window is now there ends.
                self.settle_launches();
                // DECK-01D: a drag reshapes the row (a slot closing, a gap
                // opening) and the Deck's width with it.
                let spacing = self.deck_spacing();
                let width = crate::deck::deck_width_spaced(&self.deck_cards, &spacing);
                // BAR-01C: during a travel the Deck's span eases from the
                // source side's insets to the target side's, so it never
                // recentres with a jump.
                let (inset_left, inset_right) = relocation.as_ref().map_or_else(
                    || self.rail_side().insets(),
                    crate::rail_topology::Relocation::insets,
                );
                let free = viewport.width - inset_left - inset_right;
                let left = inset_left + ((free - width) / 2.0).max(0.0);
                let bottom = layout.dock.bottom();
                // DECK-01B: the wave widens the Deck about the pointer; the
                // Dock envelope takes the widened base.
                self.deck_rest_left = left;
                let shape = crate::deck::wave::deck_shape_spaced(
                    &self.deck_cards,
                    &spacing,
                    left,
                    bottom - crate::deck::HEIGHT,
                    true,
                    (inset_left + 16.0, viewport.width - inset_right - 16.0),
                    &self.deck_wave.input(),
                );
                if let Ok(deck) = GeometryRect::try_new(
                    shape.base.x,
                    bottom - crate::deck::HEIGHT,
                    shape.base.x + shape.base.width,
                    bottom,
                    crate::deck::HEIGHT / 2.0,
                ) {
                    layout.dock = deck;
                }
            }
            // TOPO-01C2b (T03): a bottom native Bar sends the Dock to the top,
            // the mirror of its place above the bottom border; the AppStack
            // then opens downward. During a travel the Dock stays where the
            // transition constraint keeps it.
            #[cfg(feature = "component-lab")]
            let dock_on_top = self.review_native_rail
                && match &relocation {
                    Some(relocation) => relocation.side() == crate::rail_topology::RailSide::Bottom,
                    None => {
                        !transition_active
                            && self.rail_side() == crate::rail_topology::RailSide::Bottom
                    }
                };
            #[cfg(not(feature = "component-lab"))]
            let dock_on_top = false;
            let resolved = if dock_on_top {
                let dock = layout.dock;
                // The nominal Dock stands 16 px above the output's bottom
                // edge (`ProductLayout::resolve`); its mirror hangs 16 px
                // below the top edge.
                let top = 16.0;
                let moved = GeometryRect::try_new(
                    dock.left(),
                    top,
                    dock.right(),
                    top + dock.height(),
                    dock.radius(),
                );
                let safe = GeometryRect::try_new(
                    16.0,
                    16.0,
                    viewport.width - 16.0,
                    viewport.height - crate::rail_topology::rail_width() - 16.0,
                    0.0,
                );
                moved.and_then(|moved| {
                    layout.dock = moved;
                    crate::product_shared_envelope::SharedDockEnvelope::resolve_top_anchored(
                        viewport,
                        moved,
                        self.appstack_motion,
                        safe?,
                    )
                })
            } else {
                crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(
                    viewport,
                    layout.dock,
                    layout.dock,
                    self.appstack_motion,
                    layout
                        .dock_safe_bounds()
                        .expect("valid Dock available region"),
                )
            };
            let Some(mut shared) = resolved.ok() else {
                return div().size_full();
            };
            if let Some(transition) = &transition_frame {
                for _ in 0..8 {
                    match layout
                        .constrain_dock_for_bar_transition(transition, shared.geometry.outer)
                    {
                        Some(true) => {
                            let Some(reprojected) =
                                crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(
                                    viewport,
                                    layout.dock,
                                    layout.dock,
                                    self.appstack_motion,
                                    layout
                                        .dock_safe_bounds()
                                        .expect("valid Dock safe bounds after transient clearance"),
                                )
                                .ok()
                            else {
                                return div().size_full();
                            };
                            shared = reprojected;
                        }
                        Some(false) => break,
                        None => return div().size_full(),
                    }
                }
                debug_assert!(transition.arms.iter().all(|arm| {
                    let outer = shared.geometry.outer;
                    arm.bounds.left() >= outer.right()
                        || arm.bounds.right() <= outer.left()
                        || arm.bounds.top() >= outer.bottom()
                        || arm.bounds.bottom() <= outer.top()
                }));
            }
            // DECK-01B: the final shape, where the Dock now stands: under a
            // bottom Bar it hangs from the top and its hump descends; a
            // travel constraint may have moved it.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                // BAR-01C: during a travel the Deck's span eases from the
                // source side's insets to the target side's, so it never
                // recentres with a jump.
                let (inset_left, inset_right) = relocation.as_ref().map_or_else(
                    || self.rail_side().insets(),
                    crate::rail_topology::Relocation::insets,
                );
                let spacing = self.deck_spacing();
                let rest_width = crate::deck::deck_width_spaced(&self.deck_cards, &spacing);
                let provisional = crate::deck::wave::deck_shape_spaced(
                    &self.deck_cards,
                    &spacing,
                    self.deck_rest_left,
                    layout.dock.top(),
                    true,
                    (inset_left + 16.0, viewport.width - inset_right - 16.0),
                    &self.deck_wave.input(),
                );
                let shift = layout.dock.left() - provisional.base.x;
                self.deck_rest_left += shift;
                let span = (
                    inset_left + 16.0 + shift,
                    viewport.width - inset_right - 16.0 + shift,
                );
                let build = |wave: &crate::deck::wave::DeckWave| {
                    crate::deck::wave::deck_shape_spaced(
                        &self.deck_cards,
                        &spacing,
                        self.deck_rest_left,
                        layout.dock.top(),
                        !dock_on_top,
                        span,
                        &wave.input(),
                    )
                };
                let mut shape = build(&self.deck_wave);
                // The tab aims at the card it labels; a tab that starts (or
                // snaps) this frame is placed before the shape is final.
                if let Some(card) = self.deck_wave.hovered()
                    && let Some(center) = shape.tab_target(card)
                    && let Some(label) = self.deck_wave.label()
                {
                    let label_width = deck_label_width(window, label);
                    // DECK-01D: an open sheet is as wide as its widest row,
                    // within the Deck, and stays over it.
                    let (center, width) = if self.deck_wave.sheet_open() {
                        let widest = self
                            .deck_sheet_items
                            .iter()
                            .map(|item| {
                                deck_text_width(window, item.text(), crate::deck::sheet::ITEM_PX)
                            })
                            .fold(0.0, f32::max);
                        let base = shape.base;
                        let width =
                            crate::deck::sheet::sheet_width(widest, label_width).min(base.width);
                        // As wide as the Deck (a Deck of few cards), both
                        // bounds are equal but for rounding: `clamp` panics
                        // when they cross, so the upper never falls below.
                        let lo = base.x + width / 2.0;
                        let hi = (base.x + base.width - width / 2.0).max(lo);
                        let center = center.clamp(lo, hi);
                        (center, width)
                    } else {
                        (center, label_width)
                    };
                    let snap = self.effective_reduced_motion();
                    let before = self.deck_wave.input();
                    self.deck_wave.aim_tab(center, width, snap);
                    if self.deck_wave.input() != before {
                        shape = build(&self.deck_wave);
                    }
                }
                // DECK-01C: the AppStack extrudes from the Deck at rest, where
                // it stands now, toward the far side of the frame. While it is
                // out, the Deck row folds into its envelope (no wave).
                if self.deck_wave.sheet_shown().is_none() && !self.deck_sheet.is_open() {
                    self.deck_sheet_items.clear();
                }
                let rest_layout = crate::deck::deck_layout_spaced(
                    &self.deck_cards,
                    &spacing,
                    self.deck_rest_left,
                    layout.dock.top(),
                );
                let (_, inset_top, _, inset_bottom) = self.rail_side().insets4();
                let geometry = crate::deck::stack::StackGeometry {
                    viewport: (viewport.width, viewport.height),
                    border: crate::reference_bar::BORDER_THICKNESS,
                    rest: rest_layout.deck,
                    rest_control: rest_layout.control,
                    grows_up: !dock_on_top,
                    span,
                    limit: if dock_on_top {
                        viewport.height - inset_bottom - 16.0
                    } else {
                        inset_top + 16.0
                    },
                };
                if self.stack.set_geometry(geometry) {
                    self.report_stack("synced");
                }
                if let Some(frame) = self.stack.frame() {
                    let mut folded = crate::deck::wave::deck_shape_spaced(
                        &self.deck_cards,
                        &spacing,
                        self.deck_rest_left,
                        layout.dock.top(),
                        !dock_on_top,
                        span,
                        &crate::deck::wave::WaveInput {
                            amount: 0.0,
                            tab: None,
                            ..self.deck_wave.input()
                        },
                    );
                    crate::deck::stack::fold_deck(&mut folded, &frame);
                    shape = folded;
                }
                // BAR-01C: during a travel the solver lifts a bottom Dock clear
                // of the arms; the painted Deck eases into that clearance at
                // the leg's start and out of it at its end, instead of
                // stepping.
                if let Some(relocation) = &relocation
                    && !dock_on_top
                {
                    let rest_top = viewport.height - 16.0 - crate::deck::HEIGHT;
                    let lifted = shape.base.y;
                    let b = relocation.clearance();
                    let top = rest_top + (lifted - rest_top) * b;
                    if (top - lifted).abs() > 0.01 {
                        shape = shape.translated(top - lifted);
                    }
                }
                // BAR-01C: a Deck that changes edge slides into its edge and
                // out of the new one, its hump included. Only what is painted
                // moves; the Dock node keeps its place in the frame.
                if let Some(relocation) = &relocation {
                    let k = relocation.of(|side| side == crate::rail_topology::RailSide::Bottom);
                    if k > 0.0 {
                        let b = shape.bounds;
                        let dy = if relocation.side() == crate::rail_topology::RailSide::Bottom {
                            -k * (b.y + b.height + bar_travel::DECK_CLEARANCE)
                        } else {
                            k * (viewport.height - b.y + bar_travel::DECK_CLEARANCE)
                        };
                        shape = shape.translated(dy);
                    }
                }
                self.deck_shape = Some(shape);
                self.report_deck_drag();
                // A drag reports where it lands, not every step of the row;
                // a travel reports where the Deck settles.
                if self.deck_drag.is_none()
                    && !transition_active
                    && let Ok(rest) = GeometryRect::try_new(
                        self.deck_rest_left,
                        layout.dock.top(),
                        self.deck_rest_left + rest_width,
                        layout.dock.bottom(),
                        crate::deck::HEIGHT / 2.0,
                    )
                {
                    self.report_deck(rest);
                }
                if !self.deck_wave.needs_frame() {
                    self.report_deck_wave();
                    self.report_deck_sheet();
                }
            }
            // Workspace-only presentation retains its qualified Dock projection.
            // C stays present through AppStack retraction, even during a switch.
            layout.shared_dock =
                (!target_open || appstack_target_open || appstack_needs_motion).then_some(shared);
            if let Some(ref transition) = transition_frame {
                layout.bar = transition.active_allocation;
            }
            let appstack_interactable = shared.appstack.interactable;
            if self.product_diagnostics {
                println!(
                    "GNOS_UX_APPSTACK_LINEAGE DOCK->APPSTACK topology=SHARED_OUTER_TYPED_INNERS material_owner=SharedDockEnvelope visible={} interactable={}",
                    shared.appstack.visible, appstack_interactable
                );
            }
            self.record_appstack_motion_frame(&shared);
            if !reduced_motion
                && (crate::product_scene::reactive_motion_needs_frame(
                    self.product_hull.as_ref(),
                    target_open,
                    false,
                ) || !self.appstack_motion.is_settled_for(appstack_target_open))
            {
                self.start_motion_driver(cx);
            }
            if let Some(perf) = &self.perf_counters {
                let settled = self
                    .product_hull
                    .as_ref()
                    .is_some_and(|solver| solver.is_settled_for(target_open));
                if self.perf_last_settled != Some(settled) {
                    println!(
                        "GNOS_UX_PERF_STATE output={} uptime_ms={} open={} settled={}",
                        self.runtime_output
                            .as_ref()
                            .map_or("UNKNOWN", RuntimeId::as_str),
                        perf.started.map_or(0, |start| start.elapsed().as_millis()),
                        target_open,
                        settled
                    );
                    self.perf_last_settled = Some(settled);
                }
            }
            self.product_epoch = self.product_epoch.checked_add(1).expect("product epoch");
            let frame = if let Some(ref transition) = transition_frame {
                layout.frame_with_bar_transition(
                    self.scene_id,
                    SceneEpoch::new(self.product_epoch),
                    transition,
                )
            } else {
                layout.frame(self.scene_id, SceneEpoch::new(self.product_epoch))
            };
            let frame = if transition_active {
                frame
            } else {
                project_bar_reveal_frame(
                    frame,
                    viewport,
                    layout.bar_placement,
                    self.bar_reveal_state,
                )
            };
            #[cfg(debug_assertions)]
            {
                let validation = frame.validate();
                debug_assert!(
                    validation.is_valid(),
                    "invalid scene frame: {:?}",
                    validation.violations
                );
            }
            if self.perf_counters.is_some()
                && self.product_epoch.is_multiple_of(6)
                && let Some(transition) = transition_frame.as_ref()
            {
                let bar = frame.node(SceneNodeId::BAR).expect("Bar frame node");
                let dock = frame.node(SceneNodeId::DOCK).expect("Dock frame node");
                let intersects = |a: GeometryRect, b: GeometryRect| {
                    a.left() < b.right()
                        && a.right() > b.left()
                        && a.top() < b.bottom()
                        && a.bottom() > b.top()
                };
                let appstack = frame.node(SceneNodeId::APPSTACK).map(|node| node.envelope);
                let paint_overlap_count = transition
                    .arms
                    .iter()
                    .filter(|arm| {
                        intersects(arm.bounds, dock.envelope)
                            || appstack.is_some_and(|bounds| intersects(arm.bounds, bounds))
                    })
                    .count();
                let bar_inputs = transition.input_regions();
                let dock_inputs = shared.input_regions();
                let input_overlap_count = bar_inputs
                    .iter()
                    .filter(|bar_input| {
                        dock_inputs
                            .iter()
                            .any(|dock_input| intersects(**bar_input, *dock_input))
                    })
                    .count();
                println!(
                    "GNOS_UX_BAR_RELOCATION_SAMPLE scene_id={} output={} epoch={} elapsed_ms={} selected_edge={:?} active_edge={:?} phase={:?} progress={:.4} reservation=[{:.1},{:.1},{:.1},{:.1}] paint_overlap_count={} input_overlap_count={} bar=[{:.1},{:.1},{:.1},{:.1}] dock=[{:.1},{:.1},{:.1},{:.1}] dock_outer=[{:.1},{:.1},{:.1},{:.1}] appstack={:?}",
                    self.scene_id.value(),
                    self.runtime_output
                        .as_ref()
                        .map_or("COMPOSITOR_SELECTED", RuntimeId::as_str),
                    frame.epoch.value(),
                    transition_elapsed.unwrap_or_default().as_millis(),
                    self.product_bar_edge,
                    transition.active_edge,
                    transition.phase,
                    transition.progress.value(),
                    frame.reservation.top,
                    frame.reservation.right,
                    frame.reservation.bottom,
                    frame.reservation.left,
                    paint_overlap_count,
                    input_overlap_count,
                    bar.envelope.left(),
                    bar.envelope.top(),
                    bar.envelope.right(),
                    bar.envelope.bottom(),
                    dock.envelope.left(),
                    dock.envelope.top(),
                    dock.envelope.right(),
                    dock.envelope.bottom(),
                    shared.geometry.outer.left(),
                    shared.geometry.outer.top(),
                    shared.geometry.outer.right(),
                    shared.geometry.outer.bottom(),
                    appstack,
                );
            }
            self.project_surface_reservation(&frame, window);
            #[cfg(feature = "component-lab")]
            self.sync_desktop_inset(cx);
            if self.reactive_qualification_diagnostics
                && self.product_bar_edge == SurfaceEdge::Top
                && self
                    .product_hull
                    .as_ref()
                    .is_some_and(|solver| solver.is_settled_for(target_open))
                && let Some(reactive) = layout.reactive_frame()
            {
                let available_bottom = (layout.dock.top() - 12.0).max(layout.bar.bottom());
                let available_height = (available_bottom - layout.bar.bottom()).clamp(0.0, 156.0);
                let rest_target_height = available_height * reactive.openness;
                let hard_bound = reactive.geometry.bottom() >= available_bottom - 0.01;
                let dock_gap = layout.dock.top() - reactive.geometry.bottom();
                let line = format!(
                    "GNOS_UX_REACTIVE_QUALIFICATION output={} viewport={:.2}x{:.2} parent=({:.2},{:.2},{:.2},{:.2}) envelope=({:.2},{:.2},{:.2},{:.2}) dock=({:.2},{:.2},{:.2},{:.2}) available=(0.00,{:.2},{:.2},{:.2}) openness={:.4} rest_target_height={:.2} resolved_height={:.2} dock_gap={:.2} neighbor_pressure={:.4} neighbor_compression={:.2} neighbor_compression_target={:.2} constraint_pressure={:.4} edge_left={:.4} edge_right={:.4} edge_directional={:.4} edge_confinement={:.4} attachment={:?} attachment_phase={:.4} release_phase={:.4} reattach_phase={:.4} connector_width={:.2} hard_bound_involvement={}",
                    self.runtime_output
                        .as_ref()
                        .map_or("UNKNOWN", RuntimeId::as_str),
                    viewport.width,
                    viewport.height,
                    layout.bar.left(),
                    layout.bar.top(),
                    layout.bar.right(),
                    layout.bar.bottom(),
                    reactive.geometry.left(),
                    reactive.geometry.top(),
                    reactive.geometry.right(),
                    reactive.geometry.bottom(),
                    layout.dock.left(),
                    layout.dock.top(),
                    layout.dock.right(),
                    layout.dock.bottom(),
                    layout.bar.bottom(),
                    viewport.width,
                    available_bottom,
                    reactive.openness,
                    rest_target_height,
                    reactive.geometry.height(),
                    dock_gap,
                    reactive.neighbor_pressure,
                    reactive.neighbor_compression,
                    reactive.neighbor_compression_target,
                    reactive.constraint_pressure,
                    reactive.edge_response.left_pressure,
                    reactive.edge_response.right_pressure,
                    reactive.edge_response.directional_bias,
                    reactive.edge_response.bilateral_confinement,
                    reactive.attachment,
                    reactive.attachment_phase,
                    reactive.release_phase,
                    reactive.reattach_phase,
                    reactive.connector_width,
                    hard_bound,
                );
                if self.last_reactive_qualification_line.as_ref() != Some(&line) {
                    println!("{line}");
                    self.last_reactive_qualification_line = Some(line);
                }
            }
            // Every review rail region now comes from the SAME resolved
            // production placement as its paint and reservation geometry.
            // BAR-01G: while the Deck relocates, only its painted shape takes
            // input (below); the Dock node's own region stays behind.
            #[cfg(feature = "component-lab")]
            let deck_relocating = relocation
                .as_ref()
                .is_some_and(|r| r.of(|side| side == crate::rail_topology::RailSide::Bottom) > 0.0);
            #[cfg(not(feature = "component-lab"))]
            let deck_relocating = false;
            let dock_region = layout.dock;
            #[allow(unused_mut)]
            let mut input_regions = frame
                .input_regions
                .iter()
                .filter(|region| {
                    !(deck_relocating
                        && (region.left() - dock_region.left()).abs() < 1.0
                        && (region.top() - dock_region.top()).abs() < 1.0
                        && (region.width() - dock_region.width()).abs() < 1.0)
                })
                .map(|region| Bounds {
                    origin: point(px(region.left()), px(region.top())),
                    size: gpui::Size::new(px(region.width()), px(region.height())),
                })
                .collect::<Vec<_>>();
            #[cfg(feature = "component-lab")]
            {
                self.bar_dock_input = frame.input_regions.iter().any(|region| {
                    (region.left() - dock_region.left()).abs() < 1.0
                        && (region.top() - dock_region.top()).abs() < 1.0
                        && (region.width() - dock_region.width()).abs() < 1.0
                }) && !deck_relocating;
            }
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                let session = crate::product_notification_session::NotificationSession::shared(cx);
                // TOPO-01A/C: edge drawers move across when the rail takes
                // their edge (T03, T05); the centre meets a horizontal Bar.
                let side = relocation.as_ref().map_or(self.rail_side(), |r| r.side());
                let rail = crate::rail_topology::rail_width();
                let border = crate::reference_bar::BORDER_THICKNESS;
                let drawers_left = side == crate::rail_topology::RailSide::Right;
                self.notification_toasts.set_on_left(drawers_left);
                self.notification_toasts
                    .set_on_bottom((side == crate::rail_topology::RailSide::Top).then_some(rail));
                // BAR-01H (B07): on an output too narrow for both, the toast
                // drawer stands on the Deck that shares its edge.
                let base = self.toast_base_target(side, (viewport.width, viewport.height));
                self.toast_base_target_value = base;
                self.notification_toasts.set_base(self.toast_base.value);
                self.notification_centre.set_on_left(drawers_left);
                self.notification_centre.set_vertical_insets(match side {
                    crate::rail_topology::RailSide::Top => Some((rail, border)),
                    crate::rail_topology::RailSide::Bottom => Some((border, rail)),
                    _ => None,
                });
                let mut measure = crate::notification_view::WindowTextMeasure(window);
                let changed = self.notification_toasts.sync(
                    session
                        .read(cx)
                        .latest()
                        .map(|update| (&update.snapshot, update.now_ms)),
                    (viewport.width, viewport.height),
                    crate::reference_bar::BORDER_THICKNESS,
                    &mut measure,
                );
                if changed {
                    self.report_toasts("synced");
                    self.start_motion_driver(cx);
                }
                let centre_changed = self.notification_centre.sync(
                    session
                        .read(cx)
                        .latest()
                        .map(|update| (&update.snapshot, update.now_ms)),
                    (viewport.width, viewport.height),
                    crate::reference_bar::BORDER_THICKNESS,
                    &mut measure,
                );
                if centre_changed {
                    self.report_centre("synced");
                    self.start_motion_driver(cx);
                }
                for rect in self.notification_centre.input_rects() {
                    input_regions.push(Bounds {
                        origin: point(px(rect.x), px(rect.y)),
                        size: gpui::Size::new(px(rect.width), px(rect.height)),
                    });
                }
                self.appstack_search.update(cx, |search, _| {
                    search.set_bare(true);
                    search.set_grid_arrows(true);
                });
                // TOPO-01A: centred drawers sit between the frame's insets; a
                // top Bar is where Settings extrudes from (T04). DECK-01C: the
                // AppStack's geometry comes from the Deck, set with its shape.
                let (inset_left, inset_top, inset_right, inset_bottom) = side.insets4();
                for rect in self.stack.input_rects() {
                    input_regions.push(Bounds {
                        origin: point(px(rect.x), px(rect.y)),
                        size: gpui::Size::new(px(rect.width), px(rect.height)),
                    });
                }
                // T06: under a bottom Bar the Dock is at the top, and Settings
                // stands on the Bar instead.
                let settings_geometry = if side == crate::rail_topology::RailSide::Bottom {
                    self.settings.set_geometry_standing(
                        (viewport.width, viewport.height),
                        crate::reference_bar::BORDER_THICKNESS,
                        inset_left,
                        inset_right,
                        layout.dock.bottom(),
                        viewport.height - inset_bottom,
                    )
                } else {
                    self.settings.set_geometry_on(
                        (viewport.width, viewport.height),
                        crate::reference_bar::BORDER_THICKNESS,
                        inset_left,
                        inset_right,
                        layout.dock.top(),
                        inset_top,
                    )
                };
                let settings_lines =
                    self.settings.frame().is_some() && self.refresh_settings_lines(cx);
                if settings_geometry || settings_lines {
                    self.report_settings("synced");
                    if settings_lines {
                        self.start_motion_driver(cx);
                    }
                }
                for rect in self.settings.input_rects() {
                    input_regions.push(Bounds {
                        origin: point(px(rect.x), px(rect.y)),
                        size: gpui::Size::new(px(rect.width), px(rect.height)),
                    });
                }
                if let Some(panel) = self.notification_toasts.panel_rect() {
                    input_regions.push(Bounds {
                        origin: point(px(panel.x), px(panel.y)),
                        size: gpui::Size::new(px(panel.width), px(panel.height)),
                    });
                }
            }
            // SURF-03B: a pinned dashboard takes the whole output too (a
            // press outside closes it).
            #[cfg(feature = "component-lab")]
            if self.review_native_rail
                && self
                    .attention
                    .is_held(crate::attention::Attendant::Dashboard)
            {
                input_regions.push(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: gpui::Size::new(px(viewport.width), px(viewport.height)),
                });
            }
            // RAIL-01: an open tray menu takes the whole output (a press
            // outside closes it).
            #[cfg(feature = "component-lab")]
            if self.review_native_rail && self.tray_controller.read(cx).menu().is_some() {
                input_regions.push(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: gpui::Size::new(px(viewport.width), px(viewport.height)),
                });
            }
            // BAR-02A: the overflow panel, like the session panel.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail && self.overflow.is_open() {
                input_regions.push(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: gpui::Size::new(px(viewport.width), px(viewport.height)),
                });
            } else if self.review_native_rail
                && let Some(frame) = self.overflow.frame()
            {
                input_regions.push(Bounds {
                    origin: point(px(frame.visible.left()), px(frame.visible.top())),
                    size: gpui::Size::new(px(frame.visible.width()), px(frame.visible.height())),
                });
            }
            // WS-02: the workspace manager takes only what shows (the
            // pointer leaving it is what closes it).
            #[cfg(feature = "component-lab")]
            if self.review_native_rail
                && let Some(frame) = self.workspaces.frame()
            {
                input_regions.push(Bounds {
                    origin: point(px(frame.visible.left()), px(frame.visible.top())),
                    size: gpui::Size::new(px(frame.visible.width()), px(frame.visible.height())),
                });
            }
            // SURF-02B: the session panel, as Utilities.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail && self.session.is_open() {
                input_regions.push(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: gpui::Size::new(px(viewport.width), px(viewport.height)),
                });
            } else if self.review_native_rail
                && let Some(frame) = self.session.frame()
            {
                input_regions.push(Bounds {
                    origin: point(px(frame.visible.left()), px(frame.visible.top())),
                    size: gpui::Size::new(px(frame.visible.width()), px(frame.visible.height())),
                });
            }
            // SURF-01B: an open Utilities panel takes the whole output (a
            // press outside closes it); retracting, only what shows.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail && self.utilities.is_open() {
                input_regions.push(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: gpui::Size::new(px(viewport.width), px(viewport.height)),
                });
            } else if self.review_native_rail
                && let Some(frame) = self.utilities.frame()
            {
                input_regions.push(Bounds {
                    origin: point(px(frame.visible.left()), px(frame.visible.top())),
                    size: gpui::Size::new(px(frame.visible.width()), px(frame.visible.height())),
                });
            }
            // SURF-01C: the OSD takes the pointer where it shows (hover
            // holds it).
            #[cfg(feature = "component-lab")]
            if self.review_native_rail
                && let Some(frame) = self.bar_osd.frame()
            {
                input_regions.push(Bounds {
                    origin: point(px(frame.visible.left()), px(frame.visible.top())),
                    size: gpui::Size::new(px(frame.visible.width()), px(frame.visible.height())),
                });
            }
            #[cfg(feature = "component-lab")]
            if self.review_native_rail
                && let Some(popout) = self.bar_popout.frame()
            {
                input_regions.push(Bounds {
                    origin: point(px(popout.visible.left()), px(popout.visible.top())),
                    size: gpui::Size::new(px(popout.visible.width()), px(popout.visible.height())),
                });
            }
            // DECK-01B: the hump and tab take input, so a pointer that rises
            // with the wave stays over the Deck.
            #[cfg(feature = "component-lab")]
            if self.review_native_rail
                && let Some(shape) = &self.deck_shape
            {
                let b = shape.bounds;
                input_regions.push(Bounds {
                    origin: point(px(b.x), px(b.y)),
                    size: gpui::Size::new(px(b.width), px(b.height)),
                });
                // DECK-01D: a press or a drag keeps the whole output, so the
                // release is seen wherever it lands.
                if self.deck_press.is_some()
                    || self.deck_drag.is_some()
                    || self.bar_press.is_some()
                    || self.bar_drag.is_some()
                {
                    input_regions.push(Bounds {
                        origin: point(px(0.0), px(0.0)),
                        size: gpui::Size::new(px(viewport.width), px(viewport.height)),
                    });
                }
            }
            // DICTATION-01: the element is a button. The scene has to take
            // the pointer over it, or the click goes to whatever is below.
            //
            // Both input regions carry it: the review branch sets its own, and
            // the last one set is the one the compositor keeps.
            #[cfg(feature = "component-lab")]
            if let Some((x, y, w, h)) = self.dictation_reported {
                input_regions.push(Bounds {
                    origin: point(px(x), px(y)),
                    size: gpui::Size::new(px(w), px(h)),
                });
            }
            window.set_input_region(Some(&input_regions));

            let connector_width = layout.connector_paint_width();
            let reactive_frame = layout.reactive_frame();
            // Source-qualified reference renderer, not a second fake
            // screenshot surface. Never activate outside an explicit review
            // build and command. Wallpaper luminance is NOT observed here;
            // do not claim full donor colour parity.
            // SET-02B: the operator's material.
            #[cfg(feature = "component-lab")]
            let material_prefs = crate::surface_prefs::current().material;
            #[cfg(feature = "component-lab")]
            let native_material = self.review_native_rail.then(|| {
                let (surface, container) = material_prefs.tint.palette();
                let mut material = crate::product_shell_material::ShellMaterial::derive(
                    crate::product_shell_material::MaterialInputs {
                        // While the rail travels, the frame is its base
                        // border and the rail is painted as arms (TOPO-01B).
                        border: if transition_active {
                            crate::reference_bar::reference_shell_border_for(false)
                        } else {
                            crate::reference_bar::reference_shell_border_on(self.rail_side())
                        },
                        output: (viewport.width, viewport.height),
                        palette: crate::product_shell_material::MaterialPalette {
                            surface,
                            surface_container: container,
                            shadow: [0, 0, 0],
                        },
                        transparency: crate::product_shell_material::Transparency {
                            enabled: material_prefs.translucent,
                            base: material_prefs.shell_opacity_pct as f32 / 100.0,
                            layers: material_prefs.layer_opacity_pct as f32 / 100.0,
                        },
                        wallpaper_luminance: 0.0, // Unobserved; not an oracle.
                        light: false,
                        shadow_blur_max: crate::reference_bar::REFERENCE_SHADOW_BLUR_MAX,
                        shadow_alpha: crate::reference_bar::REFERENCE_SHADOW_ALPHA,
                    },
                );
                if !material_prefs.blur {
                    material.blur = None;
                }
                material
            });
            #[cfg(not(feature = "component-lab"))]
            let native_material: Option<crate::product_shell_material::ShellMaterial> = None;
            #[cfg(feature = "component-lab")]
            let popout_material = native_material
                .as_ref()
                .map(|material| rgba(u32::from_be_bytes(material.surface.to_u8())));
            // SET-02B: a surface's own opacity, when the operator gave it one.
            #[cfg(feature = "component-lab")]
            let surface_material = |slug: &str| {
                popout_material.map(|mut fill| {
                    if let Some(alpha) = material_prefs.surface_alpha(slug) {
                        fill.a = alpha;
                    }
                    fill
                })
            };
            #[cfg(feature = "component-lab")]
            let native_capsule_fill = native_material
                .as_ref()
                .map(|material| u32::from_be_bytes(material.container.to_u8()));
            if let Some(material) = &native_material {
                // Every surface out of the Bar, and the Deck, asks for its blur
                // as bands inside its painted shape (the hull fused to the
                // Bar, rounded corners and fillets included), never as its
                // bounding box: the blur stops at the painted edge.
                #[cfg(feature = "component-lab")]
                let surfaces: Vec<Bounds<gpui::Pixels>> = {
                    let frame_shape = |f: crate::bar_popout_host::PopoutFrame| match f.hull {
                        Some(hull) => (0..hull.command_count())
                            .filter_map(|i| hull.command(i))
                            .collect::<Vec<_>>(),
                        None => crate::product_hull::rounded_rect_commands(f.visible),
                    };
                    let mut shapes: Vec<Vec<gnos_ux_surface::grammar::HullCommand>> = [
                        self.utilities.frame(),
                        self.session.frame(),
                        self.workspaces.frame(),
                        self.bar_popout.frame(),
                        self.bar_osd.frame(),
                        self.overflow.frame(),
                    ]
                    .into_iter()
                    .flatten()
                    .map(frame_shape)
                    .collect();
                    if let Some(shape) = &self.deck_shape {
                        shapes.push(shape.outline.clone());
                    }
                    shapes
                        .iter()
                        .flat_map(|outline| crate::product_hull::outline_bands(outline, BLUR_BAND))
                        .map(|[x0, y0, x1, y1]| Bounds {
                            origin: point(px(x0), px(y0)),
                            size: gpui::Size::new(px(x1 - x0), px(y1 - y0)),
                        })
                        .collect::<Vec<Bounds<gpui::Pixels>>>()
                };
                // DICTATION-01: the element's surface is blurred behind, the
                // way the deck's is.
                #[cfg(feature = "component-lab")]
                let mut surfaces: Vec<Bounds<gpui::Pixels>> = surfaces;
                #[cfg(feature = "component-lab")]
                if let Some((x, y, w, h)) = self.dictation_surface {
                    surfaces.push(Bounds {
                        origin: point(px(x), px(y)),
                        size: gpui::Size::new(px(w), px(h)),
                    });
                }
                #[cfg(not(feature = "component-lab"))]
                let surfaces: Vec<Bounds<gpui::Pixels>> = Vec::new();
                // SHELL-FIELD-01: the compositor's pixel mask, or rectangles.
                #[cfg(feature = "component-lab")]
                let regions = material_prefs.blur_mask == crate::surface_prefs::BlurMask::Regions;
                #[cfg(not(feature = "component-lab"))]
                let regions = true;
                #[cfg(feature = "component-lab")]
                self.compositor_blur
                    .sync(material_prefs.blur && material_prefs.translucent && !regions);
                crate::product_shell_blur::request_shell_blur(window, material, &surfaces, regions);
            }
            let transition_paint = transition_frame.clone();
            // SHELL-FIELD-01B: the frame, the Bar and the surfaces out of it as
            // one shape (not while the Bar travels: its arms are painted on
            // the bare frame). A surface with its own opacity stays out and
            // paints its own hull.
            #[cfg(feature = "component-lab")]
            let field_shape = if self.review_native_rail
                && !transition_active
                && crate::shell_field_paint::enabled()
                && let Some(material) = native_material.as_ref()
            {
                let own = |slug: &str| material_prefs.surface_alpha(slug).is_some();
                // FOCUS-01D (layer): a surface standing above another keeps
                // out of the field and paints its own outline.
                use crate::attention::Attendant as A;
                let layered = |a: A| self.layered(a);
                let members: Vec<_> = [
                    (self.utilities.frame(), "utilities"),
                    (self.session.frame(), "session"),
                    (
                        self.workspaces.frame().filter(|_| !layered(A::Workspaces)),
                        "workspaces",
                    ),
                    (
                        self.bar_popout.frame().filter(|_| !layered(A::Dashboard)),
                        "popouts",
                    ),
                    (self.bar_osd.frame(), "osd"),
                    (self.overflow.frame(), "popouts"),
                    (
                        self.tray_menu.frame().filter(|_| !layered(A::Tray(0))),
                        "popouts",
                    ),
                ]
                .into_iter()
                .filter(|(_, slug)| !own(slug))
                .filter_map(|(frame, _)| frame)
                .map(|frame| crate::shell_field_paint::member(&frame))
                .collect::<Vec<_>>();
                // SHELL-FIELD-01C: the drawers and Settings, joined where
                // they meet the frame.
                let hole = material.border.hole((viewport.width, viewport.height));
                let edges =
                    |r: blobin_ui_core::layout::Rect| (r.x, r.y, r.x + r.width, r.y + r.height);
                let mut members = members;
                members.extend(
                    [
                        (self.notification_toasts.frame().map(|f| f.panel), "toasts"),
                        (self.notification_centre.frame().map(|f| f.panel), "centre"),
                        (self.settings.frame().map(|f| f.panel), "settings"),
                    ]
                    .into_iter()
                    .filter(|(_, slug)| !own(slug))
                    .filter_map(|(panel, _)| panel)
                    .filter(|p| p.width > 0.5 && p.height > 0.5)
                    .map(|p| crate::shell_field_paint::member_flush(edges(p), hole)),
                );
                Some(
                    self.shell_field
                        .shape(
                            (viewport.width, viewport.height),
                            &material.border,
                            &members,
                            material,
                        )
                        .clone(),
                )
            } else {
                None
            };
            #[cfg(feature = "component-lab")]
            crate::shell_field_paint::set_field_colour(
                field_shape
                    .as_ref()
                    .and(native_material.as_ref())
                    .map(|m| u32::from_be_bytes(m.surface.to_u8())),
            );
            // DECK-01B: the Deck is one outline in shell material, hump and
            // tab included; the prototype Dock fill is not painted under it.
            // DECK-01C: with the AppStack out, that outline is the Deck and
            // its grid as one envelope.
            #[cfg(feature = "component-lab")]
            let deck_outline = self
                .review_native_rail
                .then(|| self.deck_shape.as_ref().map(|shape| shape.outline.clone()))
                .flatten();
            let paint_perf = self.perf_counters.clone();
            let preview_native_rail = self.review_native_rail;
            // The native rail paints the Deck itself.
            let frame = if preview_native_rail {
                let mut frame = frame;
                frame.nodes.retain(|node| node.role != SceneNodeRole::Dock);
                frame
            } else {
                frame
            };
            let canvas = canvas(
                move |bounds, _, _| (bounds, frame.clone()),
                move |bounds, (_, frame), window, _| {
                    if let Some(perf) = &paint_perf {
                        perf.paints.fetch_add(1, Ordering::Relaxed);
                        if perf.motion_active.load(Ordering::Relaxed)
                            && let Ok(mut paints) = perf.motion_paints.lock()
                        {
                            paints.push(Instant::now());
                        }
                    }
                    #[cfg(feature = "component-lab")]
                    if let Some(material) = &native_material {
                        match &field_shape {
                            Some(shape) => {
                                crate::shell_field_paint::paint(bounds, shape, material, window)
                            }
                            None => crate::reference_bar::paint_native_shell_material(
                                bounds, material, false, window,
                            ),
                        }
                        // TOPO-01B: the travelling rail, as arms of shell
                        // material on the frame.
                        if let Some(transition) = &transition_paint {
                            let surface = rgba(u32::from_be_bytes(material.surface.to_u8()));
                            let w = f32::from(bounds.size.width);
                            let h = f32::from(bounds.size.height);
                            let b = crate::reference_bar::BORDER_THICKNESS;
                            for arm in &transition.arms {
                                // The arm's part inside the frame, joined to
                                // every inner edge it touches (frame joint).
                                let l = arm.bounds.left().max(b);
                                let t = arm.bounds.top().max(b);
                                let r = arm.bounds.right().min(w - b);
                                let btm = arm.bounds.bottom().min(h - b);
                                if r - l < 1.0 || btm - t < 1.0 {
                                    continue;
                                }
                                let flush = gnos_ux_surface::grammar::FlushSides {
                                    top: t <= b + 0.5,
                                    right: r >= w - b - 0.5,
                                    bottom: btm >= h - b - 0.5,
                                    left: l <= b + 0.5,
                                };
                                let inner = blobin_ui_core::layout::Rect::new(
                                    f32::from(bounds.origin.x) + l,
                                    f32::from(bounds.origin.y) + t,
                                    r - l,
                                    btm - t,
                                );
                                if let Some(path) =
                                    crate::product_hull::frame_joint_path(inner, flush)
                                {
                                    window.paint_path(path, surface);
                                }
                            }
                        }
                        if let Some(outline) = &deck_outline
                            && let Some(path) = crate::product_hull::outline_path(
                                Bounds::new(bounds.origin, gpui::Size::default()),
                                outline,
                            )
                        {
                            window.paint_path(
                                path,
                                rgba(u32::from_be_bytes(material.surface.to_u8())),
                            );
                        }
                    }
                    paint_product_scene_with_transition(
                        bounds,
                        &frame,
                        connector_width,
                        if preview_native_rail {
                            None
                        } else {
                            reactive_frame
                        },
                        if preview_native_rail {
                            None
                        } else {
                            layout.shared_dock.as_ref()
                        },
                        if preview_native_rail {
                            None
                        } else {
                            transition_paint.as_ref()
                        },
                        bar_visible && !preview_native_rail,
                        window,
                    )
                },
            )
            .size_full();
            let tray_presentation = crate::product_bar::BarTrayPresentation::derive(
                self.product_runtime.tray(),
                self.product_runtime.preferences(),
            );
            // Tray interaction is routed through the typed product action
            // layer. The scene host supplies the owning output's origin so the
            // coordinate hint is derived from real geometry.
            let tray_interaction = self.tray_interaction_readonly(cx);
            let audio_interaction = self.audio_interaction_readonly(cx);
            let audio_presentation = self.product_runtime.audio_presentation();
            // NETWORK-01 Phase 3: passive, read-only. The Bar receives only the
            // derived product presentation, which carries no protocol type.
            let network_presentation = self.product_runtime.network_presentation();
            let old_product_bar = crate::product_bar::content_with_network(
                &state,
                Some(&tray_presentation),
                Some(&audio_presentation),
                Some(&network_presentation),
                active_bar_placement,
                self.product_text_scale.font_size_px(),
                tray_interaction.as_ref(),
                audio_interaction.as_ref(),
            )
            .absolute()
            .left(px(layout.bar.left()))
            .top(px(layout.bar.top()))
            .w(px(layout.bar.width()))
            .h(px(layout.bar.height()));
            #[cfg(feature = "component-lab")]
            let rail_side = self.rail_side();
            #[cfg(feature = "component-lab")]
            let (bar, reference_workspace_hover) = if self.review_native_rail {
                let reference = self
                    .reference_visual
                    .as_mut()
                    .expect("reference profile consumer constructed for review mode");
                let mut visual = reference.project(
                    &self.product_runtime,
                    window.text_system(),
                    viewport.height,
                    self.motion_preference.effective_reduced_motion(),
                );
                // BAR-01I: a carried module's preview, or a dropped order the
                // profile has not reflected yet.
                let profile_order = visual.presentation.order.clone();
                let mut effective = bar_drag::effective_order(
                    &self.bar_drag,
                    &mut self.bar_order_pending,
                    &profile_order,
                );
                // BAR-03: a travel carries the order it started with, and a
                // reversing route keeps it as it arrives: the profile learns
                // its mirror. Every output ends the travel and writes the
                // same order, the mirror of where it started, so a second
                // writer finds it already there instead of mirroring back.
                if transition_active {
                    // BAR-03: only a travel from the edge the profile named
                    // before is the operator's edge change.
                    #[cfg(feature = "r12-production-client")]
                    let rested = {
                        let from = self
                            .bar_travel_route
                            .as_ref()
                            .and_then(|r| r.first().copied());
                        from.is_some() && from == self.profile_edge_before
                    };
                    #[cfg(not(feature = "r12-production-client"))]
                    let rested = false;
                    effective = self
                        .bar_travel_order
                        .get_or_insert_with(|| (effective.clone(), rested))
                        .0
                        .clone();
                } else if let Some((start, rested)) = self.bar_travel_order.take()
                    && let Some(route) = self.bar_travel_route.take()
                {
                    let reverses = rested && crate::bar_strip::route_reverses(&route);
                    let order = if reverses {
                        crate::bar_strip::mirrored(&start)
                    } else {
                        start.clone()
                    };
                    let write = reverses && profile_order != order;
                    println!(
                        "GNOS_UX_BAR_ORDER state=arrived route={route:?} rested={rested} reverses={reverses} start={:?} profile={:?} write={write}",
                        start.0, profile_order.0
                    );
                    effective = order.clone();
                    if write {
                        self.bar_order_pending = Some((order.clone(), Instant::now()));
                        cx.defer_in(window, move |this, _, cx| {
                            if this.write_module_order(&order, cx) != "SENT" {
                                this.bar_order_pending = None;
                            }
                        });
                    }
                }
                if self.bar_drag.is_none() {
                    self.bar_rest_order = Some(effective.clone());
                }
                visual.presentation.order = effective;
                // MODS-01: only what the hull booted stands on the Bar.
                for group in visual.presentation.order.0.iter_mut() {
                    group.retain(|e| crate::modules::on_bar(*e));
                }
                // SET-02C: on an edge too short for them, modules give way in
                // the operator's order; everything below reads this order.
                {
                    let fit = crate::surface_prefs::current().bar_fit;
                    let hidden = if fit.hide_what_does_not_fit {
                        let forms = bar_travel::module_forms(
                            &visual.presentation,
                            visual.active_window_plan.as_ref(),
                        );
                        let (kept, hidden) = crate::bar_strip::fitted_order(
                            &visual.presentation.order,
                            &forms,
                            rail_side.length((viewport.width, viewport.height)),
                            rail_side.is_horizontal(),
                            &fit.give_way_entries(),
                        );
                        visual.presentation.order = kept;
                        // BAR-02A: a "…" stands in for what gave way.
                        if !hidden.is_empty() {
                            visual.presentation.overflow = true;
                            visual.presentation.order.0[2]
                                .push(crate::reference_bar::IdleBarEntry::Overflow);
                        }
                        hidden
                    } else {
                        Vec::new()
                    };
                    if hidden != self.bar_fit_hidden {
                        println!(
                            "GNOS_UX_BAR_FIT side={rail_side:?} length={:.0} hidden={hidden:?}",
                            rail_side.length((viewport.width, viewport.height))
                        );
                        self.bar_fit_hidden = hidden;
                    }
                    self.bar_active_title = visual
                        .active_window_plan
                        .as_ref()
                        .map(|plan| plan.source_title.clone());
                }
                self.bar_viewport = (viewport.width, viewport.height);
                // Rail-local bounds (across, along), placed on the rail's
                // side (TOPO-01A/C).
                let (rail_rx, rail_ry, rail_rw, rail_rh) =
                    rail_side.rail_rect((viewport.width, viewport.height));
                // WS-02: the module where the Bar placed it, across the whole
                // rail. (The reference column stacks the modules from the
                // start, ignoring the groups and the operator's order, so on
                // a reordered Bar it pointed at another module.)
                let placed = self
                    .bar_rest_previous
                    .as_ref()
                    .filter(|(side, _)| *side == rail_side)
                    .and_then(|(_, placements)| {
                        placements
                            .iter()
                            .find(|m| m.entry == crate::reference_bar::IdleBarEntry::Workspaces)
                    })
                    .map(|m| {
                        if rail_side.is_horizontal() {
                            crate::product_reference_consumer::WorkspaceHoverBounds {
                                left: m.rect.x,
                                right: m.rect.x + m.rect.width,
                                top: rail_ry,
                                bottom: rail_ry + rail_rh,
                            }
                        } else {
                            crate::product_reference_consumer::WorkspaceHoverBounds {
                                left: rail_rx,
                                right: rail_rx + rail_rw,
                                top: m.rect.y,
                                bottom: m.rect.y + m.rect.height,
                            }
                        }
                    });
                let hover = placed.or_else(|| {
                    crate::product_reference_consumer::workspace_hover_bounds(&visual.presentation)
                        .map(|b| match rail_side {
                            crate::rail_topology::RailSide::Left
                            | crate::rail_topology::RailSide::Right => {
                                crate::product_reference_consumer::WorkspaceHoverBounds {
                                    left: b.left + rail_rx,
                                    right: b.right + rail_rx,
                                    ..b
                                }
                            }
                            // Horizontal: along is x, across is y from the
                            // rail's top.
                            _ => crate::product_reference_consumer::WorkspaceHoverBounds {
                                left: b.top,
                                right: b.bottom,
                                top: rail_ry + b.left,
                                bottom: rail_ry + b.right,
                            },
                        })
                });
                if self.bar_popout.set_side(rail_side) || self.topology_reported != Some(rail_side)
                {
                    self.topology_reported = Some(rail_side);
                    let (l, t, r, b) = rail_side.insets4();
                    println!(
                        "GNOS_UX_TOPO_RAIL side={} rail=[{rail_rx:.1},{rail_ry:.1},{rail_rw:.1},{rail_rh:.1}] insets=[{l:.1},{t:.1},{r:.1},{b:.1}]",
                        rail_side.as_str(),
                    );
                }
                self.bar_popout.set_side(rail_side);
                self.bar_popout.sync(
                    &visual.presentation,
                    (viewport.width, viewport.height),
                    visual.active_window_plan.as_ref().map_or(0.0, |plan| {
                        if rail_side.is_horizontal() {
                            crate::reference_bar::row_active_window_length(plan.icon_width)
                        } else {
                            plan.implicit_height
                        }
                    }),
                    self.product_runtime.audio(),
                );
                // The shown popout is already open; a content change repaints
                // with this render, so no motion is needed.
                self.bar_popout.sync_status(
                    self.product_runtime.network(),
                    self.product_runtime.bluetooth(),
                    self.product_runtime.power(),
                );
                if let Some(anchor) = self.bar_popout.take_anchor_report() {
                    println!("GNOS_UX_UXE01_ROUTER audio_anchor={anchor:?}");
                }
                if let Some(anchors) = self.bar_popout.take_status_anchor_report() {
                    let at = |y: Option<f32>| y.map_or("None".to_owned(), |y| format!("{y:.1}"));
                    println!(
                        "GNOS_UX_UXE04_ROUTER network={} bluetooth={} battery={}",
                        at(anchors[0]),
                        at(anchors[1]),
                        at(anchors[2])
                    );
                }
                let capsule_fill =
                    native_capsule_fill.expect("native material initialized for rail");
                let content = if rail_side.is_horizontal() {
                    crate::reference_bar::idle_bar_row_content(
                        &visual.presentation,
                        visual.active_window_plan.as_ref(),
                        &visual.tray_icons,
                        capsule_fill,
                    )
                } else {
                    crate::reference_bar::idle_bar_fixture_content(
                        &visual.presentation,
                        visual.active_window_plan.as_ref(),
                        reference.active_window_observed.clone(),
                        &visual.tray_icons,
                        capsule_fill,
                    )
                };
                // BAR-01B: while the rail travels, every module rides the
                // material instead of the rest content being hidden.
                let forms = bar_travel::module_forms(
                    &visual.presentation,
                    visual.active_window_plan.as_ref(),
                );
                let viewport_size = (viewport.width, viewport.height);
                let mut return_relayout: Option<gpui::Stateful<gpui::Div>> = None;
                let travelling = transition_frame
                    .as_ref()
                    .zip(self.bar_travel_route.as_ref())
                    .filter(|_| transition_active);
                let placed = if let Some((frame, route)) = travelling {
                    // A travel ends in a fresh rest layout, not a reflow.
                    self.bar_rest_previous = None;
                    self.bar_relayout = None;
                    let mirror = self
                        .bar_travel_order
                        .as_ref()
                        .is_some_and(|(_, rested)| *rested)
                        && crate::bar_strip::route_reverses(route);
                    let mut placements = bar_travel::travel_placements(
                        frame,
                        route,
                        &forms,
                        &visual.presentation.order,
                        mirror,
                        viewport_size,
                    );
                    // BAR-01J: a retarget's handoff carries each module from
                    // where it was painted.
                    if frame.phase
                        == gnos_ux_surface::grammar::BarPlacementTransitionPhase::RetargetHandoff
                        && let Some(from) = &self.bar_travel_handoff
                    {
                        let t = (frame.progress.value()
                            / gnos_ux_surface::grammar::RETARGET_HANDOFF)
                            .clamp(0.0, 1.0);
                        let arms: Vec<_> = frame
                            .arms
                            .iter()
                            .map(|a| {
                                blobin_ui_core::layout::Rect::new(
                                    a.bounds.left(),
                                    a.bounds.top(),
                                    a.bounds.width(),
                                    a.bounds.height(),
                                )
                            })
                            .collect();
                        let rect = |g: gnos_ux_surface::grammar::GeometryRect| {
                            blobin_ui_core::layout::Rect::new(
                                g.left(),
                                g.top(),
                                g.width(),
                                g.height(),
                            )
                        };
                        let other = match &self.bar_transition_qualification {
                            Some((
                                crate::product_bar::BarTransitionQualification::Rebased {
                                    origin,
                                    ..
                                },
                                _,
                            )) => origin
                                .arms
                                .iter()
                                .find(|a| a.edge != origin.active_edge)
                                .map(|a| rect(a.bounds)),
                            _ => None,
                        };
                        placements = crate::bar_strip::handoff_blend(
                            from,
                            &placements,
                            t * t * (3.0 - 2.0 * t),
                            rect(frame.active_allocation),
                            other,
                            &arms,
                        );
                    }
                    self.bar_last_travel_placements = Some(placements.clone());
                    // BAR-01G: where the pointer router (still on the
                    // rail's last side) sees each module.
                    let router_rest = bar_travel::rest_placements(
                        &forms,
                        &visual.presentation.order,
                        rail_side,
                        (rail_rx, rail_ry, rail_rw, rail_rh),
                        viewport_size,
                    );
                    self.bar_travel_map = Some((placements.clone(), router_rest));
                    if self.continuity_trace {
                        self.continuity_seq += 1;
                        self.continuity_settled = false;
                        bar_travel::report_continuity(
                            self.continuity_seq,
                            Some(frame),
                            &placements,
                            &format!(
                                "{} dock_input={}",
                                bar_travel::surfaces_trace(
                                    self.deck_shape.as_ref(),
                                    relocation.as_ref(),
                                    viewport_size,
                                ),
                                self.bar_dock_input
                            ),
                        );
                    }
                    let column = crate::reference_bar::idle_bar_column_modules(
                        &visual.presentation,
                        visual.active_window_plan.as_ref(),
                        std::rc::Rc::new(std::cell::RefCell::new(None)),
                        &visual.tray_icons,
                        capsule_fill,
                    );
                    let row = crate::reference_bar::idle_bar_row_modules(
                        &visual.presentation,
                        visual.active_window_plan.as_ref(),
                        &visual.tray_icons,
                        capsule_fill,
                    );
                    bar_travel::travel_layer(&placements, column, row).id("product-bar")
                } else {
                    // BAR-01D: the rest layout as placed modules, to see a
                    // settings change coming and reflow it continuously.
                    let edge = rail_side.edge();
                    let rest = bar_travel::rest_layout(
                        &forms,
                        &visual.presentation.order,
                        crate::rail_topology::RailSide::length(rail_side, viewport_size),
                        rail_side.is_horizontal(),
                    );
                    let route = [edge];
                    let rail_bounds = GeometryRect::try_new(
                        rail_rx,
                        rail_ry,
                        rail_rx + rail_rw,
                        rail_ry + rail_rh,
                        0.0,
                    );
                    let rest_now = rail_bounds
                        .as_ref()
                        .map(|bounds| {
                            crate::bar_strip::Conveyor {
                                viewport: viewport_size,
                                route: &route,
                                forms: &forms,
                                source: &rest,
                                target: &rest,
                                inner: crate::reference_bar::BAR_INNER_WIDTH,
                            }
                            .rest_on(edge, *bounds)
                        })
                        .unwrap_or_default();
                    if self.continuity_trace && !self.continuity_settled {
                        self.continuity_settled = true;
                        self.continuity_seq += 1;
                        bar_travel::report_continuity(
                            self.continuity_seq,
                            None,
                            &rest_now,
                            &bar_travel::surfaces_trace(
                                self.deck_shape.as_ref(),
                                None,
                                viewport_size,
                            ),
                        );
                    }
                    if !transition_active {
                        self.bar_travel_route = None;
                        self.bar_travel_map = None;
                        self.bar_travel_pointer = None;
                    }
                    let reduced = self.effective_reduced_motion();
                    if let Some((side, before)) = &self.bar_rest_previous
                        && *side == rail_side
                        && !reduced
                        && bar_travel::relayout_jumped(before, &rest_now)
                    {
                        // A jump during a reflow restarts it from what is
                        // painted, so the Bar never jumps itself.
                        let from = match (&self.bar_relayout, &self.bar_painted) {
                            (Some(_), Some(painted)) => painted.clone(),
                            _ => before.clone(),
                        };
                        println!(
                            "GNOS_UX_BAR_RELAYOUT state=start modules={}",
                            rest_now.len()
                        );
                        self.bar_relayout = Some((from, Instant::now()));
                    }
                    // Where each module stands, on every change (live
                    // automation aims with it, as the harnesses do).
                    if self
                        .bar_rest_previous
                        .as_ref()
                        .is_none_or(|(side, before)| *side != rail_side || before != &rest_now)
                    {
                        bar_travel::report_modules(rail_side, &rest_now);
                    }
                    self.bar_rest_previous = Some((rail_side, rest_now.clone()));
                    // SET-03: the reflow's duration is the operator's.
                    let duration =
                        crate::surface_prefs::advanced_ms("reflow_ms").div_f32(self.motion_tempo());
                    let blend = self.bar_relayout.as_ref().and_then(|(from, started)| {
                        let t = started.elapsed().as_secs_f32() / duration.as_secs_f32();
                        (t < 1.0).then(|| (from.clone(), t * t * (3.0 - 2.0 * t)))
                    });
                    if blend.is_none() && self.bar_relayout.take().is_some() {
                        println!("GNOS_UX_BAR_RELAYOUT state=settled");
                    }
                    // BAR-01I: the others without the carried module, where a
                    // carried module aims.
                    if let Some(drag) = &self.bar_drag {
                        let mut without = visual.presentation.clone();
                        without.order = crate::bar_strip::moved(
                            &self.bar_rest_order.clone().unwrap_or_default(),
                            drag.entry,
                            None,
                        );
                        let forms_without =
                            bar_travel::module_forms(&without, visual.active_window_plan.as_ref());
                        self.bar_rest_others = Some((
                            rail_side,
                            bar_travel::rest_placements(
                                &forms_without,
                                &without.order,
                                rail_side,
                                (rail_rx, rail_ry, rail_rw, rail_rh),
                                viewport_size,
                            ),
                        ));
                    }
                    let carried = self.bar_ghost();
                    let blend = blend.or_else(|| carried.as_ref().map(|_| (rest_now.clone(), 1.0)));
                    if let (Some((from, w)), Ok(bounds)) = (blend, rail_bounds) {
                        let material = blobin_ui_core::layout::Rect::new(
                            bounds.left(),
                            bounds.top(),
                            bounds.width(),
                            bounds.height(),
                        );
                        let mut placements =
                            bar_travel::blend_relayout(&from, &rest_now, w, material);
                        self.bar_painted = Some(placements.clone());
                        if self.continuity_trace {
                            self.continuity_seq += 1;
                            bar_travel::report_relayout(self.continuity_seq, w, &placements);
                        }
                        // The carried module's slot stays open; it follows
                        // the pointer instead.
                        if let Some(ghost) = carried {
                            placements.retain(|m| m.entry != ghost.entry);
                            placements.push(ghost);
                        }
                        self.start_motion_driver(cx);
                        let column = crate::reference_bar::idle_bar_column_modules(
                            &visual.presentation,
                            visual.active_window_plan.as_ref(),
                            std::rc::Rc::new(std::cell::RefCell::new(None)),
                            &visual.tray_icons,
                            capsule_fill,
                        );
                        let row = crate::reference_bar::idle_bar_row_modules(
                            &visual.presentation,
                            visual.active_window_plan.as_ref(),
                            &visual.tray_icons,
                            capsule_fill,
                        );
                        return_relayout = Some(
                            bar_travel::travel_layer(&placements, column, row).id("product-bar"),
                        );
                    }
                    return_relayout.unwrap_or_else(|| {
                        content
                            .absolute()
                            .left(px(rail_rx))
                            .top(px(rail_ry))
                            .w(px(rail_rw))
                            .h(px(rail_rh))
                            .id("product-bar")
                    })
                };
                (placed, hover)
            } else {
                (old_product_bar.id("product-bar"), None)
            };
            #[cfg(not(feature = "component-lab"))]
            let bar = old_product_bar.id("product-bar");
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                self.report_status_popout();
            }
            let bar_hit_regions = if let Some(ref transition) = transition_frame {
                transition.input_regions()
            } else {
                gnos_ux_surface::pointer_regions::project_resolved_bar_pointer_regions(
                    viewport,
                    layout.bar_placement,
                )
                .expect("resolved Bar input")
                .regions()
                .iter()
                .filter(|region| region.role().as_str() == "surface")
                .map(|region| region.bounds())
                .collect::<Vec<_>>()
            };
            let click_rail_side = self.rail_side();
            let bar = if let Some(accessibility_label) =
                state.bar_accessibility_action_label(self.product_panel.is_workspace_summary())
            {
                bar.role(gpui::Role::Button)
                    .aria_label(accessibility_label)
                    .aria_selected(self.product_panel.is_workspace_summary())
                    // An actionable control must be reachable by keyboard and
                    // assistive activation, not only by pointer.
                    .focusable()
                    .tab_stop(true)
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        // The reference recipe is hover-owned, not a generic
                        // 60px-wide toggle. Keyboard activation stays available.
                        if preview_native_rail && !matches!(event, gpui::ClickEvent::Keyboard(_)) {
                            return;
                        }
                        // LIVE-04: with the Bar's modules, a click belongs to
                        // the module under it and the workspace manager
                        // (WS-02A) supersedes this summary; a stray Enter on
                        // the focused Bar must not open it either.
                        #[cfg(feature = "component-lab")]
                        if this.reference_visual.is_some() {
                            return;
                        }
                        if !matches!(event, gpui::ClickEvent::Keyboard(_)) {
                            let point = event.position();
                            let hit = if preview_native_rail {
                                click_rail_side.rail_contains(
                                    (viewport.width, viewport.height),
                                    f32::from(point.x),
                                    f32::from(point.y),
                                )
                            } else {
                                bar_hit_regions.iter().any(|r| {
                                    r.contains_point(f32::from(point.x), f32::from(point.y))
                                })
                            };
                            if !hit {
                                return;
                            }
                        }
                        this.refresh_product_runtime();
                        let can_open = this.product_runtime.semantic().can_open_workspace();
                        if can_open {
                            #[cfg(feature = "component-lab")]
                            {
                                this.reference_workspace_hover_owned = false;
                            }
                            this.product_panel = this
                                .product_panel
                                .toggled(ProductPanelState::WorkspaceSummary);
                            println!(
                                "GNOS_UX_PRODUCT_WORKSPACE panel={:?} keyboard={}",
                                this.product_panel,
                                matches!(event, gpui::ClickEvent::Keyboard(_))
                            );
                            if let Some(perf) = &this.perf_counters {
                                perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
                                println!(
                                    "GNOS_UX_PERF_ACTION output={} uptime_ms={} panel={}",
                                    this.runtime_output
                                        .as_ref()
                                        .map_or("UNKNOWN", RuntimeId::as_str),
                                    perf.started.map_or(0, |start| start.elapsed().as_millis()),
                                    this.product_panel
                                );
                            }
                            this.start_motion_driver(cx);
                            cx.notify();
                        }
                    }))
            } else {
                // AccessKit Label nodes expose their text as value, not label.
                bar.role(gpui::Role::Label).aria_value(state.bar_label())
            };
            // The donor wheel is relative, but Gnosix will only activate
            // the next *observed* workspace on this exact output. No raw
            // "workspace r+1" compositor command, no implicit workspace create.
            #[cfg(all(feature = "component-lab", feature = "r13-production-client"))]
            let bar = bar.on_scroll_wheel(cx.listener(
                move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                    if !preview_native_rail
                        || !reference_workspace_hover.is_some_and(|bounds| {
                            bounds.contains(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                            )
                        })
                    {
                        return;
                    }
                    cx.stop_propagation();
                    let vertical = match event.delta {
                        gpui::ScrollDelta::Lines(delta) => delta.y,
                        gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y),
                    };
                    let targets = this.product_runtime.semantic().workspace_manager_targets();
                    let active = targets.iter().position(|candidate| {
                        this.product_runtime.semantic().workspace_manager_is_active(candidate)
                    });
                    let selected = active
                        .and_then(|index| {
                            crate::product_reference_consumer::bounded_workspace_step_index(
                                index,
                                targets.len(),
                                vertical,
                            )
                        })
                        .and_then(|index| targets.get(index))
                        .cloned();
                    if let Some(selected) = selected {
                        println!(
                            "GNOS_UX_EC04_WORKSPACE_WHEEL output={} target={} admission=typed_exact_observed",
                            this.runtime_output.as_ref().map_or("UNKNOWN", RuntimeId::as_str),
                            selected.label,
                        );
                        this.activate_workspace_intent(selected, cx);
                    } else {
                        println!(
                            "GNOS_UX_EC04_WORKSPACE_WHEEL outcome=refused reason=no_observed_adjacent_target"
                        );
                    }
                },
            ));
            // Populated with focus-aware rows by the production block below, or
            // with inert rows in a build that has no product focus registry. The
            // AppStack surface itself is not feature-gated: it is a product
            // surface, and whether it currently has keyboard focus handles is a
            // property of the build, not of the product.
            #[allow(unused_mut, unused_assignments)]
            let mut appstack_row_elements: Vec<gpui::Stateful<gpui::Div>> = Vec::new();
            let appstack_query = crate::product_appstack::AppStackQuery::parse(
                self.appstack_search.read(cx).query(),
            );
            let appstack_query_active = !appstack_query.text.is_empty();
            let appstack_preferences =
                crate::product_appstack::AppStackPreferenceProjection::default();
            let appstack_model = if appstack_target_open || appstack_needs_motion {
                crate::product_appstack::appstack_rows_for(
                    &self.product_runtime,
                    appstack_query,
                    &appstack_preferences,
                )
            } else {
                Vec::new()
            };
            let outer_origin = layout.shared_dock.map_or((0.0, 0.0), |f| {
                (f.geometry.outer.left(), f.geometry.outer.top())
            });
            // DECK-01B: the Deck's grown cards and label stand outside its
            // base, inside its painted hump; nothing clips them to the base.
            #[cfg(feature = "component-lab")]
            let deck_unclipped = self.review_native_rail;
            #[cfg(not(feature = "component-lab"))]
            let deck_unclipped = false;
            #[allow(unused_mut)]
            let mut dock = div()
                .absolute()
                .left(px(layout.dock.left() - outer_origin.0))
                .top(px(layout.dock.top() - outer_origin.1))
                .w(px(layout.dock.width()))
                .h(px(layout.dock.height()))
                .flex()
                .items_center()
                .justify_center()
                .text_color(white())
                .when(!deck_unclipped, |dock| dock.overflow_hidden())
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(self.product_text_scale.font_size_px()));

            #[cfg(feature = "r13-production-client")]
            let dock = {
                let (semantic_targets, active_reference) = match &state {
                    ProductSemanticState::Current(content) => (
                        content.workspace_manager_targets.as_slice(),
                        content
                            .active_workspace
                            .as_ref()
                            .map(|workspace| workspace.reference),
                    ),
                    ProductSemanticState::Unavailable | ProductSemanticState::Invalid => {
                        (&[][..], None)
                    }
                };
                let workspace_presentation = workspace_bar_presentation(
                    semantic_targets,
                    active_reference,
                    self.product_runtime.preferences(),
                );
                let workspace_display_mode = workspace_presentation.mode;
                let targets = workspace_presentation.targets;
                // The AppStack rows are part of the same product focus registry,
                // not a second one. `reconcile` retains only the identities it is
                // given, so a separate call for the AppStack would evict the
                // workspace handles and vice versa; the set is therefore built as
                // one union, in traversal order, before a single reconcile.
                let appstack_rows = &appstack_model;

                // Reconcile focus identity against the product's own references,
                // never against render order or the display label.
                let mut identities: Vec<ProductControlId> = Vec::new();
                if state.can_open_appstack() {
                    identities.push(panel_control_identity("appstack"));
                }
                let appstack_search_identity = panel_control_identity("appstack-search");
                if appstack_target_open {
                    identities.push(appstack_search_identity.clone());
                }
                identities.extend(
                    appstack_rows
                        .iter()
                        .filter(|_| appstack_interactable)
                        .map(|row| installed_application_control_identity(&row.entry)),
                );
                identities.extend(targets.iter().map(|workspace| {
                    workspace_control_identity(&workspace.reference.workspace_id())
                }));
                self.product_focus.reconcile(&identities);
                let handles: Vec<gpui::FocusHandle> = identities
                    .iter()
                    .enumerate()
                    .map(|(index, id)| {
                        self.product_focus
                            .ensure(id, &mut || traversable_focus_handle(cx))
                            .clone()
                            .tab_index(index as isize)
                    })
                    .collect();

                let focus_for = |identity: &ProductControlId| {
                    identities
                        .iter()
                        .position(|candidate| candidate == identity)
                        .and_then(|index| handles.get(index).map(|handle| (handle, index)))
                };

                self.appstack_search.update(cx, |input, _| {
                    input.set_design(design);
                    if let Some((handle, _)) = focus_for(&appstack_search_identity) {
                        input.set_focus_handle(handle.clone());
                    }
                });

                self.arbitrate_product_focus(&identities, &handles, window, cx);
                self.report_product_focus(&identities, &handles, window);

                // The AppStack affordance begins the AppStack keyboard path.
                // Explicit handle indices place its application rows next;
                // Dock workspace controls remain reachable after the list.
                let appstack_identity = panel_control_identity("appstack");
                let affordance = if let Some(label) =
                    state.appstack_accessibility_action_label(self.product_panel.is_appstack())
                {
                    let product_focus = match focus_for(&appstack_identity) {
                        Some((handle, index)) => ProductControlFocus::new(handle, index as isize),
                        None => ProductControlFocus::inert(),
                    };
                    Some(
                        button(
                            "product-appstack-affordance",
                            APPSTACK_AFFORDANCE_LABEL,
                            label,
                            ProductControlState::Default,
                            design,
                            &product_focus,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.product_click_blocked() {
                                return;
                            }
                            // DECK-01C: on the native rail the AppStack
                            // control opens the AppStack in the Deck (D03).
                            #[cfg(feature = "component-lab")]
                            if this.review_native_rail {
                                let outcome = this.stack.toggle();
                                this.apply_stack_outcome(outcome, window, cx);
                                return;
                            }
                            let _ = &window;
                            this.toggle_appstack_panel(cx);
                        })),
                    )
                } else {
                    None
                };

                let items = targets
                    .into_iter()
                    .map(|workspace| {
                        let active = active_reference == Some(workspace.reference);
                        let identity =
                            workspace_control_identity(&workspace.reference.workspace_id());
                        let product_focus = match focus_for(&identity) {
                            Some((handle, index)) => {
                                ProductControlFocus::new(handle, index as isize)
                            }
                            None => ProductControlFocus::inert(),
                        };
                        workspace_item_with_display_mode(
                            identity.as_str().to_owned(),
                            workspace.label.clone(),
                            active,
                            workspace_display_mode,
                            design,
                            &product_focus,
                        )
                        .mx(px(ProductSpacing::XXS))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.product_click_blocked() {
                                return;
                            }
                            this.activate_workspace_intent(workspace.clone(), cx);
                        }))
                    })
                    .collect::<Vec<_>>();
                let feedback =
                    self.product_runtime
                        .workspace_activation_feedback()
                        .map(|feedback| {
                            design_text(feedback, ProductTextStyle::Caption, design)
                                .ml(px(ProductSpacing::XS))
                        });
                // Rows are placed immediately after the affordance in keyboard
                // order. Workspace controls remain in the Dock and follow the
                // list in tab order, so they do not interrupt application traversal.
                appstack_row_elements = appstack_rows
                    .iter()
                    .map(|row| {
                        let identity = installed_application_control_identity(&row.entry);
                        let product_focus = match (appstack_target_open, focus_for(&identity)) {
                            (true, Some((handle, index))) => {
                                ProductControlFocus::new(handle, index as isize)
                            }
                            _ => ProductControlFocus::inert(),
                        };
                        let entry = row.entry.clone();
                        let item = installed_application_item(
                            row.element_id(),
                            row.name.clone(),
                            entry.as_str(),
                            design,
                            &product_focus,
                        );
                        if appstack_interactable {
                            item.on_click(cx.listener(move |this, _, _, cx| {
                                if this.product_click_blocked() {
                                    return;
                                }
                                this.activate_installed_application(entry.clone(), cx);
                            }))
                        } else {
                            item
                        }
                    })
                    .collect();

                #[cfg(feature = "component-lab")]
                let deck_native = self.review_native_rail;
                #[cfg(not(feature = "component-lab"))]
                let deck_native = false;
                // DECK-01A: on the native rail the Dock is the Deck: appcards,
                // then the AppStack control in its last slot (P01: workspace
                // controls stay on the rail).
                #[cfg(feature = "component-lab")]
                let (dock, affordance) = if deck_native {
                    (
                        self.deck_dock(
                            dock,
                            (layout.dock.left(), layout.dock.top()),
                            affordance.map(|a| a.into_any_element()),
                        ),
                        None,
                    )
                } else {
                    (dock, affordance)
                };
                let dock = dock.when_some(affordance, |dock, affordance| dock.child(affordance));
                if deck_native {
                    dock
                } else if items.is_empty() {
                    dock.child(state.dock_label())
                } else {
                    dock.children(items)
                        .when_some(feedback, |dock, feedback| dock.child(feedback))
                }
            };

            // Without a product focus registry there are no handles to bind, so
            // the rows are rendered inert. The content, ordering and identity are
            // still exactly what production renders; only keyboard reachability
            // differs, and pretending otherwise would be a claim the build cannot
            // support.
            #[cfg(not(feature = "r13-production-client"))]
            {
                appstack_row_elements = appstack_model
                    .iter()
                    .map(|row| {
                        let entry = row.entry.clone();
                        let item = installed_application_item(
                            row.element_id(),
                            row.name.clone(),
                            entry.as_str(),
                            design,
                            &ProductControlFocus::inert(),
                        );
                        // An inert control is one with no *tab stop*, not one
                        // with no action. Dropping the click handler here would
                        // make the passive build's AppStack decorative, and a
                        // surface that renders but cannot be used is worse than
                        // one that is not rendered at all.
                        if appstack_interactable {
                            item.on_click(cx.listener(move |this, _, _, cx| {
                                if this.product_click_blocked() {
                                    return;
                                }
                                this.activate_installed_application(entry.clone(), cx);
                            }))
                        } else {
                            item
                        }
                    })
                    .collect();
            }
            #[cfg(not(feature = "r13-production-client"))]
            let dock = dock.child(state.dock_label());
            let workspace = layout.workspace.map(|rect| {
                crate::product_bar::workspace_content(
                    &state,
                    layout.bar_placement,
                    self.product_text_scale.font_size_px(),
                )
                .absolute()
                .left(px(rect.left()))
                .top(px(rect.top()))
                .w(px(rect.width()))
                .h(px(rect.height()))
                .id("product-workspace")
                .role(gpui::Role::Button)
                .aria_label("Fermer le résumé de l’espace de travail")
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.product_click_blocked() {
                        return;
                    }
                    this.product_panel = ProductPanelState::Closed;
                    println!("GNOS_UX_PRODUCT_WORKSPACE panel=Closed");
                    if let Some(perf) = &this.perf_counters {
                        perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
                        println!(
                            "GNOS_UX_PERF_ACTION output={} uptime_ms={} open=false",
                            this.runtime_output
                                .as_ref()
                                .map_or("UNKNOWN", RuntimeId::as_str),
                            perf.started.map_or(0, |start| start.elapsed().as_millis())
                        );
                    }
                    this.start_motion_driver(cx);
                    cx.notify();
                }))
            });
            // The AppStack surface. The parent owns the scroll, so a list longer
            // than the body is reachable rather than clipped: the applications
            // are intrinsic-height controls and none is dropped, hidden, or
            // shrunk to fit.
            let appstack = layout
                .shared_dock
                .filter(|frame| frame.appstack.visible)
                .map(|frame| {
                    let rect = frame.content_clip;
                    let mut body = div()
                        .absolute()
                        .left(px(rect.left() - outer_origin.0))
                        .top(px(rect.top() - outer_origin.1))
                        .w(px(rect.width()))
                        .h(px(rect.height()))
                        .flex()
                        .flex_col()
                        .gap(px(ProductSpacing::XS))
                        .overflow_hidden()
                        .id("product-appstack")
                        .role(gpui::Role::Group)
                        .aria_label("AppStack")
                        .child(self.appstack_search.clone());
                    // The empty state is a `Div` and the rows are `Stateful<Div>`,
                    // so the list is heterogeneous and is collected as
                    // `AnyElement` rather than pretending both are the same type.
                    let list: Vec<gpui::AnyElement> = if appstack_row_elements.is_empty() {
                        vec![
                            crate::product_design::appstack_empty_state(
                                if appstack_query_active {
                                    "AppStack · aucun résultat"
                                } else {
                                    "AppStack · aucune application installée"
                                },
                                design,
                            )
                            .into_any_element(),
                        ]
                    } else {
                        appstack_row_elements
                            .into_iter()
                            .map(Stateful::into_any_element)
                            .collect()
                    };
                    // The scroll container has to be a `Stateful` element: gpui
                    // exposes `overflow_y_scroll` on the interactive trait, and
                    // tracking a handle is what lets the scroll position survive a
                    // rerender. Without the handle the list would jump back to the
                    // top every time anything in the scene changed.
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .flex_1()
                            .gap(px(ProductSpacing::XXS))
                            .id("product-appstack-list")
                            .overflow_y_scroll()
                            .track_scroll(&self.appstack_scroll)
                            .children(list),
                    );
                    if let Some(feedback) = crate::product_appstack::launch_feedback(
                        self.product_runtime.launch_state(),
                    ) {
                        body =
                            body.child(crate::product_appstack_view::feedback(&feedback, design));
                    }
                    body
                });
            let product_diagnostics = self.product_diagnostics.then(|| {
                let lines = ProductDiagnosticSnapshot::from_runtime(&self.product_runtime)
                    .compact_text()
                    .lines()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                panel()
                    .absolute()
                    .left(px(ProductSpacing::MD))
                    .top(px(layout.bar.bottom() + ProductSpacing::XS))
                    .max_w(px(560.0))
                    .flex()
                    .flex_col()
                    .gap(px(ProductSpacing::XXS))
                    .children(
                        lines
                            .into_iter()
                            .map(|line| design_text(line, ProductTextStyle::Caption, design)),
                    )
            });
            let bar_pointer_placement = layout.bar_placement;
            let bar_pointer_state = self.bar_reveal_state;
            // The one open menu surface, rendered only by the output that opened
            // it. Other outputs never render it, so one interaction request can
            // never fan out into per-output surfaces.
            let my_output = self
                .runtime_output
                .as_ref()
                .map(|output| output.as_str().to_owned());
            let menu_session = crate::product_tray::TraySession::shared(cx);
            let menu_controller = self.tray_controller.clone();
            let menu_dispatch: crate::product_tray_menu_render::MenuRowDispatch = {
                let session = menu_session.clone();
                let controller = menu_controller.clone();
                std::rc::Rc::new(
                    move |app: &mut gpui::App, intent: crate::product_tray_action::TrayIntent| {
                        spawn_menu_row_intent(session.clone(), controller.clone(), app, intent);
                    },
                )
            };
            // SHELL-FIELD-01C: the menu shown here, its extrusion and its size.
            #[cfg(feature = "component-lab")]
            let tray_place = if self.review_native_rail {
                let controller = self.tray_controller.read(cx);
                let shown = controller.menu().is_some()
                    && controller.open_item().is_some()
                    && controller
                        .open_requested_by()
                        .is_some_and(|by| Some(by) == my_output.as_deref());
                let at = self.rail_tray_menu_at;
                // The menu shown is the one the controller has (the next
                // one asked for replaces it when it arrives, FOCUS-01B).
                let key = controller
                    .open_item()
                    .and_then(|item| self.rail_tray_index_of(item))
                    .or(self.rail_tray_menu_key)
                    .unwrap_or_default();
                let side = self.rail_side();
                let mut moved = self.tray_menu.sync(
                    shown && at.is_some(),
                    key,
                    at.unwrap_or_default(),
                    side,
                    (viewport.width, viewport.height),
                );
                if moved {
                    // Another menu: measured afresh.
                    self.tray_menu_size.set(None);
                }
                if let Some(size) = self.tray_menu_size.get() {
                    moved |= self.tray_menu.measured(key, size);
                }
                if moved || self.tray_menu.needs_frame() {
                    self.start_motion_driver(cx);
                }
                self.tray_menu
                    .origin()
                    .or(at)
                    .map(|origin| (origin, self.tray_menu.frame()))
            } else {
                None
            };
            #[cfg(feature = "component-lab")]
            let tray_measure = self.tray_menu_size.clone();
            // FOCUS-01B: the menus passing along the surface, other than the
            // live one, inert, each at its place on the strip.
            #[cfg(feature = "component-lab")]
            let tray_inert = |live: Option<usize>| -> Vec<gpui::Stateful<gpui::Div>> {
                let Some(frame) = self.tray_menu.frame() else {
                    return Vec::new();
                };
                self.tray_menu
                    .contents()
                    .into_iter()
                    .filter(|(k, _, _)| Some(*k) != live)
                    .filter_map(|(k, offset, size)| {
                        let size = size?;
                        let rows = self.tray_menu_rows.iter().find(|(r, _)| *r == k)?;
                        let (x, y) = crate::bar_popout_view::content_origin(&frame, size, offset);
                        Some(
                            crate::product_tray_menu_render::render_tray_menu_surface(
                                &rows.1, design, None,
                            )
                            .absolute()
                            .bg(gpui::transparent_black())
                            .border_0()
                            .rounded(px(0.0))
                            .left(px(x))
                            .top(px(y)),
                        )
                    })
                    .collect()
            };
            #[cfg(feature = "component-lab")]
            let tray_live_key = self
                .tray_menu
                .is_shown()
                .then(|| self.tray_menu.key())
                .flatten();
            #[cfg(feature = "component-lab")]
            let tray_live_inert = tray_inert(tray_live_key);
            #[cfg(feature = "component-lab")]
            let tray_retract_inert = tray_inert(None);
            // FOCUS-01D (layer): standing above another surface, the menu paints
            // its own body (the field no longer does).
            #[cfg(feature = "component-lab")]
            let tray_own = self
                .layered(crate::attention::Attendant::Tray(0))
                .then(|| surface_material("popouts"))
                .flatten()
                .map(|fill| gpui::Rgba { a: 1.0, ..fill });
            #[cfg(feature = "component-lab")]
            let tray_live_place = tray_live_key.and_then(|k| {
                self.tray_menu
                    .contents()
                    .into_iter()
                    .find(|(c, _, _)| *c == k)
            });
            // The rows shown, kept for the menu's retraction once it closes.
            #[cfg(feature = "component-lab")]
            let tray_menu_last = std::cell::RefCell::new(None);
            let tray_menu_surface = self
                .tray_controller
                .read(cx)
                .menu()
                .cloned()
                .and_then(|menu| {
                    let controller = self.tray_controller.read(cx);
                    let owned_here = controller
                        .open_requested_by()
                        .is_some_and(|by| Some(by) == my_output.as_deref());
                    let item = controller.open_item().cloned()?;
                    if !owned_here {
                        return None;
                    }
                    let presentation =
                        crate::product_tray_menu_surface::TrayMenuSurfacePresentation::from_snapshot(
                            item, &menu,
                        );
                    #[cfg(feature = "component-lab")]
                    tray_menu_last.replace(Some(presentation.clone()));
                    #[cfg(feature = "component-lab")]
                    let tray_live_inert = tray_live_inert;
                    // ICONS-01: its icons draw on as it comes out of the Bar.
                    #[cfg(feature = "component-lab")]
                    let drawn = tray_place
                        .as_ref()
                        .and_then(|(_, frame)| frame.as_ref())
                        .map_or(0.0, |f| crate::icons::draw_on(f.openness));
                    #[cfg(not(feature = "component-lab"))]
                    let drawn = 1.0;
                    let surface = crate::icons::drawing(drawn, || {
                        crate::product_tray_menu_render::render_tray_menu_surface(
                            &presentation,
                            design,
                            Some(&menu_dispatch),
                        )
                    })
                    .absolute();
                    // RAIL-01: beside the rail, at the item; a press outside
                    // closes it, and a press inside reaches only its rows.
                    // SHELL-FIELD-01C: it extrudes out of the Bar at its item,
                    // one shape with it (the field paints its body; the menu
                    // paints its rows), measured once laid out.
                    #[cfg(feature = "component-lab")]
                    if self.review_native_rail {
                        let ((ox, oy), frame) = tray_place?;
                        let measure = tray_measure.clone();
                        let content = surface
                            .bg(gpui::transparent_black())
                            .border_0()
                            .rounded(px(0.0))
                            .child(
                                gpui::canvas(
                                    move |bounds, window, _| {
                                        let size = (
                                            f32::from(bounds.size.width),
                                            f32::from(bounds.size.height),
                                        );
                                        // A new size opens the menu on the
                                        // next frame.
                                        if measure.get() != Some(size) {
                                            measure.set(Some(size));
                                            window.request_animation_frame();
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .size_full(),
                            );
                        let clip = match frame {
                            // Hung from the joint, sliding out with the
                            // surface; at its place on the strip while the
                            // menus pass along (FOCUS-01B), the others inert.
                            Some(f) => {
                                let (offset, size) = tray_live_place
                                    .map(|(_, o, s)| (o, s))
                                    .unwrap_or((0.0, None));
                                let size = size.unwrap_or((f.rect.width(), f.rect.height()));
                                let (dx, dy) =
                                    crate::bar_popout_view::content_origin(&f, size, offset);
                                gpui::div()
                                    .id("tray-menu-extrusion")
                                    .absolute()
                                    .left(px(f.visible.left()))
                                    .top(px(f.visible.top()))
                                    .w(px(f.visible.width()))
                                    .h(px(f.visible.height()))
                                    .overflow_hidden()
                                    .when_some(tray_own, |d, fill| {
                                        d.bg(fill).rounded(px(crate::reference_bar::BORDER_ROUNDING))
                                    })
                                    .children(tray_live_inert)
                                    .child(content.left(px(dx)).top(px(dy)))
                            }
                            // Not measured yet: laid out, clipped to nothing.
                            None => gpui::div()
                                .id("tray-menu-extrusion")
                                .absolute()
                                .left(px(ox))
                                .top(px(oy))
                                .w(px(0.0))
                                .h(px(0.0))
                                .overflow_hidden()
                                .child(content.left(px(0.0)).top(px(0.0))),
                        };
                        return Some(
                            clip.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.rail_tray_menu_dismiss(cx);
                            })),
                        );
                    }
                    Some(
                        surface
                            .right(px(8.0))
                            .top(px(layout.bar.bottom() + ProductSpacing::XS)),
                    )
                });
            // SHELL-FIELD-01C: a closed menu retracts into the Bar with its
            // last rows, inert.
            #[cfg(feature = "component-lab")]
            let tray_menu_surface = {
                // The live menu's rows, by item, for its passing along and
                // its retraction; the others go once off the strip.
                if let (Some(shown), Some(key)) = (tray_menu_last.into_inner(), tray_live_key) {
                    self.tray_menu_rows.retain(|(k, _)| *k != key);
                    self.tray_menu_rows.push((key, shown));
                }
                let on_strip: Vec<usize> = self
                    .tray_menu
                    .contents()
                    .into_iter()
                    .map(|(k, _, _)| k)
                    .collect();
                self.tray_menu_rows.retain(|(k, _)| on_strip.contains(k));
                tray_menu_surface.or_else(|| {
                    // Retracting (or the live menu not drawn): every menu
                    // on the surface, inert.
                    if self.tray_menu.is_shown() {
                        return None;
                    }
                    let frame = self.tray_menu.frame()?;
                    Some(
                        gpui::div()
                            .id("tray-menu-extrusion")
                            .absolute()
                            .left(px(frame.visible.left()))
                            .top(px(frame.visible.top()))
                            .w(px(frame.visible.width()))
                            .h(px(frame.visible.height()))
                            .overflow_hidden()
                            .when_some(tray_own, |d, fill| {
                                d.bg(fill)
                                    .rounded(px(crate::reference_bar::BORDER_ROUNDING))
                            })
                            .children(tray_retract_inert),
                    )
                })
            };
            // Now Playing renders from ProductRuntime, so every output paints
            // the same product state. The dispatch sink is installed on the
            // owning output only, which is what makes a transport action
            // dispatch exactly once no matter how many outputs are showing it.
            let now_playing = self.now_playing_surface(cx, &layout, design, my_output.as_deref());
            let audio_osd = self.audio_osd_surface(&layout, design, my_output.as_deref(), cx);
            let shared_content = div()
                .id("shared-dock-envelope")
                .role(gpui::Role::Group)
                .aria_label("Dock et AppStack")
                .absolute();
            let shared_content = if let Some(shared) = layout.shared_dock {
                let outer = shared.geometry.outer;
                shared_content
                    .left(px(outer.left()))
                    .top(px(outer.top()))
                    .w(px(outer.width()))
                    .h(px(outer.height()))
                    .rounded(px(outer.radius()))
                    .when(!deck_unclipped, |content| content.overflow_hidden())
            } else {
                shared_content.inset_0()
            };
            let shared_content = shared_content
                .child(
                    dock.id("dock-inner")
                        .role(gpui::Role::Group)
                        .aria_label("Dock"),
                )
                .when_some(appstack, |outer, inner| outer.child(inner));
            #[cfg(feature = "component-lab")]
            let workspace_attached_region = layout.workspace;
            #[cfg(feature = "component-lab")]
            let popout_layer = if self.review_native_rail && bar_visible {
                self.bar_popout
                    .frame()
                    .zip(surface_material("popouts"))
                    .map(|(frame, material)| {
                        // FOCUS-01B: every content on the surface (two
                        // while one hands off to the next).
                        let parts = self
                            .bar_popout
                            .content_parts()
                            .into_iter()
                            .filter_map(|(popout, offset, size)| match popout {
                                crate::bar_pointer_routing::BarPopout::Status(kind) => {
                                    Some(crate::bar_popout_view::PopoutPart {
                                        size,
                                        offset,
                                        rows: if kind
                                            == crate::bar_pointer_routing::StatusEntryKind::Audio
                                        {
                                            self.bar_popout.audio_rows_any()
                                        } else {
                                            Vec::new()
                                        },
                                        info: self.bar_popout.info_of(kind),
                                    })
                                }
                                _ => None,
                            })
                            .collect();
                        // FOCUS-01D (layer): above another surface of the shell
                        // the compositor's blur is not between them, so it
                        // stands opaque.
                        let own = self.layered(crate::attention::Attendant::Dashboard);
                        crate::bar_popout_view::popout_layer(
                            (viewport.width, viewport.height),
                            frame,
                            parts,
                            self.bar_popout.preview(),
                            if own {
                                gpui::Rgba { a: 1.0, ..material }
                            } else {
                                material
                            },
                            own,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let popout_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                self.sync_dashboard(cx);
                // LIVE-02: outside the render. Updating the desktop's model
                // here made it a dependency of this window, so every
                // visualiser frame (30 a second while sound plays) redrew
                // the whole scene.
                cx.defer_in(window, |this, _, cx| this.sync_desktop(cx));
            }
            #[cfg(feature = "component-lab")]
            let dashboard_layer = if self.review_native_rail && bar_visible {
                // FOCUS-01B: the dashboard's content wherever it stands on
                // the surface (passing along during a handoff).
                let part = self
                    .bar_popout
                    .content_parts()
                    .into_iter()
                    .find(|(p, _, _)| *p == crate::bar_pointer_routing::BarPopout::Dashboard);
                self.bar_popout
                    .frame()
                    .zip(part)
                    .map(|(frame, (_, offset, size))| {
                        crate::dashboard_view::dashboard_layer(
                            frame,
                            self.dashboard.rows(),
                            size,
                            offset,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let dashboard_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                let side = self.rail_side();
                self.sync_utilities(side, (viewport.width, viewport.height), cx);
            }
            #[cfg(feature = "component-lab")]
            let utilities_layer = if self.review_native_rail && bar_visible {
                self.utilities
                    .frame()
                    .zip(surface_material("utilities"))
                    .map(|(frame, material)| {
                        crate::utilities_view::utilities_layer(
                            (viewport.width, viewport.height),
                            frame,
                            self.utilities.rows(),
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let utilities_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                let side = self.rail_side();
                self.sync_osd(side, (viewport.width, viewport.height));
                self.watch_backlights(cx);
            }
            #[cfg(feature = "component-lab")]
            let osd_layer = if self.review_native_rail && bar_visible {
                self.bar_osd
                    .frame()
                    .zip(surface_material("osd"))
                    .zip(self.bar_osd.reading())
                    .map(|((frame, material), reading)| {
                        crate::bar_osd_view::bar_osd_layer(
                            (viewport.width, viewport.height),
                            frame,
                            reading,
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let osd_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            if self.review_native_rail {
                self.start_session(cx);
                let side = self.rail_side();
                self.sync_session(side, (viewport.width, viewport.height), cx);
                self.poll_session(Instant::now(), cx);
                self.sync_workspaces(side, (viewport.width, viewport.height), cx);
                self.sync_thumbnails(window.scale_factor(), cx);
                self.sync_overflow(side, (viewport.width, viewport.height), cx);
            }
            #[cfg(feature = "component-lab")]
            let overflow_layer = if self.review_native_rail && bar_visible {
                self.overflow
                    .frame()
                    .zip(surface_material("popouts"))
                    .map(|(frame, material)| {
                        crate::overflow_view::overflow_layer(
                            (viewport.width, viewport.height),
                            frame,
                            self.overflow.rows(),
                            self.overflow.hovered(),
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let overflow_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            let workspaces_layer = if self.review_native_rail && bar_visible {
                self.workspaces
                    .frame()
                    .zip(surface_material("workspaces"))
                    .map(|(frame, material)| {
                        crate::workspace_view::workspace_layer(
                            (viewport.width, viewport.height),
                            frame,
                            self.workspaces.rows(),
                            crate::workspace_view::WorkspaceLive {
                                hovered: self.workspaces.hovered(),
                                carried: self.workspaces.carried().cloned(),
                                thumbs: self.workspace_thumbnails(cx),
                            },
                            if self.layered(crate::attention::Attendant::Workspaces) {
                                gpui::Rgba { a: 1.0, ..material }
                            } else {
                                material
                            },
                            self.layered(crate::attention::Attendant::Workspaces),
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let workspaces_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            let session_layer = if self.review_native_rail && bar_visible {
                self.session
                    .frame()
                    .zip(surface_material("session"))
                    .map(|(frame, material)| {
                        crate::session_view::session_layer(
                            (viewport.width, viewport.height),
                            frame,
                            self.session.rows(),
                            self.session.hint(),
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let session_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            let toast_layer = if self.review_native_rail {
                self.notification_toasts
                    .frame()
                    .zip(surface_material("toasts"))
                    .map(|(frame, material)| {
                        crate::notification_view::toast_layer(
                            (viewport.width, viewport.height),
                            frame,
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let toast_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            let centre_layer = if self.review_native_rail {
                self.notification_centre
                    .frame()
                    .zip(surface_material("centre"))
                    .map(|(frame, material)| {
                        crate::notification_centre_view::centre_layer(
                            (viewport.width, viewport.height),
                            frame,
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let centre_layer: Option<gpui::Div> = None;
            // DECK-01C: the AppStack's tiles, inside the extruded envelope.
            #[cfg(feature = "component-lab")]
            let stack_layer = if self.review_native_rail {
                self.stack.frame().map(crate::deck::stack_view::stack_layer)
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let stack_layer: Option<gpui::Div> = None;
            // DECK-01D: the app a drag carries, above everything it crosses.
            #[cfg(feature = "component-lab")]
            let ghost_layer = if self.review_native_rail {
                self.deck_ghost_layer()
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let ghost_layer: Option<gpui::Div> = None;
            // BAR-02B: the ring on the module the keyboard is on.
            #[cfg(feature = "component-lab")]
            let bar_keys_layer = if self.review_native_rail {
                self.bar_keys_apply(window, cx);
                // FOCUS-01: the panels' turns, with the window.
                self.attention_apply(window, cx);
                self.bar_keys_layer()
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let bar_keys_layer: Option<gpui::Div> = None;
            #[cfg(feature = "component-lab")]
            let settings_layer = if self.review_native_rail {
                self.settings
                    .frame()
                    .zip(surface_material("settings"))
                    .map(|(frame, material)| {
                        crate::settings_view::settings_layer(
                            (viewport.width, viewport.height),
                            frame,
                            material,
                        )
                    })
            } else {
                None
            };
            #[cfg(not(feature = "component-lab"))]
            let settings_layer: Option<gpui::Stateful<gpui::Div>> = None;
            // BAR-01C: the drawers and Settings, where the new side moves
            // them, slide into their edge and out of the new one.
            #[cfg(feature = "component-lab")]
            let (toast_layer, centre_layer, settings_layer) = {
                let offsets = relocation.as_ref().map(|r| {
                    bar_travel::RelocationOffsets::of(r, (viewport.width, viewport.height))
                });
                let o = offsets.unwrap_or_default();
                (
                    toast_layer.map(|l| bar_travel::shifted(l, o.toasts_dx, 0.0)),
                    centre_layer.map(|l| bar_travel::shifted(l, o.centre_dx, 0.0)),
                    settings_layer.map(|l| bar_travel::shifted(l, 0.0, o.settings_dy)),
                )
            };
            #[cfg(not(feature = "component-lab"))]
            let (toast_layer, centre_layer, settings_layer) = (
                toast_layer,
                centre_layer,
                settings_layer.map(gpui::IntoElement::into_any_element),
            );
            // FOCUS-01D (layer): a surface standing above a deliberate one is drawn
            // after it.
            #[cfg(feature = "component-lab")]
            let (popouts_up, workspaces_up) = (
                self.layered(crate::attention::Attendant::Dashboard),
                self.layered(crate::attention::Attendant::Workspaces),
            );
            #[cfg(not(feature = "component-lab"))]
            let (popouts_up, workspaces_up) = (false, false);
            let split = |layer, up: bool| if up { (None, layer) } else { (layer, None) };
            let (popout_below, popout_above) = split(popout_layer, popouts_up);
            let (dashboard_below, dashboard_above) = split(dashboard_layer, popouts_up);
            let (workspaces_below, workspaces_above) = split(workspaces_layer, workspaces_up);
            // CURSOR-01: the one shape for this frame, under the pointer.
            #[cfg(feature = "component-lab")]
            let cursor_style = if self.review_native_rail {
                self.cursor_style()
            } else {
                gpui::CursorStyle::Arrow
            };
            #[cfg(not(feature = "component-lab"))]
            let cursor_style = gpui::CursorStyle::Arrow;
            // DICTATION-01: the microphone, painted by the surface that
            // stands above the windows. Its place is the desktop module's,
            // so the drag the desktop layer owns moves the element the scene
            // paints.
            #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
            let dictation_el = {
                let view = crate::product_indicator::registry()
                    .active_publisher()
                    .map(|snapshot| crate::product_indicator::desktop_view_from_snapshot(&snapshot))
                    .unwrap_or_default();
                let [top, right, bottom, left] = self.desktop_state.inset;
                let free = (
                    (viewport.width - left - right).max(0.0),
                    (viewport.height - top - bottom).max(0.0),
                );
                let place = self
                    .desktop_state
                    .model
                    .as_ref()
                    .map(|model| {
                        model
                            .read(cx)
                            .modules
                            .place(crate::desktop_modules::ModuleKind::Dictation)
                    })
                    .unwrap_or_else(|| {
                        crate::desktop_modules::default_place(
                            crate::desktop_modules::ModuleKind::Dictation,
                            free,
                        )
                    });
                // DICTATION-ANCHOR: the element hangs from an anchor. While it
                // is carried, the pointer is the truth; the anchor is what the
                // drop writes.
                let anchor = crate::scene_runtime::desktop::profile_dictation_anchor(
                    self.product_runtime
                        .preferences()
                        .bar_overrides
                        .desktop_modules,
                );
                let carried = self.desktop_state.model.as_ref().is_some_and(|model| {
                    model.read(cx).modules.dragging()
                        == Some(crate::desktop_modules::ModuleKind::Dictation)
                });
                let max_surface_height = viewport.height * 0.40;
                // DICTATION-01: the motion, in the shell's vocabulary. The
                // extrusion opens with the recording, the surface's height
                // follows the lines that arrive, and the typewriter's reveal
                // follows the characters.
                let text = format!("{}{}", view.text, view.tentative);
                let mut measure = crate::notification_view::WindowTextMeasure(window);
                // The height follows the text *already written*: the surface
                // grows to make room for a new line, and nothing is ever cut.
                let written: String = text
                    .chars()
                    .take(self.dictation_motion.reveal.value.max(0.0) as usize)
                    .collect();
                let written_lines = <crate::notification_view::WindowTextMeasure<'_> as crate::notification_toasts::TextMeasure>::lines(
                    &mut measure,
                    &written,
                    14.0,
                    crate::desktop_layer::DICTATION_SURFACE_WIDTH - 24.0,
                );
                let target_height = (crate::desktop_layer::DICTATION_HEADER_HEIGHT
                    + crate::desktop_layer::DICTATION_SURFACE_CHROME
                    + written_lines as f32 * 18.0)
                    .min(max_surface_height);
                let mode = if self.effective_reduced_motion() {
                    gnos_ux_surface::grammar::TemporalResponseMode::Instant
                } else {
                    gnos_ux_surface::grammar::TemporalResponseMode::Full
                };
                let now = Instant::now();
                let dt = self
                    .dictation_previous
                    .replace(now)
                    .map_or(0.0, |previous| now.duration_since(previous).as_secs_f32());
                let mut motion = self.dictation_motion;
                motion.extrusion.set_open(view.active);
                let _ = motion.extrusion.step(dt, mode);
                // The calibration: the surface follows the writing at the
                // speed of the writing. A drawer that opens in one beat is not
                // a surface that grows as someone speaks.
                motion
                    .height
                    .step_at_tempo(target_height, 0.55, dt, mode, 0.5);
                motion
                    .reveal
                    .step_at_tempo(text.chars().count() as f32, 0.3, dt, mode, 0.5);
                let moving = motion.extrusion.needs_frame()
                    || !motion.height.is_settled(target_height, 0.5)
                    || !motion.reveal.is_settled(text.chars().count() as f32, 0.5);
                self.dictation_motion = motion;
                if moving {
                    self.start_motion_driver(cx);
                }
                // The typewriter: the characters appear as the model writes
                // them, and the surface grows with what has been written.
                let visible_text: String = text
                    .chars()
                    .take(motion.reveal.value.max(0.0) as usize)
                    .collect();
                // DICTATION-DOCK: the microphone's default anchor is its slot
                // in the Deck's strip. The profile's anchor is the detached
                // mode, for when the Dock is not where the element should be.
                let deck_slot = crate::deck::deck_layout(
                    &self.deck_cards,
                    layout.dock.left(),
                    layout.dock.top(),
                )
                .dictation;
                let element = if carried || anchor.is_none() {
                    if anchor.is_none() && !carried {
                        // The Deck's slot: the surface that already has the
                        // right material and the right extrusion.
                        let patch = (deck_slot.x, deck_slot.y, deck_slot.width, deck_slot.height);
                        crate::product_indicator::DictationLayout {
                            patch,
                            surface_anchor: (
                                patch.0,
                                patch.1 - crate::desktop_layer::DICTATION_SURFACE_GAP,
                                crate::desktop_layer::DICTATION_SURFACE_WIDTH,
                            ),
                        }
                    } else {
                        crate::product_indicator::dictation_layout(
                            place,
                            free,
                            [top, right, bottom, left],
                        )
                    }
                } else {
                    // The anchor resolves against what the shell leaves free:
                    // the element never lands under the Bar or the Deck.
                    let size = (
                        crate::desktop_layer::DICTATION_PATCH_SIZE,
                        crate::desktop_layer::DICTATION_PATCH_SIZE,
                    );
                    match anchor.as_ref().and_then(|anchor| {
                        anchor.resolve(
                            (viewport.width, viewport.height),
                            [top, right, bottom, left],
                            size,
                        )
                    }) {
                        Some(rect) => crate::product_indicator::DictationLayout {
                            patch: (rect.left(), rect.top(), size.0, size.1),
                            surface_anchor: (
                                rect.left(),
                                rect.top() - crate::desktop_layer::DICTATION_SURFACE_GAP,
                                crate::desktop_layer::DICTATION_SURFACE_WIDTH,
                            ),
                        },
                        None => crate::product_indicator::dictation_layout(
                            place,
                            free,
                            [top, right, bottom, left],
                        ),
                    }
                };
                let rect = element.patch;
                self.dictation_surface = None;
                let radius = gnos_ux_surface::grammar::ResponsiveRadiusToken::candidate_default()
                    .resolve(
                        gnos_ux_surface::grammar::SurfaceViewport::try_new(free.0, free.1)
                            .unwrap_or(gnos_ux_surface::grammar::SurfaceViewport::unit()),
                    )
                    .unwrap_or(12.0);
                if self.dictation_reported != Some(rect) {
                    self.dictation_reported = Some(rect);
                    println!(
                        "GNOS_UX_DICTATION rect=[{:.1},{:.1},{:.1},{:.1}] viewport={:.0}x{:.0} free={:.0}x{:.0} active={}",
                        rect.0,
                        rect.1,
                        rect.2,
                        rect.3,
                        viewport.width,
                        viewport.height,
                        free.0,
                        free.1,
                        view.active
                    );
                    println!(
                        "GNOS_UX_DICTATION_TEXT len={} tentative={}",
                        view.text.len(),
                        view.tentative.len()
                    );
                }
                Some(
                    crate::desktop_layer::dictation(
                        &view,
                        rect,
                        radius,
                        viewport.height,
                        self.dictation_motion,
                        visible_text,
                    )
                    .id("product-dictation")
                    .role(gpui::Role::Button)
                    .aria_label("Dictée")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            if this.dictation_hit(x, y) {
                                this.dictation_pointer(x, y, 0, cx);
                            }
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            if this.dictation_hit(x, y) {
                                this.dictation_pointer(x, y, 2, cx);
                            }
                        }),
                    )
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _window, _cx| {
                        // The element's wrapper fills the surface, so the click
                        // is only the button's when it lands on the patch.
                        let position = event.position();
                        let (x, y) = (f32::from(position.x), f32::from(position.y));
                        if !this.dictation_hit(x, y) {
                            return;
                        }
                        if let Some(target) = crate::product_indicator::registry().control_target()
                            && let Err(error) = crate::product_indicator::toggle_publisher(&target)
                        {
                            println!(
                                "GNOS_UX_DICTATION_CONTROL_FAILED target={target} reason={error}"
                            );
                        }
                    })),
                )
            };
            #[cfg(not(all(feature = "component-lab", feature = "indicator-production-client")))]
            let dictation_el: Option<gpui::AnyElement> = None;
            let root = div()
                .size_full()
                .cursor(cursor_style)
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, window, cx| {
                        // DICTATION-01: the element is dragged by the scene.
                        #[cfg(feature = "component-lab")]
                        this.dictation_pointer(
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            1,
                            cx,
                        );
                        let pointer =
                            Point2::new(f32::from(event.position.x), f32::from(event.position.y));
                        #[cfg(not(feature = "component-lab"))]
                        let _ = &window;
                        let zone = pointer_zone(
                            viewport,
                            bar_pointer_placement,
                            bar_pointer_state,
                            pointer,
                        );
                        let next_pointer = (zone == BarPointerZone::Surface).then_some(pointer);
                        #[cfg(feature = "component-lab")]
                        if this.review_native_rail {
                            this.bar_module_moved(pointer.x(), pointer.y(), cx);
                            this.deck_pointer(Some((pointer.x(), pointer.y())), cx);
                            if this.settings.is_open() {
                                // SET-04: the control under the pointer shows it.
                                if this.settings.pointer_moved(pointer.x(), pointer.y()) {
                                    this.report_settings("hover");
                                    cx.notify();
                                }
                                // FOCUS-01D: one at a time, Settings has the
                                // pointer; making room or layering, the rest
                                // of the shell hears it off the panel.
                                let shared = this.attention.meeting()
                                    != gnos_ux_surface::grammar::attention::Meeting::OneAtATime;
                                if !shared || this.settings.contains(pointer.x(), pointer.y()) {
                                    return;
                                }
                            }
                            let outcome = this.stack.pointer_moved(pointer.x(), pointer.y());
                            if this.apply_stack_outcome(outcome, window, cx) {
                                return;
                            }
                            let outcome =
                                this.notification_centre.pointer_moved(pointer.x(), pointer.y());
                            let changed = outcome.changed;
                            if this.apply_toast_outcome(outcome, cx) {
                                if changed {
                                    this.report_centre("pointer");
                                }
                                return;
                            }
                            // SURF-01C: hovering the OSD holds it.
                            if this.bar_osd.pointer_moved(pointer.x(), pointer.y(), Instant::now())
                            {
                                this.report_osd("hover");
                                this.start_motion_driver(cx);
                            }
                            if this.overflow.is_open() {
                                let outcome = this.overflow.pointer_moved(pointer.x(), pointer.y());
                                this.apply_overflow_outcome(outcome, cx);
                            }
                            if this.session.is_open() {
                                // SURF-02B: likewise for the session panel.
                                let outcome =
                                    this.session.pointer_moved(pointer.x(), pointer.y());
                                this.apply_session_outcome(outcome, cx);
                            } else if this.utilities.is_open() {
                                // SURF-01B: the hover popouts wait while
                                // Utilities is out.
                                let outcome =
                                    this.utilities.pointer_moved(pointer.x(), pointer.y());
                                this.apply_utilities_outcome(outcome, cx);
                            } else if !this.bar_travelling {
                                let outcome =
                                    this.bar_popout.pointer_moved(pointer.x(), pointer.y());
                                this.apply_bar_popout_outcome(outcome, cx);
                            } else {
                                this.bar_travel_pointer_moved(pointer.x(), pointer.y());
                            }
                            let outcome =
                                this.notification_toasts.pointer_moved(pointer.x(), pointer.y());
                            this.apply_toast_outcome(outcome, cx);
                        }
                        #[cfg(feature = "component-lab")]
                        let hover_panel_changed = if this.review_native_rail {
                            // WS-02: the workspace manager replaces the old
                            // summary envelope on the native rail.
                            let on_module = reference_workspace_hover.is_some_and(|bounds| {
                                bounds.contains(pointer.x(), pointer.y())
                            });
                            let on_panel = this.workspaces.contains(pointer.x(), pointer.y());
                            // A module being dragged along the Bar (BAR-01I)
                            // keeps it in: it would follow, closing and
                            // reopening as the module moves. Utilities and
                            // Session out, the hover surfaces wait.
                            let quiet = this.utilities.is_open()
                                || this.session.is_open()
                                || this.bar_travelling
                                || this.bar_drag.is_some();
                            if this.bar_drag.is_some() {
                                this.attention_close(&[crate::attention::Attendant::Workspaces], cx);
                            }
                            // FOCUS-01: the shell's attention hears every
                            // move (the module, the panel and the gap
                            // between are one zone there).
                            this.attention_pointer(
                                Some((pointer.x(), pointer.y())),
                                on_module,
                                quiet,
                                cx,
                            );
                            // CURSOR-01: the shape follows the pointer; a
                            // new one is only applied by a frame.
                            let before = this.cursor_reported;
                            this.cursor_pointer(Some((pointer.x(), pointer.y())));
                            if this.cursor_style() != before.unwrap_or_default() {
                                cx.notify();
                            }
                            if on_panel {
                                let outcome =
                                    this.workspaces.pointer_moved(pointer.x(), pointer.y());
                                this.apply_workspaces_outcome(outcome, cx);
                            }
                            let source_hit = false;
                            let connected_hit = this.reference_workspace_hover_owned
                                && reference_workspace_hover.is_some_and(|bounds| {
                                    bounds.contains_attached(
                                        pointer.x(),
                                        pointer.y(),
                                        workspace_attached_region,
                                    )
                                });
                            let (next, owned) =
                                crate::product_reference_consumer::next_reference_workspace_hover(
                                    this.product_panel,
                                    this.reference_workspace_hover_owned,
                                    source_hit,
                                    connected_hit,
                                    this.product_runtime.semantic().can_open_workspace(),
                                );
                            let changed = next != this.product_panel;
                            if changed {
                                println!(
                                    "GNOS_UX_EC04_REFERENCE_HOVER output={} state={} source={} connected={} pointer={:.1},{:.1} reference_bounds={:?} attached={:?}",
                                    this.runtime_output.as_ref().map_or("UNKNOWN", RuntimeId::as_str),
                                    next.as_str(),
                                    source_hit,
                                    connected_hit,
                                    pointer.x(), pointer.y(),
                                    reference_workspace_hover,
                                    workspace_attached_region,
                                );
                            }
                            this.product_panel = next;
                            this.reference_workspace_hover_owned = owned;
                            changed
                        } else {
                            false
                        };
                        let next_reveal = next_state(
                            BarInteractionPolicy::from_preferences(
                                this.product_runtime.preferences(),
                            ),
                            this.bar_reveal_state,
                            zone,
                            this.product_panel.is_open(),
                        );
                        let reveal_changed =
                            this.bar_pointer_zone != zone || this.bar_reveal_state != next_reveal;
                        this.bar_pointer_zone = zone;
                        this.bar_reveal_state = next_reveal;
                        let changed = match (this.product_pointer, next_pointer) {
                            (Some(previous), Some(next)) => {
                                (previous.x() - next.x()).abs() + (previous.y() - next.y()).abs()
                                    >= 1.0
                            }
                            (None, None) => false,
                            _ => true,
                        };
                        if changed || reveal_changed
                            || {
                                #[cfg(feature = "component-lab")]
                                { hover_panel_changed }
                                #[cfg(not(feature = "component-lab"))]
                                { false }
                            }
                        {
                            this.product_pointer = next_pointer;
                            if let Some(perf) = &this.perf_counters {
                                perf.pointer_changes.fetch_add(1, Ordering::Relaxed);
                                perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
                            }
                            this.start_motion_driver(cx);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_exit(cx.listener(|this, _, _, cx| {
                    #[cfg(feature = "component-lab")]
                    if this.review_native_rail {
                        this.deck_pointer(None, cx);
                        let outcome = this.bar_popout.pointer_left();
                        this.apply_bar_popout_outcome(outcome, cx);
                        // FOCUS-01: the pointer left for a window.
                        this.attention_pointer(None, false, false, cx);
                        this.cursor_pointer(None);
                        let outcome = this.notification_toasts.pointer_left();
                        this.apply_toast_outcome(outcome, cx);
                        if this.bar_osd.pointer_left(Instant::now()) {
                            this.report_osd("hover");
                            this.start_motion_driver(cx);
                        }
                    }
                    #[cfg(feature = "component-lab")]
                    let hover_panel_changed = if this.review_native_rail
                        && this.reference_workspace_hover_owned
                    {
                        this.reference_workspace_hover_owned = false;
                        this.product_panel = ProductPanelState::Closed;
                        println!("GNOS_UX_EC04_REFERENCE_HOVER state=Closed reason=pointer_exit");
                        true
                    } else {
                        false
                    };
                    let pointer_changed = this.product_pointer.take().is_some();
                    this.bar_pointer_zone = BarPointerZone::Outside;
                    let next_reveal = next_state(
                        BarInteractionPolicy::from_preferences(this.product_runtime.preferences()),
                        this.bar_reveal_state,
                        BarPointerZone::Outside,
                        this.product_panel.is_open(),
                    );
                    let reveal_changed = this.bar_reveal_state != next_reveal;
                    this.bar_reveal_state = next_reveal;
                    if pointer_changed || reveal_changed
                        || {
                            #[cfg(feature = "component-lab")]
                            { hover_panel_changed }
                            #[cfg(not(feature = "component-lab"))]
                            { false }
                        }
                    {
                        if let Some(perf) = &this.perf_counters {
                            perf.pointer_changes.fetch_add(1, Ordering::Relaxed);
                            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
                        }
                        this.start_motion_driver(cx);
                        cx.notify();
                    }
                }))
                .child(canvas)
                // BAR-01B: the rail's content rides the travelling arms.
                .when_some(bar_visible.then_some(bar), |root, bar| root.child(bar))
                .child(shared_content)
                .when_some(dictation_el, |root, element| root.child(element))
                // Transient popout above the Dock; below explicit menus/OSD.
                .when_some(popout_below, |root, layer| root.child(layer))
                .when_some(dashboard_below, |root, layer| root.child(layer))
                .when_some(utilities_layer, |root, layer| root.child(layer))
                .when_some(osd_layer, |root, layer| root.child(layer))
                .when_some(session_layer, |root, layer| root.child(layer))
                .when_some(workspaces_below, |root, layer| root.child(layer))
                .when_some(overflow_layer, |root, layer| root.child(layer))
                .when_some(toast_layer, |root, layer| root.child(layer))
                .when_some(centre_layer, |root, layer| root.child(layer))
                .when_some(stack_layer, |root, layer| root.child(layer))
                .when_some(ghost_layer, |root, layer| root.child(layer))
                .when_some(bar_keys_layer, |root, layer| root.child(layer))
                .when_some(settings_layer, |root, layer| root.child(layer))
                // FOCUS-01D (layer): the surfaces standing above another.
                .when_some(popout_above, |root, layer| root.child(layer))
                .when_some(dashboard_above, |root, layer| root.child(layer))
                .when_some(workspaces_above, |root, layer| root.child(layer))
                .when_some(tray_menu_surface, |root, menu| root.child(menu))
                .when_some(now_playing, |root, surface| root.child(surface))
                .when_some(audio_osd, |root, surface| root.child(surface))
                .when_some(workspace, |root, workspace| root.child(workspace))
                .when_some(product_diagnostics, |root, diagnostics| {
                    root.child(diagnostics)
                });
            #[cfg(feature = "component-lab")]
            let root = if self.review_native_rail {
                // The scene root holds the Settings focus, so keys keep
                // reaching the scene after any surface closes.
                root.track_focus(&self.settings_focus)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            // DICTATION-01: the element is a desktop module the
                            // scene drags.
                            this.dictation_pointer(x, y, 0, cx);
                            // BAR-02B: a press gives the keyboard back.
                            this.bar_keys_pointer(cx);
                            let outcome = this.settings.pointer_down(x, y);
                            let held = outcome.consumed && !outcome.closed;
                            if this.apply_settings_outcome(outcome, window, cx) {
                                if held {
                                    this.report_settings("pressed");
                                }
                                cx.stop_propagation();
                                return;
                            }
                            // SURF-01B: an open Utilities panel takes its
                            // presses; one outside closes it (the status
                            // icons toggle it on release instead).
                            // SURF-03B: a press outside a pinned dashboard
                            // closes it (the clock toggles it on release).
                            // The press that closes a panel is that panel's:
                            // nothing beneath acts on it.
                            // FOCUS-01: whatever the shell's attention has out
                            // that the press is not on goes back in (a
                            // surface's own source toggles it on release).
                            let held_dashboard = this
                                .attention
                                .is_held(crate::attention::Attendant::Dashboard);
                            this.attention_press(x, y, cx);
                            if held_dashboard
                                && !this
                                    .attention
                                    .is_held(crate::attention::Attendant::Dashboard)
                            {
                                this.swallow_click = true;
                                cx.stop_propagation();
                                return;
                            }
                            // SURF-02B: likewise for the session panel
                            // (the power button toggles it on release).
                            // BAR-02A: a press on the overflow panel (the "…"
                            // toggles it on release).
                            if this.overflow.is_open() && !this.on_overflow_button(x, y) {
                                let outcome = this.overflow.pointer_down(x, y);
                                if this.apply_overflow_outcome(outcome, cx) {
                                    cx.stop_propagation();
                                    return;
                                }
                                if !this.overflow.is_open() {
                                    this.swallow_click = true;
                                    cx.stop_propagation();
                                    return;
                                }
                            }
                            // WS-02: a press on the workspace manager.
                            if this.workspaces.is_open() && this.workspaces.contains(x, y) {
                                let outcome = this.workspaces.pointer_down(x, y);
                                if this.apply_workspaces_outcome(outcome, cx) {
                                    cx.stop_propagation();
                                    return;
                                }
                            }
                            if this.session.is_open() && !this.on_power_button(x, y) {
                                let outcome = this.session.pointer_down(x, y, Instant::now());
                                if this.apply_session_outcome(outcome, cx) {
                                    cx.stop_propagation();
                                    return;
                                }
                                if !this.session.is_open() {
                                    this.swallow_click = true;
                                    cx.stop_propagation();
                                    return;
                                }
                            }
                            if this.utilities.is_open() && !this.on_status_icons(x, y) {
                                let outcome = this.utilities.pointer_down(x, y);
                                if this.apply_utilities_outcome(outcome, cx) {
                                    cx.stop_propagation();
                                    return;
                                }
                                if !this.utilities.is_open() {
                                    this.swallow_click = true;
                                    cx.stop_propagation();
                                    return;
                                }
                            }
                            // BAR-01I: a press on a Bar module may lift it
                            // (a long press); the press still reaches the
                            // Bar's other handlers.
                            this.bar_module_down(x, y, cx);
                            let outcome = this.stack.pointer_down(x, y);
                            if this.apply_stack_outcome(outcome, window, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            // DECK-01D: an appcard press (a click or a drag)
                            // and the action sheet's rows.
                            if this.deck_pointer_down(x, y, cx) {
                                cx.notify();
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_centre.pointer_down(
                                x,
                                y,
                                crate::notification_centre::PointerButton::Primary,
                            );
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_toasts.pointer_down(x, y, false);
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.bar_popout.pointer_down(x, y);
                            if this.apply_bar_popout_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Middle,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            let outcome = this.notification_centre.pointer_down(
                                x,
                                y,
                                crate::notification_centre::PointerButton::Middle,
                            );
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_toasts.pointer_down(x, y, true);
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Middle,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            let outcome = this.notification_centre.pointer_up(
                                x,
                                y,
                                crate::notification_centre::PointerButton::Middle,
                            );
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_toasts.pointer_up(x, y, true);
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            // DICTATION-ANCHOR: a secondary click on the
                            // microphone pins it to the Deck, or detaches it
                            // onto the desktop.
                            if this.dictation_secondary(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            ) {
                                cx.stop_propagation();
                                return;
                            }
                            // DECK-01D: a secondary click opens a card's sheet;
                            // the scene root takes the keys, so Escape closes it.
                            if this.deck_secondary(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            ) {
                                window.focus(&this.settings_focus, cx);
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_centre.pointer_down(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                crate::notification_centre::PointerButton::Secondary,
                            );
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Right,
                        cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                            // RAIL-01: a secondary click on the rail (a tray
                            // item's menu).
                            #[cfg(feature = "component-lab")]
                            if this.review_native_rail
                                && !this.settings.is_open()
                                && !this.bar_travelling
                                && this
                                    .bar_module_at(
                                        f32::from(event.position.x),
                                        f32::from(event.position.y),
                                    )
                                    .is_some()
                                && this.rail_click(
                                    f32::from(event.position.x),
                                    f32::from(event.position.y),
                                    true,
                                    window,
                                    cx,
                                )
                            {
                                cx.stop_propagation();
                                return;
                            }
                            let _ = &window;
                            let outcome = this.notification_centre.pointer_up(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                crate::notification_centre::PointerButton::Secondary,
                            );
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            // DICTATION-01: the release is the drop.
                            this.dictation_pointer(x, y, 2, cx);
                            // A carried module's release is the drop and nothing
                            // else, wherever it lands (on its own place, over a
                            // panel or a popout): never a click on them.
                            #[cfg(feature = "component-lab")]
                            if this.review_native_rail
                                && this.bar_drag.is_some()
                                && this.bar_module_up(window, cx)
                            {
                                this.swallow_click = false;
                                cx.stop_propagation();
                                return;
                            }
                            // The release of a press that closed a panel.
                            if std::mem::take(&mut this.swallow_click) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.settings.pointer_up(x, y);
                            if this.apply_settings_outcome(outcome, window, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.session.pointer_up(x, y);
                            if this.apply_session_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.overflow.pointer_up(x, y);
                            if this.apply_overflow_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.workspaces.pointer_up(x, y);
                            if this.apply_workspaces_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.utilities.pointer_up(x, y);
                            if this.apply_utilities_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            if this.bar_module_up(window, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.stack.pointer_up(x, y);
                            if this.apply_stack_outcome(outcome, window, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            if this.deck_pointer_up(x, y, window, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_centre.pointer_up(
                                x,
                                y,
                                crate::notification_centre::PointerButton::Primary,
                            );
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_toasts.pointer_up(x, y, false);
                            if this.apply_toast_outcome(outcome, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.bar_popout.pointer_up(x, y);
                            let open_settings = outcome.open_settings;
                            if this.apply_bar_popout_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                            // UXE-05B: a popout's "Open settings" opens that page.
                            if let Some(kind) = open_settings {
                                use crate::bar_pointer_routing::StatusEntryKind as K;
                                use crate::settings_pages::PageId;
                                let page = match kind {
                                    K::Network => PageId::Network,
                                    K::Bluetooth => PageId::Bluetooth,
                                    K::BatteryOrProfile => PageId::Power,
                                    K::Audio | K::Lock => PageId::Audio,
                                };
                                println!(
                                    "GNOS_UX_UXE05_OPEN_FROM popout={kind:?} page={}",
                                    page.slug()
                                );
                                // FOCUS-01B: once the popout is back in the Bar.
                                this.request_settings(page, cx);
                            }
                        }),
                    )
                    .on_scroll_wheel(cx.listener(
                        |this, event: &gpui::ScrollWheelEvent, window, cx| {
                            // GPUI reports wheel-up as positive; the host takes the
                            // Wayland axis sign (positive = downward).
                            let up = match event.delta {
                                gpui::ScrollDelta::Lines(delta) => delta.y,
                                gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y),
                            };
                            // The launcher scrolls in pixels; a GPUI line is 20 px, so a
                            // notch (three lines) moves about one row.
                            let pixels = match event.delta {
                                gpui::ScrollDelta::Lines(delta) => -delta.y * 20.0,
                                gpui::ScrollDelta::Pixels(delta) => -f32::from(delta.y),
                            };
                            let outcome = this.settings.wheel(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                pixels / 48.0,
                            );
                            let settings_changed = outcome.changed;
                            if this.apply_settings_outcome(outcome, window, cx) {
                                if settings_changed {
                                    this.report_settings("scrolled");
                                }
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.stack.wheel(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                pixels,
                            );
                            let changed = outcome.changed;
                            if this.apply_stack_outcome(outcome, window, cx) {
                                if changed {
                                    this.report_stack("scrolled");
                                }
                                cx.stop_propagation();
                                return;
                            }
                            let outcome = this.notification_centre.wheel(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                -up,
                            );
                            let changed = outcome.changed;
                            if this.apply_toast_outcome(outcome, cx) {
                                if changed {
                                    this.report_centre("scrolled");
                                }
                                cx.stop_propagation();
                                return;
                            }
                            // BAR-01G: over a travelling module, the wheel
                            // reaches that module where the router sees it.
                            let (wx, wy) = this.bar_travel_point(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                            );
                            let mut outcome = this.bar_popout.wheel(wx, wy, -up);
                            // RAIL-01: a workspace or brightness step.
                            #[cfg(feature = "component-lab")]
                            if let Some(intent) = outcome.bar_intent.take() {
                                this.rail_intent(intent, (wx, wy), window, cx);
                            }
                            if this.apply_bar_popout_outcome(outcome, cx) {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        let modifiers = &event.keystroke.modifiers;
                        #[cfg(feature = "component-lab")]
                        if modifiers.control && event.keystroke.key == "," {
                            this.attention_toggle(crate::attention::Attendant::Settings, cx);
                            cx.stop_propagation();
                            return;
                        }
                        #[cfg(not(feature = "component-lab"))]
                        if modifiers.control && event.keystroke.key == "," {
                            let outcome = this.settings.toggle();
                            this.apply_settings_outcome(outcome, window, cx);
                            cx.stop_propagation();
                            return;
                        }
                        // BAR-02B: the Bar driven from the keyboard.
                        #[cfg(feature = "component-lab")]
                        if this.bar_keys_key(event, cx) {
                            cx.stop_propagation();
                            return;
                        }
                        if this.settings.is_open() {
                            // SETTINGS-02C: a passphrase being typed.
                            #[cfg(feature = "component-lab")]
                            if this.settings_prompt_key(event, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            use crate::settings_panel::SettingsKey;
                            let key = match (event.keystroke.key.as_str(), modifiers.shift) {
                                _ if modifiers.control || modifiers.alt || modifiers.platform => {
                                    None
                                }
                                ("up", _) => Some(SettingsKey::Up),
                                ("down", _) => Some(SettingsKey::Down),
                                ("tab", false) => Some(SettingsKey::Tab),
                                ("tab", true) => Some(SettingsKey::BackTab),
                                ("left", _) => Some(SettingsKey::Left),
                                ("right", _) => Some(SettingsKey::Right),
                                ("enter" | "space", _) => Some(SettingsKey::Activate),
                                ("escape", _) => Some(SettingsKey::Escape),
                                _ => None,
                            };
                            if let Some(key) = key {
                                let outcome = this.settings.key(key);
                                this.apply_settings_outcome(outcome, window, cx);
                            }
                            cx.stop_propagation();
                            return;
                        }
                        if this.stack.is_open() {
                            use crate::deck::stack::StackKey;
                            let modifiers = &event.keystroke.modifiers;
                            let key = match (event.keystroke.key.as_str(), modifiers.shift) {
                                _ if modifiers.control || modifiers.alt || modifiers.platform => {
                                    None
                                }
                                ("up", _) => Some(StackKey::Up),
                                ("down", _) => Some(StackKey::Down),
                                ("left", _) => Some(StackKey::Left),
                                ("right", _) => Some(StackKey::Right),
                                ("tab", false) => Some(StackKey::Tab),
                                ("tab", true) => Some(StackKey::BackTab),
                                ("pageup", _) => Some(StackKey::PageUp),
                                ("pagedown", _) => Some(StackKey::PageDown),
                                ("enter", _) => Some(StackKey::Enter),
                                ("escape", _) => Some(StackKey::Escape),
                                _ => None,
                            };
                            if let Some(key) = key {
                                let outcome = this.stack.key(key);
                                if this.apply_stack_outcome(outcome, window, cx) {
                                    cx.stop_propagation();
                                    return;
                                }
                            }
                        }
                        if event.keystroke.key == "escape" && this.bar_drag_escape(cx) {
                            cx.stop_propagation();
                            return;
                        }
                        if event.keystroke.key == "escape" && this.deck_escape(cx) {
                            cx.stop_propagation();
                            return;
                        }
                        if event.keystroke.key == "escape" && this.overflow.escape() {
                            this.report_overflow("closing");
                            this.start_motion_driver(cx);
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        // WS-02B: a carried window is put back first.
                        if event.keystroke.key == "escape" && this.workspaces.escape_carry() {
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        if event.keystroke.key == "escape" && this.session.escape() {
                            this.report_session("closing");
                            this.start_motion_driver(cx);
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        if event.keystroke.key == "escape" && this.utilities.escape() {
                            this.report_utilities("closing");
                            this.start_motion_driver(cx);
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        if event.keystroke.key == "escape" {
                            // FOCUS-01: the most recent surface out.
                            this.attention_escape(cx);
                            let outcome = this.bar_popout.escape();
                            this.apply_bar_popout_outcome(outcome, cx);
                            let outcome = this.notification_toasts.escape();
                            this.apply_toast_outcome(outcome, cx);
                            let outcome = this.notification_centre.escape();
                            this.apply_toast_outcome(outcome, cx);
                        }
                    }))
            } else {
                root
            };
            // Dock and AppStack are siblings. Routing on Dock alone cannot
            // receive bubbled Tab events from a focused application row.
            #[cfg(feature = "r13-production-client")]
            let root = root.on_key_down(cx.listener(|this, event, window, cx| {
                this.report_key_event(event, window);
                this.handle_scene_key(event, window, cx);
            }));
            #[cfg(feature = "r12-production-client")]
            let root = root.on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                // D01: on the native rail Settings is the shell surface, never
                // a window.
                if this.review_native_rail {
                    return;
                }
                if event.keystroke.modifiers.control && event.keystroke.key == "," {
                    // A normal transient Settings window in the authenticated caller
                    // process; no profile store or new layer-shell authority.
                    if let Err(error) = cx.open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(Bounds {
                                origin: point(px(100.), px(100.)),
                                size: gpui::Size::new(px(640.), px(240.)),
                            })),
                            app_id: Some(SCENE_IDENTITY.to_owned()),
                            ..Default::default()
                        },
                        |window, cx| {
                            let view = cx
                                .new(crate::product_bar_settings::BarEdgeSettingsView::production);
                            view.update(cx, |view, cx| view.focus_first(window, cx));
                            view
                        },
                    ) {
                        eprintln!("GNOS_UX_SETTINGS_OPEN failed={error}");
                    }
                    cx.stop_propagation();
                }
            }));
            return root;
        }
        self.ensure_scene(viewport);
        let frame = self.frame.clone().expect("scene frame");
        self.maybe_start_projection_qualification(&frame);
        self.project_surface_reservation(&frame, window);
        #[cfg(feature = "component-lab")]
        self.sync_desktop_inset(cx);

        #[cfg_attr(not(feature = "component-lab"), allow(unused_mut))]
        let mut input_regions = frame
            .input_regions
            .iter()
            .filter(|region| region.width() > 0.5 && region.height() > 0.5)
            .map(|region| Bounds {
                origin: point(px(region.left()), px(region.top())),
                size: gpui::Size::new(px(region.width()), px(region.height())),
            })
            .collect::<Vec<_>>();
        // DICTATION-01: the element is a button. The scene has to take the
        // pointer over it, or the click goes to whatever is below.
        #[cfg(feature = "component-lab")]
        if let Some((x, y, w, h)) = self.dictation_reported {
            input_regions.push(Bounds {
                origin: point(px(x), px(y)),
                size: gpui::Size::new(px(w), px(h)),
            });
        }
        window.set_input_region(Some(&input_regions));

        let canvas = canvas(
            move |bounds, _, _| (bounds, frame.clone()),
            move |bounds, (_, frame), window, _| {
                paint_scene(bounds, &frame, window);
            },
        )
        .size_full();
        let projection_overlay = if self.semantic_consumer.is_enabled() {
            let projection = self.semantic_consumer.get(self.runtime_output.as_ref());
            Some(
                div()
                    .absolute()
                    .top(px(8.0))
                    .left(px(8.0))
                    .p(px(6.0))
                    .text_sm()
                    .text_color(white())
                    .bg(rgba(0x0b1f2ae6))
                    .child(desktop_consumer_context_text(projection.as_ref())),
            )
        } else {
            None
        };
        div()
            .size_full()
            .child(canvas)
            .when_some(projection_overlay, |root, overlay| root.child(overlay))
    }
}

// CANDIDATE_DEFAULT: product material values await perceptual review.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_product_scene_with_transition(
    bounds: Bounds<gpui::Pixels>,
    frame: &SceneFrame,
    connector_width: f32,
    reactive: Option<gnos_ux_surface::grammar::reactive_envelope::ReactiveEnvelopeFrame>,
    appstack: Option<&crate::product_shared_envelope::SharedDockEnvelope>,
    transition: Option<&gnos_ux_surface::grammar::ResolvedBarTransitionFrame>,
    bar_visible: bool,
    window: &mut Window,
) {
    let _ = connector_width;
    // Workspace retains its connective hull. AppStack C paints one rounded
    // shared envelope; both semantic inners paint content only. Historically
    // each derived body was painted as ONE connective hull, not as its own
    // rectangle beside a rectangle for its parent. Painting the AppStack as a
    // separate filled rect under a separate filled Dock would leave an internal
    // seam where the two meet, which is precisely what the Fused relation says
    // is not there; the hull is the one place the two bodies are a single
    // connected outline.
    if bar_visible && let Some(reactive) = reactive {
        window.paint_path(
            crate::product_hull::connective_hull_path(bounds, reactive.hull),
            rgba(0x304957ff),
        );
    }
    if let Some(appstack) = appstack {
        crate::product_hull::paint_shared_dock(bounds, appstack.geometry.outer, window);
    }
    if let Some(transition) = transition {
        for arm in &transition.arms {
            window.paint_path(rounded_rect_path(bounds, arm.bounds), rgba(0x183b4dff));
        }
    }
    for node in &frame.nodes {
        if !bar_visible && node.role == SceneNodeRole::Bar {
            continue;
        }
        if transition.is_some() && node.role == SceneNodeRole::Bar {
            continue;
        }
        let fill = match node.role {
            SceneNodeRole::Bar if reactive.is_some() => continue,
            SceneNodeRole::Bar => rgba(0x183b4dff),
            SceneNodeRole::Dock if appstack.is_some() => continue,
            SceneNodeRole::Dock => rgba(0x263844ff),
            SceneNodeRole::Body => rgba(0x304957ff),
            // Painted as a connective hull above, so painting its bounding
            // rectangle here would put a second, seamed copy of the same body
            // underneath it.
            SceneNodeRole::AppStack => continue,
            _ => continue,
        };
        if node.role == SceneNodeRole::Body && reactive.is_some() {
            continue;
        }
        let path = rounded_rect_path(bounds, node.envelope);
        window.paint_path(path, fill);
    }
}

fn sdk8_safety_resync_interval(args: &[String]) -> Duration {
    const DEFAULT_MS: u64 = 2_000;
    const MIN_MS: u64 = 50;
    const MAX_MS: u64 = 10_000;

    let value = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--sdk8-safety-resync-ms="))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MS)
        .clamp(MIN_MS, MAX_MS);
    Duration::from_millis(value)
}

fn discover_product_application_catalog(product_mode: bool) -> Arc<ApplicationCatalog> {
    if !product_mode {
        return Arc::new(ApplicationCatalog::empty());
    }

    let roots = match ApplicationCatalogRoots::from_environment() {
        Ok(roots) => roots,
        Err(error) => {
            eprintln!("GNOS_UX_APPLICATION_CATALOG status=unavailable reason={error:?}");
            return Arc::new(ApplicationCatalog::empty());
        }
    };
    let locale = LocalePreferences::from_environment();
    let desktop = DesktopEnvironment::from_environment();
    let catalog = ApplicationCatalog::discover(&roots, &locale, desktop);
    println!(
        "GNOS_UX_APPLICATION_CATALOG status=ready entries={} visible={} issues={} issue_counts={:?} desktop={:?} generation={}",
        catalog.entries().len(),
        catalog.visible_entries().count(),
        catalog.issues().len(),
        catalog.issue_counts(),
        catalog.desktop_environment().names(),
        catalog.generation().hex(),
    );
    Arc::new(catalog)
}

pub fn run_scene() -> i32 {
    let args = env::args().collect::<Vec<_>>();
    // I18N-01: the operator's language (stored choice, else the system's).
    gnos_i18n::init();
    println!("GNOS_UX_LANGUAGE language={}", gnos_i18n::language());

    if let Err(error) = crate::product_bar::qualification_edge(&args) {
        eprintln!("GNOS_UX_BAR_QUALIFICATION_INVALID {error}");
        return 2;
    }
    let native_rail = args.iter().any(|arg| arg == "--review-native-rail");
    if native_rail
        && (!cfg!(feature = "component-lab") || !args.iter().any(|arg| arg == "--product-scene"))
    {
        eprintln!(
            "GNOS_UX_REVIEW_NATIVE_RAIL_REJECTED reason=requires_component_lab_product_scene"
        );
        return 2;
    }

    #[cfg(feature = "r13-production-client")]
    if let Some(connector) = r13_application_focus_qualification_output(&args) {
        return run_r13_application_focus_qualification(&connector);
    }

    #[cfg(feature = "r13-production-client")]
    if let Some(connector) = r13_stale_qualification_output(&args) {
        return run_r13_stale_active_workspace_qualification(&connector);
    }

    #[cfg(feature = "r13-production-client")]
    if let Some(connector) = r13_qualification_output(&args) {
        return run_r13_active_workspace_qualification(&connector);
    }

    let topology_reconcile = args.iter().any(|arg| arg == "--sdk8-topology-reconcile");
    let event_stream_disabled = args.iter().any(|arg| arg == "--sdk8-disable-event-stream");
    let safety_resync_interval = sdk8_safety_resync_interval(&args);
    let multi_output = args.iter().any(|arg| arg == "--sdk7-multi-output");
    let product_mode = args.iter().any(|arg| arg == "--product-scene");
    // Bounded qualification affordance for the typed tray action path. It can
    // only name a tray item id: the bus address, the method, and the coordinate
    // hint are all resolved from live observed state, so the flag cannot become
    // a generic external-call interface.
    let tray_qualification = tray_qualification_request(&args);
    let application_catalog = discover_product_application_catalog(product_mode);
    if product_mode && !topology_reconcile {
        eprintln!("GNOS_UX_PRODUCT_SCENE_FAIL reason=topology_reconcile_required");
        return 2;
    }
    let semantic_consumer_enabled =
        product_mode || args.iter().any(|arg| arg == "--dsc01-semantic-consumer");

    let lifecycle = Arc::new(Mutex::new(RuntimeLifecycle::default()));
    let exit_code = Arc::new(AtomicU8::new(0));
    let mut shutdown_signals = if topology_reconcile {
        if let Err(error) = emit_initial_health_receipt(&lifecycle) {
            eprintln!("GNOS_UX_LIFECYCLE_FATAL reason={error}");
            return 1;
        }
        match ShutdownSignals::install() {
            Ok(signals) => Some(signals),
            Err(error) => {
                eprintln!("GNOS_UX_LIFECYCLE_FATAL reason={error}");
                return 1;
            }
        }
    } else {
        None
    };

    let lifecycle_for_app = Arc::clone(&lifecycle);
    let exit_code_for_app = Arc::clone(&exit_code);
    gpui_platform::application().run(move |cx: &mut App| {
        if topology_reconcile {
            cx.set_quit_mode(QuitMode::Explicit);
            start_dynamic_topology(
                cx,
                DynamicTopologyConfig {
                    event_stream_disabled,
                    semantic_consumer_enabled,
                    safety_resync_interval,
                    application_catalog: Arc::clone(&application_catalog),
                    tray_qualification: tray_qualification.clone(),
                },
                Arc::clone(&lifecycle_for_app),
                shutdown_signals
                    .take()
                    .expect("topology mode installs shutdown signals"),
                Arc::clone(&exit_code_for_app),
            );
            return;
        }

        if !multi_output {
            open_scene_window(
                cx,
                MonitorSceneId::new(1),
                None,
                (0, 0),
                None,
                None,
                SceneSemanticConsumer::default(),
                Arc::clone(&application_catalog),
                Some(ProductFocusScope::new()),
            )
            .expect("opening GNOS.UX monitor scene window");
            return;
        }

        let application_catalog = Arc::clone(&application_catalog);
        cx.spawn(async move |cx| {
            for attempt in 1..=100_u32 {
                let displays_ready = cx.update(|app| !app.displays().is_empty());
                if displays_ready {
                    let result = cx.update(|app| {
                        open_multi_output_scene_windows(
                            tray_qualification,
                            app,
                            Arc::clone(&application_catalog),
                        )
                    });
                    match result {
                        Ok(count) => {
                            println!(
                                "GNOS_UX_MULTI_OUTPUT_ROUTING_PASS attempts={} count={}",
                                attempt, count
                            );
                        }
                        Err(error) => {
                            eprintln!("GNOS_UX_MULTI_OUTPUT_BINDING_FAIL reason={error}");
                        }
                    }
                    return;
                }

                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }

            cx.update(|_| {
                eprintln!("GNOS_UX_MULTI_OUTPUT_BINDING_FAIL reason=gpui_displays_not_ready");
            });
        })
        .detach();
    });

    if topology_reconcile {
        let (phase, generation, route_count, event_stream) = match lifecycle.lock() {
            Ok(state) => (
                state.phase(),
                state.topology_generation(),
                state.route_count(),
                state.event_stream(),
            ),
            Err(_) => {
                eprintln!("GNOS_UX_LIFECYCLE_FATAL reason=lifecycle_lock_poisoned_after_run");
                return 1;
            }
        };

        if !matches!(
            phase,
            RuntimeLifecyclePhase::Stopping
                | RuntimeLifecyclePhase::Stopped
                | RuntimeLifecyclePhase::Failed
        ) {
            let _ = emit_health_receipt(
                &lifecycle,
                RuntimeLifecyclePhase::Failed,
                generation,
                route_count,
                event_stream,
                false,
                "event_loop_returned_without_shutdown",
            );
            exit_code.store(1, Ordering::Release);
        }

        let (generation, route_count) = match lifecycle.lock() {
            Ok(state) => (state.topology_generation(), state.route_count()),
            Err(_) => {
                eprintln!("GNOS_UX_LIFECYCLE_FATAL reason=lifecycle_lock_poisoned_before_stopped");
                return 1;
            }
        };
        if let Err(error) = emit_health_receipt(
            &lifecycle,
            RuntimeLifecyclePhase::Stopped,
            generation,
            route_count,
            EventStreamState::Unavailable,
            false,
            "event_loop_stopped",
        ) {
            eprintln!("GNOS_UX_LIFECYCLE_FATAL reason={error}");
            exit_code.store(1, Ordering::Release);
        }
    }

    i32::from(exit_code.load(Ordering::Acquire))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn shared_motion_driver_continues_for_unsettled_frames_then_stops() {
        let mut driver = ProductMotionDriver::default();
        assert!(driver.arm(false));
        assert!(
            driver.tick(false),
            "the first requested render is preserved"
        );
        assert!(driver.active);
        assert!(
            driver.tick(true),
            "an unsettled reactive surface keeps the driver active"
        );
        assert!(driver.tick(true));
        assert!(
            !driver.tick(false),
            "settled surfaces stop the shared driver"
        );
        assert!(!driver.active);
        assert!(
            !driver.arm(true),
            "reduced motion does not arm timer frames"
        );
    }

    #[test]
    fn product_scene_layer_shell_is_keyboard_interactive_on_demand() {
        // The product scene owns actionable controls, so the surface must be
        // able to take keyboard focus when asked. `None` made the Workspace
        // Manager pointer- and AT-SPI-only; `Exclusive` would make a
        // persistent bar/dock a keyboard monopolist.
        assert_eq!(
            PRODUCT_SCENE_KEYBOARD_INTERACTIVITY,
            KeyboardInteractivity::OnDemand
        );
    }

    #[test]
    fn product_scene_keyboard_mode_is_not_exclusive() {
        // Guards the specific decision recorded in R09-A11Y-01B: OnDemand was
        // selected, and Exclusive must never become a silent fallback because
        // OnDemand had a defect.
        assert_ne!(
            PRODUCT_SCENE_KEYBOARD_INTERACTIVITY,
            KeyboardInteractivity::Exclusive
        );
    }

    #[cfg(feature = "r13-production-client")]
    #[test]
    fn r13_qualification_mode_requires_one_explicit_output_connector() {
        let absent = vec!["gnos-ux-scene".to_owned(), "--product-scene".to_owned()];
        assert_eq!(r13_qualification_output_from(&absent, None), None);

        let present = vec![
            "gnos-ux-scene".to_owned(),
            "--r13-qualify-active-workspace=DP-2".to_owned(),
        ];
        assert_eq!(
            r13_qualification_output_from(&present, None),
            Some("DP-2".to_owned())
        );

        let from_environment = r13_qualification_output_from(&absent, Some("HDMI-A-1".to_owned()));
        assert_eq!(from_environment, Some("HDMI-A-1".to_owned()));

        let cli_wins = r13_qualification_output_from(&present, Some("HDMI-A-1".to_owned()));
        assert_eq!(cli_wins, Some("DP-2".to_owned()));

        let empty = vec![
            "gnos-ux-scene".to_owned(),
            "--r13-qualify-active-workspace=".to_owned(),
        ];
        assert_eq!(r13_qualification_output_from(&empty, None), None);
        assert_eq!(
            r13_qualification_output_from(&absent, Some("   ".to_owned())),
            None
        );

        let stale = vec![
            "gnos-ux-scene".to_owned(),
            "--r13-qualify-stale-active-workspace=HDMI-A-1".to_owned(),
        ];
        assert_eq!(
            r13_stale_qualification_output(&stale),
            Some("HDMI-A-1".to_owned())
        );
        let stale_empty = vec![
            "gnos-ux-scene".to_owned(),
            "--r13-qualify-stale-active-workspace=".to_owned(),
        ];
        assert_eq!(r13_stale_qualification_output(&stale_empty), None);

        let focus = vec![
            "gnos-ux-scene".to_owned(),
            "--r13-qualify-focus-application=DP-2".to_owned(),
        ];
        assert_eq!(
            r13_application_focus_qualification_output(&focus),
            Some("DP-2".to_owned())
        );
        let focus_empty = vec![
            "gnos-ux-scene".to_owned(),
            "--r13-qualify-focus-application=".to_owned(),
        ];
        assert_eq!(
            r13_application_focus_qualification_output(&focus_empty),
            None
        );
    }

    #[test]
    fn scene_host_namespace_is_single_and_canonical() {
        assert_eq!(SCENE_NAMESPACE, "gnos.ux:scene");
        assert_eq!(SCENE_IDENTITY, "gnos.ux.scene");
    }

    #[test]
    fn runtime_output_name_key_is_deterministic_and_output_specific() {
        assert_eq!(
            wayland_runtime_name_key("DP-2"),
            wayland_runtime_name_key("DP-2")
        );
        assert_ne!(
            wayland_runtime_name_key("DP-2"),
            wayland_runtime_name_key("HDMI-A-1")
        );
    }

    fn test_output(id: u8, connector: &str) -> gnosix_desktop_model::OutputState {
        gnosix_desktop_model::OutputState {
            output_hub_id: gnosix_desktop_model::OutputHubId::from_bytes([id; 16]),
            connector: gnosix_desktop_model::ConnectorName::new(connector).unwrap(),
            descriptors: gnosix_desktop_model::PhysicalOutputDescriptors::default(),
            physical_size: gnosix_desktop_model::PhysicalPixelSize::new(1920, 1080).unwrap(),
            logical_position: gnosix_desktop_model::LogicalPoint::ORIGIN,
            logical_size: gnosix_desktop_model::LogicalSize::new(1920, 1080).unwrap(),
            scale: gnosix_desktop_model::ScaleMilli::from_milli(1000).unwrap(),
            transform: gnosix_desktop_model::OutputTransform::Normal,
            active_workspace_id: None,
        }
    }

    fn test_projection(connector: &str, generation: u64) -> DesktopOutputProjection {
        DesktopOutputProjection {
            generation: gnosix_desktop_model::DesktopContextGeneration(generation),
            output: test_output(1, connector),
            workspaces: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn dsc01_pairing_requires_exact_connector_set() {
        let paired = pair_projections_by_connector(
            [
                ConnectorName::new("A").unwrap(),
                ConnectorName::new("B").unwrap(),
            ],
            &[test_projection("B", 7), test_projection("A", 7)],
            DesktopContextGeneration(7),
        )
        .expect("exact route/projection pairing");
        assert_eq!(
            paired.keys().map(ConnectorName::as_str).collect::<Vec<_>>(),
            ["A", "B"]
        );

        let missing = pair_projections_by_connector(
            [
                ConnectorName::new("A").unwrap(),
                ConnectorName::new("B").unwrap(),
            ],
            &[test_projection("A", 7)],
            DesktopContextGeneration(7),
        );
        assert!(missing.is_err());

        let duplicate = pair_projections_by_connector(
            [ConnectorName::new("A").unwrap()],
            &[test_projection("A", 7), test_projection("A", 7)],
            DesktopContextGeneration(7),
        );
        assert!(duplicate.is_err());

        let wrong_generation = pair_projections_by_connector(
            [ConnectorName::new("A").unwrap()],
            &[test_projection("A", 6)],
            DesktopContextGeneration(7),
        );
        assert!(wrong_generation.is_err());
    }

    #[test]
    fn dsc01_context_uses_projection_enums_and_filters_layers() {
        let output = test_output(3, "DP-3");
        let output_hub_id = output.output_hub_id;
        let workspace = gnosix_desktop_model::WorkspaceState {
            workspace_id: gnosix_desktop_model::WorkspaceId::from_bytes([4; 16]),
            name: gnosix_desktop_model::WorkspaceName::new("work").unwrap(),
            output_hub_id: Some(output_hub_id),
            visibility: WorkspaceVisibility::Visible,
            focus: WorkspaceFocus::Focused,
        };
        let workspace_id = workspace.workspace_id;
        let application = |id: u8, class: &str, visibility: SurfaceVisibility| {
            gnosix_desktop_model::SurfaceState {
                surface_id: gnosix_desktop_model::SurfaceId::from_bytes([id; 16]),
                role: DesktopSurfaceRole::Application,
                output_hub_id: Some(output_hub_id),
                workspace_id: Some(workspace_id),
                bounds: gnosix_desktop_model::LogicalRect::new(
                    gnosix_desktop_model::LogicalPoint::ORIGIN,
                    gnosix_desktop_model::LogicalSize::new(100, 100).unwrap(),
                ),
                application: Some(gnosix_desktop_model::ApplicationSurfaceDescriptors {
                    class: class.to_owned(),
                    initial_class: class.to_owned(),
                    title: class.to_owned(),
                    initial_title: class.to_owned(),
                    tags: Vec::new(),
                    process_id: u32::from(id),
                    xwayland: false,
                }),
                visibility,
                focus: if id == 5 {
                    SurfaceFocus::Focused
                } else {
                    SurfaceFocus::Unfocused
                },
            }
        };
        let layer = gnosix_desktop_model::SurfaceState {
            surface_id: gnosix_desktop_model::SurfaceId::from_bytes([9; 16]),
            role: DesktopSurfaceRole::Overlay,
            output_hub_id: Some(output_hub_id),
            workspace_id: None,
            bounds: gnosix_desktop_model::LogicalRect::new(
                gnosix_desktop_model::LogicalPoint::ORIGIN,
                gnosix_desktop_model::LogicalSize::new(100, 100).unwrap(),
            ),
            application: None,
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Unknown,
        };
        let projection = DesktopOutputProjection {
            generation: gnosix_desktop_model::DesktopContextGeneration(8),
            output,
            workspaces: vec![workspace],
            surfaces: vec![
                application(5, "term", SurfaceVisibility::Hidden),
                application(6, "files", SurfaceVisibility::Unmapped),
                layer,
            ],
        };

        let text = desktop_consumer_context_text(Some(&projection));
        assert!(text.contains("connector=DP-3"));
        assert!(text.contains("semantic_generation=8"));
        assert!(text.contains("focused_workspace=work"));
        assert!(text.contains("focused_application=term"));
        assert!(text.contains("applications=2"));
        assert!(text.contains("application_hidden=1"));
        assert!(text.contains("application_unmapped=1"));
        assert!(!text.contains("focused_application=overlay"));
    }

    #[test]
    fn dsc01_projection_state_clear_and_publish_are_replace_only() {
        let consumer = SceneSemanticConsumer::enabled();
        let initial_revision = consumer.revision();
        let mut projections = BTreeMap::new();
        projections.insert(ConnectorName::new("A").unwrap(), test_projection("A", 9));
        consumer.publish(projections).expect("publish projection");
        let published_revision = consumer.revision();
        assert!(published_revision > initial_revision);
        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_some()
        );
        consumer.clear().expect("clear projections");
        assert!(consumer.revision() > published_revision);
        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_none()
        );
    }

    #[test]
    fn semantic_publish_and_clear_wake_each_subscribed_route() {
        let consumer = SceneSemanticConsumer::enabled();
        let mut route_a = consumer.subscribe().expect("route A subscription");
        let mut route_b = consumer.subscribe().expect("route B subscription");
        let revision_before = consumer.revision();
        consumer
            .publish(BTreeMap::from([
                (ConnectorName::new("A").unwrap(), test_projection("A", 9)),
                (ConnectorName::new("B").unwrap(), test_projection("B", 9)),
            ]))
            .expect("publish both routes");
        let published = consumer.revision();
        assert!(published > revision_before);
        assert_eq!(route_a.try_recv().unwrap(), published);
        assert_eq!(route_b.try_recv().unwrap(), published);
        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_some()
        );
        assert!(
            consumer
                .get(Some(&RuntimeId::new("B".to_owned())))
                .is_some()
        );

        consumer.clear().expect("invalidate both routes");
        let cleared = consumer.revision();
        assert!(cleared > published);
        assert_eq!(route_a.try_recv().unwrap(), cleared);
        assert_eq!(route_b.try_recv().unwrap(), cleared);
        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_none()
        );
        assert!(
            consumer
                .get(Some(&RuntimeId::new("B".to_owned())))
                .is_none()
        );
    }

    #[test]
    fn product_motion_driver_runs_only_until_settled_and_rearms() {
        let mut driver = ProductMotionDriver::default();
        assert!(!driver.arm(true));
        assert!(!driver.active);
        assert!(driver.arm(false));
        assert!(!driver.arm(false));
        assert!(
            driver.tick(false),
            "first tick must ensure a frame after new intent"
        );
        assert!(driver.tick(true), "moving geometry continues");
        assert!(!driver.tick(false), "settled geometry stops scheduling");
        assert!(!driver.active);
        assert!(
            driver.arm(false),
            "new pointer or open intent restarts motion"
        );
        assert!(driver.tick(false));
        assert!(!driver.tick(false));
    }

    #[test]
    fn enabling_reduced_motion_stops_an_armed_driver_without_another_frame() {
        let mut driver = ProductMotionDriver::default();
        assert!(driver.arm(false));
        assert!(driver.tick(true));
        driver.stop();
        assert!(!driver.active);
        assert!(!driver.tick(true));
    }

    fn observed_output(name: &str) -> ObservedRuntimeOutput {
        ObservedRuntimeOutput {
            session_signature: "test-session".to_owned(),
            runtime_monitor_id: 1,
            name: name.to_owned(),
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
            refresh_millihz: 60_000,
            scale_milli: 1_000,
            transform: 0,
        }
    }

    fn observed_output_with_id(name: &str, runtime_monitor_id: i64) -> ObservedRuntimeOutput {
        ObservedRuntimeOutput {
            runtime_monitor_id,
            ..observed_output(name)
        }
    }

    #[test]
    fn event_stream_reconnect_after_gap_requires_authoritative_rebuild() {
        let mut continuity = TopologyEventContinuity::default();
        let mut disconnected = TopologyWakeBatch::default();
        disconnected.absorb(TopologyWakeHint::StreamDisconnected);
        continuity.observe(&disconnected);

        assert_eq!(continuity.state(false), EventStreamState::Disconnected);
        assert!(!continuity.rebuild_required());

        let mut reconnected = TopologyWakeBatch::default();
        reconnected.absorb(TopologyWakeHint::StreamConnected);
        continuity.observe(&reconnected);

        assert_eq!(continuity.state(false), EventStreamState::Connected);
        assert!(continuity.rebuild_required());
        continuity.authoritative_rebuild_committed();
        assert!(!continuity.rebuild_required());
    }

    #[test]
    fn event_gap_in_a_reconnect_batch_is_not_lost_by_last_state() {
        let mut continuity = TopologyEventContinuity::default();
        let mut batch = TopologyWakeBatch::default();
        batch.absorb(TopologyWakeHint::StreamDisconnected);
        batch.absorb(TopologyWakeHint::StreamConnected);

        continuity.observe(&batch);

        assert!(continuity.rebuild_required());
        assert_eq!(continuity.state(false), EventStreamState::Connected);
    }

    #[test]
    fn snapshot_comparison_ignores_order_but_detects_route_drift() {
        let a = observed_output("A");
        let b = observed_output("B");

        assert!(same_snapshot(
            &[a.clone(), b.clone()],
            &[b.clone(), a.clone()]
        ));
        assert!(!same_snapshot(
            &[a.clone(), b.clone()],
            std::slice::from_ref(&a)
        ));

        let mut reconfigured = b.clone();
        reconfigured.width = 1024;
        assert!(!same_snapshot(
            &[a, b],
            &[observed_output("A"), reconfigured]
        ));
    }

    #[test]
    fn topology_mutation_invalidates_the_cached_semantic_projection() {
        let consumer = SceneSemanticConsumer::enabled();
        consumer
            .publish(BTreeMap::from([(
                ConnectorName::new("A").unwrap(),
                test_projection("A", 1),
            )]))
            .expect("publish current projection");
        let runtime = DynamicTopologyRuntime {
            semantic_consumer: consumer.clone(),
            ..DynamicTopologyRuntime::default()
        };
        let plan = SessionTopologyReconciler::default()
            .plan(
                "test-session",
                vec![observed_output("A")],
                &BTreeSet::new(),
                false,
            )
            .expect("initial add plan");
        assert!(plan.changed());
        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_some()
        );

        runtime
            .invalidate_semantics_for_topology_plan(&plan)
            .expect("invalidate before route commit");

        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_none()
        );
    }

    #[test]
    fn no_change_topology_plan_preserves_the_current_projection() {
        let mut reconciler = SessionTopologyReconciler::default();
        let first = reconciler
            .plan(
                "test-session",
                vec![observed_output("A")],
                &BTreeSet::new(),
                false,
            )
            .expect("initial add plan");
        reconciler.commit(&first).expect("commit initial route");
        let unchanged = reconciler
            .plan(
                "test-session",
                vec![observed_output("A")],
                &BTreeSet::new(),
                false,
            )
            .expect("unchanged plan");
        assert!(!unchanged.changed());

        let consumer = SceneSemanticConsumer::enabled();
        consumer
            .publish(BTreeMap::from([(
                ConnectorName::new("A").unwrap(),
                test_projection("A", 1),
            )]))
            .expect("publish current projection");
        let runtime = DynamicTopologyRuntime {
            semantic_consumer: consumer.clone(),
            ..DynamicTopologyRuntime::default()
        };

        runtime
            .invalidate_semantics_for_topology_plan(&unchanged)
            .expect("unchanged plan does not invalidate");

        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_some()
        );
    }

    #[test]
    fn post_snapshot_drift_clears_semantics_until_fresh_publication() {
        let consumer = SceneSemanticConsumer::enabled();
        consumer
            .publish(BTreeMap::from([(
                ConnectorName::new("A").unwrap(),
                test_projection("A", 1),
            )]))
            .expect("publish current projection");
        let runtime = DynamicTopologyRuntime {
            semantic_consumer: consumer.clone(),
            ..DynamicTopologyRuntime::default()
        };

        let drift = runtime
            .invalidate_semantics_on_snapshot_drift(
                &[observed_output("A")],
                &[observed_output("B")],
                false,
            )
            .expect("invalidate stale projection");

        assert!(drift);
        assert!(
            consumer
                .get(Some(&RuntimeId::new("A".to_owned())))
                .is_none()
        );
    }

    #[test]
    fn failed_window_retirement_stays_pending_for_retry() {
        let mut closed = Vec::new();
        let pending = retain_failed_closes([10_u8, 11], |handle| {
            if handle == 10 {
                closed.push(handle);
                Ok(())
            } else {
                Err("temporarily unavailable")
            }
        });

        assert_eq!(closed, [10]);
        assert_eq!(pending, [11]);
    }

    #[derive(Default)]
    struct TestRouteWindowOps {
        opens: Vec<String>,
        closes: Vec<String>,
        fail_open: BTreeSet<String>,
        fail_close: BTreeSet<String>,
    }

    impl RouteWindowOperations<String> for TestRouteWindowOps {
        fn open(&mut self, output: &ObservedRuntimeOutput) -> Result<String, String> {
            self.opens.push(output.name.clone());
            if self.fail_open.contains(&output.name) {
                Err(format!("open_failed:{}", output.name))
            } else {
                Ok(output.name.clone())
            }
        }

        fn close(&mut self, route: &String) -> Result<(), String> {
            self.closes.push(route.clone());
            if self.fail_close.contains(route) {
                Err(format!("close_failed:{route}"))
            } else {
                Ok(())
            }
        }
    }

    fn add_plan(names: &[(&str, i64)]) -> SessionTopologyPlan {
        SessionTopologyReconciler::default()
            .plan(
                "test-session",
                names
                    .iter()
                    .map(|(name, id)| observed_output_with_id(name, *id))
                    .collect(),
                &BTreeSet::new(),
                false,
            )
            .expect("valid add plan")
    }

    #[test]
    fn staging_open_failure_retains_any_route_whose_rollback_close_fails() {
        let plan = add_plan(&[("A", 1), ("B", 2)]);
        let mut ops = TestRouteWindowOps {
            fail_open: BTreeSet::from(["B".to_owned()]),
            fail_close: BTreeSet::from(["A".to_owned()]),
            ..TestRouteWindowOps::default()
        };

        let failure = stage_topology_windows(&plan.actions, &mut ops)
            .expect_err("second output open must fail");

        assert_eq!(ops.opens, ["A", "B"]);
        assert_eq!(ops.closes, ["A"]);
        assert_eq!(failure.output, "B");
        assert_eq!(failure.rollback_pending, ["A"]);
    }

    #[test]
    fn staging_open_failure_closes_previously_opened_routes_when_rollback_succeeds() {
        let plan = add_plan(&[("A", 1), ("B", 2)]);
        let mut ops = TestRouteWindowOps {
            fail_open: BTreeSet::from(["B".to_owned()]),
            ..TestRouteWindowOps::default()
        };

        let failure = stage_topology_windows(&plan.actions, &mut ops)
            .expect_err("second output open must fail");

        assert_eq!(ops.opens, ["A", "B"]);
        assert_eq!(ops.closes, ["A"]);
        assert!(failure.rollback_pending.is_empty());
    }

    #[test]
    fn successful_staging_keeps_both_routes_staged_until_caller_commits() {
        let plan = add_plan(&[("A", 1), ("B", 2)]);
        let mut ops = TestRouteWindowOps::default();

        let staged =
            stage_topology_windows(&plan.actions, &mut ops).expect("both output windows open");

        assert_eq!(ops.opens, ["A", "B"]);
        assert!(ops.closes.is_empty());
        assert_eq!(
            staged.keys().map(String::as_str).collect::<Vec<_>>(),
            ["A", "B"]
        );
    }

    struct OrderedRouteWindowOps {
        events: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        projection_current: std::rc::Rc<std::cell::Cell<bool>>,
    }

    impl RouteWindowOperations<String> for OrderedRouteWindowOps {
        fn open(&mut self, output: &ObservedRuntimeOutput) -> Result<String, String> {
            assert!(
                !self.projection_current.get(),
                "staged host saw stale semantics"
            );
            self.events
                .borrow_mut()
                .push(format!("open:{}", output.name));
            Ok(output.name.clone())
        }

        fn close(&mut self, route: &String) -> Result<(), String> {
            self.events.borrow_mut().push(format!("close:{route}"));
            Ok(())
        }
    }

    #[test]
    fn topology_semantics_are_invalidated_before_any_staged_window_opens() {
        let plan = add_plan(&[("A", 1)]);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let projection_current = std::rc::Rc::new(std::cell::Cell::new(true));
        let mut ops = OrderedRouteWindowOps {
            events: events.clone(),
            projection_current: projection_current.clone(),
        };

        let staged = prepare_topology_stage(
            &plan,
            |_| {
                projection_current.set(false);
                events.borrow_mut().push("invalidate".to_owned());
                Ok(())
            },
            &mut ops,
        )
        .expect("invalidation precedes staging");

        assert_eq!(&*events.borrow(), &["invalidate", "open:A"]);
        assert_eq!(staged.get("A").map(String::as_str), Some("A"));
    }

    #[test]
    fn semantic_invalidation_failure_prevents_staging() {
        let plan = add_plan(&[("A", 1)]);
        let mut ops = TestRouteWindowOps::default();

        let failure =
            prepare_topology_stage(&plan, |_| Err("semantic_clear_failed".to_owned()), &mut ops)
                .expect_err("staging cannot begin after invalidation failure");

        assert!(matches!(
            failure,
            TopologyStageError::SemanticInvalidation(ref error)
                if error == "semantic_clear_failed"
        ));
        assert!(ops.opens.is_empty());
    }

    #[test]
    fn stale_reconciler_commit_does_not_publish_staged_routes() {
        let stale_plan = add_plan(&[("A", 1)]);
        let winning_plan = add_plan(&[("B", 2)]);
        let mut reconciler = SessionTopologyReconciler::default();
        reconciler
            .commit(&winning_plan)
            .expect("commit competing plan");
        let mut current = BTreeMap::from([("OLD".to_owned(), 10_u8)]);
        let mut staged = BTreeMap::from([("A".to_owned(), 20_u8)]);

        let error = commit_pending_routes(&mut reconciler, &stale_plan, &mut current, &mut staged)
            .expect_err("stale transaction cannot publish");

        assert_eq!(
            error,
            gnosix_hyprland::topology::TopologyReconcileError::StalePlan
        );
        assert_eq!(current, BTreeMap::from([("OLD".to_owned(), 10)]));
        assert_eq!(staged, BTreeMap::from([("A".to_owned(), 20)]));
    }

    #[test]
    fn successful_reconciler_commit_publishes_the_complete_staged_route_set() {
        let plan = add_plan(&[("A", 1), ("B", 2)]);
        let mut reconciler = SessionTopologyReconciler::default();
        let mut current = BTreeMap::from([("OLD".to_owned(), 10_u8)]);
        let mut staged = BTreeMap::from([("A".to_owned(), 20_u8), ("B".to_owned(), 21)]);

        commit_pending_routes(&mut reconciler, &plan, &mut current, &mut staged)
            .expect("commit and publish staged routes");

        assert_eq!(
            current,
            BTreeMap::from([("A".to_owned(), 20), ("B".to_owned(), 21)])
        );
        assert!(staged.is_empty());
        assert_eq!(reconciler.current_outputs().len(), 2);
    }

    #[test]
    fn shutdown_attempts_all_unique_owned_windows_after_semantic_clear_failure() {
        let events = std::cell::RefCell::new(Vec::new());
        let report = shutdown_owned_windows(
            [1_u8, 1, 2],
            || {
                events.borrow_mut().push("clear".to_owned());
                Err("semantic_clear_failed".to_owned())
            },
            |handle| {
                events.borrow_mut().push(format!("close:{handle}"));
                Ok(())
            },
        );

        assert_eq!(report.attempted, 2);
        assert_eq!(*events.borrow(), ["clear", "close:1", "close:2"]);
        assert_eq!(report.errors, ["semantic_clear_failed"]);
        assert!(report.close_pending.is_empty());
    }

    #[test]
    fn shutdown_continues_after_window_close_failure_and_aggregates_errors() {
        let mut attempted = Vec::new();
        let report = shutdown_owned_windows(
            [1_u8, 2, 3],
            || Ok(()),
            |handle| {
                attempted.push(handle);
                if handle == 2 {
                    Err("close_failed:2".to_owned())
                } else {
                    Ok(())
                }
            },
        );

        assert_eq!(report.attempted, 3);
        assert_eq!(attempted, [1, 2, 3]);
        assert_eq!(report.errors, ["close_failed:2"]);
        assert_eq!(report.close_pending, [2]);
    }

    #[test]
    fn successful_shutdown_clears_and_closes_the_deduplicated_owned_set() {
        let events = std::cell::RefCell::new(Vec::new());
        let report = shutdown_owned_windows(
            [1_u8, 2, 1],
            || {
                events.borrow_mut().push("clear".to_owned());
                Ok::<(), String>(())
            },
            |handle| {
                events.borrow_mut().push(format!("close:{handle}"));
                Ok(())
            },
        );

        assert_eq!(*events.borrow(), ["clear", "close:1", "close:2"]);
        assert_eq!(report.attempted, 2);
        assert!(report.errors.is_empty());
        assert!(report.close_pending.is_empty());
    }
}
