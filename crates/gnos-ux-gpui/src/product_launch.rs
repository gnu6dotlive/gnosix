//! The product-side launch action.
//!
//! This is deliberately not in [`crate::product_controller`]. That controller
//! exists to dispatch to the R13 activation authority, which is why it is
//! gated on the production-client feature. Launching an installed application
//! is a different power with a different vocabulary and a different freshness
//! domain, and putting it there would both inherit the wrong authority gate
//! and invite the two to be confused.
//!
//! The chain is the product contract's, end to end:
//!
//! ```text
//! product intent naming an installed application
//!     -> this controller
//!     -> the typed launch capability
//!     -> a deterministic receipt
//!     -> recorded state
//! ```
//!
//! There is no way to express a program name anywhere in that chain. The
//! caller passes content derived from a catalog snapshot, and the capability
//! re-resolves the entry itself, so the controller cannot be used to run an
//! arbitrary command even by accident.
//!
//! Freshness is the catalog's, not the desktop's. Content is bound to the
//! [`CatalogGeneration`] that presented it; if the runtime has since taken a
//! different catalog, the launch is refused rather than resolved against
//! entries the user never saw.

use crate::product_runtime::{
    ProductInstalledApplicationContent, ProductLaunchOutcome, ProductLaunchRecord, ProductRuntime,
};
use gnosix_application_launch::{
    ApplicationLauncher, LaunchDocuments, LaunchExecutionError, LaunchExecutionReceipt,
    LaunchExecutor, LaunchOutcome, LaunchPlan, LaunchReceipt, LaunchReference,
    SystemdUserLaunchExecutor,
};

pub fn launch_application(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
) -> ProductLaunchRecord {
    launch_with(runtime, selected, &ApplicationLauncher::default())
}

/// Start one of the entry's desktop actions (DECK-01D). The action is named
/// by its identifier on the same typed reference, so the capability resolves
/// its `Exec` from the catalog generation the user saw, or refuses.
pub fn launch_application_action(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
    action: &str,
) -> ProductLaunchRecord {
    let request = selected.reference.clone().with_action(action);
    launch_reference(
        runtime,
        selected,
        &request,
        &ApplicationLauncher::default(),
        &SystemdUserLaunchExecutor,
    )
}

/// Launch with an explicit capability, for a session that has operator
/// configuration such as a terminal command.
pub fn launch_with(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
    launcher: &ApplicationLauncher,
) -> ProductLaunchRecord {
    launch_with_executor(runtime, selected, launcher, &SystemdUserLaunchExecutor)
}

fn launch_with_executor(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
    launcher: &ApplicationLauncher,
    executor: &dyn LaunchExecutor,
) -> ProductLaunchRecord {
    launch_reference(runtime, selected, &selected.reference, launcher, executor)
}

fn launch_reference(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
    request: &LaunchReference,
    launcher: &ApplicationLauncher,
    executor: &dyn LaunchExecutor,
) -> ProductLaunchRecord {
    runtime.begin_launch_action(selected);

    // The content was built from a catalog snapshot. If the runtime has
    // since taken a different one, the entry the user picked may no longer
    // be the entry that would run, and the capability refuses that rather
    // than resolving against a catalog the user never saw.
    let catalog = runtime.application_catalog().clone();
    let documents = LaunchDocuments::None;
    let receipt: LaunchReceipt =
        launcher.launch_with_executor(request, &catalog, &documents, executor);
    emit_launch(&receipt);
    runtime.finish_launch_action(outcome_of(&receipt), receipt.summary())
}

/// Translate a launch receipt into the product's own outcome vocabulary.
fn outcome_of(receipt: &LaunchReceipt) -> ProductLaunchOutcome {
    match &receipt.outcome {
        LaunchOutcome::Launched { pid, .. } => ProductLaunchOutcome::Launched { pid: *pid },
        LaunchOutcome::SpawnFailed { error, .. } => ProductLaunchOutcome::SpawnFailed {
            detail: error.to_string(),
        },
        LaunchOutcome::Refused(refusal) => ProductLaunchOutcome::Refused {
            code: refusal.code(),
            detail: format!("{refusal:?}"),
        },
    }
}

/// Record the launch on the product's own stream.
///
/// A launch is a local decision, so it is reported as one: the entry that was
/// resolved, the generation it was resolved against, and the outcome. The
/// receipt never says a program was started unless one was.
fn emit_launch(receipt: &LaunchReceipt) {
    if receipt.outcome.is_launched() {
        println!(
            "GNOS_UX_APPLICATION_LAUNCH entry={} catalog_generation={} status=receipt {}",
            receipt.entry.as_str(),
            receipt.generation.hex(),
            receipt.summary(),
        );
    } else {
        eprintln!(
            "GNOS_UX_APPLICATION_LAUNCH entry={} catalog_generation={} status=refused {}",
            receipt.entry.as_str(),
            receipt.generation.hex(),
            receipt.summary(),
        );
    }
}

#[cfg(test)]
mod tests;

/// Deterministic lab action: resolve real policy, but never start a process or
/// contact the service manager for synthetic catalog entries.
pub fn launch_lab_application(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
) -> ProductLaunchRecord {
    struct DisabledLabExecutor;
    impl LaunchExecutor for DisabledLabExecutor {
        fn execute(&self, _: &LaunchPlan) -> Result<LaunchExecutionReceipt, LaunchExecutionError> {
            Err(LaunchExecutionError::ExecutableStartFailed(
                "LAB_EXECUTION_DISABLED".into(),
            ))
        }
    }
    launch_with_executor(
        runtime,
        selected,
        &ApplicationLauncher::default(),
        &DisabledLabExecutor,
    )
}

#[cfg(test)]
pub(crate) fn launch_test_application(
    runtime: &mut ProductRuntime,
    selected: &ProductInstalledApplicationContent,
) -> ProductLaunchRecord {
    struct FakeExecutor;
    impl LaunchExecutor for FakeExecutor {
        fn execute(
            &self,
            plan: &LaunchPlan,
        ) -> Result<LaunchExecutionReceipt, LaunchExecutionError> {
            if plan.program() != std::ffi::OsStr::new("/usr/bin/true") {
                return Err(LaunchExecutionError::ExecutableStartFailed(
                    "fixture executable failure".into(),
                ));
            }
            Ok(LaunchExecutionReceipt {
                pid: 42,
                owner: gnosix_application_launch::LaunchExecutionOwner::ControlledHarness,
            })
        }
    }
    launch_with_executor(
        runtime,
        selected,
        &ApplicationLauncher::default(),
        &FakeExecutor,
    )
}
