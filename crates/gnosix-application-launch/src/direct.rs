//! Opt-in controlled harness backend. A direct child inherits the caller's
//! cgroup: it cannot promise independence from a supervised scene stop.
use crate::{
    LaunchExecutionError, LaunchExecutionOwner, LaunchExecutionReceipt, LaunchExecutor, LaunchPlan,
};
use std::process::{Command, Stdio};

pub struct DirectLaunchExecutor;
impl LaunchExecutor for DirectLaunchExecutor {
    fn execute(&self, plan: &LaunchPlan) -> Result<LaunchExecutionReceipt, LaunchExecutionError> {
        let mut child = Command::new(plan.program())
            .args(plan.args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LaunchExecutionError::ExecutableStartFailed(e.to_string()))?;
        let pid = child.id();
        // Reap the controlled child without granting the product lifecycle
        // controls. Dropping a Child alone would leave an exited zombie.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(LaunchExecutionReceipt {
            pid,
            owner: LaunchExecutionOwner::ControlledHarness,
        })
    }
}
