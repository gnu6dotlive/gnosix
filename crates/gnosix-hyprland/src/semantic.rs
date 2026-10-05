//! Read-only Hyprland -> GNOSIX desktop semantic observation/reconciliation.
//!
//! Runtime events are wake hints only. The admitted baseline is always rebuilt
//! from fresh read-only Hyprland JSON and reconciled through
//! `gnosix-desktop-model`. Nothing in this module writes compositor or
//! persistent state.

use std::collections::{HashMap, HashSet};

use gnosix_desktop_model::{
    ApplicationSurfaceDescriptors, ConnectorName, DesktopContextBaseline, DesktopContextCandidate,
    DesktopContextGeneration, DesktopContextValidationError, DesktopSurfaceRole, LogicalPoint,
    LogicalRect, LogicalSize, OutputHubId, OutputState, OutputTransform, PhysicalOutputDescriptors,
    PhysicalPixelSize, ScaleMilli, SurfaceFocus, SurfaceId, SurfaceState, SurfaceVisibility,
    TransitionOutcome, WorkspaceFocus, WorkspaceId, WorkspaceName, WorkspaceState,
    WorkspaceVisibility, reconcile_context,
};
use in_gnu_os_activation::{
    ActivationSessionEpoch, CurrentActivationObservation, ExecutionObservation,
    ObservedSurfaceBinding, ObservedWorkspaceBinding,
};
use serde_json::{Deserializer, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{HyprctlLuaBackend, HyprlandRuntimeBackend};

const SEMANTIC_VIEW_VALUE_COUNT: usize = 6;
const SEMANTIC_VALUE_COUNT: usize = SEMANTIC_VIEW_VALUE_COUNT * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticObservationError {
    Transport,
    Parse,
    NumericRange,
    UnsupportedTransform,
    UnknownOutput,
    UnknownWorkspace,
    FocusedClientMissing,
    SnapshotChanged,
    ExecutionBindingsChanged,
    DuplicateExecutionBinding,
    InvalidCandidate(DesktopContextValidationError),
    GenerationExhausted,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSemanticReceipt {
    pub generation: u64,
    pub changed: bool,
    pub output_count: usize,
    pub workspace_count: usize,
    pub surface_count: usize,
    pub visible_workspace_count: usize,
    pub visible_surface_count: usize,
    pub focused_workspace_count: usize,
    pub focused_surface_count: usize,
    pub digest_sha256: String,
}

#[derive(Debug)]
pub struct DesktopSemanticRuntime {
    baseline: DesktopContextBaseline,
}

/// Ephemeral exact mapping from semantic identities to native Hyprland
/// selectors, captured from the same stable observation as its generation.
/// This is a read-only observer result, not durable identity or authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopExecutionBindingSnapshot {
    session_signature: String,
    generation: DesktopContextGeneration,
    semantic_digest_sha256: String,
    workspace_runtime_ids: HashMap<WorkspaceId, i64>,
    application_stable_ids: HashMap<SurfaceId, String>,
}

impl DesktopExecutionBindingSnapshot {
    pub fn session_signature(&self) -> &str {
        &self.session_signature
    }

    pub const fn generation(&self) -> DesktopContextGeneration {
        self.generation
    }

    fn confirm_baseline(
        &self,
        baseline: &DesktopContextBaseline,
    ) -> Result<(), SemanticObservationError> {
        if baseline.generation != self.generation
            || semantic_digest_sha256(baseline) != self.semantic_digest_sha256
        {
            return Err(SemanticObservationError::SnapshotChanged);
        }
        Ok(())
    }

    /// Admit only against the semantic baseline paired with this exact native
    /// observation. The authority supplies its ephemeral observer epoch.
    pub fn activation_observation<'a>(
        &'a self,
        session_epoch: ActivationSessionEpoch,
        baseline: &'a DesktopContextBaseline,
    ) -> Result<CurrentActivationObservation<'a>, SemanticObservationError> {
        self.confirm_baseline(baseline)?;
        Ok(CurrentActivationObservation::with_runtime_session(
            session_epoch,
            baseline,
            &self.session_signature,
        ))
    }

    /// Preserve the exact semantic/native mapping from this source generation
    /// as read-only input to R13 pre-execution revalidation.
    pub fn execution_observation(
        &self,
        session_epoch: ActivationSessionEpoch,
        baseline: &DesktopContextBaseline,
    ) -> Result<ExecutionObservation, SemanticObservationError> {
        self.confirm_baseline(baseline)?;
        let workspaces = baseline
            .workspaces
            .iter()
            .map(|workspace| {
                ObservedWorkspaceBinding::with_focus(
                    workspace.workspace_id,
                    self.workspace_runtime_id(workspace.workspace_id),
                    workspace.focus == WorkspaceFocus::Focused,
                )
            })
            .collect();
        let surfaces = baseline
            .surfaces
            .iter()
            .map(|surface| {
                ObservedSurfaceBinding::with_focus(
                    surface.surface_id,
                    surface.role,
                    self.application_stable_id(surface.surface_id),
                    surface.focus == SurfaceFocus::Focused,
                )
            })
            .collect();
        Ok(ExecutionObservation::new(
            session_epoch,
            &self.session_signature,
            self.generation,
            workspaces,
            surfaces,
        ))
    }

    /// Returns the native numeric workspace ID only for the exact observed
    /// semantic ID in this snapshot.
    pub fn workspace_runtime_id(&self, id: WorkspaceId) -> Option<i64> {
        self.workspace_runtime_ids.get(&id).copied()
    }

    /// Returns the compositor's exact session-scoped stable window selector
    /// only for the exact observed application SurfaceId in this snapshot.
    pub fn application_stable_id(&self, id: SurfaceId) -> Option<&str> {
        self.application_stable_ids.get(&id).map(String::as_str)
    }
}

struct BuiltSemanticCandidate {
    candidate: DesktopContextCandidate,
    workspace_bindings: HashMap<WorkspaceId, i64>,
    application_stable_ids: HashMap<SurfaceId, String>,
}

impl Default for DesktopSemanticRuntime {
    fn default() -> Self {
        Self {
            baseline: DesktopContextBaseline::empty(),
        }
    }
}

impl DesktopSemanticRuntime {
    pub fn baseline(&self) -> &DesktopContextBaseline {
        &self.baseline
    }

    pub fn reconcile(
        &mut self,
        candidate: DesktopContextCandidate,
    ) -> Result<DesktopSemanticReceipt, SemanticObservationError> {
        let changed = match reconcile_context(&self.baseline, candidate) {
            TransitionOutcome::NoChange => false,
            TransitionOutcome::Changed(next) => {
                self.baseline = next;
                true
            }
            TransitionOutcome::InvalidCandidate(error) => {
                return Err(SemanticObservationError::InvalidCandidate(error));
            }
            TransitionOutcome::GenerationExhausted => {
                return Err(SemanticObservationError::GenerationExhausted);
            }
        };
        Ok(receipt_from_baseline(&self.baseline, changed))
    }
}

