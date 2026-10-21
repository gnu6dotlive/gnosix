//! Typed R13 user-session D-Bus boundary.
//!
//! The boundary exposes only the three ratified product operations: activate
//! a workspace, focus a window and (WS-02B) move a window onto a workspace.
//! Caller authentication is mandatory before authority-local
//! admission/execution.
//! The production runtime keeps one bounded, non-evicting completed-request
//! ledger for the current ActivationSessionEpoch. Restart/session replacement
//! destroys that ledger rather than replaying requests into a new epoch.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[cfg(test)]
use gnosix_activation_protocol::ACTIVATION_INTERFACE;
use gnosix_activation_protocol::{
    ACTIVATION_BUS_NAME, ACTIVATION_OBJECT_PATH, ActivationReceiptCode as BoundaryReceiptCode,
};
use gnosix_desktop_model::{DesktopContextGeneration, SurfaceId, WorkspaceId};
use in_gnu_os_activation::{
    ActivationExecutionBackend, ActivationExecutionOutcome, ActivationOperation, ActivationRefusal,
    ActivationRequestId, ActivationSessionEpoch, AdmittedActivation, ExecutionObservation,
    ExecutionObservationError, ExecutionRevalidationRefusal, execute_bounded,
    revalidate_for_execution,
};
use zbus::{
    blocking::{Connection, connection::Builder},
    fdo, interface,
    message::Header,
};

use crate::{
    activation_execution::{HyprlandActivationExecutor, ProductionAdmissionError},
    caller_auth::{CallerAuthRefusal, DbusCallerAuthenticator},
};

const COMPLETED_REQUEST_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoundaryReceipt {
    code: BoundaryReceiptCode,
    detail: String,
}

