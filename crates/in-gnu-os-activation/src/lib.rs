//! Renderer-independent typed activation contract owned by in.gnu.os.
//!
//! This crate admits only requests against already-observed current-session
//! workspaces and application surfaces. It does not execute compositor
//! operations, provide an external endpoint, or construct a production
//! capability. GNOS.UX is a request producer, never an executor.

use std::collections::HashSet;

use gnosix_desktop_model::{
    DesktopContextBaseline, DesktopContextGeneration, DesktopSurfaceRole, SurfaceId, WorkspaceId,
};

mod execution;
pub use execution::*;

#[cfg(all(feature = "r13-live-qualification", not(debug_assertions)))]
compile_error!("R13 live qualification cannot be enabled in a release build");

/// Opaque request correlation identity. The producer is responsible for
/// uniqueness within its request lifetime; this ID is neither authority nor
/// durable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivationRequestId([u8; 16]);

impl ActivationRequestId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Ephemeral identity for one authoritative semantic-observer lifetime.
/// It prevents a generation counter reset after restart from accepting an old
/// request. It is not persisted and is not an application/output identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivationSessionEpoch([u8; 16]);

impl ActivationSessionEpoch {
    /// The observation authority supplies a fresh opaque value per lifetime.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Exact session + semantic generation against which the target was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivationFreshness {
    session_epoch: ActivationSessionEpoch,
    generation: DesktopContextGeneration,
}

impl ActivationFreshness {
    pub const fn new(
        session_epoch: ActivationSessionEpoch,
        generation: DesktopContextGeneration,
    ) -> Self {
        Self {
            session_epoch,
            generation,
        }
    }

    pub const fn session_epoch(self) -> ActivationSessionEpoch {
        self.session_epoch
    }

    pub const fn generation(self) -> DesktopContextGeneration {
        self.generation
    }
}

/// Typed request producer. This identifies provenance only; it grants no
/// activation capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivationSource {
    GnosUx,
}

/// Exact current-session operation. Application launch is deliberately absent:
/// the repository has no ratified registered-application identity source.
/// No raw command variant exists:
///
/// ```compile_fail
/// use in_gnu_os_activation::ActivationOperation;
/// let _ = ActivationOperation::RawCommand("dispatch workspace 2".into());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivationOperation {
    ActivateObservedWorkspace(WorkspaceId),
    FocusObservedApplication(SurfaceId),
    /// WS-02B: move an observed application window onto an observed
    /// workspace, without following it there.
    MoveObservedApplicationToWorkspace(SurfaceId, WorkspaceId),
}

/// Product request, carrying typed intent and the exact observation fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivationRequest {
    request_id: ActivationRequestId,
    source: ActivationSource,
    freshness: ActivationFreshness,
    operation: ActivationOperation,
}

impl ActivationRequest {
    pub const fn new(
        request_id: ActivationRequestId,
        source: ActivationSource,
        freshness: ActivationFreshness,
        operation: ActivationOperation,
    ) -> Self {
        Self {
            request_id,
            source,
            freshness,
            operation,
        }
    }

    pub const fn request_id(self) -> ActivationRequestId {
        self.request_id
    }

    pub const fn source(self) -> ActivationSource {
        self.source
    }

    pub const fn freshness(self) -> ActivationFreshness {
        self.freshness
    }

    pub const fn operation(self) -> ActivationOperation {
        self.operation
    }
}

/// Admission refusal vocabulary. `AmbiguousTarget` and
/// `UnsupportedOperation` are explicit fail-closed outcomes for future typed
/// adapters; the current exact-ID pure admission path has no heuristic target
/// resolver and cannot admit an unsupported operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivationRefusal {
    Stale,
    UnknownTarget,
    TargetMismatch,
    AmbiguousTarget,
    NotAuthorized,
    UnsupportedOperation,
}

/// Coarse result vocabulary retained for existing admission-contract readers.
/// The bounded executor uses the richer `ActivationExecutionOutcome` receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivationExecutionResult {
    Applied,
    AlreadySatisfied,
    TargetDisappeared,
    BackendRejected,
    TransportFailure,
    ObservationFailure,
}

/// A current authoritative semantic observation supplied by the future
/// in.gnu.os adapter. It carries no ability to refresh or mutate the desktop.
#[derive(Debug, Clone, Copy)]
pub struct CurrentActivationObservation<'a> {
    session_epoch: ActivationSessionEpoch,
    baseline: &'a DesktopContextBaseline,
    runtime_session: Option<&'a str>,
}

impl<'a> CurrentActivationObservation<'a> {
    pub const fn new(
        session_epoch: ActivationSessionEpoch,
        baseline: &'a DesktopContextBaseline,
    ) -> Self {
        Self {
            session_epoch,
            baseline,
            runtime_session: None,
        }
    }

    /// Bind the admission observation to the exact runtime session reported
    /// by its observer. This is observational data, not caller authentication.
    pub const fn with_runtime_session(
        session_epoch: ActivationSessionEpoch,
        baseline: &'a DesktopContextBaseline,
        runtime_session: &'a str,
    ) -> Self {
        Self {
            session_epoch,
            baseline,
            runtime_session: Some(runtime_session),
        }
    }

    pub const fn freshness(&self) -> ActivationFreshness {
        ActivationFreshness::new(self.session_epoch, self.baseline.generation)
    }

    pub const fn runtime_session(&self) -> Option<&str> {
        self.runtime_session
    }
}

/// Workspace-only admission capability. Its constructor is private; only a
/// future, separately reviewed in.gnu.os authority adapter may issue it.
/// Product code cannot construct this capability directly:
///
/// ```compile_fail
/// use in_gnu_os_activation::WorkspaceActivationCapability;
/// use gnosix_desktop_model::WorkspaceId;
/// let _capability = WorkspaceActivationCapability {
///     workspace_id: WorkspaceId::from_bytes([1; 16]),
///     _private: (),
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceActivationCapability {
    workspace_id: WorkspaceId,
    _private: (),
}

/// Application-focus-only admission capability, independent of workspace
/// activation and of R11/R12 writer capabilities. Its private constructor
/// keeps ordinary downstream consumers from minting it directly:
///
/// ```compile_fail
/// use gnosix_desktop_model::SurfaceId;
/// use in_gnu_os_activation::ApplicationFocusCapability;
///
/// let _capability = ApplicationFocusCapability {
///     surface_id: SurfaceId::from_bytes([1; 16]),
///     _private: (),
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ApplicationFocusCapability {
    surface_id: SurfaceId,
    _private: (),
}