fn receipt_from_baseline(
    baseline: &DesktopContextBaseline,
    changed: bool,
) -> DesktopSemanticReceipt {
    DesktopSemanticReceipt {
        generation: baseline.generation.0,
        changed,
        output_count: baseline.outputs.len(),
        workspace_count: baseline.workspaces.len(),
        surface_count: baseline.surfaces.len(),
        visible_workspace_count: baseline
            .workspaces
            .iter()
            .filter(|workspace| workspace.visibility == WorkspaceVisibility::Visible)
            .count(),
        visible_surface_count: baseline
            .surfaces
            .iter()
            .filter(|surface| surface.visibility == SurfaceVisibility::Visible)
            .count(),
        focused_workspace_count: baseline
            .workspaces
            .iter()
            .filter(|workspace| workspace.focus == WorkspaceFocus::Focused)
            .count(),
        focused_surface_count: baseline
            .surfaces
            .iter()
            .filter(|surface| surface.focus == SurfaceFocus::Focused)
            .count(),
        digest_sha256: semantic_digest_sha256(baseline),
    }
}

fn semantic_digest_sha256(baseline: &DesktopContextBaseline) -> String {
    let mut rows = Vec::new();

    for output in baseline.outputs.iter() {
        rows.push(
            serde_json::to_string(&json!([
                "o",
                hex_id(output.output_hub_id.as_bytes()),
                output.connector.as_str(),
                output.active_workspace_id.map(|id| hex_id(id.as_bytes())),
                output.descriptors.make.as_deref(),
                output.descriptors.model.as_deref(),
                output.descriptors.serial.as_deref(),
                output.physical_size.width.get(),
                output.physical_size.height.get(),
                output.logical_position.x,
                output.logical_position.y,
                output.logical_size.width.get(),
                output.logical_size.height.get(),
                output.scale.as_milli(),
                output_transform_code(output.transform),
            ]))
            .expect("semantic output row must serialize"),
        );
    }

    for workspace in baseline.workspaces.iter() {
        rows.push(
            serde_json::to_string(&json!([
                "w",
                hex_id(workspace.workspace_id.as_bytes()),
                workspace.name.as_str(),
                workspace
                    .output_hub_id
                    .as_ref()
                    .map(|id| hex_id(id.as_bytes())),
                workspace_visibility_name(workspace.visibility),
                workspace_focus_name(workspace.focus),
            ]))
            .expect("semantic workspace row must serialize"),
        );
    }

    for surface in baseline.surfaces.iter() {
        rows.push(
            serde_json::to_string(&json!([
                "s",
                hex_id(surface.surface_id.as_bytes()),
                surface_role_name(surface.role),
                surface
                    .output_hub_id
                    .as_ref()
                    .map(|id| hex_id(id.as_bytes())),
                surface
                    .workspace_id
                    .as_ref()
                    .map(|id| hex_id(id.as_bytes())),
                surface.bounds.origin.x,
                surface.bounds.origin.y,
                surface.bounds.size.width.get(),
                surface.bounds.size.height.get(),
                surface.application.as_ref().map(|app| json!([
                    app.class,
                    app.initial_class,
                    app.title,
                    app.initial_title,
                    app.process_id,
                    app.xwayland,
                ])),
                surface_visibility_name(surface.visibility),
                surface_focus_name(surface.focus),
            ]))
            .expect("semantic surface row must serialize"),
        );
    }

    rows.sort();
    let mut hasher = Sha256::new();
    for row in rows {
        hasher.update(row.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

fn hex_id(bytes: &[u8; 16]) -> String {
    let mut value = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut value, "{byte:02x}").expect("writing into String cannot fail");
    }
    value
}

const fn output_transform_code(transform: OutputTransform) -> u8 {
    match transform {
        OutputTransform::Normal => 0,
        OutputTransform::Rotate90 => 1,
        OutputTransform::Rotate180 => 2,
        OutputTransform::Rotate270 => 3,
        OutputTransform::Flipped => 4,
        OutputTransform::Flipped90 => 5,
        OutputTransform::Flipped180 => 6,
        OutputTransform::Flipped270 => 7,
    }
}

const fn workspace_visibility_name(value: WorkspaceVisibility) -> &'static str {
    match value {
        WorkspaceVisibility::Hidden => "hidden",
        WorkspaceVisibility::Visible => "visible",
    }
}

const fn workspace_focus_name(value: WorkspaceFocus) -> &'static str {
    match value {
        WorkspaceFocus::Unfocused => "unfocused",
        WorkspaceFocus::Focused => "focused",
    }
}

const fn surface_role_name(value: DesktopSurfaceRole) -> &'static str {
    match value {
        DesktopSurfaceRole::Application => "application",
        DesktopSurfaceRole::Shell => "shell",
        DesktopSurfaceRole::RearPlane => "rear_plane",
        DesktopSurfaceRole::Overlay => "overlay",
        DesktopSurfaceRole::Unclassified => "unclassified",
    }
}

const fn surface_visibility_name(value: SurfaceVisibility) -> &'static str {
    match value {
        SurfaceVisibility::Visible => "visible",
        SurfaceVisibility::Hidden => "hidden",
        SurfaceVisibility::Unmapped => "unmapped",
    }
}

const fn surface_focus_name(value: SurfaceFocus) -> &'static str {
    match value {
        SurfaceFocus::Focused => "focused",
        SurfaceFocus::Unfocused => "unfocused",
        SurfaceFocus::Unknown => "unknown",
    }
}
impl HyprctlLuaBackend {
    pub fn observe_desktop_candidate(
        &self,
    ) -> Result<DesktopContextCandidate, SemanticObservationError> {
        self.observe_candidate_with_execution_bindings()
            .map(|observed| observed.candidate)
    }

    pub fn observe_and_reconcile_desktop(
        &self,
        runtime: &mut DesktopSemanticRuntime,
    ) -> Result<DesktopSemanticReceipt, SemanticObservationError> {
        runtime.reconcile(self.observe_desktop_candidate()?)
    }

    /// Reconcile one stable semantic observation and retain its exact native
    /// workspace/client selectors in an ephemeral sidecar. No compositor
    /// mutation is performed. The two source samples must agree both
    /// semantically and on native bindings before the caller's runtime is
    /// mutated.
    pub fn observe_and_reconcile_desktop_with_execution_bindings(
        &self,
        runtime: &mut DesktopSemanticRuntime,
    ) -> Result<(DesktopSemanticReceipt, DesktopExecutionBindingSnapshot), SemanticObservationError>
    {
        let observed = self.observe_candidate_with_execution_bindings()?;
        let receipt = runtime.reconcile(observed.candidate)?;
        let bindings = DesktopExecutionBindingSnapshot {
            session_signature: self.runtime_target().to_owned(),
            generation: runtime.baseline().generation,
            semantic_digest_sha256: receipt.digest_sha256.clone(),
            workspace_runtime_ids: observed.workspace_bindings,
            application_stable_ids: observed.application_stable_ids,
        };
        Ok((receipt, bindings))
    }

    fn observe_candidate_with_execution_bindings(
        &self,
    ) -> Result<BuiltSemanticCandidate, SemanticObservationError> {
        let output = crate::observation_transport::read_observation(
            self.runtime_target(),
            crate::observation_transport::ReadQuery::SemanticSnapshot,
        )
        .map_err(|_| SemanticObservationError::Transport)?;

        let values = Deserializer::from_slice(&output)
            .into_iter::<Value>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| SemanticObservationError::Parse)?;
        if values.len() != SEMANTIC_VALUE_COUNT {
            return Err(SemanticObservationError::Parse);
        }

