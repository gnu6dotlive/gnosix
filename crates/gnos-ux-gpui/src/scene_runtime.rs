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

#[derive(Clone, Default)]
struct SceneProjectionState {
    projections: Arc<Mutex<BTreeMap<ConnectorName, DesktopOutputProjection>>>,
    revision: Arc<AtomicU64>,
    subscribers: Arc<Mutex<Vec<async_mpsc::UnboundedSender<u64>>>>,
}

impl SceneProjectionState {
    fn clear(&self) -> Result<(), String> {
        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| "scene_projection_subscribers_poisoned".to_owned())?;
        let mut projections = self
            .projections
            .lock()
            .map_err(|_| "scene_projection_state_poisoned".to_owned())?;
        projections.clear();
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        drop(projections);
        subscribers.retain(|subscriber| subscriber.unbounded_send(revision).is_ok());
        Ok(())
    }

    fn publish(
        &self,
        projections: BTreeMap<ConnectorName, DesktopOutputProjection>,
    ) -> Result<(), String> {
        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| "scene_projection_subscribers_poisoned".to_owned())?;
        let mut current = self
            .projections
            .lock()
            .map_err(|_| "scene_projection_state_poisoned".to_owned())?;
        *current = projections;
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        drop(current);
        subscribers.retain(|subscriber| subscriber.unbounded_send(revision).is_ok());
        Ok(())
    }

    fn get(&self, output: Option<&RuntimeId>) -> Option<DesktopOutputProjection> {
        let connector = ConnectorName::new(output?.as_str().to_owned()).ok()?;
        self.projections.lock().ok()?.get(&connector).cloned()
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn subscribe(&self) -> Result<async_mpsc::UnboundedReceiver<u64>, String> {
        let (sender, receiver) = async_mpsc::unbounded();
        self.subscribers
            .lock()
            .map_err(|_| "scene_projection_subscribers_poisoned".to_owned())?
            .push(sender);
        Ok(receiver)
    }
}

#[derive(Clone, Default)]
struct SceneSemanticConsumer {
    enabled: bool,
    state: SceneProjectionState,
}