/// WS-02B: application-move-only admission capability, scoped to one window
/// and one destination workspace; it grants neither focus nor workspace
/// activation. Its private constructor keeps downstream code from minting it:
///
/// ```compile_fail
/// use gnosix_desktop_model::{SurfaceId, WorkspaceId};
/// use in_gnu_os_activation::ApplicationMoveCapability;
///
/// let _capability = ApplicationMoveCapability {
///     surface_id: SurfaceId::from_bytes([1; 16]),
///     workspace_id: WorkspaceId::from_bytes([2; 16]),
///     _private: (),
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ApplicationMoveCapability {
    surface_id: SurfaceId,
    workspace_id: WorkspaceId,
    _private: (),
}

/// Capability presented to the pure admission boundary. It cannot be widened
/// into another operation scope.
#[derive(Debug, Clone, Copy)]
pub enum ActivationCapability<'a> {
    Workspace(&'a WorkspaceActivationCapability),
    ApplicationFocus(&'a ApplicationFocusCapability),
    ApplicationMove(&'a ApplicationMoveCapability),
}

/// Exact operation and freshness admitted against one observation.
/// Construction is private to this crate's admission function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedActivation {
    request_id: ActivationRequestId,
    source: ActivationSource,
    freshness: ActivationFreshness,
    operation: ActivationOperation,
    runtime_session: Option<String>,
}

impl AdmittedActivation {
    pub const fn request_id(&self) -> ActivationRequestId {
        self.request_id
    }

    pub const fn source(&self) -> ActivationSource {
        self.source
    }

    pub const fn freshness(&self) -> ActivationFreshness {
        self.freshness
    }

    pub const fn operation(&self) -> ActivationOperation {
        self.operation
    }

    pub fn runtime_session(&self) -> Option<&str> {
        self.runtime_session.as_deref()
    }
}

/// Admission is pure: it neither refreshes stale requests nor performs an
/// execution. The caller must supply the exact authoritative observation and
/// an independently issued operation-scoped capability.
pub fn admit(
    request: ActivationRequest,
    observation: CurrentActivationObservation<'_>,
    capability: ActivationCapability<'_>,
) -> Result<AdmittedActivation, ActivationRefusal> {
    if request.freshness != observation.freshness() {
        return Err(ActivationRefusal::Stale);
    }

    let authorized = match (request.operation, capability) {
        (
            ActivationOperation::ActivateObservedWorkspace(workspace_id),
            ActivationCapability::Workspace(capability),
        ) => workspace_id == capability.workspace_id,
        (
            ActivationOperation::FocusObservedApplication(surface_id),
            ActivationCapability::ApplicationFocus(capability),
        ) => surface_id == capability.surface_id,
        (
            ActivationOperation::MoveObservedApplicationToWorkspace(surface_id, workspace_id),
            ActivationCapability::ApplicationMove(capability),
        ) => surface_id == capability.surface_id && workspace_id == capability.workspace_id,
        _ => false,
    };
    if !authorized {
        return Err(ActivationRefusal::NotAuthorized);
    }

    match request.operation {
        ActivationOperation::ActivateObservedWorkspace(workspace_id) => {
            if !observation
                .baseline
                .workspaces
                .contains_workspace_id(workspace_id)
            {
                return Err(ActivationRefusal::UnknownTarget);
            }
        }
        ActivationOperation::FocusObservedApplication(surface_id) => {
            let Some(surface) = observation
                .baseline
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == surface_id)
            else {
                return Err(ActivationRefusal::UnknownTarget);
            };
            if surface.role != gnosix_desktop_model::DesktopSurfaceRole::Application {
                return Err(ActivationRefusal::TargetMismatch);
            }
        }
        ActivationOperation::MoveObservedApplicationToWorkspace(surface_id, workspace_id) => {
            let Some(surface) = observation
                .baseline
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == surface_id)
            else {
                return Err(ActivationRefusal::UnknownTarget);
            };
            if surface.role != gnosix_desktop_model::DesktopSurfaceRole::Application {
                return Err(ActivationRefusal::TargetMismatch);
            }
            if !observation
                .baseline
                .workspaces
                .contains_workspace_id(workspace_id)
            {
                return Err(ActivationRefusal::UnknownTarget);
            }
        }
    }

    Ok(AdmittedActivation {
        request_id: request.request_id,
        source: request.source,
        freshness: request.freshness,
        operation: request.operation,
        runtime_session: observation.runtime_session.map(str::to_owned),
    })
}

/// Test-only, non-default issuance seam for the ignored R13-01F local harness.
/// It cannot be compiled in release mode and does not expose a capability
/// value to downstream code. It is not a production caller-authentication or
/// activation-issuer contract.
#[cfg(feature = "r13-live-qualification")]
#[doc(hidden)]
pub fn admit_for_live_qualification(
    request: ActivationRequest,
    observation: CurrentActivationObservation<'_>,
) -> Result<AdmittedActivation, ActivationRefusal> {
    issue_and_admit_exact(request, observation)
}

/// Production issuer seam for the separately authenticated in.gnu.os host.
///
/// The caller-authentication boundary is an external precondition. This
/// function does not authenticate a process and never returns a capability.
/// It constructs the exact operation-scoped capability internally, binds the
/// request to the authority's current observation freshness, and returns only
/// the admitted request.
///
/// GNOS.UX client observation generation is intentionally absent here: it is a
/// client-side send fence and is not numerically comparable to this authority
/// observer's generation.
#[cfg(feature = "r13-production-issuer")]
pub fn admit_authenticated_gnos_ux(
    request_id: ActivationRequestId,
    operation: ActivationOperation,
    observation: CurrentActivationObservation<'_>,
) -> Result<AdmittedActivation, ActivationRefusal> {
    let request = ActivationRequest::new(
        request_id,
        ActivationSource::GnosUx,
        observation.freshness(),
        operation,
    );
    issue_and_admit_exact(request, observation)
}

