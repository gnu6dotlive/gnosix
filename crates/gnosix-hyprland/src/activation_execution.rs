//! Exact, bounded Hyprland activation adapter. Construction is deliberately
//! unavailable to downstream crates until the in.gnu.os production issuer and
//! host are separately authorized. This is source/API containment, not OS
//! process authentication.

use in_gnu_os_activation::{
    ActivationExecutionBackend, ActivationOperation, ActivationSessionEpoch, BackendAttemptError,
    CurrentExecutionTarget, ExecutionObservation, ExecutionObservationError, NativeExecutionTarget,
};
#[cfg(feature = "r13-production-dbus")]
use in_gnu_os_activation::{
    ActivationRefusal, ActivationRequestId, AdmittedActivation, CurrentActivationObservation,
    admit_authenticated_gnos_ux,
};

#[cfg(any(feature = "r13-live-qualification", feature = "r14-production-host"))]
use crate::BackendError;
#[cfg(feature = "r13-live-qualification")]
use crate::semantic::DesktopSemanticReceipt;
use crate::{
    HyprctlLuaBackend, HyprlandRuntimeBackend, SUPPORTED_HYPRLAND_VERSION,
    semantic::{DesktopExecutionBindingSnapshot, DesktopSemanticRuntime, SemanticObservationError},
};
#[cfg(feature = "r13-live-qualification")]
use gnosix_desktop_model::DesktopContextBaseline;

/// The only native activation adapter. Its fields and constructor are private;
/// GNOS.UX and Gnoshow cannot construct compositor execution authority.
///
/// ```compile_fail
/// use gnosix_hyprland::activation_execution::HyprlandActivationExecutor;
/// let _executor = HyprlandActivationExecutor {};
/// ```
#[cfg(feature = "r13-production-dbus")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductionAdmissionError {
    ObservationFailed,
    EpochMismatch,
    Refused(ActivationRefusal),
}

pub struct HyprlandActivationExecutor {
    hyprctl: HyprctlLuaBackend,
    semantic: DesktopSemanticRuntime,
    epoch: ActivationSessionEpoch,
    #[cfg(feature = "r13-live-qualification")]
    last_receipt: Option<DesktopSemanticReceipt>,
    #[cfg(feature = "r13-live-qualification")]
    attempted_native_mutations: u64,
    #[cfg(feature = "r13-live-qualification")]
    last_native_expression: Option<String>,
}

impl HyprlandActivationExecutor {
    // A production constructor belongs to the separately qualified in.gnu.os
    // issuer/host. The R13-01E source tests can instantiate this exact adapter.
    #[cfg(test)]
    fn for_source_qualification(
        hyprctl: HyprctlLuaBackend,
        semantic: DesktopSemanticRuntime,
        epoch: ActivationSessionEpoch,
    ) -> Self {
        Self {
            hyprctl,
            semantic,
            epoch,
            #[cfg(feature = "r13-live-qualification")]
            last_receipt: None,
            #[cfg(feature = "r13-live-qualification")]
            attempted_native_mutations: 0,
            #[cfg(feature = "r13-live-qualification")]
            last_native_expression: None,
        }
    }

    /// Internal constructor used only by the in.gnu.os host module. Enabling
    /// the host feature exposes the host runner, not this constructor or the
    /// executor itself as a downstream capability.
    #[cfg(feature = "r14-production-host")]
    pub(crate) fn for_production_host(
        epoch: ActivationSessionEpoch,
        session_signature: &str,
    ) -> Result<Self, BackendError> {
        Ok(Self {
            hyprctl: HyprctlLuaBackend::for_instance_signature(session_signature)?,
            semantic: DesktopSemanticRuntime::default(),
            epoch,
            #[cfg(feature = "r13-live-qualification")]
            last_receipt: None,
            #[cfg(feature = "r13-live-qualification")]
            attempted_native_mutations: 0,
            #[cfg(feature = "r13-live-qualification")]
            last_native_expression: None,
        })
    }