impl SceneSemanticConsumer {
    fn enabled() -> Self {
        Self {
            enabled: true,
            state: SceneProjectionState::default(),
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn clear(&self) -> Result<(), String> {
        if self.enabled {
            self.state.clear()?;
        }
        Ok(())
    }

    fn publish(
        &self,
        projections: BTreeMap<ConnectorName, DesktopOutputProjection>,
    ) -> Result<(), String> {
        if self.enabled {
            self.state.publish(projections)?;
        }
        Ok(())
    }

    fn get(&self, output: Option<&RuntimeId>) -> Option<DesktopOutputProjection> {
        if self.enabled {
            self.state.get(output)
        } else {
            None
        }
    }

    fn revision(&self) -> u64 {
        self.state.revision()
    }

    fn subscribe(&self) -> Result<async_mpsc::UnboundedReceiver<u64>, String> {
        self.state.subscribe()
    }
}

fn pair_projections_by_connector(
    route_connectors: impl IntoIterator<Item = ConnectorName>,
    projections: &[DesktopOutputProjection],
    semantic_generation: DesktopContextGeneration,
) -> Result<BTreeMap<ConnectorName, DesktopOutputProjection>, String> {
    let mut routes = BTreeMap::new();
    for connector in route_connectors {
        if routes.insert(connector.clone(), ()).is_some() {
            return Err(format!("duplicate_route_connector:{}", connector.as_str()));
        }
    }

    let mut by_connector = BTreeMap::new();
    for projection in projections {
        if projection.generation != semantic_generation {
            return Err(format!(
                "projection_generation_mismatch:expected={} observed={}",
                semantic_generation.0, projection.generation.0
            ));
        }
        let connector = projection.output.connector.clone();
        if by_connector
            .insert(connector.clone(), projection.clone())
            .is_some()
        {
            return Err(format!(
                "duplicate_projection_connector:{}",
                connector.as_str()
            ));
        }
    }

    if routes.len() != by_connector.len() {
        return Err(format!(
            "route_projection_count_mismatch:routes={} projections={}",
            routes.len(),
            by_connector.len()
        ));
    }

    let mut paired = BTreeMap::new();
    for connector in routes.keys() {
        let projection = by_connector
            .remove(connector)
            .ok_or_else(|| format!("missing_projection_for_route:{}", connector.as_str()))?;
        paired.insert(connector.clone(), projection);
    }
    if !by_connector.is_empty() {
        return Err(format!(
            "unpaired_projection_connectors:{}",
            by_connector
                .keys()
                .map(ConnectorName::as_str)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    Ok(paired)
}

fn desktop_consumer_context_text(projection: Option<&DesktopOutputProjection>) -> String {
    let Some(view) = projection else {
        return "DSC01 semantic=unavailable".to_owned();
    };
    let active_workspace = view
        .output
        .active_workspace_id
        .and_then(|id| {
            view.workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == id)
        })
        .map(|workspace| workspace.name.as_str())
        .unwrap_or("none");
    let focused_workspace = view
        .workspaces
        .iter()
        .find(|workspace| workspace.focus == WorkspaceFocus::Focused)
        .map(|workspace| workspace.name.as_str())
        .unwrap_or("none");
    let visible_workspaces = view
        .workspaces
        .iter()
        .filter(|workspace| workspace.visibility == WorkspaceVisibility::Visible)
        .map(|workspace| workspace.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let focused_application = view
        .surfaces
        .iter()
        .find(|surface| {
            surface.role == DesktopSurfaceRole::Application
                && surface.focus == SurfaceFocus::Focused
        })
        .and_then(|surface| surface.application.as_ref())
        .map(|application| application.class.as_str())
        .unwrap_or("none");
    let application_surfaces = view
        .surfaces
        .iter()
        .filter(|surface| surface.role == DesktopSurfaceRole::Application)
        .collect::<Vec<_>>();
    let applications = application_surfaces.len();
    let application_hidden = application_surfaces
        .iter()
        .filter(|surface| surface.visibility == SurfaceVisibility::Hidden)
        .count();
    let application_unmapped = application_surfaces
        .iter()
        .filter(|surface| surface.visibility == SurfaceVisibility::Unmapped)
        .count();
    format!(
        "DSC01 connector={} semantic_generation={} active_workspace={} focused_workspace={} visible_workspaces={} focused_application={} applications={} application_hidden={} application_unmapped={}",
        view.output.connector.as_str(),
        view.generation.0,
        active_workspace,
        focused_workspace,
        if visible_workspaces.is_empty() {
            "none"
        } else {
            visible_workspaces.as_str()
        },
        focused_application,
        applications,
        application_hidden,
        application_unmapped
    )
}

struct ShutdownSignals {
    sigterm: Arc<AtomicBool>,
    sigint: Arc<AtomicBool>,
    receiver: async_mpsc::UnboundedReceiver<&'static str>,
    handle: SignalHandle,
    thread: Option<thread::JoinHandle<()>>,
}

impl ShutdownSignals {
    fn install() -> Result<Self, String> {
        let sigterm = Arc::new(AtomicBool::new(false));
        let sigint = Arc::new(AtomicBool::new(false));
        let mut signals = Signals::new([SIGTERM, SIGINT])
            .map_err(|error| format!("register_shutdown_signals:{error}"))?;
        let handle = signals.handle();
        let (sender, receiver) = async_mpsc::unbounded();
        let term = Arc::clone(&sigterm);
        let interrupt = Arc::clone(&sigint);
        let signal_thread = thread::Builder::new()
            .name("gnos-ux-shutdown-signals".to_owned())
            .spawn(move || {
                for signal in signals.forever() {
                    let reason = if signal == SIGTERM {
                        term.store(true, Ordering::Release);
                        "SIGTERM"
                    } else {
                        interrupt.store(true, Ordering::Release);
                        "SIGINT"
                    };
                    if sender.unbounded_send(reason).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("shutdown_signal_thread:{error}"))?;
        Ok(Self {
            sigterm,
            sigint,
            receiver,
            handle,
            thread: Some(signal_thread),
        })
    }

    fn requested_reason(&self) -> Option<&'static str> {
        if self.sigterm.load(Ordering::Acquire) {
            Some("SIGTERM")
        } else if self.sigint.load(Ordering::Acquire) {
            Some("SIGINT")
        } else {
            None
        }
    }
}

impl Drop for ShutdownSignals {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

fn emit_health_receipt(
    lifecycle: &SharedLifecycle,
    phase: RuntimeLifecyclePhase,
    generation: Option<u64>,
    route_count: usize,
    event_stream: EventStreamState,
    committed: bool,
    reason: impl Into<String>,
) -> Result<RuntimeHealthReceipt, String> {
    let reason = reason.into();
    let receipt = lifecycle
        .lock()
        .map_err(|_| "lifecycle_lock_poisoned".to_owned())?
        .transition(
            phase,
            generation,
            route_count,
            event_stream,
            committed,
            reason.clone(),
        )
        .map_err(|error| format!("lifecycle_transition:{error:?}"))?;

    println!(
        "GNOS_UX_HEALTH_RECEIPT sequence={} phase={} health={} ready={} topology_generation={} route_count={} event_stream={} reason={}",
        receipt.sequence,
        receipt.phase.as_str(),
        receipt.health.as_str(),
        receipt.ready,
        receipt
            .topology_generation
            .map(|value| value.to_string())
            .unwrap_or_else(|| "NONE".to_owned()),
        receipt.route_count,
        receipt.event_stream.as_str(),
        receipt.reason.replace(char::is_whitespace, "_"),
    );
    Ok(receipt)
}

fn emit_initial_health_receipt(lifecycle: &SharedLifecycle) -> Result<(), String> {
    let receipt = lifecycle
        .lock()
        .map_err(|_| "lifecycle_lock_poisoned".to_owned())?
        .initial_receipt("process_start")
        .map_err(|error| format!("lifecycle_initial:{error:?}"))?;
    println!(
        "GNOS_UX_HEALTH_RECEIPT sequence={} phase={} health={} ready={} topology_generation=NONE route_count=0 event_stream={} reason={}",
        receipt.sequence,
        receipt.phase.as_str(),
        receipt.health.as_str(),
        receipt.ready,
        receipt.event_stream.as_str(),
        receipt.reason,
    );
    Ok(())
}

#[derive(Default)]
struct ProductPerfCounters {
    started: Option<Instant>,
    timer_ticks: AtomicU64,
    motion_notifies: AtomicU64,
    projection_notifies: AtomicU64,
    interaction_notifies: AtomicU64,
    pointer_changes: AtomicU64,
    renders: AtomicU64,
    solver_resolves: AtomicU64,
    appstack_solver_resolves: AtomicU64,
    largest_motion_tick_gap_us: AtomicU64,
    bar_transition_ticks: AtomicU64,
    bar_transition_gaps_us: Mutex<Vec<u64>>,
    bar_transition_trace_emitted: AtomicBool,
    paints: AtomicU64,
}

#[derive(Default)]
struct ProductMotionDriver {
    active: bool,
    first_frame_pending: bool,
}

impl ProductMotionDriver {
    fn arm(&mut self, reduced_motion: bool) -> bool {
        if self.active || reduced_motion {
            return false;
        }
        self.active = true;
        self.first_frame_pending = true;
        true
    }

    fn stop(&mut self) {
        self.active = false;
        self.first_frame_pending = false;
    }

    fn tick(&mut self, needs_frame: bool) -> bool {
        if !self.active {
            return false;
        }
        if self.first_frame_pending {
            self.first_frame_pending = false;
            return true;
        }
        if needs_frame {
            return true;
        }
        self.active = false;
        false
    }
}

struct SceneHost {
    scene_id: MonitorSceneId,
    runtime_output: Option<RuntimeId>,
    semantic_consumer: SceneSemanticConsumer,
    last_projection_revision: u64,
    product_mode: bool,
    product_diagnostics: bool,
    /// Which product surface is open.
    ///
    /// A bounded enum rather than a boolean, so "workspace summary and AppStack
    /// both open" is not a state the product can be in. See
    /// [`ProductPanelState`].
    product_panel: ProductPanelState,
    /// The AppStack's list scroll position, owned by the host so it survives the
    /// rerenders that a live product generates constantly.
    appstack_scroll: gpui::ScrollHandle,
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
    product_pointer: Option<Point2>,
    bar_pointer_zone: BarPointerZone,
    bar_reveal_state: BarRevealState,
    product_bar_edge: SurfaceEdge,
    bar_transition_qualification: Option<(crate::product_bar::BarTransitionQualification, Instant)>,
    bar_transition_completion_notified: bool,
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
    fn new(
        scene_id: MonitorSceneId,
        runtime_output: Option<RuntimeId>,
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
            let edge = crate::product_preferences::bar_surface_edge(&preferences);
            this.product_runtime.set_preferences(preferences);
            // An unavailable projection cannot manufacture a normal/default target.
            this.pending_profile_edge = edge;
            this.consume_profile_edge();
            this.start_motion_driver(cx);
            cx.notify();
        });
        let projected_bar_edge =
            crate::product_preferences::bar_surface_edge(&effective_product_preferences);
        product_runtime.set_preferences(effective_product_preferences);
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
            runtime_output,
            semantic_consumer,
            last_projection_revision,
            product_mode,
            product_diagnostics,
            product_panel: ProductPanelState::Closed,
            appstack_scroll: gpui::ScrollHandle::new(),
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
            product_pointer: None,
            bar_pointer_zone: BarPointerZone::Outside,
            bar_reveal_state,
            product_bar_edge: bar_transition_qualification.as_ref().map_or_else(
                || {
                    crate::product_bar::qualification_edge(&args)
                        .expect("validated qualification edge")
                        .or(projected_bar_edge)
                        .unwrap_or(SurfaceEdge::Top)
                },
                |(plan, _)| plan.final_target(),
            ),
            bar_transition_qualification,
            bar_transition_completion_notified: false,
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
        let Some(target) = crate::product_profile::take_relocation_target(
            &mut self.pending_profile_edge,
            source,
            active,
        ) else {
            return false;
        };
        self.bar_transition_qualification = Some((
            crate::product_bar::BarTransitionQualification::between_edges(source, target),
            Instant::now(),
        ));
        self.bar_transition_completion_notified = false;
        self.bar_transition_reduced_logged = false;
        self.bar_transition_driver_started = None;
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

    fn refresh_effective_product_preferences(&mut self) -> bool {
        let effective = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            self.system_product_preferences,
        );
        #[cfg(feature = "r12-production-client")]
        let effective = crate::product_preferences::preserve_durable_bar_projection(
            effective,
            self.product_runtime.preferences(),
        );
        self.product_runtime.set_preferences(effective)
    }

    fn set_system_color_scheme(
        &mut self,
        scheme: Option<ProductColorScheme>,
        cx: &mut Context<Self>,
    ) {
        self.system_product_preferences.color_scheme = scheme;
        let changed = self.refresh_effective_product_preferences();
        let effective = self.product_runtime.preferences();
        println!(
            "GNOS_UX_EFFECTIVE_PREFERENCES output={} field=color_scheme value={:?} source={:?} projection_state={:?}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            effective.color_scheme,
            effective.color_scheme_source,
            effective.projection_state,
        );
        if changed {
            cx.notify();
        }
    }

    fn set_system_motion_preference(
        &mut self,
        preference: MotionPreference,
        cx: &mut Context<Self>,
    ) {
        let motion_changed = self.motion_preference.update_system(preference);
        self.system_product_preferences.reduced_motion = preference.as_system_observation();
        let preferences_changed = self.refresh_effective_product_preferences();
        let effective = self.effective_reduced_motion();
        println!(
            "GNOS_UX_MOTION_PREFERENCE output={} source=portal value={} effective={} cli_override={}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            preference.as_str(),
            if effective { "reduce" } else { "no_preference" },
            self.motion_preference.cli_override_enabled(),
        );
        if motion_changed && effective {
            self.motion_driver.stop();
        }
        let product_preferences = self.product_runtime.preferences();
        println!(
            "GNOS_UX_EFFECTIVE_PREFERENCES output={} field=reduced_motion value={} source={:?} projection_state={:?}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            product_preferences.reduced_motion,
            product_preferences.reduced_motion_source,
            product_preferences.projection_state,
        );
        if motion_changed || preferences_changed {
            cx.notify();
        }
    }

    fn start_motion_driver(&mut self, cx: &mut Context<Self>) {
        if !self.product_mode || !self.motion_driver.arm(self.effective_reduced_motion()) {
            return;
        }
        let first_bar_transition_driver = self.bar_transition_driver_started.is_none()
            && self
                .bar_transition_qualification
                .as_ref()
                .is_some_and(|(plan, started)| started.elapsed() < plan.total_duration());
        if first_bar_transition_driver {
            self.bar_transition_driver_started = Some(Instant::now());
        }
        self.motion_tick_previous = None;
        if let Some(perf) = &self.perf_counters {
            perf.largest_motion_tick_gap_us.store(0, Ordering::Relaxed);
            if first_bar_transition_driver {
                perf.bar_transition_ticks.store(0, Ordering::Relaxed);
                perf.bar_transition_trace_emitted
                    .store(false, Ordering::Relaxed);
                if let Ok(mut gaps) = perf.bar_transition_gaps_us.lock() {
                    gaps.clear();
                }
            }
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let Ok(continue_motion) = this.update(cx, |view, cx| {
                    if !view.motion_driver.active {
                        return false;
                    }
                    let now = Instant::now();
                    if let Some(perf) = &view.perf_counters {
                        perf.timer_ticks.fetch_add(1, Ordering::Relaxed);
                        if view.bar_transition_qualification.is_some()
                            && !view.effective_reduced_motion()
                        {
                            perf.bar_transition_ticks.fetch_add(1, Ordering::Relaxed);
                            if let Some(gap) = view
                                .motion_tick_previous
                                .map(|previous| now.duration_since(previous).as_micros() as u64)
                                && let Ok(mut gaps) = perf.bar_transition_gaps_us.lock()
                            {
                                gaps.push(gap);
                            }
                        }
                    }
                    if let Some(previous) = view.motion_tick_previous.replace(now)
                        && let Some(perf) = &view.perf_counters
                    {
                        let gap_us = now.duration_since(previous).as_micros() as u64;
                        perf.largest_motion_tick_gap_us
                            .fetch_max(gap_us, Ordering::Relaxed);
                    }
                    let needs_frame = view.advance_product_motion();
                    let continue_motion = view.motion_driver.tick(needs_frame);
                    if !continue_motion {
                        view.log_bar_transition_trace();
                    }
                    if continue_motion {
                        if let Some(perf) = &view.perf_counters {
                            perf.motion_notifies.fetch_add(1, Ordering::Relaxed);
                        }
                        cx.notify();
                    }
                    continue_motion
                }) else {
                    break;
                };
                if !continue_motion {
                    break;
                }
            }
        })
        .detach();
    }

    fn advance_product_motion(&mut self) -> bool {
        #[cfg(feature = "r12-production-client")]
        if self.consume_profile_edge() {
            return true;
        }
        self.refresh_product_runtime();
        // Re-constrain on every frame. The product's capability to show a
        // surface can be lost at any time - a projection goes stale, semantics
        // become unavailable - and a panel that outlives its capability would be
        // showing something the product can no longer honour.
        self.product_panel = self.product_panel.constrained_by(
            self.product_runtime.semantic().can_open_workspace(),
            self.product_runtime.semantic().can_open_appstack(),
        );
        if !self.effective_reduced_motion()
            && let Some((plan, started)) = &self.bar_transition_qualification
        {
            if started.elapsed() < plan.total_duration() {
                return true;
            }
            if !self.bar_transition_completion_notified {
                // One final notification renders the exact target endpoint and
                // releases the source input/reservation generation.
                self.bar_transition_completion_notified = true;
                return true;
            }
        }
        // One scene driver owns every reactive surface with unsettled motion.
        // Each solver retains its own target and lineage; the driver only asks
        // whether either one still needs a render step.
        !self.effective_reduced_motion()
            && (self
                .bar_transition_qualification
                .as_ref()
                .is_some_and(|(plan, started)| started.elapsed() < plan.total_duration())
                || crate::product_scene::reactive_motion_needs_frame(
                    self.product_hull.as_ref(),
                    self.product_panel.is_workspace_summary(),
                    false,
                )
                || !self
                    .appstack_motion
                    .is_settled_for(self.product_panel.is_appstack()))
    }

    fn begin_appstack_motion_trace(&mut self, target_open: bool) {
        if self.appstack_motion_trace {
            self.appstack_motion_trace_target = Some(target_open);
            self.appstack_motion_trace_frame = 0;
            self.appstack_motion_trace_started = Some(Instant::now());
            self.appstack_motion_trace_previous = None;
        }
    }

    fn record_appstack_motion_frame(
        &mut self,
        frame: &crate::product_shared_envelope::SharedDockEnvelope,
    ) {
        if !self.appstack_motion_trace {
            return;
        }
        let open = frame.motion.target_open;
        if self.appstack_motion_trace_target != Some(open) {
            self.begin_appstack_motion_trace(open);
        }
        if self.appstack_motion_trace_frame >= 512 {
            return;
        }
        let now = Instant::now();
        let dt = self
            .appstack_motion_trace_previous
            .replace(now)
            .map_or(0.0, |p| now.duration_since(p).as_secs_f64() * 1000.0);
        let elapsed = self
            .appstack_motion_trace_started
            .map_or(0.0, |p| now.duration_since(p).as_secs_f64() * 1000.0);
        self.appstack_motion_trace_frame += 1;
        let rect =
            |r: gnos_ux_surface::grammar::GeometryRect| [r.left(), r.top(), r.right(), r.bottom()];
        println!(
            "GNOS_UX_APPSTACK_MOTION_TRACE {{\"schema\":\"gnosix.shared-envelope.debug.v1\",\"surface\":\"AppStackInner\",\"parent\":\"SharedDockEnvelope\",\"material_owner\":\"SharedDockEnvelope\",\"inner_roles\":[\"DockControls\",\"ApplicationList\"],\"frame\":{},\"elapsed_ms\":{elapsed:.3},\"delta_ms\":{dt:.3},\"driver_active\":{},\"needs_next_frame\":{},\"target_open\":{open},\"horizontal\":{:.6},\"vertical\":{:.6},\"reveal\":{:.6},\"velocity\":[{:.6},{:.6},{:.6}],\"outer_bounds\":{:?},\"structural_dock\":{:?},\"painted_dock_request\":{:?},\"DockInner\":{:?},\"AppStackInner\":{:?},\"content_clip\":{:?},\"interactable\":{},\"baseline\":{},\"input_regions\":{:?}}}",
            self.appstack_motion_trace_frame,
            self.motion_driver.active,
            !frame.motion.is_settled_for(open),
            frame.motion.horizontal.value,
            frame.motion.vertical.value,
            frame.motion.reveal.value,
            frame.motion.horizontal.velocity,
            frame.motion.vertical.velocity,
            frame.motion.reveal.velocity,
            rect(frame.geometry.outer),
            rect(frame.geometry.structural),
            rect(frame.geometry.painted_request),
            rect(frame.dock.bounds),
            rect(frame.appstack.bounds),
            rect(frame.content_clip),
            frame.appstack.interactable,
            frame.geometry.outer.bottom(),
            frame
                .input_regions()
                .into_iter()
                .map(rect)
                .collect::<Vec<_>>()
        );
    }

    fn notify_projection_revision(&mut self, cx: &mut Context<Self>) {
        if !self.semantic_consumer.is_enabled() {
            return;
        }
        let revision = self.semantic_consumer.revision();
        if revision != self.last_projection_revision {
            self.last_projection_revision = revision;
            if self.product_mode {
                self.refresh_product_runtime();
                match self.product_runtime.semantic() {
                    ProductSemanticState::Current(content) => println!(
                        "GNOS_UX_PRODUCT_CONTENT output={} status=current generation={} workspace={:?}",
                        content.connector, content.generation.0, content.active_workspace
                    ),
                    ProductSemanticState::Unavailable => {
                        println!("GNOS_UX_PRODUCT_CONTENT status=unavailable")
                    }
                    ProductSemanticState::Invalid => {
                        println!("GNOS_UX_PRODUCT_CONTENT status=invalid")
                    }
                }
            }
            if let Some(perf) = &self.perf_counters {
                perf.projection_notifies.fetch_add(1, Ordering::Relaxed);
            }
            cx.notify();
        }
    }

    fn log_perf_sample(&self) {
        let Some(perf) = &self.perf_counters else {
            return;
        };
        let uptime_ms = perf.started.map_or(0, |start| start.elapsed().as_millis());
        let settled = self
            .product_hull
            .as_ref()
            .is_some_and(|solver| solver.is_settled_for(self.product_panel.is_workspace_summary()));
        let appstack_settled = self
            .appstack_motion
            .is_settled_for(self.product_panel.is_appstack());
        println!(
            "GNOS_UX_PERF_SAMPLE output={} uptime_ms={} panel={} settled={} appstack_settled={} pointer={} reduced={} timer_ticks={} motion_notifies={} projection_notifies={} interaction_notifies={} pointer_changes={} renders={} solver_resolves={} appstack_solver_resolves={} largest_motion_tick_gap_us={} paints={}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            uptime_ms,
            self.product_panel,
            settled,
            appstack_settled,
            self.product_pointer.is_some(),
            self.effective_reduced_motion(),
            perf.timer_ticks.load(Ordering::Relaxed),
            perf.motion_notifies.load(Ordering::Relaxed),
            perf.projection_notifies.load(Ordering::Relaxed),
            perf.interaction_notifies.load(Ordering::Relaxed),
            perf.pointer_changes.load(Ordering::Relaxed),
            perf.renders.load(Ordering::Relaxed),
            perf.solver_resolves.load(Ordering::Relaxed),
            perf.appstack_solver_resolves.load(Ordering::Relaxed),
            perf.largest_motion_tick_gap_us.load(Ordering::Relaxed),
            perf.paints.load(Ordering::Relaxed),
        );
    }

    fn log_bar_transition_trace(&self) {
        let (Some((plan, _)), Some(perf)) =
            (&self.bar_transition_qualification, &self.perf_counters)
        else {
            return;
        };
        if perf
            .bar_transition_trace_emitted
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let mut gaps = perf
            .bar_transition_gaps_us
            .lock()
            .map(|gaps| gaps.clone())
            .unwrap_or_default();
        gaps.sort_unstable();
        let percentile = |p: f64| -> u64 {
            if gaps.is_empty() {
                return 0;
            }
            let index = ((gaps.len() as f64 * p).ceil() as usize)
                .saturating_sub(1)
                .min(gaps.len() - 1);
            gaps[index]
        };
        println!(
            "GNOS_UX_BAR_TRANSITION_TRACE scene_id={} output={} plan={:?} active_frames={} gap_samples={} median_gap_us={} p95_gap_us={} max_gap_us={} settle_ms={} driver_active=false driver_sleeps_after_settle=true",
            self.scene_id.value(),
            self.runtime_output
                .as_ref()
                .map_or("COMPOSITOR_SELECTED", RuntimeId::as_str),
            plan,
            perf.bar_transition_ticks.load(Ordering::Relaxed),
            gaps.len(),
            percentile(0.50),
            percentile(0.95),
            gaps.last().copied().unwrap_or_default(),
            self.bar_transition_driver_started
                .map_or(0, |started| started.elapsed().as_millis()),
        );
    }

    fn ensure_scene(&mut self, viewport: SurfaceViewport) {
        if self.scene.is_none() {
            let mut scene =
                MonitorScene::new(self.scene_id, viewport).expect("creating monitor scene");
            let frame = scene.tick(1.0 / 60.0).expect("initial scene frame");
            self.log_frame(&frame, "initial");
            self.previous_step = Some(frame.cycle_step);
            self.previous_viewport = Some(viewport);
            self.frame = Some(frame);
            self.scene = Some(scene);
            return;
        }

        if self.previous_viewport != Some(viewport) {
            let previous = self.previous_viewport;
            let previous_frame = self.frame.clone();
            let scene = self.scene.as_mut().expect("scene");
            scene
                .rebase_viewport(viewport)
                .expect("rebasing monitor scene viewport");
            let frame = scene.tick(1.0 / 60.0).expect("scene frame after rebase");
            println!(
                "GNOS_UX_SCENE_VIEWPORT_REBASE scene_id={} host_token={} from={} to={:.1}x{:.1} epoch={}",
                scene.id().value(),
                self.host_token,
                previous
                    .map(|value| format!("{:.1}x{:.1}", value.width, value.height))
                    .unwrap_or_else(|| "NONE".to_owned()),
                viewport.width,
                viewport.height,
                frame.epoch.value(),
            );
            if let (Some(old_viewport), Some(old_frame)) = (previous, previous_frame.as_ref())
                && old_viewport.width > 8.0
                && old_viewport.height > 8.0
            {
                let delta = normalized_relation_endpoint_delta(old_frame, &frame);
                println!(
                    "GNOS_UX_SCENE_REBASE_CONTINUITY scene_id={} host_token={} normalized_endpoint_delta={:.6}",
                    frame.scene_id.value(),
                    self.host_token,
                    delta,
                );
            }
            self.log_frame(&frame, "rebase");
            self.previous_step = Some(frame.cycle_step);
            self.previous_viewport = Some(viewport);
            self.frame = Some(frame);
        }
    }

    fn advance_scene(&mut self) {
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        if scene.script_complete() {
            if !self.completion_logged {
                let max_delta = scene.max_connector_endpoint_delta();
                if max_delta < 24.0 {
                    println!(
                        "GNOS_UX_SCENE_CONTINUITY_PASS scene_id={} host_token={} max_connector_endpoint_delta={:.4} bound=24.0",
                        scene.id().value(),
                        self.host_token,
                        max_delta,
                    );
                } else {
                    println!(
                        "GNOS_UX_SCENE_CONTINUITY_FAIL scene_id={} host_token={} max_connector_endpoint_delta={:.4} bound=24.0",
                        scene.id().value(),
                        self.host_token,
                        max_delta,
                    );
                }
                println!(
                    "GNOS_UX_SCENE_IDENTITY_PASS scene_id={} host_token={} epoch={}",
                    scene.id().value(),
                    self.host_token,
                    scene.epoch().value(),
                );
                println!(
                    "GNOS_UX_SCENE_CYCLE_COMPLETE scene_id={} host_token={} epoch={} max_connector_endpoint_delta={:.4}",
                    scene.id().value(),
                    self.host_token,
                    scene.epoch().value(),
                    max_delta,
                );
                self.completion_logged = true;
            }
            return;
        }

        let frame = scene.tick(1.0 / 60.0).expect("advancing scene frame");
        let step_changed = self.previous_step != Some(frame.cycle_step);
        if step_changed {
            println!(
                "GNOS_UX_SCENE_STEP scene_id={} host_token={} step={} epoch={} viewport={:.1}x{:.1}",
                frame.scene_id.value(),
                self.host_token,
                frame.cycle_step.as_str(),
                frame.epoch.value(),
                frame.viewport.width,
                frame.viewport.height,
            );
            self.previous_step = Some(frame.cycle_step);
        }
        if step_changed || frame.epoch.value() % 30 == 0 || frame.influence_pressure > 0.001 {
            self.log_frame(&frame, "tick");
        }
        self.frame = Some(frame);
    }

    fn maybe_start_projection_qualification(&mut self, frame: &SceneFrame) {
        if !self.projection_qualification_enabled
            || self.projection_worker_started
            || frame.viewport.width <= 8.0
            || frame.viewport.height <= 8.0
        {
            return;
        }
        self.projection_worker_started = true;
        let source = CompositorIntentSource {
            scene_id: frame.scene_id,
            scene_epoch: frame.epoch,
        };
        thread::spawn(move || {
            if let Err(error) = run_global_projection_qualification(source) {
                eprintln!("GNOSIX_HYPRLAND_GLOBAL_PROJECTION_FAIL error={error}");
            }
        });
    }

    fn project_surface_reservation(&mut self, frame: &SceneFrame, window: &mut Window) {
        if frame.viewport.width <= 8.0 || frame.viewport.height <= 8.0 {
            return;
        }

        let intent = match ResolvedCompositorIntent::try_from_scene_frame_for_output(
            frame,
            self.runtime_output.clone(),
        ) {
            Ok(intent) => intent,
            Err(error) => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason={error:?}",
                    frame.scene_id.value(),
                    frame.epoch.value()
                );
                return;
            }
        };
        let plan = match surface_reservation_plan(&intent) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason={error:?}",
                    frame.scene_id.value(),
                    frame.epoch.value()
                );
                return;
            }
        };

        if plan.output != self.runtime_output {
            eprintln!(
                "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason=OUTPUT_TARGET_MISMATCH",
                plan.source.scene_id.value(),
                plan.source.scene_epoch.value()
            );
            return;
        }

        let key = ProjectionKey {
            scene_id: plan.source.scene_id,
            slot: ProjectionSlot::SurfaceReservation,
            runtime_target: SCENE_NAMESPACE.to_owned(),
        };
        let token = surface_request_token(plan.edge, plan.exclusive_zone);
        match self
            .surface_fence
            .evaluate(&key, plan.source.scene_epoch, token)
        {
            FenceDecision::Stale => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason=STALE",
                    plan.source.scene_id.value(),
                    plan.source.scene_epoch.value()
                );
                return;
            }
            FenceDecision::Conflict => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason=CONFLICT",
                    plan.source.scene_id.value(),
                    plan.source.scene_epoch.value()
                );
                return;
            }
            FenceDecision::AlreadyAccepted => return,
            FenceDecision::Fresh => {}
        }

        let current_reservation = (plan.edge, plan.exclusive_zone);
        if self.initial_reservation.is_none() {
            self.initial_reservation = Some(current_reservation);
        } else if self.initial_reservation != Some(current_reservation) {
            self.reservation_changed_from_initial = true;
        }

        if self.applied_reservation != Some(current_reservation) {
            window.set_exclusive_edge(surface_edge_anchor(plan.edge));
            window.set_exclusive_zone(px(plan.exclusive_zone as f32));
            self.applied_reservation = Some(current_reservation);
            let bar = frame.node(SceneNodeId::BAR).expect("Bar frame node");
            let dock = frame.node(SceneNodeId::DOCK).expect("Dock frame node");
            println!(
                "GNOS_UX_SCENE_RESERVATION_APPLIED scene_id={} host_token={} epoch={} edge={} zone={} intent=[{:.1},{:.1},{:.1},{:.1}] bar=[{:.1},{:.1},{:.1},{:.1}] dock=[{:.1},{:.1},{:.1},{:.1}]",
                plan.source.scene_id.value(),
                self.host_token,
                plan.source.scene_epoch.value(),
                surface_edge_name(plan.edge),
                plan.exclusive_zone,
                frame.reservation.top,
                frame.reservation.right,
                frame.reservation.bottom,
                frame.reservation.left,
                bar.envelope.left(),
                bar.envelope.top(),
                bar.envelope.right(),
                bar.envelope.bottom(),
                dock.envelope.left(),
                dock.envelope.top(),
                dock.envelope.right(),
                dock.envelope.bottom(),
            );

            if self.reservation_changed_from_initial
                && self.initial_reservation == Some(current_reservation)
                && !self.reservation_return_logged
            {
                self.reservation_return_logged = true;
                println!(
                    "GNOS_UX_SCENE_RESERVATION_RETURNED scene_id={} host_token={} epoch={} edge={} zone={}",
                    plan.source.scene_id.value(),
                    self.host_token,
                    plan.source.scene_epoch.value(),
                    surface_edge_name(plan.edge),
                    plan.exclusive_zone
                );
            }
        }

        self.surface_fence
            .commit(key, plan.source.scene_epoch, token);
    }

    fn log_frame(&self, frame: &SceneFrame, phase: &str) {
        let relation = frame.relations.first().expect("scene relation");
        let body = frame.node(SceneNodeId::BODY).expect("body node");
        let dock = frame.node(SceneNodeId::DOCK).expect("dock node");
        let bar = frame.node(SceneNodeId::BAR).expect("bar node");
        println!(
            "GNOS_UX_SCENE_FRAME phase={} scene_id={} host_token={} epoch={} step={} viewport={:.1}x{:.1} nodes={} relations={} bar=[{:.1},{:.1},{:.1},{:.1}] body=[{:.1},{:.1},{:.1},{:.1}] dock=[{:.1},{:.1},{:.1},{:.1}] relation={:?} presence={:.4} influence_pressure={:.3} reservation=[{:.1},{:.1},{:.1},{:.1}]",
            phase,
            frame.scene_id.value(),
            self.host_token,
            frame.epoch.value(),
            frame.cycle_step.as_str(),
            frame.viewport.width,
            frame.viewport.height,
            frame.nodes.len(),
            frame.relations.len(),
            bar.envelope.left(),
            bar.envelope.top(),
            bar.envelope.right(),
            bar.envelope.bottom(),
            body.envelope.left(),
            body.envelope.top(),
            body.envelope.right(),
            body.envelope.bottom(),
            dock.envelope.left(),
            dock.envelope.top(),
            dock.envelope.right(),
            dock.envelope.bottom(),
            relation.state,
            relation.presence.value(),
            frame.influence_pressure,
            frame.reservation.top,
            frame.reservation.right,
            frame.reservation.bottom,
            frame.reservation.left,
        );
    }
}

