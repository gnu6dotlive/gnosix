//! SDK-8: the dynamic output topology: one scene route per output,
//! staged and committed as outputs come and go, with the semantic
//! projections kept consistent.

use super::*;

#[derive(Clone)]
pub(super) struct DynamicSceneRoute {
    pub(super) scene_id: MonitorSceneId,
    pub(super) output: RuntimeId,
    pub(super) display_id: DisplayId,
    pub(super) window: AnyWindowHandle,
    pub(super) observed: ObservedRuntimeOutput,
}

#[derive(Debug)]
pub(super) enum DynamicTopologyError {
    Observe(String),
    MappingPending(String),
    Plan(String),
    Open(String),
    RetirePending(String),
    Commit(String),
    Invariant(String),
}

impl DynamicTopologyError {
    pub(super) fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Observe(_) | Self::MappingPending(_) | Self::Open(_) | Self::RetirePending(_)
        )
    }

    pub(super) fn message(&self) -> &str {
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
pub(super) struct DynamicReconcileReceipt {
    pub(super) generation: u64,
    pub(super) hint_count: usize,
    pub(super) added: usize,
    pub(super) removed: usize,
    pub(super) preserved: usize,
    pub(super) reconfigured: usize,
    pub(super) reattached: usize,
    pub(super) route_count: usize,
    pub(super) pending_retired: usize,
    pub(super) post_snapshot_drift: bool,
    pub(super) rebuilt_after_event_gap: bool,
}

pub(super) struct PendingTopologyTransaction {
    pub(super) plan: SessionTopologyPlan,
    pub(super) next_routes: BTreeMap<String, DynamicSceneRoute>,
    pub(super) retire_pending: Vec<AnyWindowHandle>,
    pub(super) next_scene_id: u64,
    pub(super) hint_count: usize,
    pub(super) rebuilt_after_event_gap: bool,
    pub(super) semantics_invalidated: bool,
    pub(super) committed: bool,
}

pub(super) trait RouteWindowOperations<R> {
    fn open(&mut self, output: &ObservedRuntimeOutput) -> Result<R, String>;
    fn close(&mut self, route: &R) -> Result<(), String>;
}

#[derive(Debug)]
pub(super) struct RouteStageFailure<R> {
    pub(super) output: String,
    pub(super) error: String,
    pub(super) rollback_pending: Vec<R>,
}

#[derive(Debug)]
pub(super) enum TopologyStageError<R> {
    SemanticInvalidation(String),
    WindowStage(RouteStageFailure<R>),
}

pub(super) fn prepare_topology_stage<R>(
    plan: &SessionTopologyPlan,
    mut invalidate: impl FnMut(&SessionTopologyPlan) -> Result<(), String>,
    operations: &mut impl RouteWindowOperations<R>,
) -> Result<BTreeMap<String, R>, TopologyStageError<R>> {
    if plan.changed() {
        invalidate(plan).map_err(TopologyStageError::SemanticInvalidation)?;
    }
    stage_topology_windows(&plan.actions, operations).map_err(TopologyStageError::WindowStage)
}

pub(super) fn commit_pending_routes<R>(
    reconciler: &mut SessionTopologyReconciler,
    plan: &SessionTopologyPlan,
    current: &mut BTreeMap<String, R>,
    staged: &mut BTreeMap<String, R>,
) -> Result<(), gnosix_hyprland::topology::TopologyReconcileError> {
    reconciler.commit(plan)?;
    *current = std::mem::take(staged);
    Ok(())
}

pub(super) struct ShutdownReport<H, E> {
    pub(super) attempted: usize,
    pub(super) errors: Vec<E>,
    pub(super) close_pending: Vec<H>,
}

pub(super) fn stage_topology_windows<R>(
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

pub(super) struct GpuiRouteWindowOperations<'a> {
    /// Bounded tray qualification request, applied to each host opened here.
    pub(super) tray_qualification: Option<TrayQualificationRequest>,
    pub(super) cx: &'a mut App,
    pub(super) display_ids: &'a BTreeMap<String, DisplayId>,
    pub(super) semantic_consumer: SceneSemanticConsumer,
    pub(super) application_catalog: Arc<ApplicationCatalog>,
    pub(super) next_scene_id: u64,
    pub(super) product_focus: ProductFocusScope,
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
        let output_origin = (current.x as i32, current.y as i32);
        let window = open_scene_window(
            self.cx,
            scene_id,
            Some(output.clone()),
            output_origin,
            self.tray_qualification.clone(),
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
pub(super) struct TopologyEventContinuity {
    pub(super) connected: bool,
    pub(super) lost_since_rebuild: bool,
}

impl TopologyEventContinuity {
    pub(super) fn observe(&mut self, batch: &TopologyWakeBatch) {
        if batch.stream_disconnected {
            self.lost_since_rebuild = true;
        }
        if let Some(connected) = batch.last_stream_state {
            self.connected = connected;
        }
    }

    pub(super) fn rebuild_required(&self) -> bool {
        self.connected && self.lost_since_rebuild
    }

    pub(super) fn authoritative_rebuild_committed(&mut self) {
        if self.rebuild_required() {
            self.lost_since_rebuild = false;
        }
    }

    pub(super) fn state(&self, disabled: bool) -> EventStreamState {
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

pub(super) struct DynamicTopologyRuntime {
    pub(super) reconciler: SessionTopologyReconciler,
    pub(super) routes: BTreeMap<String, DynamicSceneRoute>,
    pub(super) pending: Option<PendingTopologyTransaction>,
    pub(super) cleanup_pending: Vec<AnyWindowHandle>,
    pub(super) next_scene_id: u64,
    pub(super) event_continuity: TopologyEventContinuity,
    pub(super) semantic_consumer: SceneSemanticConsumer,
    pub(super) application_catalog: Arc<ApplicationCatalog>,
    /// Shared by every host this runtime opens, so focus stays exclusive
    /// across outputs for the life of the session.
    pub(super) product_focus: ProductFocusScope,
    /// Bounded tray qualification request applied to every host it opens.
    pub(super) tray_qualification: Option<TrayQualificationRequest>,
}

impl Default for DynamicTopologyRuntime {
    fn default() -> Self {
        Self {
            tray_qualification: None,
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
    pub(super) fn reconcile(
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

    pub(super) fn apply_plan(
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
            tray_qualification: self.tray_qualification.clone(),
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

        crate::receipt!(
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

    pub(super) fn complete_pending_transaction(
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

        crate::receipt!(
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
        crate::receipt!(
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

    pub(super) fn clear_consumer_projections(&self, reason: &str) -> Result<(), String> {
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

    pub(super) fn invalidate_semantics_on_snapshot_drift(
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

    pub(super) fn invalidate_semantics_for_topology_plan(
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

    pub(super) fn publish_consumer_projections(
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
            crate::receipt!(
                "GNOS_UX_DSC01_CONSUMER_PUBLISH connector={} output_hub_id={} semantic_generation={} scene_id={} display_id={:?} workspaces={} surfaces={}",
                connector.as_str(),
                hex_bytes(projection.output.output_hub_id.as_bytes()),
                projection.generation.0,
                route.scene_id.value(),
                route.display_id,
                projection.workspaces.len(),
                projection.surfaces.len()
            );
            crate::receipt!(
                "GNOS_UX_DSC01_CONSUMER_CONTEXT connector={} context={}",
                connector.as_str(),
                desktop_consumer_context_text(Some(&projection))
            );
        }
        Ok(())
    }

    pub(super) fn shutdown(&mut self, cx: &mut App) -> Result<usize, String> {
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

    pub(super) fn log_routes(&self) {
        for route in self.routes.values() {
            crate::receipt!(
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

pub(super) fn retain_failed_closes<T: Copy, E>(
    handles: impl IntoIterator<Item = T>,
    mut close: impl FnMut(T) -> Result<(), E>,
) -> Vec<T> {
    handles
        .into_iter()
        .filter(|handle| close(*handle).is_err())
        .collect()
}

pub(super) fn shutdown_owned_windows<H, E>(
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

pub(super) fn same_snapshot(
    expected: &[ObservedRuntimeOutput],
    observed: &[ObservedRuntimeOutput],
) -> bool {
    let mut expected = expected.to_vec();
    let mut observed = observed.to_vec();
    expected.sort_by(|left, right| left.name.cmp(&right.name));
    observed.sort_by(|left, right| left.name.cmp(&right.name));
    expected == observed
}

pub(super) fn topology_action_counts(
    actions: &[TopologyAction],
) -> (usize, usize, usize, usize, usize) {
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

pub(super) fn semantic_error_retryable(error: SemanticObservationError) -> bool {
    !matches!(
        error,
        SemanticObservationError::UnsupportedTransform
            | SemanticObservationError::GenerationExhausted
    )
}

pub(super) fn semantic_wake_reason(batch: &TopologyWakeBatch) -> String {
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

pub(super) fn emit_semantic_receipt(receipt: &DesktopSemanticReceipt, batch: &TopologyWakeBatch) {
    crate::receipt!(
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

pub(super) fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn emit_semantic_output_views(views: &[DesktopOutputProjection]) {
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

        crate::receipt!(
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

pub(super) fn topology_event_stream_state(
    runtime: &DynamicTopologyRuntime,
    event_stream_disabled: bool,
) -> EventStreamState {
    runtime.event_continuity.state(event_stream_disabled)
}

#[derive(Clone)]
pub(super) struct DynamicTopologyConfig {
    pub(super) event_stream_disabled: bool,
    pub(super) tray_qualification: Option<TrayQualificationRequest>,
    pub(super) semantic_consumer_enabled: bool,
    pub(super) safety_resync_interval: Duration,
    pub(super) application_catalog: Arc<ApplicationCatalog>,
}

pub(super) fn start_dynamic_topology(
    cx: &mut App,
    config: DynamicTopologyConfig,
    lifecycle: SharedLifecycle,
    mut shutdown_signals: ShutdownSignals,
    exit_code: Arc<AtomicU8>,
) {
    let DynamicTopologyConfig {
        tray_qualification,
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
            tray_qualification,
            ..DynamicTopologyRuntime::default()
        };
        let mut semantic_runtime = DesktopSemanticRuntime::default();
        let mut next_safety = Instant::now() + safety_resync_interval;
        // LIVE-03: the semantic generation the consumers hold, and the
        // cycles that changed nothing since the last report.
        let mut last_published: Option<u64> = None;
        let mut quiet = crate::receipt_hold::Quiet::default();

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
            if batch.config_reloaded {
                crate::compositor_blur::config_reloaded();
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

            // A cycle's receipts wait until it is known whether it changed
            // anything (startup always prints).
            if !batch.startup {
                crate::receipt_hold::hold();
            }
            let mut quiet_cycle: Option<u64> = None;
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
                        crate::receipt!(
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
                                    last_published = None;
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
                                    crate::receipt_hold::release(true);
                                    return;
                                }
                            }
                        }

                        let Some((semantic_receipt, projections)) = semantic_projections else {
                            completed = true;
                            break;
                        };

                        // Nothing changed and the consumers already hold this
                        // generation: republishing would only make every
                        // consumer refresh the same content.
                        let unchanged = !batch.startup
                            && receipt.added == 0
                            && receipt.removed == 0
                            && receipt.reconfigured == 0
                            && receipt.reattached == 0
                            && !receipt.rebuilt_after_event_gap
                            && !receipt.post_snapshot_drift
                            && !semantic_receipt.changed
                            && last_published == Some(semantic_receipt.generation);
                        if unchanged {
                            quiet_cycle = Some(semantic_receipt.generation);
                        }
                        let consumer_publication_issue = if receipt.post_snapshot_drift {
                            last_published = None;
                            Some("publication_skipped:post_snapshot_drift".to_owned())
                        } else if unchanged {
                            None
                        } else {
                            match runtime.publish_consumer_projections(
                                &projections,
                                semantic_receipt.generation,
                            ) {
                                Ok(()) => {
                                    last_published = Some(semantic_receipt.generation);
                                    None
                                }
                                Err(error) => {
                                    last_published = None;
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
                            crate::receipt_hold::release(true);
                            return;
                        }

                        if receipt.post_snapshot_drift || consumer_publication_issue.is_some() {
                            let _ = sender.unbounded_send(TopologyWakeHint::SafetyResync);
                        }
                        completed = true;
                        break;
                    }
                    Err(error) if error.retryable() && attempt < 40 => {
                        last_published = None;
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
                        last_published = None;
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
                            crate::receipt_hold::release(true);
                            return;
                        }
                        break;
                    }
                }
            }

            let held = crate::receipt_hold::release(quiet_cycle.is_none());
            if let Some(generation) = quiet_cycle
                && let Some(report) = quiet.count(held, generation)
            {
                println!("{report}");
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