#[cfg(any(feature = "r13-live-qualification", feature = "r13-production-issuer"))]
fn issue_and_admit_exact(
    request: ActivationRequest,
    observation: CurrentActivationObservation<'_>,
) -> Result<AdmittedActivation, ActivationRefusal> {
    match request.operation() {
        ActivationOperation::ActivateObservedWorkspace(workspace_id) => {
            let capability = WorkspaceActivationCapability {
                workspace_id,
                _private: (),
            };
            admit(
                request,
                observation,
                ActivationCapability::Workspace(&capability),
            )
        }
        ActivationOperation::FocusObservedApplication(surface_id) => {
            let capability = ApplicationFocusCapability {
                surface_id,
                _private: (),
            };
            admit(
                request,
                observation,
                ActivationCapability::ApplicationFocus(&capability),
            )
        }
        ActivationOperation::MoveObservedApplicationToWorkspace(surface_id, workspace_id) => {
            let capability = ApplicationMoveCapability {
                surface_id,
                workspace_id,
                _private: (),
            };
            admit(
                request,
                observation,
                ActivationCapability::ApplicationMove(&capability),
            )
        }
    }
}

/// A semantic workspace and its observer-supplied native numeric binding.
/// `None` means the semantic target is present but not executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWorkspaceBinding {
    workspace_id: WorkspaceId,
    native_id: Option<i64>,
    focused: bool,
}

impl ObservedWorkspaceBinding {
    pub const fn new(workspace_id: WorkspaceId, native_id: Option<i64>) -> Self {
        Self {
            workspace_id,
            native_id,
            focused: false,
        }
    }

    pub const fn with_focus(
        workspace_id: WorkspaceId,
        native_id: Option<i64>,
        focused: bool,
    ) -> Self {
        Self {
            workspace_id,
            native_id,
            focused,
        }
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn native_id(&self) -> Option<i64> {
        self.native_id
    }

    pub const fn is_focused(&self) -> bool {
        self.focused
    }
}

/// A semantic surface and its observer-supplied stable native window ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedSurfaceBinding {
    surface_id: SurfaceId,
    role: DesktopSurfaceRole,
    stable_id: Option<String>,
    focused: bool,
    /// The semantic workspace holding the surface, when the observer saw one.
    workspace_id: Option<WorkspaceId>,
}

impl ObservedSurfaceBinding {
    pub fn new(surface_id: SurfaceId, role: DesktopSurfaceRole, stable_id: Option<&str>) -> Self {
        Self {
            surface_id,
            role,
            stable_id: stable_id.map(str::to_owned),
            focused: false,
            workspace_id: None,
        }
    }

    pub fn with_focus(
        surface_id: SurfaceId,
        role: DesktopSurfaceRole,
        stable_id: Option<&str>,
        focused: bool,
    ) -> Self {
        Self {
            surface_id,
            role,
            stable_id: stable_id.map(str::to_owned),
            focused,
            workspace_id: None,
        }
    }

    /// The workspace the observer saw the surface on (WS-02B).
    pub fn on_workspace(self, workspace_id: Option<WorkspaceId>) -> Self {
        Self {
            workspace_id,
            ..self
        }
    }

    pub const fn workspace_id(&self) -> Option<WorkspaceId> {
        self.workspace_id
    }

    pub const fn surface_id(&self) -> SurfaceId {
        self.surface_id
    }

    pub const fn role(&self) -> DesktopSurfaceRole {
        self.role
    }

    pub fn stable_id(&self) -> Option<&str> {
        self.stable_id.as_deref()
    }

    pub const fn is_focused(&self) -> bool {
        self.focused
    }
}

/// One immutable, observer-derived view of a runtime session. The caller must
/// supply the same authority epoch used at admission. This is policy input,
/// not a credential or an executable compositor command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionObservation {
    session_epoch: ActivationSessionEpoch,
    runtime_session: String,
    generation: DesktopContextGeneration,
    workspaces: Vec<ObservedWorkspaceBinding>,
    surfaces: Vec<ObservedSurfaceBinding>,
}

impl ExecutionObservation {
    pub fn new(
        session_epoch: ActivationSessionEpoch,
        runtime_session: impl Into<String>,
        generation: DesktopContextGeneration,
        workspaces: Vec<ObservedWorkspaceBinding>,
        surfaces: Vec<ObservedSurfaceBinding>,
    ) -> Self {
        Self {
            session_epoch,
            runtime_session: runtime_session.into(),
            generation,
            workspaces,
            surfaces,
        }
    }

    pub fn session_epoch(&self) -> ActivationSessionEpoch {
        self.session_epoch
    }

    pub fn runtime_session(&self) -> &str {
        &self.runtime_session
    }

    pub fn generation(&self) -> DesktopContextGeneration {
        self.generation
    }

    pub fn workspace_binding(&self, id: WorkspaceId) -> Option<&ObservedWorkspaceBinding> {
        self.workspaces.iter().find(|row| row.workspace_id == id)
    }