    fn observe_paired(
        &mut self,
    ) -> Result<DesktopExecutionBindingSnapshot, SemanticObservationError> {
        let (receipt, binding) = self
            .hyprctl
            .observe_and_reconcile_desktop_with_execution_bindings(&mut self.semantic)?;
        #[cfg(feature = "r13-live-qualification")]
        {
            self.last_receipt = Some(receipt);
        }
        #[cfg(not(feature = "r13-live-qualification"))]
        {
            let _ = receipt;
        }
        Ok(binding)
    }

    /// Observe and admit one authenticated product operation while the
    /// coherent semantic baseline remains borrowed inside this executor.
    /// Neither the baseline nor a capability escapes the authority adapter.
    #[cfg(feature = "r13-production-dbus")]
    pub(crate) fn admit_for_production(
        &mut self,
        request_id: ActivationRequestId,
        operation: ActivationOperation,
    ) -> Result<(AdmittedActivation, ExecutionObservation), ProductionAdmissionError> {
        let observation = self
            .observe()
            .map_err(|_| ProductionAdmissionError::ObservationFailed)?;
        if observation.session_epoch() != self.epoch {
            return Err(ProductionAdmissionError::EpochMismatch);
        }
        let current = CurrentActivationObservation::with_runtime_session(
            self.epoch,
            self.semantic.baseline(),
            observation.runtime_session(),
        );
        let admitted = admit_authenticated_gnos_ux(request_id, operation, current)
            .map_err(ProductionAdmissionError::Refused)?;
        Ok((admitted, observation))
    }

    /// Non-default, local-only test seam. It grants no activation capability
    /// and is not a production issuer, transport or authenticated host.
    #[cfg(feature = "r13-live-qualification")]
    pub fn for_live_qualification(epoch: ActivationSessionEpoch) -> Result<Self, BackendError> {
        Ok(Self {
            hyprctl: HyprctlLuaBackend::from_env()?,
            semantic: DesktopSemanticRuntime::default(),
            epoch,
            last_receipt: None,
            attempted_native_mutations: 0,
            last_native_expression: None,
        })
    }

    /// Paired baseline and native sidecar from one fresh observer generation.
    #[cfg(feature = "r13-live-qualification")]
    pub fn paired_observation_for_live_qualification(
        &mut self,
    ) -> Result<(&DesktopContextBaseline, DesktopExecutionBindingSnapshot), SemanticObservationError>
    {
        let binding = self.observe_paired()?;
        Ok((self.semantic.baseline(), binding))
    }

    #[cfg(feature = "r13-live-qualification")]
    pub fn last_receipt_for_live_qualification(&self) -> Option<&DesktopSemanticReceipt> {
        self.last_receipt.as_ref()
    }

    #[cfg(feature = "r13-live-qualification")]
    pub fn native_attempts_for_live_qualification(&self) -> u64 {
        self.attempted_native_mutations
    }

    #[cfg(feature = "r13-live-qualification")]
    pub fn last_native_expression_for_live_qualification(&self) -> Option<&str> {
        self.last_native_expression.as_deref()
    }
}

/// Translate only the exact, prechecked semantic/native pair. The hex stable
/// ID and bounded numeric workspace ID leave no free-form Lua from a caller.
fn native_lua(target: &CurrentExecutionTarget) -> Result<String, BackendAttemptError> {
    native_lua_for_pair(target.operation(), target.native_target())
}