fn surface_edge_anchor(edge: SurfaceEdge) -> Anchor {
    match edge {
        SurfaceEdge::Top => Anchor::TOP,
        SurfaceEdge::Right => Anchor::RIGHT,
        SurfaceEdge::Bottom => Anchor::BOTTOM,
        SurfaceEdge::Left => Anchor::LEFT,
    }
}

fn surface_edge_name(edge: SurfaceEdge) -> &'static str {
    match edge {
        SurfaceEdge::Top => "top",
        SurfaceEdge::Right => "right",
        SurfaceEdge::Bottom => "bottom",
        SurfaceEdge::Left => "left",
    }
}

fn surface_request_token(edge: SurfaceEdge, exclusive_zone: i32) -> i64 {
    let edge_code = match edge {
        SurfaceEdge::Top => 1_i64,
        SurfaceEdge::Right => 2,
        SurfaceEdge::Bottom => 3,
        SurfaceEdge::Left => 4,
    };
    edge_code * 1_000_000 + i64::from(exclusive_zone)
}

fn run_global_projection_qualification(source: CompositorIntentSource) -> Result<(), String> {
    let backend =
        HyprctlLuaBackend::from_env().map_err(|error| format!("backend_init:{error:?}"))?;

    let outputs = backend
        .observe_outputs()
        .map_err(|error| format!("observe_outputs:{error:?}"))?;
    let first_output = outputs
        .first()
        .ok_or_else(|| "observe_outputs:empty".to_owned())?;
    let output_target = RuntimeOutputIdentity {
        session_signature: backend.runtime_target().to_owned(),
        name: first_output.name.clone(),
    };
    validate_runtime_output_target(&output_target, &outputs)
        .map_err(|error| format!("output_target:{error:?}"))?;
    println!(
        "GNOSIX_RUNTIME_OUTPUT_TARGET_PASS session={} output={}",
        backend.runtime_target(),
        output_target.name
    );

    let probe = HyprlandGlobalMutation::QualificationBorderSize(0);
    let before = backend
        .observe(probe)
        .map_err(|error| format!("observe_before:{error:?}"))?;
    let requested = if before < 64 {
        before + 1
    } else {
        before.saturating_sub(1)
    };
    if requested == before {
        return Err("qualification_mutation_has_no_delta".to_owned());
    }

    let mut boundary = ProjectionBoundary::default();
    let mut receipt = boundary.apply_global(
        &backend,
        GlobalProjectionIntent {
            source,
            mutation: HyprlandGlobalMutation::QualificationBorderSize(requested),
            expected_before: Some(before),
        },
    );
    if receipt.application_outcome != ApplicationOutcome::Applied
        || receipt.observed_after != Some(requested)
    {
        return Err(format!(
            "apply_outcome={:?} before={:?} after={:?}",
            receipt.application_outcome, receipt.observed_before, receipt.observed_after
        ));
    }

    let stale_source = CompositorIntentSource {
        scene_id: source.scene_id,
        scene_epoch: SceneEpoch::new(source.scene_epoch.value().saturating_sub(1)),
    };
    let stale = boundary.apply_global(
        &backend,
        GlobalProjectionIntent {
            source: stale_source,
            mutation: HyprlandGlobalMutation::QualificationBorderSize(requested + 1),
            expected_before: Some(requested),
        },
    );
    if stale.application_outcome != ApplicationOutcome::Stale {
        return Err(format!(
            "stale_fence_outcome={:?}",
            stale.application_outcome
        ));
    }

    boundary.rollback_global(&backend, &mut receipt);
    if receipt.rollback_outcome != Some(RollbackOutcome::Restored)
        || receipt.observed_final != Some(before)
    {
        return Err(format!(
            "rollback_outcome={:?} final={:?}",
            receipt.rollback_outcome, receipt.observed_final
        ));
    }

    println!(
        "GNOSIX_HYPRLAND_GLOBAL_PROJECTION_PASS session={} scene_id={} epoch={} before={} requested={} observed_after={} stale=STALE rollback=RESTORED final={}",
        backend.runtime_target(),
        source.scene_id.value(),
        source.scene_epoch.value(),
        before,
        requested,
        receipt.observed_after.unwrap_or(requested),
        receipt.observed_final.unwrap_or(before)
    );
    Ok(())
}