        let first = build_candidate_with_execution_bindings(
            self.runtime_target(),
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )?;
        let second = build_candidate_with_execution_bindings(
            self.runtime_target(),
            &values[6],
            &values[7],
            &values[8],
            &values[9],
            &values[10],
            &values[11],
        )?;

        ensure_stable_semantic_snapshot(first.candidate, second.candidate.clone())?;
        if first.workspace_bindings != second.workspace_bindings
            || first.application_stable_ids != second.application_stable_ids
        {
            return Err(SemanticObservationError::ExecutionBindingsChanged);
        }
        Ok(second)
    }
}

fn ensure_stable_semantic_snapshot(
    first: DesktopContextCandidate,
    second: DesktopContextCandidate,
) -> Result<(), SemanticObservationError> {
    let mut probe = DesktopSemanticRuntime::default();
    probe.reconcile(first)?;
    let second = probe.reconcile(second)?;
    if second.changed {
        Err(SemanticObservationError::SnapshotChanged)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn build_candidate(
    session: &str,
    monitors: &Value,
    workspaces: &Value,
    clients: &Value,
    active_workspace: &Value,
    active_window: &Value,
    layers: &Value,
) -> Result<DesktopContextCandidate, SemanticObservationError> {
    Ok(build_candidate_with_execution_bindings(
        session,
        monitors,
        workspaces,
        clients,
        active_workspace,
        active_window,
        layers,
    )?
    .candidate)
}

fn build_candidate_with_execution_bindings(
    session: &str,
    monitors: &Value,
    workspaces: &Value,
    clients: &Value,
    active_workspace: &Value,
    active_window: &Value,
    layers: &Value,
) -> Result<BuiltSemanticCandidate, SemanticObservationError> {
    if session.trim().is_empty() {
        return Err(SemanticObservationError::Parse);
    }

    let (outputs, output_by_runtime_id, output_by_name, visible_workspace_ids, focused_monitor_id) =
        parse_outputs(session, monitors)?;
    let focused_workspace_id = optional_object_i64(active_workspace, "id")?.filter(|id| *id != 0);
    let (workspaces, workspace_ids) = parse_workspaces(
        session,
        workspaces,
        &output_by_runtime_id,
        &visible_workspace_ids,
        focused_workspace_id,
        focused_monitor_id,
    )?;
    let mut workspace_bindings = HashMap::with_capacity(workspace_ids.len());
    for (native_id, semantic_id) in &workspace_ids {
        if workspace_bindings
            .insert(*semantic_id, *native_id)
            .is_some()
        {
            return Err(SemanticObservationError::DuplicateExecutionBinding);
        }
    }
    if visible_workspace_ids
        .iter()
        .any(|workspace_id| !workspace_ids.contains_key(workspace_id))
        || focused_workspace_id
            .is_some_and(|workspace_id| !workspace_ids.contains_key(&workspace_id))
    {
        return Err(SemanticObservationError::UnknownWorkspace);
    }
    let active_window_address = optional_object_string(active_window, "address")?;
    let active_window_identity = optional_object_string(active_window, "stableId")?
        .or_else(|| active_window_address.clone());
    let (mut surfaces, application_stable_ids) = parse_clients(
        session,
        clients,
        &output_by_runtime_id,
        &workspace_ids,
        active_window_identity.as_deref(),
    )?;
    if let Some(identity) = active_window_identity.as_deref()
        && !identity.trim().is_empty()
        && !surfaces.iter().any(|surface| {
            surface.focus == SurfaceFocus::Focused
                && surface.surface_id == surface_id(session, "client", identity)
        })
    {
        return Err(SemanticObservationError::FocusedClientMissing);
    }
    surfaces.extend(parse_layers(session, layers, &output_by_name)?);

    Ok(BuiltSemanticCandidate {
        candidate: DesktopContextCandidate {
            outputs,
            workspaces,
            surfaces,
        },
        workspace_bindings,
        application_stable_ids,
    })
}
type OutputParse = (
    Vec<OutputState>,
    HashMap<i64, OutputHubId>,
    HashMap<String, OutputHubId>,
    HashSet<i64>,
    Option<i64>,
);

fn parse_outputs(session: &str, value: &Value) -> Result<OutputParse, SemanticObservationError> {
    let monitors = value.as_array().ok_or(SemanticObservationError::Parse)?;
    let mut outputs = Vec::with_capacity(monitors.len());
    let mut by_runtime_id = HashMap::with_capacity(monitors.len());
    let mut by_name = HashMap::with_capacity(monitors.len());
    let mut visible_workspace_ids = HashSet::new();
    let mut focused_monitor_id = None;

    for monitor in monitors {
        let runtime_id = required_i64(monitor, "id")?;
        let name = required_string(monitor, "name")?;
        if name.trim().is_empty() {
            return Err(SemanticObservationError::Parse);
        }
        if by_runtime_id.contains_key(&runtime_id) || by_name.contains_key(name) {
            return Err(SemanticObservationError::Parse);
        }
        let width = positive_u32(monitor, "width")?;
        let height = positive_u32(monitor, "height")?;
        let x = i32_value(monitor, "x")?;
        let y = i32_value(monitor, "y")?;
        let scale = required_f64(monitor, "scale")?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(SemanticObservationError::NumericRange);
        }
        let scale_milli_f = (scale * 1000.0).round();
        if !(1.0..=u32::MAX as f64).contains(&scale_milli_f) {
            return Err(SemanticObservationError::NumericRange);
        }
        let scale_milli = scale_milli_f as u32;
        let transform_raw = required_i64(monitor, "transform")?;
        let transform = output_transform(transform_raw)?;

        let mut logical_width = ((width as f64) / scale).round();
        let mut logical_height = ((height as f64) / scale).round();
        if !logical_width.is_finite()
            || !logical_height.is_finite()
            || logical_width < 1.0
            || logical_height < 1.0
            || logical_width > u32::MAX as f64
            || logical_height > u32::MAX as f64
        {
            return Err(SemanticObservationError::NumericRange);
        }
        if matches!(
            transform,
            OutputTransform::Rotate90
                | OutputTransform::Rotate270
                | OutputTransform::Flipped90
                | OutputTransform::Flipped270
        ) {
            std::mem::swap(&mut logical_width, &mut logical_height);
        }

        let hub_id = output_hub_id(session, runtime_id, name);
        by_runtime_id.insert(runtime_id, hub_id);
        by_name.insert(name.to_owned(), hub_id);

        let active_id = nested_i64(monitor, "activeWorkspace", "id")?;
        if active_id != 0 {
            visible_workspace_ids.insert(active_id);
        }
        let special_id = nested_i64(monitor, "specialWorkspace", "id")?;
        if special_id != 0 {
            visible_workspace_ids.insert(special_id);
        }
        if optional_bool(monitor, "focused")?.unwrap_or(false)
            && focused_monitor_id.replace(runtime_id).is_some()
        {
            return Err(SemanticObservationError::Parse);
        }

        outputs.push(OutputState {
            output_hub_id: hub_id,
            connector: ConnectorName::new(name.to_owned())
                .map_err(|_| SemanticObservationError::Parse)?,
            active_workspace_id: (active_id != 0).then(|| workspace_id(session, active_id)),
            descriptors: PhysicalOutputDescriptors {
                make: optional_string(monitor, "make")?,
                model: optional_string(monitor, "model")?,
                serial: optional_string(monitor, "serial")?,
            },
            physical_size: PhysicalPixelSize::new(width, height)
                .ok_or(SemanticObservationError::NumericRange)?,
            logical_position: LogicalPoint::new(x, y),
            logical_size: LogicalSize::new(logical_width as u32, logical_height as u32)
                .ok_or(SemanticObservationError::NumericRange)?,
            scale: ScaleMilli::from_milli(scale_milli)
                .map_err(|_| SemanticObservationError::NumericRange)?,
            transform,
        });
    }

    Ok((
        outputs,
        by_runtime_id,
        by_name,
        visible_workspace_ids,
        focused_monitor_id,
    ))
}
fn parse_workspaces(
    session: &str,
    value: &Value,
    outputs: &HashMap<i64, OutputHubId>,
    visible_workspace_ids: &HashSet<i64>,
    focused_workspace_id: Option<i64>,
    focused_monitor_id: Option<i64>,
) -> Result<(Vec<WorkspaceState>, HashMap<i64, WorkspaceId>), SemanticObservationError> {
    let items = value.as_array().ok_or(SemanticObservationError::Parse)?;
    let mut workspace_ids = HashMap::with_capacity(items.len());
    for workspace in items {
        let runtime_id = required_i64(workspace, "id")?;
        if workspace_ids
            .insert(runtime_id, workspace_id(session, runtime_id))
            .is_some()
        {
            return Err(SemanticObservationError::Parse);
        }
    }

    let mut result = Vec::with_capacity(items.len());
    for workspace in items {
        let runtime_id = required_i64(workspace, "id")?;
        let monitor_id = required_i64(workspace, "monitorID")?;
        let output_hub_id = if monitor_id < 0 {
            None
        } else {
            Some(
                *outputs
                    .get(&monitor_id)
                    .ok_or(SemanticObservationError::UnknownOutput)?,
            )
        };
        let focus = if focused_workspace_id == Some(runtime_id)
            && focused_monitor_id.is_some_and(|monitor| monitor == monitor_id)
        {
            WorkspaceFocus::Focused
        } else {
            WorkspaceFocus::Unfocused
        };
        result.push(WorkspaceState {
            workspace_id: *workspace_ids
                .get(&runtime_id)
                .ok_or(SemanticObservationError::Parse)?,
            name: WorkspaceName::new(required_string(workspace, "name")?.to_owned())
                .map_err(|_| SemanticObservationError::Parse)?,
            output_hub_id,
            visibility: if visible_workspace_ids.contains(&runtime_id) {
                WorkspaceVisibility::Visible
            } else {
                WorkspaceVisibility::Hidden
            },
            focus,
        });
    }

    Ok((result, workspace_ids))
}

fn parse_clients(
    session: &str,
    value: &Value,
    outputs: &HashMap<i64, OutputHubId>,
    workspaces: &HashMap<i64, WorkspaceId>,
    active_window_identity: Option<&str>,
) -> Result<(Vec<SurfaceState>, HashMap<SurfaceId, String>), SemanticObservationError> {
    let clients = value.as_array().ok_or(SemanticObservationError::Parse)?;
    let mut result = Vec::with_capacity(clients.len());
    let mut application_stable_ids = HashMap::with_capacity(clients.len());
    let mut seen_stable_ids = HashSet::with_capacity(clients.len());

    for client in clients {
        let address = required_string(client, "address")?;
        if address.trim().is_empty() {
            return Err(SemanticObservationError::Parse);
        }
        let monitor_id = required_i64(client, "monitor")?;
        let output_hub_id = if monitor_id < 0 {
            None
        } else {
            Some(
                *outputs
                    .get(&monitor_id)
                    .ok_or(SemanticObservationError::UnknownOutput)?,
            )
        };
        let workspace_runtime_id = nested_i64(client, "workspace", "id")?;
        let workspace_id = if workspace_runtime_id == 0 {
            None
        } else {
            Some(
                *workspaces
                    .get(&workspace_runtime_id)
                    .ok_or(SemanticObservationError::UnknownWorkspace)?,
            )
        };
        let at = required_i64_pair(client, "at")?;
        let size = required_i64_pair(client, "size")?;
        let width = positive_i64_to_u32(size[0])?;
        let height = positive_i64_to_u32(size[1])?;
        let mapped = required_bool(client, "mapped")?;
        let hidden = required_bool(client, "hidden")?;
        let visible = required_bool(client, "visible")?;

        let stable_id = optional_string(client, "stableId")?;
        if let Some(stable_id) = stable_id.as_ref() {
            if stable_id.is_empty() || !stable_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(SemanticObservationError::Parse);
            }
            if !seen_stable_ids.insert(stable_id.clone()) {
                return Err(SemanticObservationError::DuplicateExecutionBinding);
            }
        }
        // Hyprland's per-window stableId is monotone within the compositor
        // session. Prefer it for semantic identity so a recycled pointer
        // address cannot make a newly created window inherit an old
        // SurfaceId. Older/incomplete observations retain their existing
        // address-derived semantic identity but receive no execution binding.
        let id = surface_id(session, "client", stable_id.as_deref().unwrap_or(address));
        if let Some(stable_id) = stable_id.as_ref()
            && application_stable_ids
                .insert(id, stable_id.clone())
                .is_some()
        {
            return Err(SemanticObservationError::DuplicateExecutionBinding);
        }
        result.push(SurfaceState {
            surface_id: id,
            role: DesktopSurfaceRole::Application,
            output_hub_id,
            workspace_id,
            bounds: LogicalRect::new(
                LogicalPoint::new(i64_to_i32(at[0])?, i64_to_i32(at[1])?),
                LogicalSize::new(width, height).ok_or(SemanticObservationError::NumericRange)?,
            ),
            application: Some(ApplicationSurfaceDescriptors {
                class: required_string(client, "class")?.to_owned(),
                initial_class: required_string(client, "initialClass")?.to_owned(),
                title: required_string(client, "title")?.to_owned(),
                initial_title: required_string(client, "initialTitle")?.to_owned(),
                process_id: nonnegative_i64_to_u32(required_i64(client, "pid")?)?,
                xwayland: required_bool(client, "xwayland")?,
            }),
            visibility: if !mapped {
                SurfaceVisibility::Unmapped
            } else if visible && !hidden {
                SurfaceVisibility::Visible
            } else {
                SurfaceVisibility::Hidden
            },
            focus: if active_window_identity == Some(stable_id.as_deref().unwrap_or(address)) {
                SurfaceFocus::Focused
            } else {
                SurfaceFocus::Unfocused
            },
        });
    }

    Ok((result, application_stable_ids))
}
fn parse_layers(
    session: &str,
    value: &Value,
    outputs: &HashMap<String, OutputHubId>,
) -> Result<Vec<SurfaceState>, SemanticObservationError> {
    let monitors = value.as_object().ok_or(SemanticObservationError::Parse)?;
    let mut result = Vec::new();

    for (output_name, monitor) in monitors {
        let output_hub_id = *outputs
            .get(output_name)
            .ok_or(SemanticObservationError::UnknownOutput)?;
        let levels = monitor
            .get("levels")
            .and_then(Value::as_object)
            .ok_or(SemanticObservationError::Parse)?;
        for (level, surfaces) in levels {
            let level = level
                .parse::<u8>()
                .map_err(|_| SemanticObservationError::Parse)?;
            let role = match level {
                0 => DesktopSurfaceRole::RearPlane,
                1 | 2 => DesktopSurfaceRole::Shell,
                3 => DesktopSurfaceRole::Overlay,
                _ => DesktopSurfaceRole::Unclassified,
            };
            let surfaces = surfaces.as_array().ok_or(SemanticObservationError::Parse)?;
            for surface in surfaces {
                let address = required_string(surface, "address")?;
                if address.trim().is_empty() {
                    return Err(SemanticObservationError::Parse);
                }
                let width = positive_u32(surface, "w")?;
                let height = positive_u32(surface, "h")?;
                result.push(SurfaceState {
                    surface_id: surface_id(session, "layer", address),
                    role,
                    output_hub_id: Some(output_hub_id),
                    workspace_id: None,
                    bounds: LogicalRect::new(
                        LogicalPoint::new(i32_value(surface, "x")?, i32_value(surface, "y")?),
                        LogicalSize::new(width, height)
                            .ok_or(SemanticObservationError::NumericRange)?,
                    ),
                    application: None,
                    visibility: SurfaceVisibility::Visible,
                    focus: SurfaceFocus::Unknown,
                });
            }
        }
    }

    Ok(result)
}