impl BoundaryReceipt {
    fn new(code: BoundaryReceiptCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    fn wire(&self) -> (u32, String) {
        (self.code as u32, self.detail.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestFingerprint {
    operation: ActivationOperation,
    client_observation_generation: DesktopContextGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedRequest {
    fingerprint: RequestFingerprint,
    receipt: BoundaryReceipt,
}

#[derive(Debug)]
struct ReplayLedger {
    epoch: ActivationSessionEpoch,
    completed: HashMap<ActivationRequestId, CompletedRequest>,
}

enum ReplayLookup {
    Miss,
    Hit(BoundaryReceipt),
    Conflict,
    Full,
    WrongEpoch,
}

impl ReplayLedger {
    fn new(epoch: ActivationSessionEpoch) -> Self {
        Self {
            epoch,
            completed: HashMap::new(),
        }
    }

    fn lookup(
        &self,
        epoch: ActivationSessionEpoch,
        request_id: ActivationRequestId,
        fingerprint: RequestFingerprint,
    ) -> ReplayLookup {
        if epoch != self.epoch {
            return ReplayLookup::WrongEpoch;
        }
        match self.completed.get(&request_id) {
            Some(entry) if entry.fingerprint == fingerprint => {
                ReplayLookup::Hit(entry.receipt.clone())
            }
            Some(_) => ReplayLookup::Conflict,
            None if self.completed.len() >= COMPLETED_REQUEST_CAPACITY => ReplayLookup::Full,
            None => ReplayLookup::Miss,
        }
    }

    fn record(
        &mut self,
        request_id: ActivationRequestId,
        fingerprint: RequestFingerprint,
        receipt: BoundaryReceipt,
    ) {
        debug_assert!(self.completed.len() < COMPLETED_REQUEST_CAPACITY);
        self.completed.insert(
            request_id,
            CompletedRequest {
                fingerprint,
                receipt,
            },
        );
    }
}

trait AuthorityExecutor: ActivationExecutionBackend {
    fn admit_current(
        &mut self,
        request_id: ActivationRequestId,
        operation: ActivationOperation,
    ) -> Result<(AdmittedActivation, ExecutionObservation), ProductionAdmissionError>;
}

impl AuthorityExecutor for HyprlandActivationExecutor {
    fn admit_current(
        &mut self,
        request_id: ActivationRequestId,
        operation: ActivationOperation,
    ) -> Result<(AdmittedActivation, ExecutionObservation), ProductionAdmissionError> {
        self.admit_for_production(request_id, operation)
    }
}

pub(crate) struct ProductionActivationRuntime {
    epoch: ActivationSessionEpoch,
    executor: HyprlandActivationExecutor,
    replay: ReplayLedger,
}

impl ProductionActivationRuntime {
    pub(crate) fn new(epoch: ActivationSessionEpoch, executor: HyprlandActivationExecutor) -> Self {
        Self {
            epoch,
            executor,
            replay: ReplayLedger::new(epoch),
        }
    }

    pub(crate) fn observe_runtime_session(&mut self) -> Result<String, ExecutionObservationError> {
        self.executor
            .observe()
            .map(|observation| observation.runtime_session().to_owned())
    }

    fn dispatch(
        &mut self,
        request_id: ActivationRequestId,
        client_observation_generation: DesktopContextGeneration,
        operation: ActivationOperation,
    ) -> BoundaryReceipt {
        let fingerprint = RequestFingerprint {
            operation,
            client_observation_generation,
        };
        let epoch = self.epoch;
        let replay = &mut self.replay;
        let executor = &mut self.executor;
        replay_or_execute(replay, epoch, request_id, fingerprint, || {
            execute_authenticated_uncached(executor, epoch, request_id, operation)
        })
    }
}

fn replay_or_execute(
    replay: &mut ReplayLedger,
    epoch: ActivationSessionEpoch,
    request_id: ActivationRequestId,
    fingerprint: RequestFingerprint,
    execute: impl FnOnce() -> BoundaryReceipt,
) -> BoundaryReceipt {
    match replay.lookup(epoch, request_id, fingerprint) {
        ReplayLookup::Hit(receipt) => receipt,
        ReplayLookup::Conflict => BoundaryReceipt::new(
            BoundaryReceiptCode::ProtocolFailure,
            "PROTOCOL_FAILURE: request_id reused with different payload",
        ),
        ReplayLookup::Full => BoundaryReceipt::new(
            BoundaryReceiptCode::ProtocolFailure,
            "PROTOCOL_FAILURE: completed-request ledger capacity reached; refusing new mutation",
        ),
        ReplayLookup::WrongEpoch => BoundaryReceipt::new(
            BoundaryReceiptCode::SessionUnavailable,
            "SESSION_UNAVAILABLE: activation epoch changed",
        ),
        ReplayLookup::Miss => {
            let receipt = execute();
            replay.record(request_id, fingerprint, receipt.clone());
            receipt
        }
    }
}

fn execute_authenticated_uncached<B: AuthorityExecutor>(
    backend: &mut B,
    epoch: ActivationSessionEpoch,
    request_id: ActivationRequestId,
    operation: ActivationOperation,
) -> BoundaryReceipt {
    let (admitted, admitted_observation) = match backend.admit_current(request_id, operation) {
        Ok(admitted) => admitted,
        Err(ProductionAdmissionError::ObservationFailed) => {
            return BoundaryReceipt::new(
                BoundaryReceiptCode::ObservationFailure,
                "OBSERVATION_FAILURE: admission observation unavailable",
            );
        }
        Err(ProductionAdmissionError::EpochMismatch) => {
            return BoundaryReceipt::new(
                BoundaryReceiptCode::SessionUnavailable,
                "SESSION_UNAVAILABLE: observer epoch mismatch",
            );
        }
        Err(ProductionAdmissionError::Refused(refusal)) => {
            return map_admission_refusal(refusal);
        }
    };

    if admitted_observation.session_epoch() != epoch {
        return BoundaryReceipt::new(
            BoundaryReceiptCode::SessionUnavailable,
            "SESSION_UNAVAILABLE: observer epoch mismatch",
        );
    }

    let current_observation = match backend.observe() {
        Ok(observation) => observation,
        Err(_) => {
            return BoundaryReceipt::new(
                BoundaryReceiptCode::ObservationFailure,
                "OBSERVATION_FAILURE: pre-execution revalidation observation unavailable",
            );
        }
    };

    let target =
        match revalidate_for_execution(&admitted, &admitted_observation, &current_observation) {
            Ok(target) => target,
            Err(refusal) => return map_revalidation_refusal(refusal),
        };

    map_execution_receipt(execute_bounded(target, backend).outcome)
}

fn map_admission_refusal(refusal: ActivationRefusal) -> BoundaryReceipt {
    let code = match refusal {
        ActivationRefusal::Stale => BoundaryReceiptCode::Stale,
        ActivationRefusal::UnknownTarget => BoundaryReceiptCode::UnknownTarget,
        ActivationRefusal::NotAuthorized => BoundaryReceiptCode::NotAuthorized,
        ActivationRefusal::TargetMismatch
        | ActivationRefusal::AmbiguousTarget
        | ActivationRefusal::UnsupportedOperation => BoundaryReceiptCode::ProtocolFailure,
    };
    BoundaryReceipt::new(code, format!("{code:?}: admission refused: {refusal:?}"))
}

fn map_revalidation_refusal(refusal: ExecutionRevalidationRefusal) -> BoundaryReceipt {
    let code = match refusal {
        ExecutionRevalidationRefusal::SessionChanged => BoundaryReceiptCode::SessionUnavailable,
        ExecutionRevalidationRefusal::TargetDisappeared => BoundaryReceiptCode::TargetDisappeared,
        ExecutionRevalidationRefusal::ExecutionBindingChanged => {
            BoundaryReceiptCode::ExecutionBindingChanged
        }
        ExecutionRevalidationRefusal::StaleSemanticGeneration
        | ExecutionRevalidationRefusal::ExecutionBindingUnavailable
        | ExecutionRevalidationRefusal::DuplicateNativeIdentity
        | ExecutionRevalidationRefusal::MalformedNativeIdentity
        | ExecutionRevalidationRefusal::TargetTypeChanged => BoundaryReceiptCode::Stale,
    };
    BoundaryReceipt::new(code, format!("{code:?}: revalidation refused: {refusal:?}"))
}

fn map_execution_receipt(outcome: ActivationExecutionOutcome) -> BoundaryReceipt {
    let code = match outcome {
        ActivationExecutionOutcome::Applied => BoundaryReceiptCode::Applied,
        ActivationExecutionOutcome::AlreadySatisfied => BoundaryReceiptCode::AlreadySatisfied,
        ActivationExecutionOutcome::RefusedBeforeExecution(reason) => {
            return map_revalidation_refusal(reason);
        }
        ActivationExecutionOutcome::BackendRejected => BoundaryReceiptCode::BackendRejected,
        ActivationExecutionOutcome::TransportFailure => BoundaryReceiptCode::TransportFailure,
        ActivationExecutionOutcome::TargetDisappearedDuringExecution => {
            BoundaryReceiptCode::TargetDisappeared
        }
        ActivationExecutionOutcome::PostconditionMismatch => {
            BoundaryReceiptCode::PostconditionMismatch
        }
        ActivationExecutionOutcome::ObservationFailure => BoundaryReceiptCode::ObservationFailure,
        ActivationExecutionOutcome::ObservationSessionChanged => {
            BoundaryReceiptCode::SessionUnavailable
        }
        ActivationExecutionOutcome::ExecutionBindingChanged => {
            BoundaryReceiptCode::ExecutionBindingChanged
        }
    };
    BoundaryReceipt::new(code, format!("{code:?}: {outcome:?}"))
}

fn exact_id(bytes: Vec<u8>, field: &str) -> fdo::Result<[u8; 16]> {
    bytes
        .try_into()
        .map_err(|_| fdo::Error::InvalidArgs(format!("{field} must contain exactly 16 bytes")))
}

fn access_denied(reason: CallerAuthRefusal) -> fdo::Error {
    fdo::Error::AccessDenied(format!("caller authentication refused: {reason:?}"))
}

pub struct ActivationDbusBoundary {
    authenticator: DbusCallerAuthenticator,
    runtime: Option<Arc<Mutex<ProductionActivationRuntime>>>,
}

impl ActivationDbusBoundary {
    fn source_qualification(authenticator: DbusCallerAuthenticator) -> Self {
        Self {
            authenticator,
            runtime: None,
        }
    }

    fn production(
        authenticator: DbusCallerAuthenticator,
        runtime: Arc<Mutex<ProductionActivationRuntime>>,
    ) -> Self {
        Self {
            authenticator,
            runtime: Some(runtime),
        }
    }

    fn authenticate(&self, header: &Header<'_>) -> fdo::Result<()> {
        self.authenticator
            .authenticate(header)
            .map(|_| ())
            .map_err(access_denied)
    }

    fn dispatch(
        &self,
        request_id: ActivationRequestId,
        generation: DesktopContextGeneration,
        operation: ActivationOperation,
    ) -> BoundaryReceipt {
        let Some(runtime) = &self.runtime else {
            return BoundaryReceipt::new(
                BoundaryReceiptCode::SessionUnavailable,
                "SESSION_UNAVAILABLE: source-qualification boundary has no execution runtime",
            );
        };
        let Ok(mut runtime) = runtime.lock() else {
            return BoundaryReceipt::new(
                BoundaryReceiptCode::SessionUnavailable,
                "SESSION_UNAVAILABLE: activation runtime lock poisoned",
            );
        };
        runtime.dispatch(request_id, generation, operation)
    }
}

#[interface(name = "in.gnu.os.Activation1")]
impl ActivationDbusBoundary {
    fn activate_observed_workspace(
        &self,
        request_id: Vec<u8>,
        client_observation_generation: u64,
        workspace_id: Vec<u8>,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, String)> {
        let request_id = ActivationRequestId::from_bytes(exact_id(request_id, "request_id")?);
        let workspace_id = WorkspaceId::from_bytes(exact_id(workspace_id, "workspace_id")?);
        self.authenticate(&header)?;
        Ok(self
            .dispatch(
                request_id,
                DesktopContextGeneration(client_observation_generation),
                ActivationOperation::ActivateObservedWorkspace(workspace_id),
            )
            .wire())
    }

    fn focus_observed_application(
        &self,
        request_id: Vec<u8>,
        client_observation_generation: u64,
        surface_id: Vec<u8>,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, String)> {
        let request_id = ActivationRequestId::from_bytes(exact_id(request_id, "request_id")?);
        let surface_id = SurfaceId::from_bytes(exact_id(surface_id, "surface_id")?);
        self.authenticate(&header)?;
        Ok(self
            .dispatch(
                request_id,
                DesktopContextGeneration(client_observation_generation),
                ActivationOperation::FocusObservedApplication(surface_id),
            )
            .wire())
    }

    /// WS-02B: move an observed application window onto an observed
    /// workspace (it stays where the operator is looking).
    fn move_observed_application(
        &self,
        request_id: Vec<u8>,
        client_observation_generation: u64,
        surface_id: Vec<u8>,
        workspace_id: Vec<u8>,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, String)> {
        let request_id = ActivationRequestId::from_bytes(exact_id(request_id, "request_id")?);
        let surface_id = SurfaceId::from_bytes(exact_id(surface_id, "surface_id")?);
        let workspace_id = WorkspaceId::from_bytes(exact_id(workspace_id, "workspace_id")?);
        self.authenticate(&header)?;
        Ok(self
            .dispatch(
                request_id,
                DesktopContextGeneration(client_observation_generation),
                ActivationOperation::MoveObservedApplicationToWorkspace(surface_id, workspace_id),
            )
            .wire())
    }
}

fn session_authenticator() -> Result<DbusCallerAuthenticator, zbus::Error> {
    let auth_connection = Connection::session()?;
    Ok(DbusCallerAuthenticator::from_connection(
        auth_connection,
        // SAFETY: geteuid has no preconditions and does not mutate memory.
        unsafe { libc::geteuid() },
    ))
}

pub fn build_source_qualified_boundary() -> Result<Connection, zbus::Error> {
    Builder::session()?
        .max_queued(32)
        .serve_at(
            ACTIVATION_OBJECT_PATH,
            ActivationDbusBoundary::source_qualification(session_authenticator()?),
        )?
        .name(ACTIVATION_BUS_NAME)?
        .build()
}

pub(crate) fn build_production_boundary(
    runtime: Arc<Mutex<ProductionActivationRuntime>>,
) -> Result<Connection, zbus::Error> {
    Builder::session()?
        .max_queued(32)
        .serve_at(
            ACTIVATION_OBJECT_PATH,
            ActivationDbusBoundary::production(session_authenticator()?, runtime),
        )?
        .name(ACTIVATION_BUS_NAME)?
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    use gnosix_desktop_model::{
        ConnectorName, DesktopContextBaseline, LogicalPoint, LogicalSize, OutputHubId, OutputState,
        OutputTransform, PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli,
        ValidatedOutputCollection, ValidatedSurfaceCollection, ValidatedWorkspaceCollection,
        WorkspaceFocus, WorkspaceName, WorkspaceState, WorkspaceVisibility,
    };
    use in_gnu_os_activation::{
        BackendAttemptError, CurrentActivationObservation, CurrentExecutionTarget,
        NativeExecutionTarget, ObservedWorkspaceBinding, admit_authenticated_gnos_ux,
    };

    fn epoch(byte: u8) -> ActivationSessionEpoch {
        ActivationSessionEpoch::from_bytes([byte; 16])
    }

    fn request(byte: u8) -> ActivationRequestId {
        ActivationRequestId::from_bytes([byte; 16])
    }

    fn workspace(byte: u8) -> ActivationOperation {
        ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([byte; 16]))
    }

    fn workspace_baseline(generation: u64) -> DesktopContextBaseline {
        let output_id = OutputHubId::from_bytes([10; 16]);
        let workspace_id = WorkspaceId::from_bytes([20; 16]);
        let outputs = ValidatedOutputCollection::from_outputs(vec![OutputState {
            output_hub_id: output_id,
            connector: ConnectorName::new("DP-1").unwrap(),
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
                focus: WorkspaceFocus::Unfocused,
            }],
            &outputs,
        )
        .unwrap();
        let surfaces =
            ValidatedSurfaceCollection::from_surfaces(Vec::new(), &outputs, &workspaces).unwrap();
        DesktopContextBaseline {
            generation: DesktopContextGeneration(generation),
            outputs,
            workspaces,
            surfaces,
        }
    }

    fn execution_observation(
        epoch: ActivationSessionEpoch,
        generation: u64,
        focused: bool,
        present: bool,
    ) -> ExecutionObservation {
        ExecutionObservation::new(
            epoch,
            "session-a",
            DesktopContextGeneration(generation),
            if present {
                vec![ObservedWorkspaceBinding::with_focus(
                    WorkspaceId::from_bytes([20; 16]),
                    Some(7),
                    focused,
                )]
            } else {
                Vec::new()
            },
            Vec::new(),
        )
    }

    struct FakeAuthorityExecutor {
        epoch: ActivationSessionEpoch,
        baseline: DesktopContextBaseline,
        observations: VecDeque<Result<ExecutionObservation, ExecutionObservationError>>,
        attempts: u64,
    }

    impl ActivationExecutionBackend for FakeAuthorityExecutor {
        fn observe(&mut self) -> Result<ExecutionObservation, ExecutionObservationError> {
            self.observations
                .pop_front()
                .expect("unexpected observation request")
        }

        fn attempt(&mut self, target: &CurrentExecutionTarget) -> Result<(), BackendAttemptError> {
            assert_eq!(
                target.native_target(),
                &NativeExecutionTarget::WorkspaceNumericId(7)
            );
            self.attempts += 1;
            Ok(())
        }
    }

    impl AuthorityExecutor for FakeAuthorityExecutor {
        fn admit_current(
            &mut self,
            request_id: ActivationRequestId,
            operation: ActivationOperation,
        ) -> Result<(AdmittedActivation, ExecutionObservation), ProductionAdmissionError> {
            let observation =
                execution_observation(self.epoch, self.baseline.generation.0, false, true);
            let current = CurrentActivationObservation::with_runtime_session(
                self.epoch,
                &self.baseline,
                observation.runtime_session(),
            );
            let admitted = admit_authenticated_gnos_ux(request_id, operation, current)
                .map_err(ProductionAdmissionError::Refused)?;
            Ok((admitted, observation))
        }
    }

    #[test]
    fn wire_ids_are_exactly_128_bits() {
        assert_eq!(exact_id(vec![1; 16], "id").unwrap(), [1; 16]);
        assert!(exact_id(vec![1; 15], "id").is_err());
        assert!(exact_id(vec![1; 17], "id").is_err());
    }

    #[test]
    fn canonical_dbus_identity_tracks_ratified_in_gnu_os_owner() {
        assert_eq!(ACTIVATION_BUS_NAME, "in.gnu.os.Activation1");
        assert_eq!(ACTIVATION_INTERFACE, "in.gnu.os.Activation1");
        assert_eq!(ACTIVATION_OBJECT_PATH, "/in/gnu/os/Activation1");
        assert!(!ACTIVATION_BUS_NAME.contains("gnu6"));
    }

    #[test]
    fn authenticated_pipeline_reaches_applied_through_existing_bounded_executor() {
        let authority_epoch = epoch(9);
        let mut fake = FakeAuthorityExecutor {
            epoch: authority_epoch,
            baseline: workspace_baseline(1),
            observations: VecDeque::from(vec![
                Ok(execution_observation(authority_epoch, 1, false, true)),
                Ok(execution_observation(authority_epoch, 1, false, true)),
                Ok(execution_observation(authority_epoch, 2, true, true)),
            ]),
            attempts: 0,
        };

        let receipt =
            execute_authenticated_uncached(&mut fake, authority_epoch, request(40), workspace(20));

        assert_eq!(receipt.code, BoundaryReceiptCode::Applied);
        assert_eq!(fake.attempts, 1);
        assert!(fake.observations.is_empty());
    }

    #[test]
    fn stale_target_refuses_before_native_attempt() {
        let authority_epoch = epoch(9);
        let mut fake = FakeAuthorityExecutor {
            epoch: authority_epoch,
            baseline: workspace_baseline(1),
            observations: VecDeque::from(vec![Ok(execution_observation(
                authority_epoch,
                2,
                false,
                false,
            ))]),
            attempts: 0,
        };

        let receipt =
            execute_authenticated_uncached(&mut fake, authority_epoch, request(41), workspace(20));

        assert_eq!(receipt.code, BoundaryReceiptCode::TargetDisappeared);
        assert_eq!(fake.attempts, 0);
        assert!(fake.observations.is_empty());
    }

    #[test]
    fn duplicate_full_pipeline_never_attempts_twice() {
        let authority_epoch = epoch(9);
        let mut fake = FakeAuthorityExecutor {
            epoch: authority_epoch,
            baseline: workspace_baseline(1),
            observations: VecDeque::from(vec![
                Ok(execution_observation(authority_epoch, 1, false, true)),
                Ok(execution_observation(authority_epoch, 1, false, true)),
                Ok(execution_observation(authority_epoch, 2, true, true)),
            ]),
            attempts: 0,
        };
        let mut ledger = ReplayLedger::new(authority_epoch);
        let request_id = request(42);
        let fingerprint = RequestFingerprint {
            operation: workspace(20),
            client_observation_generation: DesktopContextGeneration(77),
        };

        let first = replay_or_execute(
            &mut ledger,
            authority_epoch,
            request_id,
            fingerprint,
            || {
                execute_authenticated_uncached(
                    &mut fake,
                    authority_epoch,
                    request_id,
                    workspace(20),
                )
            },
        );
        let duplicate = replay_or_execute(
            &mut ledger,
            authority_epoch,
            request_id,
            fingerprint,
            || {
                execute_authenticated_uncached(
                    &mut fake,
                    authority_epoch,
                    request_id,
                    workspace(20),
                )
            },
        );

        assert_eq!(first.code, BoundaryReceiptCode::Applied);
        assert_eq!(duplicate, first);
        assert_eq!(fake.attempts, 1);
        assert!(fake.observations.is_empty());
    }

    #[test]
    fn duplicate_request_returns_exact_receipt_without_second_execution() {
        let mut ledger = ReplayLedger::new(epoch(1));
        let id = request(2);
        let fingerprint = RequestFingerprint {
            operation: workspace(3),
            client_observation_generation: DesktopContextGeneration(7),
        };
        let mut executions = 0_u64;
        let first = replay_or_execute(&mut ledger, epoch(1), id, fingerprint, || {
            executions += 1;
            BoundaryReceipt::new(BoundaryReceiptCode::Applied, "first")
        });
        let duplicate = replay_or_execute(&mut ledger, epoch(1), id, fingerprint, || {
            executions += 1;
            BoundaryReceipt::new(BoundaryReceiptCode::BackendRejected, "must not run")
        });
        assert_eq!(executions, 1);
        assert_eq!(first, duplicate);
        assert_eq!(first.code, BoundaryReceiptCode::Applied);
    }

    #[test]
    fn request_id_reuse_with_changed_payload_fails_closed() {
        let mut ledger = ReplayLedger::new(epoch(1));
        let id = request(2);
        let original = RequestFingerprint {
            operation: workspace(3),
            client_observation_generation: DesktopContextGeneration(7),
        };
        ledger.record(
            id,
            original,
            BoundaryReceipt::new(BoundaryReceiptCode::Applied, "first"),
        );
        let changed = RequestFingerprint {
            operation: workspace(4),
            client_observation_generation: DesktopContextGeneration(7),
        };
        assert!(matches!(
            ledger.lookup(epoch(1), id, changed),
            ReplayLookup::Conflict
        ));
    }

    #[test]
    fn replay_ledger_rejects_cross_epoch_lookup() {
        let ledger = ReplayLedger::new(epoch(1));
        assert!(matches!(
            ledger.lookup(
                epoch(2),
                request(3),
                RequestFingerprint {
                    operation: workspace(4),
                    client_observation_generation: DesktopContextGeneration(9),
                },
            ),
            ReplayLookup::WrongEpoch
        ));
    }

    #[test]
    fn full_replay_ledger_refuses_before_execution_and_never_evicts() {
        let mut ledger = ReplayLedger::new(epoch(1));
        for index in 0..COMPLETED_REQUEST_CAPACITY {
            let mut bytes = [0_u8; 16];
            bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
            ledger.record(
                ActivationRequestId::from_bytes(bytes),
                RequestFingerprint {
                    operation: workspace(3),
                    client_observation_generation: DesktopContextGeneration(7),
                },
                BoundaryReceipt::new(BoundaryReceiptCode::Applied, "done"),
            );
        }
        let mut executions = 0_u64;
        let refusal = replay_or_execute(
            &mut ledger,
            epoch(1),
            request(99),
            RequestFingerprint {
                operation: workspace(3),
                client_observation_generation: DesktopContextGeneration(7),
            },
            || {
                executions += 1;
                BoundaryReceipt::new(BoundaryReceiptCode::Applied, "must not execute")
            },
        );
        assert_eq!(executions, 0);
        assert_eq!(refusal.code, BoundaryReceiptCode::ProtocolFailure);
        assert_eq!(ledger.completed.len(), COMPLETED_REQUEST_CAPACITY);
    }

    #[test]
    #[ignore = "requires the live user D-Bus and systemd --user manager"]
    fn live_same_uid_process_outside_scene_unit_is_denied() {
        let _server = build_source_qualified_boundary().expect("serve qualification boundary");
        let client = Connection::session().expect("client session bus");
        let proxy = zbus::blocking::Proxy::new(
            &client,
            ACTIVATION_BUS_NAME,
            ACTIVATION_OBJECT_PATH,
            ACTIVATION_INTERFACE,
        )
        .expect("activation proxy");

        let result: Result<(u32, String), zbus::Error> = proxy.call(
            "ActivateObservedWorkspace",
            &(vec![1_u8; 16], 7_u64, vec![2_u8; 16]),
        );
        let error = result.expect_err("ordinary same-UID process must be refused");
        assert!(
            error.to_string().contains("AccessDenied")
                || error.to_string().contains("caller authentication refused"),
            "unexpected D-Bus refusal: {error}"
        );
    }

    #[test]
    #[ignore = "requires the live user D-Bus"]
    fn live_protocol_rejects_malformed_ids_and_unknown_operations() {
        let _server = build_source_qualified_boundary().expect("serve qualification boundary");
        let client = Connection::session().expect("client session bus");
        let proxy = zbus::blocking::Proxy::new(
            &client,
            ACTIVATION_BUS_NAME,
            ACTIVATION_OBJECT_PATH,
            ACTIVATION_INTERFACE,
        )
        .expect("activation proxy");

        let malformed: Result<(u32, String), zbus::Error> = proxy.call(
            "ActivateObservedWorkspace",
            &(vec![1_u8; 15], 7_u64, vec![2_u8; 16]),
        );
        let malformed = malformed.expect_err("malformed id must fail");
        assert!(
            malformed.to_string().contains("InvalidArgs")
                || malformed.to_string().contains("exactly 16 bytes"),
            "unexpected malformed-request error: {malformed}"
        );

        let unknown: Result<(), zbus::Error> = proxy.call("RawCommand", &());
        let unknown = unknown.expect_err("unknown operation must fail");
        assert!(
            unknown.to_string().contains("UnknownMethod")
                || unknown.to_string().contains("unknown method"),
            "unexpected unknown-operation error: {unknown}"
        );
    }
}