impl SceneHost {
    /// Report the scene's own focus ownership, independently of any
    /// accessibility bus.
    ///
    /// The value comes from `FocusHandle::is_focused(window)`, so it observes
    /// the framework's focus state directly rather than what AccessKit chose
    /// to publish. That makes agreement with the AT-SPI observer real evidence
    /// instead of a circular read-back, and it is the only reason a
    /// `GrabFocus` returning true is not accepted as proof of focus.
    /// The product's single keyboard routing point.
    ///
    /// Traversal and activation are decided here, once, for the whole
    /// interactive surface, rather than per control. Traversal uses gpui's
    /// own tab order, which is built from the same `tab_stop` and
    /// `tab_index` the accessibility tree reflects, so the two cannot
    /// drift apart. Each key produces at most one action, so a keypress
    /// can never move focus twice.
    #[cfg(feature = "r13-production-client")]
    fn handle_scene_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        dispatch_scene_key(event, window, cx);
    }

    /// Toggle the AppStack surface.
    ///
    /// Opening the AppStack closes the workspace summary and vice versa, which
    /// is what `ProductPanelState::toggled` encodes. The transition is a
    /// replacement rather than a second flag so the two surfaces can never both
    /// be open.
    ///
    /// Gated with the Dock control that owns it: the affordance is a production
    /// Dock item, alongside the workspace controls, and a build without that
    /// Dock has no affordance to activate.
    #[cfg(feature = "r13-production-client")]
    fn toggle_appstack_panel(&mut self, cx: &mut Context<Self>) {
        self.refresh_product_runtime();
        if !self.product_runtime.semantic().can_open_appstack() {
            return;
        }
        self.product_panel = self.product_panel.toggled(ProductPanelState::AppStack);
        self.begin_appstack_motion_trace(self.product_panel.is_appstack());
        println!(
            "GNOS_UX_PRODUCT_APPSTACK panel={}",
            self.product_panel.as_str()
        );
        if let Some(perf) = &self.perf_counters {
            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
        }
        // The workspace summary animates out of the bar, so closing one surface
        // to open another is a motion the solver still has to resolve.
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// The single product action behind a AppStack row.
    ///
    /// Every route into a launch - pointer, keyboard, assistive activation -
    /// lands here. That is the whole reason this method exists rather than three
    /// call sites: a second activation path is a second chance to run the same
    /// intent twice, and the AppStack's contract is that one selection produces
    /// one receipt.
    ///
    /// The selection is carried as a `DesktopEntryId` and re-resolved against the
    /// runtime's *current* content, which preserves the content's own
    /// generation fence. Looking the entry up in whatever the catalog holds now
    /// would silently retarget a stale selection onto a newer catalog, and the
    /// capability exists precisely to refuse that.
    fn activate_installed_application(
        &mut self,
        entry: gnosix_application_catalog::DesktopEntryId,
        cx: &mut Context<Self>,
    ) {
        self.refresh_product_runtime();
        let Some(selected) = self
            .product_runtime
            .installed_applications()
            .into_iter()
            .find(|candidate| candidate.reference.entry() == &entry)
        else {
            // The entry vanished from the catalog between render and click. Not
            // a launch attempt, so there is no receipt; say so plainly rather
            // than reporting a refusal the capability never issued.
            println!(
                "GNOS_UX_PRODUCT_LAUNCH panel={} entry={} status=absent",
                self.product_panel.as_str(),
                entry.as_str()
            );
            cx.notify();
            return;
        };
        let record =
            crate::product_launch::launch_application(&mut self.product_runtime, &selected);
        let terminal = record.terminal.as_ref().map(|t| t.code()).unwrap_or("NONE");
        println!(
            "GNOS_UX_PRODUCT_LAUNCH panel={} entry={} outcome={} detail={}",
            self.product_panel.as_str(),
            record.entry.as_str(),
            terminal,
            record.detail
        );
        if let Some(perf) = &self.perf_counters {
            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
        }
        // v1 policy: a successful launch hands the desktop back to the user.
        // A refusal leaves the AppStack open so the feedback line stays visible,
        // which is the difference between "nothing happened" and an explanation.
        if record.terminal.as_ref().is_some_and(|t| t.is_launched()) {
            self.product_panel = self.product_panel.after_launch(&record);
            self.begin_appstack_motion_trace(false);
        }
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// Keep this host's focus in step with the session-wide arbitration.
    ///
    /// Called from render, which is the one place every focus source
    /// passes through: a key, a click, or an assistive `GrabFocus` all
    /// end up changing `window.focus` and repainting. Claiming here means
    /// there is no accessibility-only focus universe, and losing here
    /// means a stale handle cannot keep claiming the session.
    #[cfg(feature = "r13-production-client")]
    fn arbitrate_product_focus(
        &mut self,
        identities: &[ProductControlId],
        handles: &[gpui::FocusHandle],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row_index) = focused_appstack_row_index(identities, handles, window) {
            // GPUI moves focus through every painted tab stop, but does not
            // reveal a focused child inside a scroll container. Queue the
            // nearest-edge scroll before this frame's list prepaint.
            self.appstack_scroll.scroll_to_item(row_index);
        }
        let local = identities
            .iter()
            .zip(handles.iter())
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(id, _)| id.clone());

        match local {
            Some(control) => {
                if let Some(displaced) = self.focus_coordinator.claim(self.host_id, control) {
                    // Another output still holds focus. Blur it so the
                    // desktop has exactly one keyboard focus, and defer
                    // the call: touching another window from inside this
                    // render would be re-entrant.
                    if let Some(window) = self.host_windows.window(displaced.host) {
                        cx.defer(move |cx| {
                            // A host that has already gone away is not a
                            // failure: the invariant it would have
                            // enforced no longer applies to it.
                            let _ = window.update(cx, |_, window, cx| window.blur(cx));
                        });
                    }
                }
            }
            None => {
                // This host has no focus of its own, so it cannot be the
                // session's focus owner.
                self.focus_coordinator.release(self.host_id);
            }
        }
    }

    #[cfg(feature = "r13-production-client")]
    /// Record key events the window actually receives.
    ///
    /// Read-only and opt-in, like the focus oracle. Without it, "Enter did
    /// nothing" cannot be distinguished from "Enter never arrived", which are
    /// different defects in different layers.
    fn report_key_event(&self, event: &gpui::KeyDownEvent, window: &Window) {
        if !self.key_qualification_diagnostics {
            return;
        }
        let focused = self
            .product_focus
            .order()
            .iter()
            .enumerate()
            .find(|(_, identity)| {
                self.product_focus
                    .handle(identity)
                    .is_some_and(|handle| handle.is_focused(window))
            });
        println!(
            "GNOS_UX_SCENE_KEY_EVENT scene={} key={:?} keystroke={:?} is_focused={} focused_control={} tab_index={:?}",
            self.scene_id.value(),
            format!("{:?}", event.keystroke.key),
            format!("{:?}", event.keystroke.modifiers),
            window.is_window_active(),
            focused.map_or("NONE", |(_, identity)| identity.as_str()),
            focused.map(|(index, _)| index),
        );
    }

    #[cfg(feature = "r13-production-client")]
    fn report_product_focus(
        &mut self,
        identities: &[ProductControlId],
        handles: &[gpui::FocusHandle],
        window: &Window,
    ) {
        if !self.focus_qualification_diagnostics {
            return;
        }
        for (identity, handle) in identities.iter().zip(handles.iter()) {
            let focused = handle.is_focused(window);
            let previous = self.focus_report.get(identity.as_str()).copied();
            if previous == Some(focused) {
                continue;
            }
            self.focus_report
                .insert(identity.as_str().to_owned(), focused);
            println!(
                "GNOS_UX_SCENE_FOCUS_ORACLE scene={} id={} focused={focused}",
                self.scene_id.value(),
                identity.as_str(),
            );
        }
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
            let Some(layout) = ProductLayout::resolve_reactive_at_edge_with_policies(
                viewport,
                &mut self.product_hull,
                target_open,
                reduced_motion,
                self.product_pointer,
                self.product_bar_edge,
                self.product_text_scale.font_size_px(),
                reservation_policy,
                sensing_policy,
            ) else {
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
                let resolve_placement = |edge| {
                    ProductLayout::resolve_reactive_at_edge(
                        viewport, &mut None, false, true, None, edge, font_px,
                    )
                    .expect("valid qualified transition endpoint")
                    .bar_placement
                };
                let evaluated = match plan {
                    crate::product_bar::BarTransitionQualification::Path {
                        edges,
                        leg_duration,
                    } => {
                        let leg_index = ((elapsed.as_millis() / leg_duration.as_millis()) as usize)
                            .min(edges.len() - 2);
                        let leg_elapsed = elapsed.saturating_sub(*leg_duration * leg_index as u32);
                        let progress = gnos_ux_surface::grammar::UnitInterval::try_new(
                            (leg_elapsed.as_secs_f32() / leg_duration.as_secs_f32())
                                .clamp(0.0, 0.999_999),
                        )
                        .expect("bounded path progress");
                        gnos_ux_surface::grammar::BarPlacementTransition::new(
                            viewport,
                            resolve_placement(edges[leg_index]),
                            resolve_placement(edges[leg_index + 1]),
                        )
                        .expect("valid path endpoints")
                        .evaluate(progress)
                        .expect("path frame resolves")
                    }
                    crate::product_bar::BarTransitionQualification::Retarget {
                        source,
                        via,
                        target,
                        leg_duration,
                    } => {
                        let first_leg = gnos_ux_surface::grammar::BarPlacementTransition::new(
                            viewport,
                            resolve_placement(*source),
                            resolve_placement(*via),
                        )
                        .expect("valid retarget source endpoints");
                        let retarget_at = *leg_duration / 2;
                        if elapsed < retarget_at {
                            let progress = gnos_ux_surface::grammar::UnitInterval::try_new(
                                (elapsed.as_secs_f32() / leg_duration.as_secs_f32())
                                    .clamp(0.0, 0.5),
                            )
                            .expect("bounded pre-retarget progress");
                            first_leg
                                .evaluate(progress)
                                .expect("pre-retarget frame resolves")
                        } else {
                            let current = first_leg
                                .evaluate(gnos_ux_surface::grammar::UnitInterval::clamped(0.5))
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
                            let progress = gnos_ux_surface::grammar::UnitInterval::try_new(
                                (elapsed.saturating_sub(retarget_at).as_secs_f32()
                                    / leg_duration.as_secs_f32())
                                .clamp(0.0, 0.999_999),
                            )
                            .expect("bounded retarget progress");
                            transition
                                .evaluate(progress)
                                .expect("retarget frame resolves")
                        }
                    }
                };
                active_bar_placement = resolve_placement(evaluated.active_edge);
                transition_frame = Some(evaluated);
                self.start_motion_driver(cx);
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
            let Some(mut shared) =
                crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(
                    viewport,
                    layout.dock,
                    layout.dock,
                    self.appstack_motion,
                    layout
                        .dock_safe_bounds()
                        .expect("valid Dock available region"),
                )
                .ok()
            else {
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
            debug_assert!(frame.validate().is_valid());
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
            let input_regions = frame
                .input_regions
                .iter()
                .map(|region| Bounds {
                    origin: point(px(region.left()), px(region.top())),
                    size: gpui::Size::new(px(region.width()), px(region.height())),
                })
                .collect::<Vec<_>>();
            window.set_input_region(Some(&input_regions));

            let connector_width = layout.connector_paint_width();
            let reactive_frame = layout.reactive_frame();
            let transition_paint = transition_frame.clone();
            let paint_perf = self.perf_counters.clone();
            let canvas = canvas(
                move |bounds, _, _| (bounds, frame.clone()),
                move |bounds, (_, frame), window, _| {
                    if let Some(perf) = &paint_perf {
                        perf.paints.fetch_add(1, Ordering::Relaxed);
                    }
                    paint_product_scene_with_transition(
                        bounds,
                        &frame,
                        connector_width,
                        reactive_frame,
                        layout.shared_dock.as_ref(),
                        transition_paint.as_ref(),
                        bar_visible,
                        window,
                    )
                },
            )
            .size_full();
            let bar = crate::product_bar::content(
                &state,
                active_bar_placement,
                self.product_text_scale.font_size_px(),
            )
            .absolute()
            .left(px(layout.bar.left()))
            .top(px(layout.bar.top()))
            .w(px(layout.bar.width()))
            .h(px(layout.bar.height()))
            .id("product-bar");
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
                        if !matches!(event, gpui::ClickEvent::Keyboard(_)) {
                            let point = event.position();
                            if !bar_hit_regions
                                .iter()
                                .any(|r| r.contains_point(f32::from(point.x), f32::from(point.y)))
                            {
                                return;
                            }
                        }
                        this.refresh_product_runtime();
                        let can_open = this.product_runtime.semantic().can_open_workspace();
                        if can_open {
                            this.product_panel = this
                                .product_panel
                                .toggled(ProductPanelState::WorkspaceSummary);
                            println!("GNOS_UX_PRODUCT_WORKSPACE panel={:?}", this.product_panel);
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
            // Populated with focus-aware rows by the production block below, or
            // with inert rows in a build that has no product focus registry. The
            // AppStack surface itself is not feature-gated: it is a product
            // surface, and whether it currently has keyboard focus handles is a
            // property of the build, not of the product.
            #[allow(unused_mut, unused_assignments)]
            let mut appstack_row_elements: Vec<gpui::Stateful<gpui::Div>> = Vec::new();
            let appstack_model = if appstack_target_open || appstack_needs_motion {
                crate::product_appstack::appstack_rows(&self.product_runtime)
            } else {
                Vec::new()
            };
            let outer_origin = layout.shared_dock.map_or((0.0, 0.0), |f| {
                (f.geometry.outer.left(), f.geometry.outer.top())
            });
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
                .overflow_hidden()
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
                        .on_click(cx.listener(|this, _, _, cx| {
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
                                this.activate_installed_application(entry.clone(), cx);
                            }))
                        } else {
                            item
                        }
                    })
                    .collect();

                let dock = dock.when_some(affordance, |dock, affordance| dock.child(affordance));
                if items.is_empty() {
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
                        .aria_label("AppStack");
                    // The empty state is a `Div` and the rows are `Stateful<Div>`,
                    // so the list is heterogeneous and is collected as
                    // `AnyElement` rather than pretending both are the same type.
                    let list: Vec<gpui::AnyElement> = if appstack_row_elements.is_empty() {
                        vec![
                            crate::product_design::appstack_empty_state(
                                "AppStack · aucune application installée",
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
                    .overflow_hidden()
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
            let root = div()
                .size_full()
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        let pointer =
                            Point2::new(f32::from(event.position.x), f32::from(event.position.y));
                        let zone = pointer_zone(
                            viewport,
                            bar_pointer_placement,
                            bar_pointer_state,
                            pointer,
                        );
                        let next_pointer = (zone == BarPointerZone::Surface).then_some(pointer);
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
                        if changed || reveal_changed {
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
                    if pointer_changed || reveal_changed {
                        if let Some(perf) = &this.perf_counters {
                            perf.pointer_changes.fetch_add(1, Ordering::Relaxed);
                            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
                        }
                        this.start_motion_driver(cx);
                        cx.notify();
                    }
                }))
                .child(canvas)
                .when_some(bar_visible.then_some(bar), |root, bar| root.child(bar))
                .child(shared_content)
                .when_some(workspace, |root, workspace| root.child(workspace))
                .when_some(product_diagnostics, |root, diagnostics| {
                    root.child(diagnostics)
                });
            // Dock and AppStack are siblings. Routing on Dock alone cannot
            // receive bubbled Tab events from a focused application row.
            #[cfg(feature = "r13-production-client")]
            let root = root.on_key_down(cx.listener(|this, event, window, cx| {
                this.report_key_event(event, window);
                this.handle_scene_key(event, window, cx);
            }));
            #[cfg(feature = "r12-production-client")]
            let root = root.on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, _, cx| {
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

        let input_regions = frame
            .input_regions
            .iter()
            .filter(|region| region.width() > 0.5 && region.height() > 0.5)
            .map(|region| Bounds {
                origin: point(px(region.left()), px(region.top())),
                size: gpui::Size::new(px(region.width()), px(region.height())),
            })
            .collect::<Vec<_>>();
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

fn normalized_relation_endpoint_delta(previous: &SceneFrame, current: &SceneFrame) -> f32 {
    let previous_relation = previous.relations.first().expect("previous scene relation");
    let current_relation = current.relations.first().expect("current scene relation");

    let normalize = |point: gnos_ux_surface::grammar::Point2, viewport: SurfaceViewport| {
        (
            point.x() / viewport.width.max(1.0),
            point.y() / viewport.height.max(1.0),
        )
    };
    let distance = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);

    distance(
        normalize(previous_relation.connector.p0(), previous.viewport),
        normalize(current_relation.connector.p0(), current.viewport),
    )
    .max(distance(
        normalize(previous_relation.connector.p2(), previous.viewport),
        normalize(current_relation.connector.p2(), current.viewport),
    ))
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

fn paint_scene(bounds: Bounds<gpui::Pixels>, frame: &SceneFrame, window: &mut Window) {
    let relation = frame.relations.first().expect("scene relation");
    if relation.presence.value() > 0.005 {
        let alpha = (relation.presence.value() * 220.0)
            .round()
            .clamp(0.0, 255.0) as u32;
        let connector_color = rgba(0x315f_7900 | alpha);
        window.paint_path(
            connector_path(bounds, relation.connector, 22.0),
            connector_color,
        );
    }

    for node in &frame.nodes {
        let fill = match node.role {
            SceneNodeRole::Bar => rgba(0x17374bff),
            SceneNodeRole::Body => rgba(0x315f79ff),
            SceneNodeRole::Dock => rgba(0x315f79ff),
            // Inherits the Dock's material, which is the lineage the Envelope
            // model asks for: the derived body is the same material family as
            // its parent, not a new one.
            SceneNodeRole::AppStack => rgba(0x315f79ff),
            SceneNodeRole::Notification => rgba(0x456b7fff),
            SceneNodeRole::Overlay => rgba(0x5c7380ff),
            SceneNodeRole::Plugin => rgba(0x6d7e87ff),
        };
        if node.envelope.width() > 0.01 && node.envelope.height() > 0.01 {
            window.paint_path(rounded_rect_path(bounds, node.envelope), fill);
        }
    }
}

fn connector_path(
    bounds: Bounds<gpui::Pixels>,
    pair: gnos_ux_surface::grammar::CubicJoinPair,
    width: f32,
) -> gpui::Path<gpui::Pixels> {
    let mut path = PathBuilder::stroke(px(width));
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |value: gnos_ux_surface::grammar::Point2| point(px(ox + value.x()), px(oy + value.y()));

    path.move_to(p(pair.p0()));
    path.cubic_bezier_to(p(pair.p1()), p(pair.c1()), p(pair.c2()));
    path.cubic_bezier_to(p(pair.p2()), p(pair.c3()), p(pair.c4()));
    path.build().expect("building scene connector")
}

fn rounded_rect_path(bounds: Bounds<gpui::Pixels>, rect: GeometryRect) -> gpui::Path<gpui::Pixels> {
    crate::product_hull::rounded_rect_path(bounds, rect)
}

fn wayland_runtime_name_key(name: &str) -> Uuid {
    // The pinned GPUI Wayland backend derives PlatformDisplay::uuid() from the
    // current wl_output name. SDK7 uses that implementation detail only as an
    // in-session bridge from an observed Hyprland output name to DisplayId.
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, name.as_bytes())
}

fn runtime_display_id_for_output_name(cx: &App, name: &str) -> Result<DisplayId, String> {
    let expected = wayland_runtime_name_key(name);
    let matches = cx
        .displays()
        .into_iter()
        .filter_map(|display| {
            display
                .uuid()
                .ok()
                .filter(|uuid| *uuid == expected)
                .map(|_| display.id())
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(format!("missing_gpui_display:{name}")),
        [display_id] => Ok(*display_id),
        _ => Err(format!("ambiguous_gpui_display:{name}")),
    }
}

/// The focus scope shared by every output host of one session.
///
/// Created by the session that opens the hosts and cloned into each of
/// them, so the whole product shares one arbitration point without any
/// global mutable state. A single-output session simply creates its own.
#[derive(Clone, Default)]
struct ProductFocusScope {
    coordinator: ProductFocusCoordinator,
    windows: ProductHostWindows,
}

impl ProductFocusScope {
    fn new() -> Self {
        Self::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn open_scene_window(
    cx: &mut App,
    scene_id: MonitorSceneId,
    runtime_output: Option<RuntimeId>,
    display_id: Option<DisplayId>,
    semantic_consumer: SceneSemanticConsumer,
    application_catalog: Arc<ApplicationCatalog>,
    product_focus: Option<ProductFocusScope>,
) -> Result<AnyWindowHandle, String> {
    let plan_size = gpui::Size::new(px(0.0), px(0.0));
    let runtime_output_for_host = runtime_output.clone();
    let semantic_consumer_for_host = semantic_consumer.clone();
    let application_catalog_for_host = Arc::clone(&application_catalog);
    let product_focus_for_host = product_focus.clone();
    let product_focus_registrar = product_focus;
    cx.open_window(
        WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: plan_size,
            })),
            display_id,
            app_id: Some(SCENE_IDENTITY.to_owned()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: SCENE_NAMESPACE.to_owned(),
                layer: Layer::Top,
                anchor: Anchor::TOP | Anchor::RIGHT | Anchor::BOTTOM | Anchor::LEFT,
                exclusive_zone: None,
                keyboard_interactivity: PRODUCT_SCENE_KEYBOARD_INTERACTIVITY,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            let runtime_output = runtime_output_for_host.clone();
            let semantic_consumer = semantic_consumer_for_host.clone();
            let application_catalog = Arc::clone(&application_catalog_for_host);
            let (coordinator, host_windows) = product_focus_for_host
                .clone()
                .map(|scope| (scope.coordinator, scope.windows))
                .unwrap_or_else(|| (ProductFocusCoordinator::new(), ProductHostWindows::new()));
            cx.new(|cx| {
                SceneHost::new(
                    scene_id,
                    runtime_output,
                    semantic_consumer,
                    application_catalog,
                    coordinator,
                    host_windows,
                    cx,
                )
            })
        },
    )
    .map(Into::into)
    .map_err(|error| format!("open_scene_window:{error}"))
    .inspect(|handle: &AnyWindowHandle| {
        if let Some(scope) = &product_focus_registrar {
            scope
                .windows
                .register(ProductHostId(scene_id.value()), *handle);
        }
    })
}

fn open_multi_output_scene_windows(
    cx: &mut App,
    application_catalog: Arc<ApplicationCatalog>,
) -> Result<usize, String> {
    // One focus scope for the whole session: the desktop has one keyboard
    // focus, so the product keeps one.
    let product_focus = ProductFocusScope::new();
    let backend =
        HyprctlLuaBackend::from_env().map_err(|error| format!("backend_init:{error:?}"))?;
    let mut outputs = backend
        .observe_outputs()
        .map_err(|error| format!("observe_outputs:{error:?}"))?;
    outputs.sort_by(|left, right| left.name.cmp(&right.name));

    let gpui_displays = cx.displays();
    for display in &gpui_displays {
        println!(
            "GNOS_UX_GPUI_DISPLAY_OBSERVED id={:?} uuid={} bounds={:?}",
            display.id(),
            display
                .uuid()
                .map(|uuid| uuid.to_string())
                .unwrap_or_else(|_| "UNAVAILABLE".to_owned()),
            display.bounds()
        );
    }

    let mut routes = Vec::with_capacity(outputs.len());
    for (index, observed) in outputs.iter().enumerate() {
        let target = RuntimeOutputIdentity {
            session_signature: backend.runtime_target().to_owned(),
            name: observed.name.clone(),
        };
        validate_runtime_output_target(&target, &outputs)
            .map_err(|error| format!("runtime_target:{}:{error:?}", target.name))?;
        let display_id = runtime_display_id_for_output_name(cx, &target.name)?;
        routes.push((
            MonitorSceneId::new((index + 1) as u64),
            RuntimeId::new(target.name),
            display_id,
        ));
    }

    if routes.is_empty() {
        return Err("no_outputs".to_owned());
    }

    let semantic_consumer = SceneSemanticConsumer::default();
    for (scene_id, output, display_id) in routes.iter().cloned() {
        println!(
            "GNOS_UX_MULTI_OUTPUT_BINDING_REQUEST session={} scene_id={} output={} display_id={:?}",
            backend.runtime_target(),
            scene_id.value(),
            output.as_str(),
            display_id
        );
        open_scene_window(
            cx,
            scene_id,
            Some(output.clone()),
            Some(display_id),
            semantic_consumer.clone(),
            Arc::clone(&application_catalog),
            Some(product_focus.clone()),
        )
        .map_err(|error| format!("output={}:{}", output.as_str(), error))?;
    }

    println!(
        "GNOS_UX_MULTI_OUTPUT_HOSTS_READY session={} count={}",
        backend.runtime_target(),
        routes.len()
    );
    Ok(routes.len())
}

#[derive(Clone)]
struct DynamicSceneRoute {
    scene_id: MonitorSceneId,
    output: RuntimeId,
    display_id: DisplayId,
    window: AnyWindowHandle,
    observed: ObservedRuntimeOutput,
}

#[derive(Debug)]
enum DynamicTopologyError {
    Observe(String),
    MappingPending(String),
    Plan(String),
    Open(String),
    RetirePending(String),
    Commit(String),
    Invariant(String),
}

impl DynamicTopologyError {
    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Observe(_) | Self::MappingPending(_) | Self::Open(_) | Self::RetirePending(_)
        )
    }

    fn message(&self) -> &str {
        match self {
            Self::Observe(message)
            | Self::MappingPending(message)
            | Self::Plan(message)
            | Self::Open(message)
            | Self::RetirePending(message)
            | Self::Commit(message)
            | Self::Invariant(message) => message,
        }
    }
}

#[derive(Debug)]
struct DynamicReconcileReceipt {
    generation: u64,
    hint_count: usize,
    added: usize,
    removed: usize,
    preserved: usize,
    reconfigured: usize,
    reattached: usize,
    route_count: usize,
    pending_retired: usize,
    post_snapshot_drift: bool,
    rebuilt_after_event_gap: bool,
}

struct PendingTopologyTransaction {
    plan: SessionTopologyPlan,
    next_routes: BTreeMap<String, DynamicSceneRoute>,
    retire_pending: Vec<AnyWindowHandle>,
    next_scene_id: u64,
    hint_count: usize,
    rebuilt_after_event_gap: bool,
    semantics_invalidated: bool,
    committed: bool,
}

trait RouteWindowOperations<R> {
    fn open(&mut self, output: &ObservedRuntimeOutput) -> Result<R, String>;
    fn close(&mut self, route: &R) -> Result<(), String>;
}

#[derive(Debug)]
struct RouteStageFailure<R> {
    output: String,
    error: String,
    rollback_pending: Vec<R>,
}

#[derive(Debug)]
enum TopologyStageError<R> {
    SemanticInvalidation(String),
    WindowStage(RouteStageFailure<R>),
}

fn prepare_topology_stage<R>(
    plan: &SessionTopologyPlan,
    mut invalidate: impl FnMut(&SessionTopologyPlan) -> Result<(), String>,
    operations: &mut impl RouteWindowOperations<R>,
) -> Result<BTreeMap<String, R>, TopologyStageError<R>> {
    if plan.changed() {
        invalidate(plan).map_err(TopologyStageError::SemanticInvalidation)?;
    }
    stage_topology_windows(&plan.actions, operations).map_err(TopologyStageError::WindowStage)
}

fn commit_pending_routes<R>(
    reconciler: &mut SessionTopologyReconciler,
    plan: &SessionTopologyPlan,
    current: &mut BTreeMap<String, R>,
    staged: &mut BTreeMap<String, R>,
) -> Result<(), gnosix_hyprland::topology::TopologyReconcileError> {
    reconciler.commit(plan)?;
    *current = std::mem::take(staged);
    Ok(())
}

struct ShutdownReport<H, E> {
    attempted: usize,
    errors: Vec<E>,
    close_pending: Vec<H>,
}

fn stage_topology_windows<R>(
    actions: &[TopologyAction],
    operations: &mut impl RouteWindowOperations<R>,
) -> Result<BTreeMap<String, R>, RouteStageFailure<R>> {
    let mut staged = BTreeMap::new();
    for action in actions {
        let current = match action {
            TopologyAction::Add { current } | TopologyAction::Reattach { current, .. } => current,
            _ => continue,
        };
        match operations.open(current) {
            Ok(route) => {
                staged.insert(current.name.clone(), route);
            }
            Err(error) => {
                let mut rollback_pending = Vec::new();
                for route in staged.into_values() {
                    if operations.close(&route).is_err() {
                        rollback_pending.push(route);
                    }
                }
                return Err(RouteStageFailure {
                    output: current.name.clone(),
                    error,
                    rollback_pending,
                });
            }
        }
    }
    Ok(staged)
}

struct GpuiRouteWindowOperations<'a> {
    cx: &'a mut App,
    display_ids: &'a BTreeMap<String, DisplayId>,
    semantic_consumer: SceneSemanticConsumer,
    application_catalog: Arc<ApplicationCatalog>,
    next_scene_id: u64,
    product_focus: ProductFocusScope,
}