    pub fn surface_binding(&self, id: SurfaceId) -> Option<&ObservedSurfaceBinding> {
        self.surfaces.iter().find(|row| row.surface_id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeExecutionTarget {
    WorkspaceNumericId(i64),
    ApplicationStableId(String),
    /// WS-02B: a window's stable ID and its destination's numeric ID.
    ApplicationToWorkspace {
        stable_id: String,
        workspace: i64,
    },
}

/// A freshly checked exact target. It retains both the original admission
/// fence and the current observation generation. It does not execute anything
/// and cannot make a later compositor action atomic with this observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentExecutionTarget {
    request_id: ActivationRequestId,
    operation: ActivationOperation,
    admission_freshness: ActivationFreshness,
    runtime_session: String,
    current_generation: DesktopContextGeneration,
    native_target: NativeExecutionTarget,
}

impl CurrentExecutionTarget {
    pub const fn request_id(&self) -> ActivationRequestId {
        self.request_id
    }

    pub const fn operation(&self) -> ActivationOperation {
        self.operation
    }

    pub const fn admission_freshness(&self) -> ActivationFreshness {
        self.admission_freshness
    }

    pub const fn current_generation(&self) -> DesktopContextGeneration {
        self.current_generation
    }

    pub fn runtime_session(&self) -> &str {
        &self.runtime_session
    }

    pub fn native_target(&self) -> &NativeExecutionTarget {
        &self.native_target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRevalidationRefusal {
    StaleSemanticGeneration,
    SessionChanged,
    TargetDisappeared,
    ExecutionBindingUnavailable,
    ExecutionBindingChanged,
    DuplicateNativeIdentity,
    MalformedNativeIdentity,
    TargetTypeChanged,
}

fn validate_execution_observation(
    observation: &ExecutionObservation,
) -> Result<(), ExecutionRevalidationRefusal> {
    if observation.runtime_session.trim().is_empty() {
        return Err(ExecutionRevalidationRefusal::SessionChanged);
    }
    let mut workspace_semantic = HashSet::new();
    let mut workspace_native = HashSet::new();
    for workspace in &observation.workspaces {
        if !workspace_semantic.insert(workspace.workspace_id)
            || workspace
                .native_id
                .is_some_and(|id| !workspace_native.insert(id))
        {
            return Err(ExecutionRevalidationRefusal::DuplicateNativeIdentity);
        }
        if workspace.native_id == Some(0) {
            return Err(ExecutionRevalidationRefusal::MalformedNativeIdentity);
        }
    }
    let mut surface_semantic = HashSet::new();
    let mut surface_native = HashSet::new();
    for surface in &observation.surfaces {
        if !surface_semantic.insert(surface.surface_id) {
            return Err(ExecutionRevalidationRefusal::DuplicateNativeIdentity);
        }
        if let Some(stable_id) = surface.stable_id.as_deref() {
            if stable_id.is_empty() || !stable_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(ExecutionRevalidationRefusal::MalformedNativeIdentity);
            }
            if !surface_native.insert(stable_id.to_ascii_lowercase()) {
                return Err(ExecutionRevalidationRefusal::DuplicateNativeIdentity);
            }
        }
    }
    Ok(())
}

/// Recheck an already-admitted exact target against a fresh observation.
/// A later generation is accepted only when the same semantic target, native
/// identity, session and observer epoch are still present. The admission fence
/// is retained unchanged; no request is automatically readmitted or retargeted.
/// A raw product request cannot cross this boundary:
///
/// ```compile_fail
/// use in_gnu_os_activation::{
///     ActivationRequest, CurrentExecutionTarget, ExecutionObservation,
///     ExecutionRevalidationRefusal, revalidate_for_execution,
/// };
/// let _raw_adapter: fn(
///     &ActivationRequest,
///     &ExecutionObservation,
///     &ExecutionObservation,
/// ) -> Result<CurrentExecutionTarget, ExecutionRevalidationRefusal> = revalidate_for_execution;
/// ```
pub fn revalidate_for_execution(
    admitted: &AdmittedActivation,
    admitted_observation: &ExecutionObservation,
    current_observation: &ExecutionObservation,
) -> Result<CurrentExecutionTarget, ExecutionRevalidationRefusal> {
    let Some(admitted_session) = admitted.runtime_session() else {
        return Err(ExecutionRevalidationRefusal::ExecutionBindingUnavailable);
    };
    if admitted_observation.runtime_session != admitted_session
        || current_observation.runtime_session != admitted_session
    {
        return Err(ExecutionRevalidationRefusal::SessionChanged);
    }
    let admitted_freshness = admitted.freshness();
    if admitted_observation.session_epoch != admitted_freshness.session_epoch()
        || current_observation.session_epoch != admitted_freshness.session_epoch()
        || admitted_observation.generation != admitted_freshness.generation()
        || current_observation.generation.0 < admitted_observation.generation.0
    {
        return Err(ExecutionRevalidationRefusal::StaleSemanticGeneration);
    }
    validate_execution_observation(admitted_observation)?;
    validate_execution_observation(current_observation)?;

    let native_target = match admitted.operation() {
        ActivationOperation::ActivateObservedWorkspace(id) => {
            let original = admitted_observation
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == id)
                .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?
                .native_id
                .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
            let current = current_observation
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == id)
                .ok_or(ExecutionRevalidationRefusal::TargetDisappeared)?
                .native_id
                .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
            if original != current {
                return Err(ExecutionRevalidationRefusal::ExecutionBindingChanged);
            }
            NativeExecutionTarget::WorkspaceNumericId(current)
        }
        ActivationOperation::FocusObservedApplication(id) => {
            let original = admitted_observation
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == id)
                .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
            if original.role != DesktopSurfaceRole::Application {
                return Err(ExecutionRevalidationRefusal::TargetTypeChanged);
            }
            let original = original
                .stable_id
                .as_deref()
                .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
            let current = current_observation
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == id)
                .ok_or(ExecutionRevalidationRefusal::TargetDisappeared)?;
            if current.role != DesktopSurfaceRole::Application {
                return Err(ExecutionRevalidationRefusal::TargetTypeChanged);
            }
            let current = current
                .stable_id
                .as_deref()
                .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
            if original != current {
                return Err(ExecutionRevalidationRefusal::ExecutionBindingChanged);
            }
            NativeExecutionTarget::ApplicationStableId(current.to_owned())
        }
        ActivationOperation::MoveObservedApplicationToWorkspace(surface, workspace) => {
            let pair = |observation: &ExecutionObservation, missing| {
                let window = observation.surface_binding(surface).ok_or(missing)?;
                if window.role != DesktopSurfaceRole::Application {
                    return Err(ExecutionRevalidationRefusal::TargetTypeChanged);
                }
                let stable_id = window
                    .stable_id
                    .clone()
                    .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
                let destination = observation
                    .workspace_binding(workspace)
                    .ok_or(missing)?
                    .native_id
                    .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?;
                Ok(NativeExecutionTarget::ApplicationToWorkspace {
                    stable_id,
                    workspace: destination,
                })
            };
            let original = pair(
                admitted_observation,
                ExecutionRevalidationRefusal::ExecutionBindingUnavailable,
            )?;
            let current = pair(
                current_observation,
                ExecutionRevalidationRefusal::TargetDisappeared,
            )?;
            if original != current {
                return Err(ExecutionRevalidationRefusal::ExecutionBindingChanged);
            }
            current
        }
    };
    Ok(CurrentExecutionTarget {
        request_id: admitted.request_id(),
        operation: admitted.operation(),
        admission_freshness: admitted_freshness,
        runtime_session: current_observation.runtime_session.clone(),
        current_generation: current_observation.generation,
        native_target,
    })
}