fn scoped_uuid(session: &str, kind: &str, identity: &str) -> [u8; 16] {
    let material = format!("gnosix-session:{session}:{kind}:{identity}");
    *Uuid::new_v5(&Uuid::NAMESPACE_OID, material.as_bytes()).as_bytes()
}

fn output_hub_id(session: &str, runtime_id: i64, name: &str) -> OutputHubId {
    OutputHubId::from_bytes(scoped_uuid(
        session,
        "output",
        &format!("{runtime_id}:{name}"),
    ))
}

fn workspace_id(session: &str, runtime_id: i64) -> WorkspaceId {
    WorkspaceId::from_bytes(scoped_uuid(session, "workspace", &runtime_id.to_string()))
}

fn surface_id(session: &str, kind: &str, address: &str) -> SurfaceId {
    SurfaceId::from_bytes(scoped_uuid(session, kind, address))
}
fn output_transform(value: i64) -> Result<OutputTransform, SemanticObservationError> {
    match value {
        0 => Ok(OutputTransform::Normal),
        1 => Ok(OutputTransform::Rotate90),
        2 => Ok(OutputTransform::Rotate180),
        3 => Ok(OutputTransform::Rotate270),
        4 => Ok(OutputTransform::Flipped),
        5 => Ok(OutputTransform::Flipped90),
        6 => Ok(OutputTransform::Flipped180),
        7 => Ok(OutputTransform::Flipped270),
        _ => Err(SemanticObservationError::UnsupportedTransform),
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, SemanticObservationError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(SemanticObservationError::Parse)
}

fn optional_string(value: &Value, key: &str) -> Result<Option<String>, SemanticObservationError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or(SemanticObservationError::Parse),
    }
}