impl RouteWindowOperations<DynamicSceneRoute> for GpuiRouteWindowOperations<'_> {
    fn open(&mut self, current: &ObservedRuntimeOutput) -> Result<DynamicSceneRoute, String> {
        let display_id = *self
            .display_ids
            .get(&current.name)
            .ok_or_else(|| format!("missing_prevalidated_display:{}", current.name))?;
        let scene_id = MonitorSceneId::new(self.next_scene_id);
        self.next_scene_id = self
            .next_scene_id
            .checked_add(1)
            .ok_or_else(|| "scene_id_exhausted".to_owned())?;
        let output = RuntimeId::new(current.name.clone());
        let window = open_scene_window(
            self.cx,
            scene_id,
            Some(output.clone()),
            Some(display_id),
            self.semantic_consumer.clone(),
            Arc::clone(&self.application_catalog),
            Some(self.product_focus.clone()),
        )?;
        Ok(DynamicSceneRoute {
            scene_id,
            output,
            display_id,
            window,
            observed: current.clone(),
        })
    }

    fn close(&mut self, route: &DynamicSceneRoute) -> Result<(), String> {
        close_scene_window(self.cx, route.window)
    }
}

#[derive(Debug, Default)]
struct TopologyEventContinuity {
    connected: bool,
    lost_since_rebuild: bool,
}

impl TopologyEventContinuity {
    fn observe(&mut self, batch: &TopologyWakeBatch) {
        if batch.stream_disconnected {
            self.lost_since_rebuild = true;
        }
        if let Some(connected) = batch.last_stream_state {
            self.connected = connected;
        }
    }

    fn rebuild_required(&self) -> bool {
        self.connected && self.lost_since_rebuild
    }

    fn authoritative_rebuild_committed(&mut self) {
        if self.rebuild_required() {
            self.lost_since_rebuild = false;
        }
    }

    fn state(&self, disabled: bool) -> EventStreamState {
        if disabled {
            EventStreamState::Disabled
        } else if self.connected {
            EventStreamState::Connected
        } else if self.lost_since_rebuild {
            EventStreamState::Disconnected
        } else {
            EventStreamState::Degraded
        }
    }
}

struct DynamicTopologyRuntime {
    reconciler: SessionTopologyReconciler,
    routes: BTreeMap<String, DynamicSceneRoute>,
    pending: Option<PendingTopologyTransaction>,
    cleanup_pending: Vec<AnyWindowHandle>,
    next_scene_id: u64,
    event_continuity: TopologyEventContinuity,
    semantic_consumer: SceneSemanticConsumer,
    application_catalog: Arc<ApplicationCatalog>,
    /// Shared by every host this runtime opens, so focus stays exclusive
    /// across outputs for the life of the session.
    product_focus: ProductFocusScope,
}

impl Default for DynamicTopologyRuntime {
    fn default() -> Self {
        Self {
            reconciler: SessionTopologyReconciler::default(),
            routes: BTreeMap::new(),
            pending: None,
            cleanup_pending: Vec::new(),
            next_scene_id: 1,
            event_continuity: TopologyEventContinuity::default(),
            semantic_consumer: SceneSemanticConsumer::default(),
            application_catalog: Arc::new(ApplicationCatalog::empty()),
            product_focus: ProductFocusScope::new(),
        }
    }
}

impl DynamicTopologyRuntime {
    fn reconcile(
        &mut self,
        cx: &mut App,
        backend: &HyprctlLuaBackend,
        batch: &TopologyWakeBatch,
    ) -> Result<DynamicReconcileReceipt, DynamicTopologyError> {
        if !self.cleanup_pending.is_empty() {
            self.cleanup_pending =
                retain_failed_closes(std::mem::take(&mut self.cleanup_pending), |handle| {
                    close_scene_window(cx, handle)
                });
            if !self.cleanup_pending.is_empty() {
                return Err(DynamicTopologyError::RetirePending(format!(
                    "rollback_windows_pending={}",
                    self.cleanup_pending.len()
                )));
            }
        }

        self.event_continuity.observe(batch);
        let rebuild_after_event_gap = self.event_continuity.rebuild_required();

        if self.pending.is_some() {
            let mut receipt = self.complete_pending_transaction(cx)?;
            if receipt.rebuilt_after_event_gap {
                self.event_continuity.authoritative_rebuild_committed();
            }

            let post = backend
                .observe_outputs()
                .map_err(|error| DynamicTopologyError::Observe(format!("{error:?}")))?;
            receipt.post_snapshot_drift = self
                .invalidate_semantics_on_snapshot_drift(
                    &self.reconciler.current_outputs(),
                    &post,
                    self.event_continuity.rebuild_required(),
                )
                .map_err(DynamicTopologyError::Invariant)?;
            return Ok(receipt);
        }

        let observed = backend
            .observe_outputs()
            .map_err(|error| DynamicTopologyError::Observe(format!("{error:?}")))?;
        let display_ids = resolve_current_display_ids(cx, backend, &observed)?;

        let mut force_reattach = batch.force_reattach_names.clone();
        for (name, route) in &self.routes {
            if display_ids
                .get(name)
                .is_some_and(|display_id| *display_id != route.display_id)
            {
                force_reattach.insert(name.clone());
            }
        }

        let plan = self
            .reconciler
            .plan(
                backend.runtime_target(),
                observed,
                &force_reattach,
                rebuild_after_event_gap,
            )
            .map_err(|error| DynamicTopologyError::Plan(format!("{error:?}")))?;

        let mut receipt = self.apply_plan(
            cx,
            &plan,
            &display_ids,
            batch.hint_count,
            rebuild_after_event_gap,
        )?;

        if receipt.rebuilt_after_event_gap {
            self.event_continuity.authoritative_rebuild_committed();
        }

        let post = backend
            .observe_outputs()
            .map_err(|error| DynamicTopologyError::Observe(format!("{error:?}")))?;
        receipt.post_snapshot_drift = self
            .invalidate_semantics_on_snapshot_drift(
                &self.reconciler.current_outputs(),
                &post,
                false,
            )
            .map_err(DynamicTopologyError::Invariant)?;

        Ok(receipt)
    }

