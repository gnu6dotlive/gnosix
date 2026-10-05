//! Bounded execution protocol. Only an already-admitted and revalidated
//! `CurrentExecutionTarget` can enter this state machine. The compositor
//! adapter and capability issuer are separate, unimplemented authorities.

use super::{
    revalidate_current_target, ActivationFreshness, ActivationOperation, ActivationRequestId,
    CurrentExecutionTarget, ExecutionObservation, ExecutionRevalidationRefusal,
    NativeExecutionTarget,
};
use gnosix_desktop_model::{DesktopContextGeneration, DesktopSurfaceRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendAttemptError {
    Rejected,
    Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionObservationError {
    Unavailable,
}

/// The observation method must return a coherent, observer-derived semantic
/// and native-binding snapshot. `attempt` receives only the exact typed target.
/// Implementing this trait does not grant a product caller a native executor.
pub trait ActivationExecutionBackend {
    fn observe(&mut self) -> Result<ExecutionObservation, ExecutionObservationError>;
    fn attempt(&mut self, target: &CurrentExecutionTarget) -> Result<(), BackendAttemptError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreExecutionStatus {
    ObservationFailed,
    Refused(ExecutionRevalidationRefusal),
    Ready,
    AlreadySatisfied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendAttemptStatus {
    NotAttempted,
    Accepted,
    Rejected,
    TransportFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostconditionStatus {
    NotObserved,
    Matched,
    Mismatch,
    SessionChanged,
    TargetDisappeared,
    BindingChanged,
    ObservationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationExecutionOutcome {
    Applied,
    AlreadySatisfied,
    RefusedBeforeExecution(ExecutionRevalidationRefusal),
    BackendRejected,
    TransportFailure,
    TargetDisappearedDuringExecution,
    PostconditionMismatch,
    ObservationFailure,
    ObservationSessionChanged,
    ExecutionBindingChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationExecutionReceipt {
    pub request_id: ActivationRequestId,
    pub operation: ActivationOperation,
    pub native_target: NativeExecutionTarget,
    pub admission_freshness: ActivationFreshness,
    pub runtime_session: String,
    pub pre_generation: Option<DesktopContextGeneration>,
    pub post_generation: Option<DesktopContextGeneration>,
    pub pre_execution: PreExecutionStatus,
    pub backend_attempt: BackendAttemptStatus,
    pub postcondition: PostconditionStatus,
    pub outcome: ActivationExecutionOutcome,
}

fn exact_focused(target: &CurrentExecutionTarget, observation: &ExecutionObservation) -> bool {
    match target.operation() {
        ActivationOperation::ActivateObservedWorkspace(id) => observation
            .workspace_binding(id)
            .is_some_and(|binding| binding.is_focused()),
        ActivationOperation::FocusObservedApplication(id) => {
            observation.surface_binding(id).is_some_and(|binding| {
                binding.role() == DesktopSurfaceRole::Application && binding.is_focused()
            })
        }
    }
}

/// Precheck, issue at most one exact native attempt, then verify the exact
/// semantic postcondition. The compositor-side compare/action gap remains
/// bounded but non-atomic.
///
/// A raw request cannot be passed to this boundary:
/// ```compile_fail
/// use in_gnu_os_activation::{ActivationRequest, ActivationExecutionBackend, ActivationExecutionReceipt, execute_bounded};
/// fn wrong<B: ActivationExecutionBackend>(request: ActivationRequest, backend: &mut B) -> ActivationExecutionReceipt {
///     execute_bounded(request, backend)
/// }
/// ```
pub fn execute_bounded<B: ActivationExecutionBackend>(
    target: CurrentExecutionTarget,
    backend: &mut B,
) -> ActivationExecutionReceipt {
    let mut receipt = ActivationExecutionReceipt {
        request_id: target.request_id(),
        operation: target.operation(),
        native_target: target.native_target().clone(),
        admission_freshness: target.admission_freshness(),
        runtime_session: target.runtime_session().to_owned(),
        pre_generation: None,
        post_generation: None,
        pre_execution: PreExecutionStatus::ObservationFailed,
        backend_attempt: BackendAttemptStatus::NotAttempted,
        postcondition: PostconditionStatus::NotObserved,
        outcome: ActivationExecutionOutcome::ObservationFailure,
    };
    let Ok(before) = backend.observe() else {
        return receipt;
    };
    receipt.pre_generation = Some(before.generation());
    let ready = match revalidate_current_target(&target, &before) {
        Ok(ready) => ready,
        Err(reason) => {
            receipt.pre_execution = PreExecutionStatus::Refused(reason);
            receipt.outcome = ActivationExecutionOutcome::RefusedBeforeExecution(reason);
            return receipt;
        }
    };
    if exact_focused(&ready, &before) {
        receipt.pre_execution = PreExecutionStatus::AlreadySatisfied;
        receipt.outcome = ActivationExecutionOutcome::AlreadySatisfied;
        return receipt;
    }
    receipt.pre_execution = PreExecutionStatus::Ready;
    match backend.attempt(&ready) {
        Ok(()) => receipt.backend_attempt = BackendAttemptStatus::Accepted,
        Err(BackendAttemptError::Rejected) => {
            receipt.backend_attempt = BackendAttemptStatus::Rejected
        }
        Err(BackendAttemptError::Transport) => {
            receipt.backend_attempt = BackendAttemptStatus::TransportFailure
        }
    }
    // Even an uncertain transport result gets an observation. It cannot be
    // promoted to Applied; the record still tells us what was seen afterward.
    let after = match backend.observe() {
        Ok(after) => after,
        Err(_) => {
            receipt.postcondition = PostconditionStatus::ObservationFailed;
            receipt.outcome = if receipt.backend_attempt == BackendAttemptStatus::TransportFailure {
                ActivationExecutionOutcome::TransportFailure
            } else if receipt.backend_attempt == BackendAttemptStatus::Rejected {
                ActivationExecutionOutcome::BackendRejected
            } else {
                ActivationExecutionOutcome::ObservationFailure
            };
            return receipt;
        }
    };
    receipt.post_generation = Some(after.generation());
    if after.runtime_session() != ready.runtime_session()
        || after.session_epoch() != ready.admission_freshness().session_epoch()
    {
        receipt.postcondition = PostconditionStatus::SessionChanged;
        receipt.outcome = ActivationExecutionOutcome::ObservationSessionChanged;
        return receipt;
    }
    match revalidate_current_target(&ready, &after) {
        Ok(_) => {
            receipt.postcondition = if exact_focused(&ready, &after) {
                PostconditionStatus::Matched
            } else {
                PostconditionStatus::Mismatch
            };
        }
        Err(ExecutionRevalidationRefusal::TargetDisappeared) => {
            receipt.postcondition = PostconditionStatus::TargetDisappeared;
        }
        Err(_) => {
            receipt.postcondition = PostconditionStatus::BindingChanged;
        }
    }
    receipt.outcome = match receipt.backend_attempt {
        BackendAttemptStatus::Rejected => ActivationExecutionOutcome::BackendRejected,
        BackendAttemptStatus::TransportFailure => ActivationExecutionOutcome::TransportFailure,
        BackendAttemptStatus::Accepted => match receipt.postcondition {
            PostconditionStatus::Matched => ActivationExecutionOutcome::Applied,
            PostconditionStatus::TargetDisappeared => {
                ActivationExecutionOutcome::TargetDisappearedDuringExecution
            }
            PostconditionStatus::BindingChanged => {
                ActivationExecutionOutcome::ExecutionBindingChanged
            }
            _ => ActivationExecutionOutcome::PostconditionMismatch,
        },
        BackendAttemptStatus::NotAttempted => unreachable!(),
    };
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivationSessionEpoch, ObservedSurfaceBinding, ObservedWorkspaceBinding};
    use gnosix_desktop_model::{SurfaceId, WorkspaceId};
    use std::collections::VecDeque;

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::from_bytes([1; 16])
    }
    fn surface_id() -> SurfaceId {
        SurfaceId::from_bytes([2; 16])
    }
    fn epoch() -> ActivationSessionEpoch {
        ActivationSessionEpoch::from_bytes([3; 16])
    }

    fn target(operation: ActivationOperation) -> CurrentExecutionTarget {
        let native_target = match operation {
            ActivationOperation::ActivateObservedWorkspace(_) => {
                NativeExecutionTarget::WorkspaceNumericId(7)
            }
            ActivationOperation::FocusObservedApplication(_) => {
                NativeExecutionTarget::ApplicationStableId("aabb".into())
            }
        };
        CurrentExecutionTarget {
            request_id: ActivationRequestId::from_bytes([4; 16]),
            operation,
            admission_freshness: ActivationFreshness::new(epoch(), DesktopContextGeneration(1)),
            runtime_session: "session-a".into(),
            current_generation: DesktopContextGeneration(1),
            native_target,
        }
    }

    fn observation(workspace_focused: bool, surface_focused: bool) -> ExecutionObservation {
        ExecutionObservation::new(
            epoch(),
            "session-a",
            DesktopContextGeneration(1),
            vec![ObservedWorkspaceBinding::with_focus(
                workspace_id(),
                Some(7),
                workspace_focused,
            )],
            vec![ObservedSurfaceBinding::with_focus(
                surface_id(),
                DesktopSurfaceRole::Application,
                Some("aabb"),
                surface_focused,
            )],
        )
    }

    struct FakeBackend {
        observations: VecDeque<Result<ExecutionObservation, ExecutionObservationError>>,
        attempted: Vec<NativeExecutionTarget>,
        attempt_result: Result<(), BackendAttemptError>,
    }

    impl FakeBackend {
        fn new(observations: Vec<Result<ExecutionObservation, ExecutionObservationError>>) -> Self {
            Self {
                observations: observations.into(),
                attempted: Vec::new(),
                attempt_result: Ok(()),
            }
        }
    }

    impl ActivationExecutionBackend for FakeBackend {
        fn observe(&mut self) -> Result<ExecutionObservation, ExecutionObservationError> {
            self.observations
                .pop_front()
                .expect("unexpected observation")
        }
        fn attempt(&mut self, target: &CurrentExecutionTarget) -> Result<(), BackendAttemptError> {
            self.attempted.push(target.native_target().clone());
            self.attempt_result
        }
    }

    #[test]
    fn workspace_exact_attempt_requires_exact_fresh_postcondition() {
        let mut fake = FakeBackend::new(vec![
            Ok(observation(false, false)),
            Ok(observation(true, false)),
        ]);
        let receipt = execute_bounded(
            target(ActivationOperation::ActivateObservedWorkspace(
                workspace_id(),
            )),
            &mut fake,
        );
        assert_eq!(receipt.outcome, ActivationExecutionOutcome::Applied);
        assert_eq!(receipt.pre_execution, PreExecutionStatus::Ready);
        assert_eq!(receipt.backend_attempt, BackendAttemptStatus::Accepted);
        assert_eq!(receipt.postcondition, PostconditionStatus::Matched);
        assert_eq!(
            fake.attempted,
            vec![NativeExecutionTarget::WorkspaceNumericId(7)]
        );
        assert_eq!(
            receipt.admission_freshness.generation(),
            DesktopContextGeneration(1)
        );
    }

    #[test]
    fn exact_already_satisfied_workspace_and_application_do_not_mutate() {
        for operation in [
            ActivationOperation::ActivateObservedWorkspace(workspace_id()),
            ActivationOperation::FocusObservedApplication(surface_id()),
        ] {
            let mut fake = FakeBackend::new(vec![
                Ok(observation(true, true)),
                Ok(observation(true, true)),
            ]);
            let receipt = execute_bounded(target(operation), &mut fake);
            assert_eq!(
                receipt.outcome,
                ActivationExecutionOutcome::AlreadySatisfied
            );
            assert_eq!(receipt.backend_attempt, BackendAttemptStatus::NotAttempted);
            assert!(fake.attempted.is_empty());
        }
    }

    #[test]
    fn stale_session_or_binding_refuses_before_any_attempt() {
        let mut wrong_session = observation(false, false);
        wrong_session.runtime_session = "session-b".into();
        let mut wrong_binding = observation(false, false);
        wrong_binding.workspaces[0] =
            ObservedWorkspaceBinding::with_focus(workspace_id(), Some(8), false);
        for (input, expected) in [
            (wrong_session, ExecutionRevalidationRefusal::SessionChanged),
            (
                wrong_binding,
                ExecutionRevalidationRefusal::ExecutionBindingChanged,
            ),
        ] {
            let mut fake = FakeBackend::new(vec![Ok(input)]);
            let receipt = execute_bounded(
                target(ActivationOperation::ActivateObservedWorkspace(
                    workspace_id(),
                )),
                &mut fake,
            );
            assert_eq!(
                receipt.outcome,
                ActivationExecutionOutcome::RefusedBeforeExecution(expected)
            );
            assert!(fake.attempted.is_empty());
        }
    }

    #[test]
    fn application_attempt_uses_stable_id_and_never_another_surface() {
        let mut pre = observation(false, false);
        pre.surfaces.push(ObservedSurfaceBinding::with_focus(
            SurfaceId::from_bytes([8; 16]),
            DesktopSurfaceRole::Application,
            Some("ccdd"),
            false,
        ));
        let mut post = pre.clone();
        post.surfaces[0] = ObservedSurfaceBinding::with_focus(
            surface_id(),
            DesktopSurfaceRole::Application,
            Some("aabb"),
            true,
        );
        let mut fake = FakeBackend::new(vec![Ok(pre), Ok(post)]);
        let receipt = execute_bounded(
            target(ActivationOperation::FocusObservedApplication(surface_id())),
            &mut fake,
        );
        assert_eq!(receipt.outcome, ActivationExecutionOutcome::Applied);
        assert_eq!(
            fake.attempted,
            vec![NativeExecutionTarget::ApplicationStableId("aabb".into())]
        );
    }

    #[test]
    fn success_without_exact_postcondition_is_not_applied() {
        for operation in [
            ActivationOperation::ActivateObservedWorkspace(workspace_id()),
            ActivationOperation::FocusObservedApplication(surface_id()),
        ] {
            let mut fake = FakeBackend::new(vec![
                Ok(observation(false, false)),
                Ok(observation(false, false)),
            ]);
            let receipt = execute_bounded(target(operation), &mut fake);
            assert_eq!(
                receipt.outcome,
                ActivationExecutionOutcome::PostconditionMismatch
            );
            assert_eq!(receipt.backend_attempt, BackendAttemptStatus::Accepted);
        }
    }

    #[test]
    fn another_focused_target_never_satisfies_the_requested_identity() {
        let mut workspace_post = observation(false, false);
        workspace_post
            .workspaces
            .push(ObservedWorkspaceBinding::with_focus(
                WorkspaceId::from_bytes([9; 16]),
                Some(9),
                true,
            ));
        let mut workspace_fake =
            FakeBackend::new(vec![Ok(observation(false, false)), Ok(workspace_post)]);
        let workspace_receipt = execute_bounded(
            target(ActivationOperation::ActivateObservedWorkspace(
                workspace_id(),
            )),
            &mut workspace_fake,
        );
        assert_eq!(
            workspace_receipt.outcome,
            ActivationExecutionOutcome::PostconditionMismatch
        );

        let mut application_post = observation(false, false);
        application_post
            .surfaces
            .push(ObservedSurfaceBinding::with_focus(
                SurfaceId::from_bytes([9; 16]),
                DesktopSurfaceRole::Application,
                Some("ccdd"),
                true,
            ));
        let mut application_fake =
            FakeBackend::new(vec![Ok(observation(false, false)), Ok(application_post)]);
        let application_receipt = execute_bounded(
            target(ActivationOperation::FocusObservedApplication(surface_id())),
            &mut application_fake,
        );
        assert_eq!(
            application_receipt.outcome,
            ActivationExecutionOutcome::PostconditionMismatch
        );
    }

    #[test]
    fn disappearance_does_not_retarget_same_descriptors() {
        let mut post = observation(false, false);
        post.surfaces.remove(0);
        post.surfaces.push(ObservedSurfaceBinding::with_focus(
            SurfaceId::from_bytes([8; 16]),
            DesktopSurfaceRole::Application,
            Some("ccdd"),
            true,
        ));
        let mut fake = FakeBackend::new(vec![Ok(observation(false, false)), Ok(post)]);
        let receipt = execute_bounded(
            target(ActivationOperation::FocusObservedApplication(surface_id())),
            &mut fake,
        );
        assert_eq!(
            receipt.outcome,
            ActivationExecutionOutcome::TargetDisappearedDuringExecution
        );
        assert_eq!(
            fake.attempted,
            vec![NativeExecutionTarget::ApplicationStableId("aabb".into())]
        );
    }

    #[test]
    fn post_action_session_change_and_observation_failure_never_apply() {
        let mut changed = observation(true, false);
        changed.runtime_session = "session-b".into();
        for (post, expected) in [
            (
                Ok(changed),
                ActivationExecutionOutcome::ObservationSessionChanged,
            ),
            (
                Err(ExecutionObservationError::Unavailable),
                ActivationExecutionOutcome::ObservationFailure,
            ),
        ] {
            let mut fake = FakeBackend::new(vec![Ok(observation(false, false)), post]);
            let receipt = execute_bounded(
                target(ActivationOperation::ActivateObservedWorkspace(
                    workspace_id(),
                )),
                &mut fake,
            );
            assert_eq!(receipt.outcome, expected);
        }
    }

    #[test]
    fn backend_rejection_and_transport_uncertainty_are_typed() {
        for (error, expected) in [
            (
                BackendAttemptError::Rejected,
                ActivationExecutionOutcome::BackendRejected,
            ),
            (
                BackendAttemptError::Transport,
                ActivationExecutionOutcome::TransportFailure,
            ),
        ] {
            let mut fake = FakeBackend::new(vec![
                Ok(observation(false, false)),
                Ok(observation(true, false)),
            ]);
            fake.attempt_result = Err(error);
            let receipt = execute_bounded(
                target(ActivationOperation::ActivateObservedWorkspace(
                    workspace_id(),
                )),
                &mut fake,
            );
            assert_eq!(receipt.outcome, expected);
            assert_eq!(receipt.postcondition, PostconditionStatus::Matched);
        }
    }
}