/// Check a previously revalidated target once more against an observer-derived
/// snapshot immediately before mutation. This cannot make the compositor's
/// observation/action interval atomic.
pub fn revalidate_current_target(
    target: &CurrentExecutionTarget,
    current: &ExecutionObservation,
) -> Result<CurrentExecutionTarget, ExecutionRevalidationRefusal> {
    if current.runtime_session != target.runtime_session {
        return Err(ExecutionRevalidationRefusal::SessionChanged);
    }
    if current.session_epoch != target.admission_freshness.session_epoch()
        || current.generation.0 < target.current_generation.0
    {
        return Err(ExecutionRevalidationRefusal::StaleSemanticGeneration);
    }
    validate_execution_observation(current)?;
    let actual = match target.operation {
        ActivationOperation::ActivateObservedWorkspace(id) => {
            let binding = current
                .workspace_binding(id)
                .ok_or(ExecutionRevalidationRefusal::TargetDisappeared)?;
            NativeExecutionTarget::WorkspaceNumericId(
                binding
                    .native_id
                    .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?,
            )
        }
        ActivationOperation::FocusObservedApplication(id) => {
            let binding = current
                .surface_binding(id)
                .ok_or(ExecutionRevalidationRefusal::TargetDisappeared)?;
            if binding.role != DesktopSurfaceRole::Application {
                return Err(ExecutionRevalidationRefusal::TargetTypeChanged);
            }
            NativeExecutionTarget::ApplicationStableId(
                binding
                    .stable_id
                    .clone()
                    .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?,
            )
        }
        ActivationOperation::MoveObservedApplicationToWorkspace(surface, workspace) => {
            let window = current
                .surface_binding(surface)
                .ok_or(ExecutionRevalidationRefusal::TargetDisappeared)?;
            if window.role != DesktopSurfaceRole::Application {
                return Err(ExecutionRevalidationRefusal::TargetTypeChanged);
            }
            NativeExecutionTarget::ApplicationToWorkspace {
                stable_id: window
                    .stable_id
                    .clone()
                    .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?,
                workspace: current
                    .workspace_binding(workspace)
                    .ok_or(ExecutionRevalidationRefusal::TargetDisappeared)?
                    .native_id
                    .ok_or(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)?,
            }
        }
    };
    if actual != target.native_target {
        return Err(ExecutionRevalidationRefusal::ExecutionBindingChanged);
    }
    Ok(CurrentExecutionTarget {
        current_generation: current.generation,
        ..target.clone()
    })
}

/// Admission receipts preserve the original correlation and exact intent,
/// including refusal. Execution results are absent because this crate has no
/// executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationAdmissionReceipt {
    request_id: ActivationRequestId,
    source: ActivationSource,
    freshness: ActivationFreshness,
    operation: ActivationOperation,
    decision: Result<AdmittedActivation, ActivationRefusal>,
}

impl ActivationAdmissionReceipt {
    pub fn for_request(
        request: ActivationRequest,
        observation: CurrentActivationObservation<'_>,
        capability: ActivationCapability<'_>,
    ) -> Self {
        Self {
            request_id: request.request_id,
            source: request.source,
            freshness: request.freshness,
            operation: request.operation,
            decision: admit(request, observation, capability),
        }
    }

    pub const fn request_id(&self) -> ActivationRequestId {
        self.request_id
    }

    pub const fn source(&self) -> ActivationSource {
        self.source
    }

    pub const fn freshness(&self) -> ActivationFreshness {
        self.freshness
    }

    pub const fn operation(&self) -> ActivationOperation {
        self.operation
    }

    pub const fn decision(&self) -> &Result<AdmittedActivation, ActivationRefusal> {
        &self.decision
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use gnosix_desktop_model::{
        ApplicationSurfaceDescriptors, ConnectorName, DesktopContextGeneration, DesktopSurfaceRole,
        LogicalPoint, LogicalRect, LogicalSize, OutputHubId, OutputState, OutputTransform,
        PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli, SurfaceFocus, SurfaceId,
        SurfaceState, SurfaceVisibility, ValidatedOutputCollection, ValidatedSurfaceCollection,
        ValidatedWorkspaceCollection, WorkspaceFocus, WorkspaceId, WorkspaceName, WorkspaceState,
        WorkspaceVisibility,
    };

    fn baseline_with_targets(generation: DesktopContextGeneration) -> DesktopContextBaseline {
        let output_id = OutputHubId::from_bytes([10; 16]);
        let workspace_id = WorkspaceId::from_bytes([20; 16]);
        let outputs = ValidatedOutputCollection::from_outputs(vec![OutputState {
            output_hub_id: output_id,
            connector: ConnectorName::new("DP-2").unwrap(),
            active_workspace_id: Some(workspace_id),
            descriptors: PhysicalOutputDescriptors::default(),
            physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
            logical_position: LogicalPoint::ORIGIN,
            logical_size: LogicalSize::new(1920, 1080).unwrap(),
            scale: ScaleMilli::from_milli(1000).unwrap(),
            transform: OutputTransform::Normal,
        }])
        .unwrap();
        let workspaces = ValidatedWorkspaceCollection::from_workspaces(
            vec![WorkspaceState {
                workspace_id,
                name: WorkspaceName::new("1").unwrap(),
                output_hub_id: Some(output_id),
                visibility: WorkspaceVisibility::Visible,
                focus: WorkspaceFocus::Focused,
            }],
            &outputs,
        )
        .unwrap();
        let descriptors = |title: &str| ApplicationSurfaceDescriptors {
            class: "org.example.Editor".into(),
            initial_class: "org.example.Editor".into(),
            title: title.into(),
            initial_title: title.into(),
            tags: Vec::new(),
            process_id: 42,
            xwayland: false,
        };
        let surfaces = ValidatedSurfaceCollection::from_surfaces(
            vec![
                SurfaceState {
                    surface_id: SurfaceId::from_bytes([30; 16]),
                    role: DesktopSurfaceRole::Application,
                    output_hub_id: Some(output_id),
                    workspace_id: Some(workspace_id),
                    bounds: LogicalRect::new(
                        LogicalPoint::ORIGIN,
                        LogicalSize::new(800, 600).unwrap(),
                    ),
                    application: Some(descriptors("Duplicate")),
                    visibility: SurfaceVisibility::Visible,
                    focus: SurfaceFocus::Unfocused,
                },
                SurfaceState {
                    surface_id: SurfaceId::from_bytes([31; 16]),
                    role: DesktopSurfaceRole::Application,
                    output_hub_id: Some(output_id),
                    workspace_id: Some(workspace_id),
                    bounds: LogicalRect::new(
                        LogicalPoint::new(20, 20),
                        LogicalSize::new(800, 600).unwrap(),
                    ),
                    application: Some(descriptors("Duplicate")),
                    visibility: SurfaceVisibility::Visible,
                    focus: SurfaceFocus::Unfocused,
                },
                SurfaceState {
                    surface_id: SurfaceId::from_bytes([32; 16]),
                    role: DesktopSurfaceRole::Shell,
                    output_hub_id: Some(output_id),
                    workspace_id: None,
                    bounds: LogicalRect::new(
                        LogicalPoint::ORIGIN,
                        LogicalSize::new(100, 40).unwrap(),
                    ),
                    application: None,
                    visibility: SurfaceVisibility::Visible,
                    focus: SurfaceFocus::Unfocused,
                },
            ],
            &outputs,
            &workspaces,
        )
        .unwrap();
        DesktopContextBaseline {
            generation,
            outputs,
            workspaces,
            surfaces,
        }
    }

    fn request(operation: ActivationOperation, generation: u64) -> ActivationRequest {
        ActivationRequest::new(
            ActivationRequestId::from_bytes([90; 16]),
            ActivationSource::GnosUx,
            ActivationFreshness::new(
                ActivationSessionEpoch::from_bytes([80; 16]),
                DesktopContextGeneration(generation),
            ),
            operation,
        )
    }

    fn workspace_capability(workspace_id: WorkspaceId) -> WorkspaceActivationCapability {
        WorkspaceActivationCapability {
            workspace_id,
            _private: (),
        }
    }

    fn focus_capability(surface_id: SurfaceId) -> ApplicationFocusCapability {
        ApplicationFocusCapability {
            surface_id,
            _private: (),
        }
    }

    fn observation(baseline: &DesktopContextBaseline) -> CurrentActivationObservation<'_> {
        CurrentActivationObservation::new(ActivationSessionEpoch::from_bytes([80; 16]), baseline)
    }