    fn apply_plan(
        &mut self,
        cx: &mut App,
        plan: &SessionTopologyPlan,
        display_ids: &BTreeMap<String, DisplayId>,
        hint_count: usize,
        rebuilt_after_event_gap: bool,
    ) -> Result<DynamicReconcileReceipt, DynamicTopologyError> {
        if self.pending.is_some() {
            return Err(DynamicTopologyError::Invariant(
                "new_plan_while_transaction_pending".to_owned(),
            ));
        }

        let mut window_operations = GpuiRouteWindowOperations {
            cx,
            display_ids,
            semantic_consumer: self.semantic_consumer.clone(),
            application_catalog: Arc::clone(&self.application_catalog),
            next_scene_id: self.next_scene_id,
            product_focus: self.product_focus.clone(),
        };
        let mut staged = match prepare_topology_stage(
            plan,
            |plan| self.invalidate_semantics_for_topology_plan(plan),
            &mut window_operations,
        ) {
            Ok(staged) => staged,
            Err(TopologyStageError::SemanticInvalidation(error)) => {
                return Err(DynamicTopologyError::Invariant(error));
            }
            Err(TopologyStageError::WindowStage(failure)) => {
                self.next_scene_id = window_operations.next_scene_id;
                self.cleanup_pending.extend(
                    failure
                        .rollback_pending
                        .into_iter()
                        .map(|route| route.window),
                );
                return Err(DynamicTopologyError::Open(format!(
                    "output={}:{}; rollback_windows_pending={}",
                    failure.output,
                    failure.error,
                    self.cleanup_pending.len()
                )));
            }
        };
        let candidate_next_scene_id = window_operations.next_scene_id;
        self.next_scene_id = candidate_next_scene_id;
        let newly_opened = staged
            .values()
            .map(|route| route.window)
            .collect::<Vec<_>>();
        drop(window_operations);

        let route_build = (|| {
            let mut next_routes = BTreeMap::new();
            let mut retire_pending = Vec::new();
            for action in &plan.actions {
                match action {
                    TopologyAction::Add { current } => {
                        let route = staged
                            .remove(&current.name)
                            .ok_or_else(|| format!("missing_staged_add:{}", current.name))?;
                        next_routes.insert(current.name.clone(), route);
                    }
                    TopologyAction::Remove { previous } => {
                        if let Some(route) = self.routes.get(&previous.name) {
                            retire_pending.push(route.window);
                        }
                    }
                    TopologyAction::Reattach { previous, current } => {
                        let route = staged
                            .remove(&current.name)
                            .ok_or_else(|| format!("missing_staged_reattach:{}", current.name))?;
                        if let Some(old) = self.routes.get(&previous.name) {
                            retire_pending.push(old.window);
                        }
                        next_routes.insert(current.name.clone(), route);
                    }
                    TopologyAction::Preserve { current }
                    | TopologyAction::Reconfigure { current, .. } => {
                        let mut route =
                            self.routes.get(&current.name).cloned().ok_or_else(|| {
                                format!("missing_preserved_route:{}", current.name)
                            })?;
                        route.observed = current.clone();
                        next_routes.insert(current.name.clone(), route);
                    }
                }
            }
            Ok::<_, String>((next_routes, retire_pending))
        })();
        let (next_routes, retire_pending) = match route_build {
            Ok(routes) => routes,
            Err(error) => {
                self.cleanup_pending
                    .extend(retain_failed_closes(newly_opened, |handle| {
                        close_scene_window(cx, handle)
                    }));
                return Err(DynamicTopologyError::Invariant(format!(
                    "route_build_failed:{error}; rollback_windows_pending={}",
                    self.cleanup_pending.len()
                )));
            }
        };

        if next_routes.len() != plan.target_outputs.len() || !staged.is_empty() {
            self.cleanup_pending
                .extend(retain_failed_closes(newly_opened, |handle| {
                    close_scene_window(cx, handle)
                }));
            return Err(DynamicTopologyError::Invariant(format!(
                "route_build_incomplete target={} built={} unstaged={}",
                plan.target_outputs.len(),
                next_routes.len(),
                staged.len()
            )));
        }

        println!(
            "GNOS_UX_TOPOLOGY_TRANSACTION_PREPARED base_generation={} target_generation={} new_windows={} retire_windows={} route_count={} rebuild_after_event_gap={}",
            plan.base_generation,
            plan.target_generation,
            newly_opened.len(),
            retire_pending.len(),
            next_routes.len(),
            rebuilt_after_event_gap,
        );

        self.pending = Some(PendingTopologyTransaction {
            plan: plan.clone(),
            next_routes,
            retire_pending,
            next_scene_id: candidate_next_scene_id,
            hint_count,
            rebuilt_after_event_gap,
            semantics_invalidated: plan.changed(),
            committed: false,
        });

        self.complete_pending_transaction(cx)
    }

    fn complete_pending_transaction(
        &mut self,
        cx: &mut App,
    ) -> Result<DynamicReconcileReceipt, DynamicTopologyError> {
        let mut pending = self.pending.take().ok_or_else(|| {
            DynamicTopologyError::Invariant("missing_pending_transaction".to_owned())
        })?;

        if !pending.semantics_invalidated {
            if let Err(error) = self.invalidate_semantics_for_topology_plan(&pending.plan) {
                self.pending = Some(pending);
                return Err(DynamicTopologyError::Invariant(error));
            }
            pending.semantics_invalidated = true;
        }

        let (added, removed, preserved, reconfigured, reattached) =
            topology_action_counts(&pending.plan.actions);
        let route_mutation_count = pending.plan.route_mutation_count();

        if !pending.committed {
            if let Err(error) = commit_pending_routes(
                &mut self.reconciler,
                &pending.plan,
                &mut self.routes,
                &mut pending.next_routes,
            ) {
                self.pending = Some(pending);
                return Err(DynamicTopologyError::Commit(format!("{error:?}")));
            }
            pending.committed = true;
            self.next_scene_id = pending.next_scene_id;
        }

        let still_pending = retain_failed_closes(pending.retire_pending.drain(..), |handle| {
            close_scene_window(cx, handle)
        });

        if !still_pending.is_empty() {
            let count = still_pending.len();
            pending.retire_pending = still_pending;
            self.pending = Some(pending);
            eprintln!("GNOS_UX_TOPOLOGY_TRANSACTION_PENDING phase=RETIRE remaining={count}");
            return Err(DynamicTopologyError::RetirePending(format!(
                "retire_remaining={count}"
            )));
        }

        println!(
            "GNOS_UX_TOPOLOGY_RECONCILE generation={} hints={} added={} removed={} preserved={} reconfigured={} reattached={} routes={} route_mutations={} pending_retired=0",
            pending.plan.target_generation,
            pending.hint_count,
            added,
            removed,
            preserved,
            reconfigured,
            reattached,
            self.routes.len(),
            route_mutation_count,
        );
        println!(
            "GNOS_UX_TOPOLOGY_TRANSACTION_COMMIT generation={} routes={} rebuild_after_event_gap={}",
            pending.plan.target_generation,
            self.routes.len(),
            pending.rebuilt_after_event_gap,
        );
        self.log_routes();

        Ok(DynamicReconcileReceipt {
            generation: pending.plan.target_generation,
            hint_count: pending.hint_count,
            added,
            removed,
            preserved,
            reconfigured,
            reattached,
            route_count: self.routes.len(),
            pending_retired: 0,
            post_snapshot_drift: false,
            rebuilt_after_event_gap: pending.rebuilt_after_event_gap,
        })
    }

    fn clear_consumer_projections(&self, reason: &str) -> Result<(), String> {
        if !self.semantic_consumer.is_enabled() {
            return Ok(());
        }
        self.semantic_consumer.clear()?;
        println!(
            "GNOS_UX_DSC01_CONSUMER_INVALIDATE reason={reason} routes={}",
            self.routes.len()
        );
        Ok(())
    }

    fn invalidate_semantics_on_snapshot_drift(
        &self,
        expected: &[ObservedRuntimeOutput],
        observed: &[ObservedRuntimeOutput],
        continuity_gap: bool,
    ) -> Result<bool, String> {
        let drift = continuity_gap || !same_snapshot(expected, observed);
        if drift {
            self.clear_consumer_projections("TOPOLOGY_POST_SNAPSHOT_DRIFT")?;
        }
        Ok(drift)
    }

    fn invalidate_semantics_for_topology_plan(
        &self,
        plan: &SessionTopologyPlan,
    ) -> Result<(), String> {
        if self.semantic_consumer.is_enabled() && plan.changed() {
            self.semantic_consumer.clear()?;
            println!(
                "GNOS_UX_DSC01_CONSUMER_INVALIDATE reason=TOPOLOGY_MUTATION generation={}",
                plan.target_generation
            );
        }
        Ok(())
    }