fn native_lua_for_pair(
    operation: ActivationOperation,
    native_target: &NativeExecutionTarget,
) -> Result<String, BackendAttemptError> {
    match (operation, native_target) {
        (
            ActivationOperation::ActivateObservedWorkspace(_),
            NativeExecutionTarget::WorkspaceNumericId(id),
        ) if (1..=i32::MAX as i64).contains(id) => Ok(format!(
            "hl.dispatch(hl.dsp.focus({{ workspace = \"{id}\" }}))"
        )),
        (
            ActivationOperation::FocusObservedApplication(_),
            NativeExecutionTarget::ApplicationStableId(stable_id),
        ) if !stable_id.is_empty() && stable_id.bytes().all(|byte| byte.is_ascii_hexdigit()) => Ok(
            format!("hl.dispatch(hl.dsp.focus({{ window = \"stableid:{stable_id}\" }}))"),
        ),
        _ => Err(BackendAttemptError::Rejected),
    }
}

impl ActivationExecutionBackend for HyprlandActivationExecutor {
    fn observe(&mut self) -> Result<ExecutionObservation, ExecutionObservationError> {
        let binding = self
            .observe_paired()
            .map_err(|_| ExecutionObservationError::Unavailable)?;
        binding
            .execution_observation(self.epoch, self.semantic.baseline())
            .map_err(|_| ExecutionObservationError::Unavailable)
    }

    fn attempt(&mut self, target: &CurrentExecutionTarget) -> Result<(), BackendAttemptError> {
        let lua = native_lua(target)?;
        match self.hyprctl.version() {
            Ok(version) if version == SUPPORTED_HYPRLAND_VERSION => {}
            Ok(_) => return Err(BackendAttemptError::Rejected),
            Err(_) => return Err(BackendAttemptError::Transport),
        }
        #[cfg(feature = "r13-live-qualification")]
        {
            self.attempted_native_mutations += 1;
            self.last_native_expression = Some(lua.clone());
        }
        let response = self
            .hyprctl
            .run("eval", &lua)
            .map_err(|_| BackendAttemptError::Transport)?;
        if response.trim() == "ok" {
            Ok(())
        } else {
            Err(BackendAttemptError::Rejected)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_constructor_is_not_public() {
        // This exercises the internal source seam without issuing authority to
        // downstream crates. The fields remain private to this module.
        let _constructor: fn(
            HyprctlLuaBackend,
            DesktopSemanticRuntime,
            ActivationSessionEpoch,
        ) -> HyprlandActivationExecutor = HyprlandActivationExecutor::for_source_qualification;
    }

    #[test]
    fn exact_native_lua_has_only_two_typed_forms() {
        use gnosix_desktop_model::{SurfaceId, WorkspaceId};
        let workspace =
            ActivationOperation::ActivateObservedWorkspace(WorkspaceId::from_bytes([1; 16]));
        let application =
            ActivationOperation::FocusObservedApplication(SurfaceId::from_bytes([2; 16]));
        assert_eq!(
            native_lua_for_pair(workspace, &NativeExecutionTarget::WorkspaceNumericId(7)),
            Ok("hl.dispatch(hl.dsp.focus({ workspace = \"7\" }))".into())
        );
        assert_eq!(
            native_lua_for_pair(
                application,
                &NativeExecutionTarget::ApplicationStableId("aabb".into())
            ),
            Ok("hl.dispatch(hl.dsp.focus({ window = \"stableid:aabb\" }))".into())
        );
        assert_eq!(
            native_lua_for_pair(
                workspace,
                &NativeExecutionTarget::ApplicationStableId("aabb".into())
            ),
            Err(BackendAttemptError::Rejected)
        );
        assert_eq!(
            native_lua_for_pair(workspace, &NativeExecutionTarget::WorkspaceNumericId(0)),
            Err(BackendAttemptError::Rejected)
        );
        assert_eq!(
            native_lua_for_pair(workspace, &NativeExecutionTarget::WorkspaceNumericId(-7)),
            Err(BackendAttemptError::Rejected)
        );
        assert_eq!(
            native_lua_for_pair(
                application,
                &NativeExecutionTarget::ApplicationStableId("aabb\"; hl.dsp.exit()".into())
            ),
            Err(BackendAttemptError::Rejected)
        );
    }
}

#[cfg(all(test, feature = "r13-live-qualification"))]
mod live_qualification;