fn optional_object_string(
    value: &Value,
    key: &str,
) -> Result<Option<String>, SemanticObservationError> {
    let Some(object) = value.as_object() else {
        return Err(SemanticObservationError::Parse);
    };
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| {
                if value.trim().is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                }
            })
            .ok_or(SemanticObservationError::Parse),
    }
}

fn required_i64(value: &Value, key: &str) -> Result<i64, SemanticObservationError> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or(SemanticObservationError::Parse)
}

fn required_f64(value: &Value, key: &str) -> Result<f64, SemanticObservationError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or(SemanticObservationError::Parse)
}

fn required_bool(value: &Value, key: &str) -> Result<bool, SemanticObservationError> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or(SemanticObservationError::Parse)
}

fn optional_bool(value: &Value, key: &str) -> Result<Option<bool>, SemanticObservationError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or(SemanticObservationError::Parse),
    }
}
fn optional_object_i64(value: &Value, key: &str) -> Result<Option<i64>, SemanticObservationError> {
    let Some(object) = value.as_object() else {
        return Err(SemanticObservationError::Parse);
    };
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or(SemanticObservationError::Parse),
    }
}

fn optional_nested_i64(
    value: &Value,
    object: &str,
    key: &str,
) -> Result<Option<i64>, SemanticObservationError> {
    let Some(root) = value.as_object() else {
        return Err(SemanticObservationError::Parse);
    };
    let Some(nested) = root.get(object) else {
        return Ok(None);
    };
    let nested = nested.as_object().ok_or(SemanticObservationError::Parse)?;
    match nested.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or(SemanticObservationError::Parse),
    }
}

fn nested_i64(value: &Value, object: &str, key: &str) -> Result<i64, SemanticObservationError> {
    optional_nested_i64(value, object, key)?.ok_or(SemanticObservationError::Parse)
}

fn required_i64_pair(value: &Value, key: &str) -> Result<[i64; 2], SemanticObservationError> {
    let pair = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or(SemanticObservationError::Parse)?;
    if pair.len() != 2 {
        return Err(SemanticObservationError::Parse);
    }
    Ok([
        pair[0].as_i64().ok_or(SemanticObservationError::Parse)?,
        pair[1].as_i64().ok_or(SemanticObservationError::Parse)?,
    ])
}

fn i64_to_i32(value: i64) -> Result<i32, SemanticObservationError> {
    i32::try_from(value).map_err(|_| SemanticObservationError::NumericRange)
}

fn i32_value(value: &Value, key: &str) -> Result<i32, SemanticObservationError> {
    i64_to_i32(required_i64(value, key)?)
}

fn nonnegative_i64_to_u32(value: i64) -> Result<u32, SemanticObservationError> {
    if value < 0 {
        return Err(SemanticObservationError::NumericRange);
    }
    u32::try_from(value).map_err(|_| SemanticObservationError::NumericRange)
}

fn positive_i64_to_u32(value: i64) -> Result<u32, SemanticObservationError> {
    if value <= 0 {
        return Err(SemanticObservationError::NumericRange);
    }
    u32::try_from(value).map_err(|_| SemanticObservationError::NumericRange)
}