    fn publish_consumer_projections(
        &self,
        projections: &[DesktopOutputProjection],
        semantic_generation: u64,
    ) -> Result<(), String> {
        if !self.semantic_consumer.is_enabled() {
            return Ok(());
        }
        let route_connectors = self
            .routes
            .keys()
            .map(|connector| {
                ConnectorName::new(connector.clone())
                    .map_err(|_| format!("empty_route_connector:{connector}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let paired = pair_projections_by_connector(
            route_connectors,
            projections,
            DesktopContextGeneration(semantic_generation),
        )?;
        self.semantic_consumer.publish(paired.clone())?;
        for (connector, projection) in paired {
            let route = self
                .routes
                .get(connector.as_str())
                .ok_or_else(|| format!("published_route_missing:{}", connector.as_str()))?;
            println!(
                "GNOS_UX_DSC01_CONSUMER_PUBLISH connector={} output_hub_id={} semantic_generation={} scene_id={} display_id={:?} workspaces={} surfaces={}",
                connector.as_str(),
                hex_bytes(projection.output.output_hub_id.as_bytes()),
                projection.generation.0,
                route.scene_id.value(),
                route.display_id,
                projection.workspaces.len(),
                projection.surfaces.len()
            );
            println!(
                "GNOS_UX_DSC01_CONSUMER_CONTEXT connector={} context={}",
                connector.as_str(),
                desktop_consumer_context_text(Some(&projection))
            );
        }
        Ok(())
    }

    fn shutdown(&mut self, cx: &mut App) -> Result<usize, String> {
        let mut handles = self
            .routes
            .values()
            .map(|route| route.window)
            .collect::<Vec<_>>();

        if let Some(pending) = self.pending.take() {
            handles.extend(pending.next_routes.values().map(|route| route.window));
            handles.extend(pending.retire_pending);
        }
        handles.extend(std::mem::take(&mut self.cleanup_pending));

        let semantic_consumer = self.semantic_consumer.clone();
        let report = shutdown_owned_windows(
            handles,
            || semantic_consumer.clear(),
            |handle| close_scene_window(cx, handle),
        );
        self.routes.clear();
        self.cleanup_pending = report.close_pending;

        if report.errors.is_empty() {
            println!(
                "GNOS_UX_LIFECYCLE_TEARDOWN owned_windows_attempted={} remaining_routes=0",
                report.attempted
            );
            Ok(report.attempted)
        } else {
            Err(format!(
                "teardown_cleanup_failures={} details={}",
                report.errors.len(),
                report
                    .errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("|")
            ))
        }
    }

    fn log_routes(&self) {
        for route in self.routes.values() {
            println!(
                "GNOS_UX_TOPOLOGY_ROUTE output={} runtime_monitor_id={} scene_id={} host_token={} display_id={:?} geometry={}x{}+{}+{} refresh_millihz={} scale_milli={} transform={}",
                route.output.as_str(),
                route.observed.runtime_monitor_id,
                route.scene_id.value(),
                scene_host_token(route.scene_id),
                route.display_id,
                route.observed.width,
                route.observed.height,
                route.observed.x,
                route.observed.y,
                route.observed.refresh_millihz,
                route.observed.scale_milli,
                route.observed.transform,
            );
        }
    }
}

fn retain_failed_closes<T: Copy, E>(
    handles: impl IntoIterator<Item = T>,
    mut close: impl FnMut(T) -> Result<(), E>,
) -> Vec<T> {
    handles
        .into_iter()
        .filter(|handle| close(*handle).is_err())
        .collect()
}

fn shutdown_owned_windows<H, E>(
    handles: impl IntoIterator<Item = H>,
    clear_semantics: impl FnOnce() -> Result<(), E>,
    mut close: impl FnMut(H) -> Result<(), E>,
) -> ShutdownReport<H, E>
where
    H: Copy + Eq + std::hash::Hash,
{
    let mut errors = Vec::new();
    if let Err(error) = clear_semantics() {
        errors.push(error);
    }

    let mut seen = HashSet::new();
    let mut attempted = 0;
    let mut close_pending = Vec::new();
    for handle in handles {
        if !seen.insert(handle) {
            continue;
        }
        attempted += 1;
        if let Err(error) = close(handle) {
            errors.push(error);
            close_pending.push(handle);
        }
    }
    ShutdownReport {
        attempted,
        errors,
        close_pending,
    }
}

fn scene_host_token(scene_id: MonitorSceneId) -> u64 {
    (u64::from(std::process::id()) << 32) | scene_id.value()
}

fn close_scene_window(cx: &mut App, handle: AnyWindowHandle) -> Result<(), String> {
    if !cx.windows().contains(&handle) {
        return Ok(());
    }
    handle
        .update(cx, |_, window, _| window.remove_window())
        .map_err(|error| format!("close_scene_window:{error}"))
}

fn resolve_current_display_ids(
    cx: &App,
    backend: &HyprctlLuaBackend,
    observed: &[ObservedRuntimeOutput],
) -> Result<BTreeMap<String, DisplayId>, DynamicTopologyError> {
    let mut resolved = BTreeMap::new();
    for output in observed {
        let target = RuntimeOutputIdentity {
            session_signature: backend.runtime_target().to_owned(),
            name: output.name.clone(),
        };
        validate_runtime_output_target(&target, observed).map_err(|error| {
            DynamicTopologyError::Plan(format!("runtime_target:{}:{error:?}", output.name))
        })?;
        let display_id = runtime_display_id_for_output_name(cx, &output.name).map_err(|error| {
            if error.starts_with("missing_gpui_display:") {
                DynamicTopologyError::MappingPending(error)
            } else {
                DynamicTopologyError::Plan(error)
            }
        })?;
        resolved.insert(output.name.clone(), display_id);
    }
    Ok(resolved)
}

fn same_snapshot(expected: &[ObservedRuntimeOutput], observed: &[ObservedRuntimeOutput]) -> bool {
    let mut expected = expected.to_vec();
    let mut observed = observed.to_vec();
    expected.sort_by(|left, right| left.name.cmp(&right.name));
    observed.sort_by(|left, right| left.name.cmp(&right.name));
    expected == observed
}

fn topology_action_counts(actions: &[TopologyAction]) -> (usize, usize, usize, usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    let mut preserved = 0;
    let mut reconfigured = 0;
    let mut reattached = 0;
    for action in actions {
        match action {
            TopologyAction::Add { .. } => added += 1,
            TopologyAction::Remove { .. } => removed += 1,
            TopologyAction::Preserve { .. } => preserved += 1,
            TopologyAction::Reconfigure { .. } => reconfigured += 1,
            TopologyAction::Reattach { .. } => reattached += 1,
        }
    }
    (added, removed, preserved, reconfigured, reattached)
}

fn semantic_error_retryable(error: SemanticObservationError) -> bool {
    !matches!(
        error,
        SemanticObservationError::UnsupportedTransform
            | SemanticObservationError::GenerationExhausted
    )
}

fn semantic_wake_reason(batch: &TopologyWakeBatch) -> String {
    if !batch.semantic_events.is_empty() {
        format!(
            "semantic_event:{}",
            batch
                .semantic_events
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        )
    } else if batch.startup {
        "startup".to_owned()
    } else if batch.safety_resync {
        "safety_resync".to_owned()
    } else if !batch.added_names.is_empty() || !batch.removed_names.is_empty() {
        "output_topology".to_owned()
    } else if batch.config_reloaded {
        "config_reloaded".to_owned()
    } else {
        "runtime_event".to_owned()
    }
}

fn emit_semantic_receipt(receipt: &DesktopSemanticReceipt, batch: &TopologyWakeBatch) {
    println!(
        "GNOSIX_DESKTOP_SEMANTIC_RECEIPT generation={} changed={} outputs={} workspaces={} surfaces={} visible_workspaces={} visible_surfaces={} focused_workspaces={} focused_surfaces={} digest_sha256={} wake={} hints={}",
        receipt.generation,
        receipt.changed,
        receipt.output_count,
        receipt.workspace_count,
        receipt.surface_count,
        receipt.visible_workspace_count,
        receipt.visible_surface_count,
        receipt.focused_workspace_count,
        receipt.focused_surface_count,
        receipt.digest_sha256,
        semantic_wake_reason(batch),
        batch.hint_count,
    );
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn emit_semantic_output_views(views: &[DesktopOutputProjection]) {
    for view in views {
        let applications = view
            .surfaces
            .iter()
            .filter(|surface| surface.role == DesktopSurfaceRole::Application)
            .count();
        let non_application = view.surfaces.len() - applications;
        let visible_workspaces = view
            .workspaces
            .iter()
            .filter(|workspace| workspace.visibility == WorkspaceVisibility::Visible)
            .count();
        let visible_surfaces = view
            .surfaces
            .iter()
            .filter(|surface| surface.visibility == SurfaceVisibility::Visible)
            .count();
        let focused_workspaces = view
            .workspaces
            .iter()
            .filter(|workspace| workspace.focus == WorkspaceFocus::Focused)
            .count();
        let focused_surfaces = view
            .surfaces
            .iter()
            .filter(|surface| surface.focus == SurfaceFocus::Focused)
            .count();

        println!(
            "GNOSIX_DESKTOP_OUTPUT_VIEW generation={} output_hub_id={} connector={} connector_hex={} workspaces={} surfaces={} applications={} non_application={} visible_workspaces={} visible_surfaces={} focused_workspaces={} focused_surfaces={}",
            view.generation.0,
            hex_bytes(view.output.output_hub_id.as_bytes()),
            view.output.connector.as_str(),
            hex_bytes(view.output.connector.as_str().as_bytes()),
            view.workspaces.len(),
            view.surfaces.len(),
            applications,
            non_application,
            visible_workspaces,
            visible_surfaces,
            focused_workspaces,
            focused_surfaces,
        );
    }
}

fn topology_event_stream_state(
    runtime: &DynamicTopologyRuntime,
    event_stream_disabled: bool,
) -> EventStreamState {
    runtime.event_continuity.state(event_stream_disabled)
}

#[derive(Clone)]
struct DynamicTopologyConfig {
    event_stream_disabled: bool,
    semantic_consumer_enabled: bool,
    safety_resync_interval: Duration,
    application_catalog: Arc<ApplicationCatalog>,
}

fn start_dynamic_topology(
    cx: &mut App,
    config: DynamicTopologyConfig,
    lifecycle: SharedLifecycle,
    mut shutdown_signals: ShutdownSignals,
    exit_code: Arc<AtomicU8>,
) {
    let DynamicTopologyConfig {
        event_stream_disabled,
        semantic_consumer_enabled,
        safety_resync_interval,
        application_catalog,
    } = config;
    if let Err(error) = emit_health_receipt(
        &lifecycle,
        RuntimeLifecyclePhase::Connecting,
        None,
        0,
        if event_stream_disabled {
            EventStreamState::Disabled
        } else {
            EventStreamState::Unknown
        },
        false,
        "initializing_runtime_observers",
    ) {
        eprintln!("GNOS_UX_LIFECYCLE_FATAL reason={error}");
        exit_code.store(1, Ordering::Release);
        cx.quit();
        return;
    }

    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            let reason = format!("backend_init:{error:?}");
            eprintln!("GNOS_UX_TOPOLOGY_FATAL reason={reason}");
            let _ = emit_health_receipt(
                &lifecycle,
                RuntimeLifecyclePhase::Failed,
                None,
                0,
                EventStreamState::Unknown,
                false,
                reason,
            );
            exit_code.store(1, Ordering::Release);
            cx.quit();
            return;
        }
    };

    let (sender, mut receiver) = async_mpsc::unbounded::<TopologyWakeHint>();
    let subscription = if event_stream_disabled {
        println!(
            "GNOS_UX_TOPOLOGY_EVENT_STREAM mode=DISABLED_QUALIFICATION safety_resync_ms={}",
            safety_resync_interval.as_millis()
        );
        None
    } else {
        let event_sender = sender.clone();
        match TopologyEventSubscription::spawn_with_sink(
            backend.runtime_target().to_owned(),
            move |hint| event_sender.unbounded_send(hint).is_ok(),
        ) {
            Ok(subscription) => Some(subscription),
            Err(error) => {
                eprintln!(
                    "GNOS_UX_TOPOLOGY_EVENT_STREAM_DEGRADED reason={error:?} fallback=SAFETY_RESYNC"
                );
                None
            }
        }
    };
    let subscription_started = subscription.is_some();

    let _ = sender.unbounded_send(TopologyWakeHint::Startup);
    println!(
        "GNOS_UX_TOPOLOGY_START session={} event_stream={} safety_resync_ms={}",
        backend.runtime_target(),
        if event_stream_disabled {
            "disabled-qualification"
        } else if subscription_started {
            "socket2"
        } else {
            "degraded"
        },
        safety_resync_interval.as_millis(),
    );

    cx.spawn(async move |cx| {
        let _subscription = subscription;
        let semantic_consumer = if semantic_consumer_enabled {
            SceneSemanticConsumer::enabled()
        } else {
            SceneSemanticConsumer::default()
        };
        let mut runtime = DynamicTopologyRuntime {
            semantic_consumer,
            application_catalog,
            ..DynamicTopologyRuntime::default()
        };
        let mut semantic_runtime = DesktopSemanticRuntime::default();
        let mut next_safety = Instant::now() + safety_resync_interval;

        loop {
            let executor = cx.background_executor().clone();
            let wake = crate::topology_wake::next_runtime_wake(
                &mut receiver,
                &mut shutdown_signals.receiver,
                next_safety.saturating_duration_since(Instant::now()),
                |delay| executor.timer(delay),
            ).await;

            if let Some(signal) = shutdown_signals.requested_reason() {
                let event_state =
                    topology_event_stream_state(&runtime, event_stream_disabled);
                let generation = runtime.reconciler.generation();
                let route_count = runtime.routes.len();
                if let Err(error) = emit_health_receipt(
                    &lifecycle,
                    RuntimeLifecyclePhase::Stopping,
                    Some(generation),
                    route_count,
                    event_state,
                    false,
                    format!("shutdown_requested:{signal}"),
                ) {
                    eprintln!("GNOS_UX_LIFECYCLE_FATAL reason={error}");
                    exit_code.store(1, Ordering::Release);
                }

                let teardown = cx.update(|app| runtime.shutdown(app));
                match teardown {
                    Ok(_) => {
                        let _ = emit_health_receipt(
                            &lifecycle,
                            RuntimeLifecyclePhase::Stopping,
                            Some(generation),
                            0,
                            event_state,
                            false,
                            "shutdown_teardown_complete",
                        );
                    }
                    Err(error) => {
                        eprintln!("GNOS_UX_LIFECYCLE_TEARDOWN_FAIL reason={error}");
                        let _ = emit_health_receipt(
                            &lifecycle,
                            RuntimeLifecyclePhase::Failed,
                            Some(generation),
                            runtime.routes.len(),
                            event_state,
                            false,
                            format!("shutdown_teardown:{error}"),
                        );
                        exit_code.store(1, Ordering::Release);
                    }
                }

                cx.update(|app| app.quit());
                return;
            }

            let now = Instant::now();
            let mut batch = TopologyWakeBatch::default();
            match wake {
                crate::topology_wake::RuntimeWake::Event(hint) => batch.absorb(hint),
                crate::topology_wake::RuntimeWake::SafetyResync => batch.absorb(TopologyWakeHint::SafetyResync),
                crate::topology_wake::RuntimeWake::Shutdown(_) => continue,
                crate::topology_wake::RuntimeWake::Unavailable(reason) => {
                    eprintln!("GNOS_UX_TOPOLOGY_FATAL reason={reason}");
                    exit_code.store(1, Ordering::Release);
                    let _ = cx.update(|app| runtime.shutdown(app));
                    cx.update(|app| app.quit());
                    return;
                }
            }
            while let Ok(hint) = receiver.try_recv() {
                batch.absorb(hint);
            }
            if now >= next_safety && !batch.safety_resync {
                batch.absorb(TopologyWakeHint::SafetyResync);
            }
            if now >= next_safety {
                next_safety = now + safety_resync_interval;
            }
            if batch.hint_count == 0 {
                continue;
            }

            if !batch.startup && !batch.safety_resync {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                while let Ok(hint) = receiver.try_recv() {
                    batch.absorb(hint);
                }
            }

            let event_state =
                topology_event_stream_state(&runtime, event_stream_disabled);
            let _ = emit_health_receipt(
                &lifecycle,
                RuntimeLifecyclePhase::Reconciling,
                Some(runtime.reconciler.generation()),
                runtime.routes.len(),
                event_state,
                false,
                if batch.startup {
                    "startup_reconcile"
                } else if batch.safety_resync {
                    "safety_reconcile"
                } else {
                    "event_reconcile"
                },
            );

            let mut completed = false;
            for attempt in 1..=40_u32 {
                let result = cx.update(|app| runtime.reconcile(app, &backend, &batch));
                match result {
                    Ok(receipt) => {
                        println!(
                            "GNOS_UX_TOPOLOGY_RECEIPT generation={} attempts={} hints={} added={} removed={} preserved={} reconfigured={} reattached={} routes={} pending_retired={} event_gap_rebuild={} post_snapshot_drift={}",
                            receipt.generation,
                            attempt,
                            receipt.hint_count,
                            receipt.added,
                            receipt.removed,
                            receipt.preserved,
                            receipt.reconfigured,
                            receipt.reattached,
                            receipt.route_count,
                            receipt.pending_retired,
                            receipt.rebuilt_after_event_gap,
                            receipt.post_snapshot_drift,
                        );

                        let mut semantic_projections = None;
                        for semantic_attempt in 1..=20_u32 {
                            match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
                                Ok(observed) => {
                                    let projections =
                                        project_all_output_contexts(semantic_runtime.baseline());
                                    emit_semantic_receipt(&observed, &batch);
                                    emit_semantic_output_views(&projections);
                                    semantic_projections = Some((observed, projections));
                                    break;
                                }
                                Err(error)
                                    if semantic_error_retryable(error) && semantic_attempt < 20 =>
                                {
                                    let event_state =
                                        topology_event_stream_state(&runtime, event_stream_disabled);
                                    let _ = emit_health_receipt(
                                        &lifecycle,
                                        RuntimeLifecyclePhase::Recovering,
                                        Some(receipt.generation),
                                        receipt.route_count,
                                        event_state,
                                        false,
                                        format!("semantic_retry:{error:?}"),
                                    );
                                    if semantic_attempt == 1 || semantic_attempt % 5 == 0 {
                                        eprintln!(
                                            "GNOSIX_DESKTOP_SEMANTIC_RETRY attempt={} reason={error:?}",
                                            semantic_attempt
                                        );
                                    }
                                    cx.background_executor()
                                        .timer(Duration::from_millis(30))
                                        .await;
                                }
                                Err(error) if semantic_error_retryable(error) => {
                                    eprintln!(
                                        "GNOSIX_DESKTOP_SEMANTIC_RECONCILE_FAIL attempt={} retryable=true reason={error:?}",
                                        semantic_attempt
                                    );
                                    let event_state =
                                        topology_event_stream_state(&runtime, event_stream_disabled);
                                    let _ = runtime.clear_consumer_projections(
                                        "SEMANTIC_RECONCILE_RETRY_EXHAUSTED",
                                    );
                                    let _ = emit_health_receipt(
                                        &lifecycle,
                                        RuntimeLifecyclePhase::Recovering,
                                        Some(receipt.generation),
                                        receipt.route_count,
                                        event_state,
                                        false,
                                        format!("semantic_retry_exhausted:{error:?}"),
                                    );
                                    let _ = sender.unbounded_send(TopologyWakeHint::SafetyResync);
                                    break;
                                }
                                Err(error) => {
                                    eprintln!(
                                        "GNOSIX_DESKTOP_SEMANTIC_RECONCILE_FAIL attempt={} retryable=false reason={error:?}",
                                        semantic_attempt
                                    );
                                    let event_state =
                                        topology_event_stream_state(&runtime, event_stream_disabled);
                                    let _ = runtime.clear_consumer_projections(
                                        "SEMANTIC_RECONCILE_FATAL",
                                    );
                                    let _ = emit_health_receipt(
                                        &lifecycle,
                                        RuntimeLifecyclePhase::Failed,
                                        Some(receipt.generation),
                                        receipt.route_count,
                                        event_state,
                                        false,
                                        format!("semantic_fatal:{error:?}"),
                                    );
                                    exit_code.store(1, Ordering::Release);
                                    let _ = cx.update(|app| runtime.shutdown(app));
                                    cx.update(|app| app.quit());
                                    return;
                                }
                            }
                        }

                        let Some((semantic_receipt, projections)) = semantic_projections else {
                            completed = true;
                            break;
                        };

                        let consumer_publication_issue = if receipt.post_snapshot_drift {
                            Some("publication_skipped:post_snapshot_drift".to_owned())
                        } else {
                            match runtime.publish_consumer_projections(
                                &projections,
                                semantic_receipt.generation,
                            ) {
                                Ok(()) => None,
                                Err(error) => {
                                    let clear_error = runtime
                                        .clear_consumer_projections("PUBLICATION_FAILED")
                                        .err();
                                    Some(match clear_error {
                                        Some(clear_error) => {
                                            format!("{error};clear={clear_error}")
                                        }
                                        None => error,
                                    })
                                }
                            }
                        };
                        if let Some(error) = &consumer_publication_issue {
                            eprintln!(
                                "GNOS_UX_DSC01_CONSUMER_PUBLISH_BLOCKED reason={error} topology_generation={} semantic_generation={}",
                                receipt.generation, semantic_receipt.generation
                            );
                        }

                        let event_state =
                            topology_event_stream_state(&runtime, event_stream_disabled);
                        let phase = if consumer_publication_issue.is_some() {
                            RuntimeLifecyclePhase::Recovering
                        } else if event_state == EventStreamState::Connected {
                            RuntimeLifecyclePhase::Ready
                        } else {
                            RuntimeLifecyclePhase::Degraded
                        };
                        let reason = if let Some(error) = &consumer_publication_issue {
                            format!("consumer_publish_failed:{error}")
                        } else if phase == RuntimeLifecyclePhase::Ready {
                            "topology_and_semantic_snapshots_committed".to_owned()
                        } else {
                            "topology_and_semantic_snapshots_committed_event_rail_degraded"
                                .to_owned()
                        };
                        if let Err(error) = emit_health_receipt(
                            &lifecycle,
                            phase,
                            Some(receipt.generation),
                            receipt.route_count,
                            event_state,
                            true,
                            reason,
                        ) {
                            eprintln!("GNOS_UX_LIFECYCLE_FATAL reason={error}");
                            exit_code.store(1, Ordering::Release);
                            let _ = cx.update(|app| runtime.shutdown(app));
                            cx.update(|app| app.quit());
                            return;
                        }

                        if receipt.post_snapshot_drift || consumer_publication_issue.is_some() {
                            let _ = sender.unbounded_send(TopologyWakeHint::SafetyResync);
                        }
                        completed = true;
                        break;
                    }
                    Err(error) if error.retryable() && attempt < 40 => {
                        if let Err(clear_error) =
                            runtime.clear_consumer_projections("TOPOLOGY_RECONCILE_FAILED")
                        {
                            eprintln!(
                                "GNOS_UX_DSC01_CONSUMER_INVALIDATE_FAIL reason={clear_error}"
                            );
                        }
                        let event_state = topology_event_stream_state(&runtime, event_stream_disabled);
                        let _ = emit_health_receipt(
                            &lifecycle,
                            RuntimeLifecyclePhase::Recovering,
                            Some(runtime.reconciler.generation()),
                            runtime.routes.len(),
                            event_state,
                            false,
                            format!("topology_retry:{}", error.message()),
                        );
                        if attempt == 1 || attempt % 10 == 0 {
                            eprintln!(
                                "GNOS_UX_TOPOLOGY_RETRY attempt={} reason={}",
                                attempt,
                                error.message()
                            );
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(50))
                            .await;
                    }
                    Err(error) => {
                        if let Err(clear_error) =
                            runtime.clear_consumer_projections("TOPOLOGY_RECONCILE_FAILED")
                        {
                            eprintln!(
                                "GNOS_UX_DSC01_CONSUMER_INVALIDATE_FAIL reason={clear_error}"
                            );
                        }
                        eprintln!(
                            "GNOS_UX_TOPOLOGY_RECONCILE_FAIL attempt={} retryable={} reason={}",
                            attempt,
                            error.retryable(),
                            error.message()
                        );
                        let event_state = topology_event_stream_state(&runtime, event_stream_disabled);
                        if error.retryable() {
                            let _ = emit_health_receipt(
                                &lifecycle,
                                RuntimeLifecyclePhase::Recovering,
                                Some(runtime.reconciler.generation()),
                                runtime.routes.len(),
                                event_state,
                                false,
                                format!("topology_retry_exhausted:{}", error.message()),
                            );
                        } else {
                            let _ = emit_health_receipt(
                                &lifecycle,
                                RuntimeLifecyclePhase::Failed,
                                Some(runtime.reconciler.generation()),
                                runtime.routes.len(),
                                event_state,
                                false,
                                format!("topology_fatal:{}", error.message()),
                            );
                            exit_code.store(1, Ordering::Release);
                            let _ = cx.update(|app| runtime.shutdown(app));
                            cx.update(|app| app.quit());
                            return;
                        }
                        break;
                    }
                }
            }

            if !completed {
                eprintln!(
                    "GNOS_UX_TOPOLOGY_BATCH_UNCOMMITTED hints={} safety_resync={}",
                    batch.hint_count, batch.safety_resync
                );
            }
        }
    })
    .detach();
}

#[cfg(feature = "r13-production-client")]
fn r13_qualification_output_from(
    args: &[String],
    environment_output: Option<String>,
) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--r13-qualify-active-workspace="))
        .filter(|connector| !connector.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| environment_output.filter(|connector| !connector.trim().is_empty()))
}