    #[cfg(feature = "r13-production-issuer")]
    #[test]
    fn production_issuer_binds_request_to_authority_observation_and_hides_capability() {
        let baseline = baseline_with_targets(DesktopContextGeneration(12));
        let observation = observation(&baseline);
        let admitted = admit_authenticated_gnos_ux(
            ActivationRequestId::from_bytes([71; 16]),
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([20; 16])),
            observation,
        )
        .unwrap();

        assert_eq!(
            admitted.request_id(),
            ActivationRequestId::from_bytes([71; 16])
        );
        assert_eq!(
            admitted.freshness().generation(),
            DesktopContextGeneration(12)
        );
        assert_eq!(admitted.source(), ActivationSource::GnosUx);
    }

    #[cfg(feature = "r13-production-issuer")]
    #[test]
    fn production_issuer_refuses_unknown_exact_target() {
        let baseline = baseline_with_targets(DesktopContextGeneration(12));
        let refusal = admit_authenticated_gnos_ux(
            ActivationRequestId::from_bytes([72; 16]),
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([99; 16])),
            observation(&baseline),
        )
        .unwrap_err();

        assert_eq!(refusal, ActivationRefusal::UnknownTarget);
    }

    #[test]
    fn request_preserves_typed_source_target_and_session_generation_fence() {
        let request = ActivationRequest::new(
            ActivationRequestId::from_bytes([1; 16]),
            ActivationSource::GnosUx,
            ActivationFreshness::new(
                ActivationSessionEpoch::from_bytes([2; 16]),
                DesktopContextGeneration(9),
            ),
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([3; 16])),
        );

        assert_eq!(
            request.request_id(),
            ActivationRequestId::from_bytes([1; 16])
        );
        assert_eq!(request.source(), ActivationSource::GnosUx);
        assert_eq!(
            request.freshness().generation(),
            DesktopContextGeneration(9)
        );
        assert_eq!(
            request.operation(),
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([3; 16]))
        );

        let focus_request = ActivationRequest::new(
            ActivationRequestId::from_bytes([4; 16]),
            ActivationSource::GnosUx,
            ActivationFreshness::new(
                ActivationSessionEpoch::from_bytes([2; 16]),
                DesktopContextGeneration(9),
            ),
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([5; 16])),
        );
        assert_eq!(
            focus_request.operation(),
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([5; 16]))
        );
    }

    #[test]
    fn execution_result_vocabulary_keeps_backend_outcomes_distinct() {
        let outcomes = [
            ActivationExecutionResult::Applied,
            ActivationExecutionResult::AlreadySatisfied,
            ActivationExecutionResult::TargetDisappeared,
            ActivationExecutionResult::BackendRejected,
            ActivationExecutionResult::TransportFailure,
            ActivationExecutionResult::ObservationFailure,
        ];
        for left in 0..outcomes.len() {
            for right in (left + 1)..outcomes.len() {
                assert_ne!(outcomes[left], outcomes[right]);
            }
        }
    }

    #[test]
    fn matching_workspace_request_admits_the_exact_workspace() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let request = request(
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([20; 16])),
            9,
        );
        let capability = workspace_capability(WorkspaceId::from_bytes([20; 16]));
        let admitted = admit(
            request,
            observation(&baseline),
            ActivationCapability::Workspace(&capability),
        )
        .unwrap();

        assert_eq!(admitted.request_id(), request.request_id());
        assert_eq!(admitted.operation(), request.operation());
        assert_eq!(admitted.freshness(), request.freshness());
    }

    #[test]
    fn stale_generation_or_session_epoch_is_refused_without_retargeting() {
        let baseline = baseline_with_targets(DesktopContextGeneration(10));
        let request = request(
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([20; 16])),
            9,
        );
        let capability = workspace_capability(WorkspaceId::from_bytes([20; 16]));
        assert_eq!(
            admit(
                request,
                observation(&baseline),
                ActivationCapability::Workspace(&capability),
            ),
            Err(ActivationRefusal::Stale)
        );

        let mut request = request;
        request.freshness = ActivationFreshness::new(
            ActivationSessionEpoch::from_bytes([81; 16]),
            DesktopContextGeneration(10),
        );
        assert_eq!(
            admit(
                request,
                observation(&baseline),
                ActivationCapability::Workspace(&capability),
            ),
            Err(ActivationRefusal::Stale)
        );
    }

    #[test]
    fn unknown_workspace_is_refused() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let request = request(
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([21; 16])),
            9,
        );
        let capability = workspace_capability(WorkspaceId::from_bytes([21; 16]));
        assert_eq!(
            admit(
                request,
                observation(&baseline),
                ActivationCapability::Workspace(&capability),
            ),
            Err(ActivationRefusal::UnknownTarget)
        );
    }

    #[test]
    fn a_move_needs_its_own_capability_for_that_window_and_destination() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let window = SurfaceId::from_bytes([30; 16]);
        let destination = WorkspaceId::from_bytes([20; 16]);
        let operation =
            ActivationOperation::MoveObservedApplicationToWorkspace(window, destination);
        // A focus capability for the same window grants no move.
        let focus = focus_capability(window);
        assert_eq!(
            admit(
                request(operation, 9),
                observation(&baseline),
                ActivationCapability::ApplicationFocus(&focus),
            ),
            Err(ActivationRefusal::NotAuthorized)
        );
        // A move capability for another destination grants nothing here.
        let elsewhere = ApplicationMoveCapability {
            surface_id: window,
            workspace_id: WorkspaceId::from_bytes([21; 16]),
            _private: (),
        };
        assert_eq!(
            admit(
                request(operation, 9),
                observation(&baseline),
                ActivationCapability::ApplicationMove(&elsewhere),
            ),
            Err(ActivationRefusal::NotAuthorized)
        );
        // The exact capability is admitted.
        let exact = ApplicationMoveCapability {
            surface_id: window,
            workspace_id: destination,
            _private: (),
        };
        assert!(admit(
            request(operation, 9),
            observation(&baseline),
            ActivationCapability::ApplicationMove(&exact),
        )
        .is_ok());
        // An unknown destination is refused even with its capability.
        let unknown = WorkspaceId::from_bytes([99; 16]);
        let to_unknown = ApplicationMoveCapability {
            surface_id: window,
            workspace_id: unknown,
            _private: (),
        };
        assert_eq!(
            admit(
                request(
                    ActivationOperation::MoveObservedApplicationToWorkspace(window, unknown),
                    9
                ),
                observation(&baseline),
                ActivationCapability::ApplicationMove(&to_unknown),
            ),
            Err(ActivationRefusal::UnknownTarget)
        );
    }

    #[test]
    fn workspace_capability_cannot_focus_an_application() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let request = request(
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([30; 16])),
            9,
        );
        let capability = workspace_capability(WorkspaceId::from_bytes([20; 16]));
        assert_eq!(
            admit(
                request,
                observation(&baseline),
                ActivationCapability::Workspace(&capability),
            ),
            Err(ActivationRefusal::NotAuthorized)
        );
    }

    #[test]
    fn exact_application_surface_id_is_admitted_even_with_duplicate_descriptors() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let request = request(
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([31; 16])),
            9,
        );
        let capability = focus_capability(SurfaceId::from_bytes([31; 16]));
        let admitted = admit(
            request,
            observation(&baseline),
            ActivationCapability::ApplicationFocus(&capability),
        )
        .unwrap();

        assert_eq!(
            admitted.operation(),
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([31; 16]))
        );
    }

    #[test]
    fn capability_for_one_surface_cannot_focus_another_surface() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let request = request(
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([31; 16])),
            9,
        );
        let capability = focus_capability(SurfaceId::from_bytes([30; 16]));

        assert_eq!(
            admit(
                request,
                observation(&baseline),
                ActivationCapability::ApplicationFocus(&capability),
            ),
            Err(ActivationRefusal::NotAuthorized)
        );
    }

    #[test]
    fn missing_or_non_application_surface_is_refused_without_descriptor_fallback() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let capability = focus_capability(SurfaceId::from_bytes([33; 16]));
        let missing = request(
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([33; 16])),
            9,
        );
        assert_eq!(
            admit(
                missing,
                observation(&baseline),
                ActivationCapability::ApplicationFocus(&capability),
            ),
            Err(ActivationRefusal::UnknownTarget)
        );

        let shell = request(
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([32; 16])),
            9,
        );
        let shell_capability = focus_capability(SurfaceId::from_bytes([32; 16]));
        assert_eq!(
            admit(
                shell,
                observation(&baseline),
                ActivationCapability::ApplicationFocus(&shell_capability),
            ),
            Err(ActivationRefusal::TargetMismatch)
        );
    }

    #[test]
    fn application_capability_cannot_activate_a_workspace() {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let request = request(
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([20; 16])),
            9,
        );
        let capability = focus_capability(SurfaceId::from_bytes([30; 16]));
        assert_eq!(
            admit(
                request,
                observation(&baseline),
                ActivationCapability::ApplicationFocus(&capability),
            ),
            Err(ActivationRefusal::NotAuthorized)
        );
    }

    #[test]
    fn admission_receipt_retains_request_and_exact_refusal() {
        let baseline = baseline_with_targets(DesktopContextGeneration(10));
        let request = request(
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([20; 16])),
            9,
        );
        let capability = workspace_capability(WorkspaceId::from_bytes([20; 16]));
        let receipt = ActivationAdmissionReceipt::for_request(
            request,
            observation(&baseline),
            ActivationCapability::Workspace(&capability),
        );

        assert_eq!(receipt.request_id(), request.request_id());
        assert_eq!(receipt.operation(), request.operation());
        assert_eq!(receipt.freshness(), request.freshness());
        assert_eq!(receipt.decision(), &Err(ActivationRefusal::Stale));
    }

    fn admitted_for_runtime(operation: ActivationOperation) -> AdmittedActivation {
        let baseline = baseline_with_targets(DesktopContextGeneration(9));
        let capability = match operation {
            ActivationOperation::ActivateObservedWorkspace(id) => {
                let capability = workspace_capability(id);
                return admit(
                    request(operation, 9),
                    CurrentActivationObservation::with_runtime_session(
                        ActivationSessionEpoch::from_bytes([80; 16]),
                        &baseline,
                        "hypr-session-a",
                    ),
                    ActivationCapability::Workspace(&capability),
                )
                .unwrap();
            }
            ActivationOperation::FocusObservedApplication(id) => focus_capability(id),
            ActivationOperation::MoveObservedApplicationToWorkspace(surface, workspace) => {
                let capability = ApplicationMoveCapability {
                    surface_id: surface,
                    workspace_id: workspace,
                    _private: (),
                };
                return admit(
                    request(operation, 9),
                    CurrentActivationObservation::with_runtime_session(
                        ActivationSessionEpoch::from_bytes([80; 16]),
                        &baseline,
                        "hypr-session-a",
                    ),
                    ActivationCapability::ApplicationMove(&capability),
                )
                .unwrap();
            }
        };
        admit(
            request(operation, 9),
            CurrentActivationObservation::with_runtime_session(
                ActivationSessionEpoch::from_bytes([80; 16]),
                &baseline,
                "hypr-session-a",
            ),
            ActivationCapability::ApplicationFocus(&capability),
        )
        .unwrap()
    }

    fn execution_observation(generation: u64) -> ExecutionObservation {
        ExecutionObservation::new(
            ActivationSessionEpoch::from_bytes([80; 16]),
            "hypr-session-a",
            DesktopContextGeneration(generation),
            vec![ObservedWorkspaceBinding::new(
                WorkspaceId::from_bytes([20; 16]),
                Some(2),
            )],
            vec![
                ObservedSurfaceBinding::new(
                    SurfaceId::from_bytes([30; 16]),
                    DesktopSurfaceRole::Application,
                    Some("18000001"),
                ),
                ObservedSurfaceBinding::new(
                    SurfaceId::from_bytes([31; 16]),
                    DesktopSurfaceRole::Application,
                    Some("18000002"),
                ),
                ObservedSurfaceBinding::new(
                    SurfaceId::from_bytes([32; 16]),
                    DesktopSurfaceRole::Shell,
                    None,
                ),
            ],
        )
    }

    #[test]
    fn same_session_workspace_binding_revalidates_without_rewriting_admission() {
        let admitted = admitted_for_runtime(ActivationOperation::ActivateObservedWorkspace(
            WorkspaceId::from_bytes([20; 16]),
        ));
        let original = execution_observation(9);
        let same_generation =
            revalidate_for_execution(&admitted, &original, &execution_observation(9)).unwrap();
        assert_eq!(
            same_generation.native_target(),
            &NativeExecutionTarget::WorkspaceNumericId(2)
        );
        assert_eq!(
            same_generation.current_generation(),
            DesktopContextGeneration(9)
        );
        let mut current = execution_observation(10);
        current.surfaces.push(ObservedSurfaceBinding::new(
            SurfaceId::from_bytes([33; 16]),
            DesktopSurfaceRole::Application,
            Some("18000004"),
        ));
        let target = revalidate_for_execution(&admitted, &original, &current).unwrap();
        assert_eq!(
            target.native_target(),
            &NativeExecutionTarget::WorkspaceNumericId(2)
        );
        assert_eq!(
            target.admission_freshness().generation(),
            DesktopContextGeneration(9)
        );
        assert_eq!(target.current_generation(), DesktopContextGeneration(10));
    }

    #[test]
    fn session_change_refuses_even_when_semantic_id_and_native_number_match() {
        let admitted = admitted_for_runtime(ActivationOperation::ActivateObservedWorkspace(
            WorkspaceId::from_bytes([20; 16]),
        ));
        let original = execution_observation(9);
        let mut current = execution_observation(9);
        current.runtime_session = "hypr-session-b".into();
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &current),
            Err(ExecutionRevalidationRefusal::SessionChanged)
        );
    }

    #[test]
    fn disappearance_or_native_rebinding_refuses_without_target_substitution() {
        let admitted = admitted_for_runtime(ActivationOperation::ActivateObservedWorkspace(
            WorkspaceId::from_bytes([20; 16]),
        ));
        let original = execution_observation(9);
        let mut missing = execution_observation(10);
        missing.workspaces.clear();
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &missing),
            Err(ExecutionRevalidationRefusal::TargetDisappeared)
        );
        let mut rebound = execution_observation(10);
        rebound.workspaces[0].native_id = Some(3);
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &rebound),
            Err(ExecutionRevalidationRefusal::ExecutionBindingChanged)
        );
    }

    #[test]
    fn application_binding_loss_change_and_type_change_are_distinct() {
        let admitted = admitted_for_runtime(ActivationOperation::FocusObservedApplication(
            SurfaceId::from_bytes([30; 16]),
        ));
        let original = execution_observation(9);
        let mut unavailable = execution_observation(10);
        unavailable.surfaces[0].stable_id = None;
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &unavailable),
            Err(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)
        );
        let mut rebound = execution_observation(10);
        rebound.surfaces[0].stable_id = Some("18000003".into());
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &rebound),
            Err(ExecutionRevalidationRefusal::ExecutionBindingChanged)
        );
        let mut shell = execution_observation(10);
        shell.surfaces[0].role = DesktopSurfaceRole::Shell;
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &shell),
            Err(ExecutionRevalidationRefusal::TargetTypeChanged)
        );
    }

    #[test]
    fn duplicate_and_malformed_native_identities_cannot_be_revalidated() {
        let admitted = admitted_for_runtime(ActivationOperation::FocusObservedApplication(
            SurfaceId::from_bytes([30; 16]),
        ));
        let original = execution_observation(9);
        let mut duplicate = execution_observation(10);
        duplicate.surfaces[1].stable_id = Some("18000001".into());
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &duplicate),
            Err(ExecutionRevalidationRefusal::DuplicateNativeIdentity)
        );
        let mut malformed = execution_observation(10);
        malformed.surfaces[0].stable_id = Some("title:Document".into());
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &malformed),
            Err(ExecutionRevalidationRefusal::MalformedNativeIdentity)
        );
        let workspace_admitted = admitted_for_runtime(
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([20; 16])),
        );
        let mut duplicate_workspace = execution_observation(10);
        duplicate_workspace
            .workspaces
            .push(ObservedWorkspaceBinding::new(
                WorkspaceId::from_bytes([21; 16]),
                Some(2),
            ));
        assert_eq!(
            revalidate_for_execution(&workspace_admitted, &original, &duplicate_workspace,),
            Err(ExecutionRevalidationRefusal::DuplicateNativeIdentity)
        );
    }

    #[test]
    fn semantic_application_without_native_binding_is_not_executable() {
        let admitted = admitted_for_runtime(ActivationOperation::FocusObservedApplication(
            SurfaceId::from_bytes([30; 16]),
        ));
        let mut original = execution_observation(9);
        original.surfaces[0].stable_id = None;
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &execution_observation(9)),
            Err(ExecutionRevalidationRefusal::ExecutionBindingUnavailable)
        );
    }

    #[test]
    fn admission_generation_or_observer_epoch_cannot_be_refreshed_silently() {
        let admitted = admitted_for_runtime(ActivationOperation::ActivateObservedWorkspace(
            WorkspaceId::from_bytes([20; 16]),
        ));
        let original = execution_observation(10);
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &execution_observation(10)),
            Err(ExecutionRevalidationRefusal::StaleSemanticGeneration)
        );
        let original = execution_observation(9);
        let mut new_epoch = execution_observation(10);
        new_epoch.session_epoch = ActivationSessionEpoch::from_bytes([81; 16]);
        assert_eq!(
            revalidate_for_execution(&admitted, &original, &new_epoch),
            Err(ExecutionRevalidationRefusal::StaleSemanticGeneration)
        );
    }

    #[test]
    fn duplicate_descriptors_never_participate_in_exact_application_revalidation() {
        let admitted = admitted_for_runtime(ActivationOperation::FocusObservedApplication(
            SurfaceId::from_bytes([31; 16]),
        ));
        let target = revalidate_for_execution(
            &admitted,
            &execution_observation(9),
            &execution_observation(10),
        )
        .unwrap();
        assert_eq!(
            target.native_target(),
            &NativeExecutionTarget::ApplicationStableId("18000002".into())
        );
    }
}