fn positive_u32(value: &Value, key: &str) -> Result<u32, SemanticObservationError> {
    positive_i64_to_u32(required_i64(value, key)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> [Value; 6] {
        [
            json!([{
                "id": 0, "name": "DP-1", "make": "Vendor", "model": "Panel", "serial": "ABC",
                "width": 1920, "height": 1080, "x": 0, "y": 0, "scale": 1.0,
                "transform": 0, "focused": true,
                "activeWorkspace": {"id": 1, "name": "1"},
                "specialWorkspace": {"id": 0, "name": ""}
            }]),
            json!([{
                "id": 1, "name": "1", "monitor": "DP-1", "monitorID": 0,
                "windows": 2, "hasfullscreen": false, "ispersistent": false,
                "lastwindow": "0xabc", "lastwindowtitle": "A", "tiledLayout": "dwindle"
            }]),
            json!([
                {
                    "address": "0xabc", "mapped": true, "hidden": false, "visible": true,
                    "stableId": "18000001",
                    "at": [10, 20], "size": [800, 600], "workspace": {"id": 1, "name": "1"},
                    "monitor": 0, "class": "org.example.App", "initialClass": "org.example.App",
                    "title": "Document A", "initialTitle": "Document A", "pid": 101, "xwayland": false
                },
                {
                    "address": "0xdef", "mapped": true, "hidden": true, "visible": false,
                    "stableId": "18000002",
                    "at": [40, 50], "size": [640, 480], "workspace": {"id": 1, "name": "1"},
                    "monitor": 0, "class": "org.example.Tool", "initialClass": "org.example.Tool",
                    "title": "Tool", "initialTitle": "Tool", "pid": 202, "xwayland": true
                }
            ]),
            json!({"id": 1, "name": "1", "monitor": "DP-1", "monitorID": 0}),
            json!({"address": "0xabc", "stableId": "18000001"}),
            json!({
                "DP-1": {
                    "levels": {
                        "0": [{"address": "0x100", "x": 0, "y": 0, "w": 1920, "h": 1080}],
                        "2": [{"address": "0x200", "x": 0, "y": 0, "w": 1920, "h": 32}],
                        "3": [{"address": "0x300", "x": 100, "y": 100, "w": 400, "h": 300}]
                    }
                }
            }),
        ]
    }

    #[test]
    fn semantic_snapshot_maps_outputs_workspaces_clients_layers_focus_and_visibility() {
        let values = fixture();
        let candidate = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("candidate");

        assert_eq!(candidate.outputs.len(), 1);
        assert_eq!(candidate.workspaces.len(), 1);
        assert_eq!(candidate.surfaces.len(), 5);
        assert_eq!(candidate.workspaces[0].name.as_str(), "1");
        assert_eq!(candidate.workspaces[0].focus, WorkspaceFocus::Focused);
        assert_eq!(
            candidate.workspaces[0].visibility,
            WorkspaceVisibility::Visible
        );

        let focused = candidate
            .surfaces
            .iter()
            .filter(|surface| surface.focus == SurfaceFocus::Focused)
            .collect::<Vec<_>>();
        assert_eq!(focused.len(), 1);
        assert_eq!(focused[0].role, DesktopSurfaceRole::Application);
        let application = focused[0]
            .application
            .as_ref()
            .expect("application descriptors");
        assert_eq!(application.class, "org.example.App");
        assert_eq!(application.title, "Document A");
        assert_eq!(application.process_id, 101);
        assert!(!application.xwayland);
        assert_eq!(focused[0].visibility, SurfaceVisibility::Visible);

        assert!(candidate.surfaces.iter().any(|surface| {
            surface.role == DesktopSurfaceRole::Application
                && surface.visibility == SurfaceVisibility::Hidden
        }));
        assert!(
            candidate
                .surfaces
                .iter()
                .any(|surface| surface.role == DesktopSurfaceRole::RearPlane)
        );
        assert!(
            candidate
                .surfaces
                .iter()
                .any(|surface| surface.role == DesktopSurfaceRole::Shell)
        );
        assert!(
            candidate
                .surfaces
                .iter()
                .any(|surface| surface.role == DesktopSurfaceRole::Overlay)
        );
    }

    #[test]
    fn execution_binding_snapshot_preserves_exact_workspace_ids_and_window_stable_ids() {
        let values = fixture();
        let observed = build_candidate_with_execution_bindings(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("semantic candidate and exact native bindings");

        let snapshot = DesktopExecutionBindingSnapshot {
            session_signature: "session-a".to_owned(),
            generation: DesktopContextGeneration(7),
            semantic_digest_sha256: String::new(),
            workspace_runtime_ids: observed.workspace_bindings,
            application_stable_ids: observed.application_stable_ids,
        };

        assert_eq!(snapshot.session_signature(), "session-a");
        assert_eq!(snapshot.generation(), DesktopContextGeneration(7));
        assert_eq!(
            snapshot.workspace_runtime_id(workspace_id("session-a", 1)),
            Some(1)
        );
        assert_eq!(
            snapshot.application_stable_id(surface_id("session-a", "client", "18000001")),
            Some("18000001")
        );
        assert_eq!(
            snapshot.application_stable_id(surface_id("session-a", "client", "18000002")),
            Some("18000002")
        );
        assert_eq!(snapshot.application_stable_ids.len(), 2);
        assert_eq!(
            snapshot.application_stable_id(surface_id("session-b", "client", "18000001")),
            None
        );
    }

    #[test]
    fn activation_freshness_uses_the_same_semantic_digest_and_runtime_session() {
        let values = fixture();
        let observed = build_candidate_with_execution_bindings(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .unwrap();
        let mut runtime = DesktopSemanticRuntime::default();
        let receipt = runtime.reconcile(observed.candidate).unwrap();
        let snapshot = DesktopExecutionBindingSnapshot {
            session_signature: "session-a".to_owned(),
            generation: runtime.baseline().generation,
            semantic_digest_sha256: receipt.digest_sha256,
            workspace_runtime_ids: observed.workspace_bindings,
            application_stable_ids: observed.application_stable_ids,
        };
        let epoch = ActivationSessionEpoch::from_bytes([42; 16]);
        let admission = snapshot
            .activation_observation(epoch, runtime.baseline())
            .unwrap();
        assert_eq!(admission.runtime_session(), Some("session-a"));
        assert_eq!(admission.freshness().generation(), snapshot.generation());
        let execution = snapshot
            .execution_observation(epoch, runtime.baseline())
            .unwrap();
        assert_eq!(execution.runtime_session(), "session-a");
        assert_eq!(execution.generation(), snapshot.generation());
        assert_eq!(
            execution
                .workspace_binding(workspace_id("session-a", 1))
                .unwrap()
                .native_id(),
            Some(1)
        );
        assert!(
            execution
                .workspace_binding(workspace_id("session-a", 1))
                .unwrap()
                .is_focused()
        );
        assert_eq!(
            execution
                .surface_binding(surface_id("session-a", "client", "18000001"))
                .unwrap()
                .stable_id(),
            Some("18000001")
        );
        assert!(
            execution
                .surface_binding(surface_id("session-a", "client", "18000001"))
                .unwrap()
                .is_focused()
        );
        assert!(
            !execution
                .surface_binding(surface_id("session-a", "client", "18000002"))
                .unwrap()
                .is_focused()
        );

        let mut changed = values;
        changed[2][0]["title"] = json!("Changed without changing generation number");
        let changed_candidate = build_candidate(
            "session-a",
            &changed[0],
            &changed[1],
            &changed[2],
            &changed[3],
            &changed[4],
            &changed[5],
        )
        .unwrap();
        let mut other_runtime = DesktopSemanticRuntime::default();
        other_runtime.reconcile(changed_candidate).unwrap();
        assert_eq!(other_runtime.baseline().generation, snapshot.generation());
        assert!(matches!(
            snapshot.activation_observation(epoch, other_runtime.baseline()),
            Err(SemanticObservationError::SnapshotChanged)
        ));
        assert!(matches!(
            snapshot.execution_observation(epoch, other_runtime.baseline()),
            Err(SemanticObservationError::SnapshotChanged)
        ));
    }

    #[test]
    fn duplicate_application_descriptors_still_resolve_to_distinct_stable_ids() {
        let mut values = fixture();
        values[2][1]["class"] = json!("org.example.App");
        values[2][1]["title"] = json!("Document A");
        let observed = build_candidate_with_execution_bindings(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("duplicate descriptive attributes do not merge identities");

        assert_eq!(
            observed
                .application_stable_ids
                .get(&surface_id("session-a", "client", "18000001"))
                .map(String::as_str),
            Some("18000001")
        );
        assert_eq!(
            observed
                .application_stable_ids
                .get(&surface_id("session-a", "client", "18000002"))
                .map(String::as_str),
            Some("18000002")
        );
        assert_ne!(
            surface_id("session-a", "client", "18000001"),
            surface_id("session-a", "client", "18000002")
        );
    }

    #[test]
    fn new_window_stable_id_advances_semantic_generation_even_if_address_repeats() {
        let first = fixture();
        let mut replacement = first.clone();
        replacement[2][0]["stableId"] = json!("18000003");
        replacement[4]["stableId"] = json!("18000003");

        let build = |values: &[Value; 6]| {
            build_candidate(
                "session-a",
                &values[0],
                &values[1],
                &values[2],
                &values[3],
                &values[4],
                &values[5],
            )
            .expect("stable window IDs identify the observed app instances")
        };
        let first_candidate = build(&first);
        let replacement_candidate = build(&replacement);
        let first_id = first_candidate
            .surfaces
            .iter()
            .find(|surface| {
                surface.role == DesktopSurfaceRole::Application
                    && surface.focus == SurfaceFocus::Focused
            })
            .expect("focused application")
            .surface_id;
        let replacement_id = replacement_candidate
            .surfaces
            .iter()
            .find(|surface| {
                surface.role == DesktopSurfaceRole::Application
                    && surface.focus == SurfaceFocus::Focused
            })
            .expect("replacement focused application")
            .surface_id;

        assert_ne!(first_id, replacement_id);
        assert_eq!(first[2][0]["address"], replacement[2][0]["address"]);

        let mut runtime = DesktopSemanticRuntime::default();
        let initial = runtime
            .reconcile(first_candidate)
            .expect("initial reconcile");
        let changed = runtime
            .reconcile(replacement_candidate)
            .expect("replacement reconcile");
        assert!(changed.changed);
        assert_eq!(changed.generation, initial.generation + 1);
    }

    #[test]
    fn missing_stable_window_id_leaves_semantics_observable_but_not_executable() {
        let mut values = fixture();
        values[2][0].as_object_mut().unwrap().remove("stableId");
        values[4].as_object_mut().unwrap().remove("stableId");
        let observed = build_candidate_with_execution_bindings(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("missing optional execution binding does not break semantic observation");

        assert!(
            observed.candidate.surfaces.iter().any(|surface| {
                surface.surface_id == surface_id("session-a", "client", "0xabc")
            })
        );
        assert!(!observed.application_stable_ids.contains_key(&surface_id(
            "session-a",
            "client",
            "0xabc"
        )));
        assert_eq!(
            observed
                .application_stable_ids
                .get(&surface_id("session-a", "client", "18000002"))
                .map(String::as_str),
            Some("18000002")
        );
    }

    #[test]
    fn duplicate_stable_window_ids_fail_closed_for_execution_binding() {
        let mut values = fixture();
        values[2][1]["stableId"] = json!("18000001");

        assert!(matches!(
            build_candidate_with_execution_bindings(
                "session-a",
                &values[0],
                &values[1],
                &values[2],
                &values[3],
                &values[4],
                &values[5],
            ),
            Err(SemanticObservationError::DuplicateExecutionBinding)
        ));
    }
    #[test]
    fn session_scoped_ids_are_deterministic_but_not_cross_session() {
        let same_a = workspace_id("session-a", 7);
        let same_b = workspace_id("session-a", 7);
        let other_session = workspace_id("session-b", 7);
        assert_eq!(same_a, same_b);
        assert_ne!(same_a, other_session);

        let output_a = output_hub_id("session-a", 3, "DP-1");
        let reattach = output_hub_id("session-a", 4, "DP-1");
        assert_ne!(output_a, reattach);
    }

    #[test]
    fn monitor_visible_workspace_references_must_resolve() {
        let mut active_missing = fixture();
        active_missing[1] = json!([]);
        active_missing[2] = json!([]);
        active_missing[4] = json!({});
        assert!(matches!(
            build_candidate(
                "session-a",
                &active_missing[0],
                &active_missing[1],
                &active_missing[2],
                &active_missing[3],
                &active_missing[4],
                &active_missing[5],
            ),
            Err(SemanticObservationError::UnknownWorkspace)
        ));

        let mut special_missing = fixture();
        special_missing[0][0]["specialWorkspace"] = json!({"id": -99, "name": "special:scratch"});
        assert!(matches!(
            build_candidate(
                "session-a",
                &special_missing[0],
                &special_missing[1],
                &special_missing[2],
                &special_missing[3],
                &special_missing[4],
                &special_missing[5],
            ),
            Err(SemanticObservationError::UnknownWorkspace)
        ));
    }

    #[test]
    fn unknown_surface_association_is_rejected() {
        let mut values = fixture();
        values[2][0]["monitor"] = json!(99);
        assert!(matches!(
            build_candidate(
                "session-a",
                &values[0],
                &values[1],
                &values[2],
                &values[3],
                &values[4],
                &values[5],
            ),
            Err(SemanticObservationError::UnknownOutput)
        ));
    }

    #[test]
    fn detached_workspace_and_client_associations_remain_admitted() {
        let mut values = fixture();
        values[1][0]["monitorID"] = json!(-1);
        values[2][0]["monitor"] = json!(-1);
        values[3] = json!({});
        values[4] = json!({});

        let candidate = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("detached associations are valid observations");

        assert_eq!(candidate.workspaces[0].output_hub_id, None);
        assert_eq!(candidate.workspaces[0].focus, WorkspaceFocus::Unfocused);
        assert_eq!(candidate.surfaces[0].output_hub_id, None);
        assert_eq!(
            candidate.surfaces[0].workspace_id,
            Some(candidate.workspaces[0].workspace_id)
        );
    }

    #[test]
    fn negative_special_workspace_identity_is_resolved_and_visible() {
        let mut values = fixture();
        values[0][0]["specialWorkspace"] = json!({"id": -99, "name": "special:scratch"});
        values[1] = json!([
            {
                "id": 1, "name": "1", "monitor": "DP-1", "monitorID": 0,
                "windows": 1, "hasfullscreen": false, "ispersistent": false
            },
            {
                "id": -99, "name": "special:scratch", "monitor": "DP-1", "monitorID": 0,
                "windows": 1, "hasfullscreen": false, "ispersistent": false
            }
        ]);
        values[2][0]["workspace"] = json!({"id": -99, "name": "special:scratch"});
        values[2].as_array_mut().expect("clients").truncate(1);
        values[4] = json!({"address": "0xabc", "stableId": "18000001"});

        let candidate = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("special workspace is a valid current-session association");

        let special = candidate
            .workspaces
            .iter()
            .find(|workspace| workspace.name.as_str() == "special:scratch")
            .expect("special workspace");
        assert_eq!(special.visibility, WorkspaceVisibility::Visible);
        assert_eq!(
            candidate.surfaces[0].workspace_id,
            Some(special.workspace_id)
        );
    }

    #[test]
    fn output_active_workspace_is_independent_of_global_focus_and_special_visibility() {
        let mut values = fixture();
        values[0].as_array_mut().unwrap().push(json!({
            "id": 1, "name": "HDMI-A-1", "width": 1280, "height": 800,
            "x": 1920, "y": 0, "scale": 1.0, "transform": 0, "focused": false,
            "activeWorkspace": {"id": 11, "name": "11"},
            "specialWorkspace": {"id": -99, "name": "special:scratch"}
        }));
        values[1].as_array_mut().unwrap().extend([
            json!({"id": 11, "name": "11", "monitor": "HDMI-A-1", "monitorID": 1}),
            json!({"id": -99, "name": "special:scratch", "monitor": "HDMI-A-1", "monitorID": 1}),
        ]);
        let candidate = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .unwrap();
        let second = &candidate.outputs[1];
        assert_eq!(
            second.active_workspace_id,
            Some(workspace_id("session-a", 11))
        );
        assert_eq!(
            candidate.outputs[0].active_workspace_id,
            Some(workspace_id("session-a", 1))
        );
        assert_eq!(
            candidate
                .workspaces
                .iter()
                .filter(|w| w.focus == WorkspaceFocus::Focused)
                .count(),
            1
        );
        assert!(
            candidate
                .workspaces
                .iter()
                .filter(|w| w.visibility == WorkspaceVisibility::Visible)
                .count()
                >= 3
        );
    }

    #[test]
    fn unmapped_client_visibility_is_preserved_without_focus_inference() {
        let mut values = fixture();
        values[2][1]["mapped"] = json!(false);
        values[2][1]["hidden"] = json!(false);
        values[2][1]["visible"] = json!(false);

        let candidate = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("unmapped client is a valid observation");

        let unmapped = candidate
            .surfaces
            .iter()
            .find(|surface| {
                surface
                    .application
                    .as_ref()
                    .is_some_and(|app| app.process_id == 202)
            })
            .expect("unmapped application");
        assert_eq!(unmapped.visibility, SurfaceVisibility::Unmapped);
        assert_eq!(unmapped.focus, SurfaceFocus::Unfocused);
    }

    #[test]
    fn workspace_focus_comes_from_activeworkspace_not_activewindow_workspace() {
        let mut values = fixture();
        values[4] = json!({"address": "0xabc", "stableId": "18000001", "workspace": {"id": 99}});
        let candidate = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("candidate");
        assert_eq!(candidate.workspaces[0].focus, WorkspaceFocus::Focused);
        assert_eq!(candidate.workspaces[0].name.as_str(), "1");
    }

    #[test]
    fn focused_window_must_resolve_to_observed_client() {
        let mut values = fixture();
        values[4] = json!({"address": "0xmissing", "stableId": "18000099"});
        assert!(matches!(
            build_candidate(
                "session-a",
                &values[0],
                &values[1],
                &values[2],
                &values[3],
                &values[4],
                &values[5],
            ),
            Err(SemanticObservationError::FocusedClientMissing)
        ));
    }

    #[test]
    fn double_sample_accepts_semantic_reorder_but_rejects_state_change() {
        let values = fixture();
        let first = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("first");

        let mut reordered = values.clone();
        reordered[2].as_array_mut().expect("clients").reverse();
        let second = build_candidate(
            "session-a",
            &reordered[0],
            &reordered[1],
            &reordered[2],
            &reordered[3],
            &reordered[4],
            &reordered[5],
        )
        .expect("second");
        assert_eq!(ensure_stable_semantic_snapshot(first, second), Ok(()));

        let first = build_candidate(
            "session-a",
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
        )
        .expect("first");
        let mut changed = values.clone();
        changed[4] = json!({"address": "0xdef", "stableId": "18000002"});
        let second = build_candidate(
            "session-a",
            &changed[0],
            &changed[1],
            &changed[2],
            &changed[3],
            &changed[4],
            &changed[5],
        )
        .expect("changed");
        assert_eq!(
            ensure_stable_semantic_snapshot(first, second),
            Err(SemanticObservationError::SnapshotChanged)
        );
    }

    #[test]
    fn application_descriptor_change_advances_generation_once() {
        let values = fixture();
        let mut runtime = DesktopSemanticRuntime::default();
        let first = runtime
            .reconcile(
                build_candidate(
                    "session-a",
                    &values[0],
                    &values[1],
                    &values[2],
                    &values[3],
                    &values[4],
                    &values[5],
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(first.generation, 1);

        let mut changed = values.clone();
        changed[2][0]["title"] = json!("Document B");
        let second = runtime
            .reconcile(
                build_candidate(
                    "session-a",
                    &changed[0],
                    &changed[1],
                    &changed[2],
                    &changed[3],
                    &changed[4],
                    &changed[5],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(second.changed);
        assert_eq!(second.generation, 2);
        assert_ne!(second.digest_sha256, first.digest_sha256);
    }

    #[test]
    fn reorder_is_semantic_noop_and_focus_change_advances_once() {
        let values = fixture();
        let mut runtime = DesktopSemanticRuntime::default();
        let first = runtime
            .reconcile(
                build_candidate(
                    "session-a",
                    &values[0],
                    &values[1],
                    &values[2],
                    &values[3],
                    &values[4],
                    &values[5],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(first.changed);
        assert_eq!(first.generation, 1);

        let mut reordered = values.clone();
        reordered[2].as_array_mut().expect("clients").reverse();
        let no_change = runtime
            .reconcile(
                build_candidate(
                    "session-a",
                    &reordered[0],
                    &reordered[1],
                    &reordered[2],
                    &reordered[3],
                    &reordered[4],
                    &reordered[5],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(!no_change.changed);
        assert_eq!(no_change.generation, 1);
        assert_eq!(no_change.digest_sha256, first.digest_sha256);

        let mut focus_changed = values.clone();
        focus_changed[4] = json!({"address": "0xdef", "stableId": "18000002"});
        let changed = runtime
            .reconcile(
                build_candidate(
                    "session-a",
                    &focus_changed[0],
                    &focus_changed[1],
                    &focus_changed[2],
                    &focus_changed[3],
                    &focus_changed[4],
                    &focus_changed[5],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(changed.changed);
        assert_eq!(changed.generation, 2);
        assert_eq!(changed.focused_surface_count, 1);
        assert_ne!(changed.digest_sha256, first.digest_sha256);
    }
}