#[cfg(feature = "r13-production-client")]
fn r13_qualification_output(args: &[String]) -> Option<String> {
    r13_qualification_output_from(args, env::var("GNOSIX_R13_QUALIFY_ACTIVE_WORKSPACE").ok())
}

#[cfg(feature = "r13-production-client")]
fn r13_application_focus_qualification_output(args: &[String]) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--r13-qualify-focus-application="))
        .filter(|connector| !connector.trim().is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(feature = "r13-production-client")]
fn r13_stale_qualification_output(args: &[String]) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--r13-qualify-stale-active-workspace="))
        .filter(|connector| !connector.trim().is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(feature = "r13-production-client")]
fn run_r13_stale_active_workspace_qualification(connector: &str) -> i32 {
    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!("GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=backend reason={error:?}");
            return 1;
        }
    };
    let mut semantic_runtime = DesktopSemanticRuntime::default();

    let first = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=select_observation reason={error:?}"
            );
            return 1;
        }
    };
    let first_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let output = RuntimeId::new(connector);
    let selected_state = ProductSemanticState::from_projection(&output, first_projection.as_ref());
    let Some(selected) = (match selected_state {
        ProductSemanticState::Current(content) => content.active_workspace.clone(),
        ProductSemanticState::Unavailable | ProductSemanticState::Invalid => None,
    }) else {
        eprintln!(
            "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_active_workspace",
            connector, first.generation
        );
        return 1;
    };
    println!(
        "GNOS_UX_R13_STALE_QUALIFICATION_ARMED output={} selected_generation={} waiting_for_new_generation=true",
        connector, first.generation
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let second = loop {
        match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
            Ok(receipt) if receipt.generation != first.generation => break receipt,
            Ok(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(receipt) => {
                eprintln!(
                    "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=await_generation output={} selected_generation={} current_generation={} reason=timeout",
                    connector, first.generation, receipt.generation
                );
                return 1;
            }
            Err(error) => {
                eprintln!(
                    "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=await_generation reason={error:?}"
                );
                return 1;
            }
        }
    };

    let second_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let mut product_runtime =
        ProductRuntime::from_projection(Some(&output), second_projection.as_ref());
    let record = ProductController.activate_workspace(&mut product_runtime, selected);

    if record.terminal == Some(ProductActionTerminal::LocalStale)
        && record.selected_generation == DesktopContextGeneration(first.generation)
        && record.current_generation == Some(DesktopContextGeneration(second.generation))
    {
        println!(
            "GNOS_UX_R13_STALE_QUALIFICATION_PASS output={} selected_generation={} current_generation={} transport=NOT_CALLED",
            connector, record.selected_generation.0, second.generation
        );
        0
    } else {
        eprintln!(
            "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=send output={} selected_generation={} current_generation={} terminal={:?} detail={}",
            connector, first.generation, second.generation, record.terminal, record.detail
        );
        1
    }
}

#[cfg(feature = "r13-production-client")]
fn run_r13_application_focus_qualification(connector: &str) -> i32 {
    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=backend reason={error:?}"
            );
            return 1;
        }
    };
    let mut semantic_runtime = DesktopSemanticRuntime::default();
    let output = RuntimeId::new(connector);

    let first = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_observation reason={error:?}"
            );
            return 1;
        }
    };
    let first_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let first_state = ProductSemanticState::from_projection(&output, first_projection.as_ref());
    let ProductSemanticState::Current(first_content) = first_state else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=semantic_unavailable",
            connector, first.generation
        );
        return 1;
    };
    let Some(original) = first_content.focused_application.clone() else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_focused_application",
            connector, first.generation
        );
        return 1;
    };
    let Some(selected) = first_content
        .applications
        .iter()
        .find(|application| {
            application.reference.surface_id() != original.reference.surface_id()
                && application.visibility == SurfaceVisibility::Visible
        })
        .cloned()
    else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_alternate_visible_application",
            connector, first.generation
        );
        return 1;
    };

    let second = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=send_fence_observation reason={error:?}"
            );
            return 1;
        }
    };
    let second_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let mut product_runtime =
        ProductRuntime::from_projection(Some(&output), second_projection.as_ref());
    let controller = ProductController;
    let first_record = controller.focus_application(&mut product_runtime, selected.clone());
    if first_record.terminal != Some(ProductActionTerminal::Applied)
        || first_record.current_generation != Some(DesktopContextGeneration(second.generation))
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=focus_selected output={} terminal={:?} detail={}",
            connector, first_record.terminal, first_record.detail
        );
        return 1;
    }

    let third = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=selected_post_observation reason={error:?}"
            );
            return 1;
        }
    };
    let third_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let third_state = ProductSemanticState::from_projection(&output, third_projection.as_ref());
    let ProductSemanticState::Current(third_content) = third_state else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=selected_postcondition reason=semantic_unavailable"
        );
        return 1;
    };
    if !third_content
        .focused_application
        .as_ref()
        .is_some_and(|application| {
            application.reference.surface_id() == selected.reference.surface_id()
        })
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=selected_postcondition reason=surface_not_focused"
        );
        return 1;
    }

    let Some(restore_target) = third_content
        .applications
        .iter()
        .find(|application| application.reference.surface_id() == original.reference.surface_id())
        .cloned()
    else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_target reason=original_surface_disappeared"
        );
        return 1;
    };
    let mut restore_runtime =
        ProductRuntime::from_projection(Some(&output), third_projection.as_ref());
    let restore_record = controller.focus_application(&mut restore_runtime, restore_target);
    if restore_record.terminal != Some(ProductActionTerminal::Applied)
        || restore_record.current_generation != Some(DesktopContextGeneration(third.generation))
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_send output={} terminal={:?} detail={}",
            connector, restore_record.terminal, restore_record.detail
        );
        return 1;
    }

    let fourth = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_post_observation reason={error:?}"
            );
            return 1;
        }
    };
    let fourth_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let fourth_state = ProductSemanticState::from_projection(&output, fourth_projection.as_ref());
    let ProductSemanticState::Current(fourth_content) = fourth_state else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_postcondition reason=semantic_unavailable"
        );
        return 1;
    };
    if !fourth_content
        .focused_application
        .as_ref()
        .is_some_and(|application| {
            application.reference.surface_id() == original.reference.surface_id()
        })
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_postcondition reason=original_surface_not_focused"
        );
        return 1;
    }

    println!(
        "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_PASS output={} first_generation={} selected_generation={} selected_surface={:?} selected_title={:?} selected_receipt={} selected_post_generation={} restore_generation={} original_surface={:?} original_title={:?} restore_receipt={} restore_post_generation={}",
        connector,
        first.generation,
        first_record.selected_generation.0,
        selected.reference.surface_id(),
        selected.title,
        first_record.terminal.expect("selected terminal").as_str(),
        third.generation,
        restore_record.selected_generation.0,
        original.reference.surface_id(),
        original.title,
        restore_record.terminal.expect("restore terminal").as_str(),
        fourth.generation,
    );
    0
}

#[cfg(feature = "r13-production-client")]
fn run_r13_active_workspace_qualification(connector: &str) -> i32 {
    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!("GNOS_UX_R13_QUALIFICATION_FAIL stage=backend reason={error:?}");
            return 1;
        }
    };
    let mut semantic_runtime = DesktopSemanticRuntime::default();

    let first = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!("GNOS_UX_R13_QUALIFICATION_FAIL stage=select_observation reason={error:?}");
            return 1;
        }
    };
    let first_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let output = RuntimeId::new(connector);
    let selected_state = ProductSemanticState::from_projection(&output, first_projection.as_ref());
    let Some(selected) = (match selected_state {
        ProductSemanticState::Current(content) => content.active_workspace.clone(),
        ProductSemanticState::Unavailable | ProductSemanticState::Invalid => None,
    }) else {
        eprintln!(
            "GNOS_UX_R13_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_active_workspace",
            connector, first.generation
        );
        return 1;
    };

    let second = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_QUALIFICATION_FAIL stage=send_fence_observation reason={error:?}"
            );
            return 1;
        }
    };
    let second_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let mut product_runtime =
        ProductRuntime::from_projection(Some(&output), second_projection.as_ref());

    let send_started = Instant::now();
    let record = ProductController.activate_workspace(&mut product_runtime, selected);
    let send_elapsed_us = send_started.elapsed().as_micros();
    match record.terminal {
        Some(ProductActionTerminal::Applied | ProductActionTerminal::AlreadySatisfied)
            if record.current_generation == Some(DesktopContextGeneration(second.generation)) =>
        {
            println!(
                "GNOS_UX_R13_QUALIFICATION_RECEIPT output={} selected_generation={} current_generation={} send_elapsed_us={} code={} detail={}",
                connector,
                record.selected_generation.0,
                record
                    .current_generation
                    .map_or(0, |generation| generation.0),
                send_elapsed_us,
                record.terminal.expect("matched terminal outcome").as_str(),
                record.detail
            );
            0
        }
        terminal => {
            eprintln!(
                "GNOS_UX_R13_QUALIFICATION_FAIL stage=send output={} selected_generation={} current_generation={} send_elapsed_us={} terminal={terminal:?} detail={}",
                connector,
                record.selected_generation.0,
                record
                    .current_generation
                    .map_or(0, |generation| generation.0),
                send_elapsed_us,
                record.detail
            );
            1
        }
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

    if let Err(error) = crate::product_bar::qualification_edge(&args) {
        eprintln!("GNOS_UX_BAR_QUALIFICATION_INVALID {error}");
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
                        open_multi_output_scene_windows(app, Arc::clone(&application_catalog))
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
