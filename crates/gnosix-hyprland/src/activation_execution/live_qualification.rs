//! Ignored-by-default local runtime proof. The non-default debug-only feature
//! admits exact targets inside the activation authority crate; it returns no
//! capability to this harness. No normal product build contains this path.

use super::HyprlandActivationExecutor;
use gnosix_desktop_model::{DesktopSurfaceRole, SurfaceFocus, WorkspaceFocus};
use in_gnu_os_activation::{
    ActivationOperation, ActivationRequest, ActivationRequestId, ActivationSessionEpoch,
    ActivationSource, admit_for_live_qualification, execute_bounded, revalidate_for_execution,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf, time::SystemTime};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn current_binary_identity() -> Result<(String, String), String> {
    let path = std::env::current_exe().map_err(|error| format!("binary path: {error}"))?;
    let mut binary = fs::File::open(&path).map_err(|error| format!("binary open: {error}"))?;
    let mut sha256 = Sha256::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let count = binary
            .read(&mut chunk)
            .map_err(|error| format!("binary read: {error}"))?;
        if count == 0 {
            break;
        }
        sha256.update(&chunk[..count]);
    }
    Ok((
        path.display().to_string(),
        format!("{:x}", sha256.finalize()),
    ))
}

fn epoch() -> ActivationSessionEpoch {
    let mut bytes = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos()
        .to_le_bytes();
    for (index, byte) in std::process::id().to_le_bytes().iter().enumerate() {
        bytes[index] ^= byte;
    }
    ActivationSessionEpoch::from_bytes(bytes)
}

fn exercise(
    executor: &mut HyprlandActivationExecutor,
    epoch: ActivationSessionEpoch,
    initial_session: &str,
    operation: ActivationOperation,
    request_byte: u8,
) -> Result<Value, String> {
    let (baseline, sidecar) = executor
        .paired_observation_for_live_qualification()
        .map_err(|error| format!("observation: {error:?}"))?;
    if sidecar.session_signature() != initial_session {
        return Err("SessionChanged before admission".into());
    }
    let generation = baseline.generation;
    let request_id = ActivationRequestId::from_bytes([request_byte; 16]);
    let admission_observation = sidecar
        .activation_observation(epoch, baseline)
        .map_err(|error| format!("paired admission: {error:?}"))?;
    let execution_observation = sidecar
        .execution_observation(epoch, baseline)
        .map_err(|error| format!("paired execution binding: {error:?}"))?;
    let request = ActivationRequest::new(
        request_id,
        ActivationSource::GnosUx,
        admission_observation.freshness(),
        operation,
    );
    let admitted = admit_for_live_qualification(request, admission_observation)
        .map_err(|error| format!("admission: {error:?}"))?;
    let target =
        revalidate_for_execution(&admitted, &execution_observation, &execution_observation)
            .map_err(|error| format!("execution binding: {error:?}"))?;
    let selected_target = match operation {
        ActivationOperation::ActivateObservedWorkspace(id) => hex(id.as_bytes()),
        ActivationOperation::FocusObservedApplication(id) => hex(id.as_bytes()),
    };
    let admitted_target = format!("{:?}", admitted.operation());
    let native_target = format!("{:?}", target.native_target());
    let admission_digest = executor
        .last_receipt_for_live_qualification()
        .map(|receipt| receipt.digest_sha256.clone());
    let attempts_before = executor.native_attempts_for_live_qualification();
    let result = execute_bounded(target, executor);
    let attempts_after = executor.native_attempts_for_live_qualification();
    let native_expression = (attempts_after > attempts_before)
        .then(|| {
            executor
                .last_native_expression_for_live_qualification()
                .map(str::to_owned)
        })
        .flatten();
    let post_digest = result.post_generation.and_then(|_| {
        executor
            .last_receipt_for_live_qualification()
            .map(|receipt| receipt.digest_sha256.clone())
    });
    Ok(json!({
        "request_id": hex(&request_id.into_bytes()),
        "operation": format!("{operation:?}"),
        "selected_semantic_target": selected_target,
        "admitted_operation": admitted_target,
        "revalidated_native_target": native_target,
        "admission_generation": generation.0,
        "admission_digest_sha256": admission_digest,
        "session_signature": initial_session,
        "execution_runtime_session": result.runtime_session,
        "pre_execution_generation": result.pre_generation.map(|generation| generation.0),
        "backend_attempt": format!("{:?}", result.backend_attempt),
        "native_attempts_before": attempts_before,
        "native_attempts_after": attempts_after,
        "native_expression": native_expression,
        "postcondition_generation": result.post_generation.map(|generation| generation.0),
        "postcondition_digest_sha256": post_digest,
        "postcondition": format!("{:?}", result.postcondition),
        "outcome": format!("{:?}", result.outcome),
    }))
}

fn successful(value: &Value, expected: &str) -> bool {
    value.get("outcome").and_then(Value::as_str) == Some(expected)
        && value.get("postcondition").and_then(Value::as_str) == Some("Matched")
}

fn phase_record(trace: Result<Value, String>, pass: bool) -> Value {
    let trace = match trace {
        Ok(value) => value,
        Err(error) => json!({"error": error}),
    };
    json!({"status": if pass { "PASS" } else { "FAIL" }, "trace": trace})
}

fn run_live() -> Value {
    let epoch = epoch();
    let binary_identity = current_binary_identity();
    let mut executor = match HyprlandActivationExecutor::for_live_qualification(epoch) {
        Ok(executor) => executor,
        Err(error) => {
            return json!({"overall": "NOT_RUN_PREREQUISITE_ABSENT", "reason": format!("backend: {error:?}")});
        }
    };
    let (baseline, sidecar) = match executor.paired_observation_for_live_qualification() {
        Ok(pair) => pair,
        Err(error) => {
            return json!({"overall": "NOT_RUN_PREREQUISITE_ABSENT", "reason": format!("initial observation: {error:?}")});
        }
    };
    let session = sidecar.session_signature().to_owned();
    let original_workspace = baseline
        .workspaces
        .iter()
        .find(|workspace| {
            workspace.focus == WorkspaceFocus::Focused
                && sidecar
                    .workspace_runtime_id(workspace.workspace_id)
                    .is_some()
        })
        .map(|workspace| workspace.workspace_id);
    let alternate_workspace = baseline
        .workspaces
        .iter()
        .find(|workspace| {
            Some(workspace.workspace_id) != original_workspace
                && sidecar
                    .workspace_runtime_id(workspace.workspace_id)
                    .is_some_and(|id| id > 0 && id <= i32::MAX as i64)
        })
        .map(|workspace| workspace.workspace_id);
    let mut record = json!({
        "schema": "gnosix.r13-01f-live-activation-qualification.v1",
        "qualification_build": "non-default debug-only feature + ignored test",
        "source_commit": std::env::var("GNOSIX_R13_01F_SOURCE_COMMIT").ok(),
        "binary_identity": binary_identity.as_ref().map(|(path, sha256)| json!({"path": path, "sha256": sha256})).unwrap_or_else(|error| json!({"error": error})),
        "session_signature": session,
        "observer_epoch": hex(&epoch.into_bytes()),
        "pre_action_to_mutation_atomicity": "NOT_PROVEN",
        "workspace_live_execution": "NOT_RUN_PREREQUISITE_ABSENT",
        "workspace_restoration": "NOT_RUN_PREREQUISITE_ABSENT",
        "application_focus_live_execution": "NOT_RUN_PREREQUISITE_ABSENT",
        "application_focus_restoration": "NOT_RUN_PREREQUISITE_ABSENT",
        "already_satisfied_control": "NOT_RUN_PREREQUISITE_ABSENT",
    });
    if let Some(original) = original_workspace {
        record["initial_workspace"] = json!({
            "semantic_id": hex(original.as_bytes()),
            "native_id": sidecar.workspace_runtime_id(original),
        });
    }
    if let Some(original) = original_workspace {
        let control = exercise(
            &mut executor,
            epoch,
            &session,
            ActivationOperation::ActivateObservedWorkspace(original),
            1,
        );
        let control_pass = control.as_ref().is_ok_and(|trace| {
            trace.get("outcome").and_then(Value::as_str) == Some("AlreadySatisfied")
                && trace.get("backend_attempt").and_then(Value::as_str) == Some("NotAttempted")
                && trace["native_attempts_before"] == trace["native_attempts_after"]
        });
        record["already_satisfied_control"] = phase_record(control, control_pass);
        if !control_pass {
            record["overall"] = json!("FAIL");
            return record;
        }
    }
    if let (Some(original), Some(alternate)) = (original_workspace, alternate_workspace) {
        let attempt = exercise(
            &mut executor,
            epoch,
            &session,
            ActivationOperation::ActivateObservedWorkspace(alternate),
            2,
        );
        let attempt_pass = attempt
            .as_ref()
            .is_ok_and(|trace| successful(trace, "Applied"));
        record["workspace_live_execution"] = phase_record(attempt, attempt_pass);
        // Restoration is attempted independently even when the requested
        // mutation failed; the same typed path decides whether it is needed.
        let restoration = exercise(
            &mut executor,
            epoch,
            &session,
            ActivationOperation::ActivateObservedWorkspace(original),
            3,
        );
        let restoration_pass = restoration.as_ref().is_ok_and(|trace| {
            successful(trace, "Applied")
                || trace.get("outcome").and_then(Value::as_str) == Some("AlreadySatisfied")
        });
        record["workspace_restoration"] = phase_record(restoration, restoration_pass);
        if !restoration_pass {
            record["overall"] = json!("RESTORATION_FAILED");
            return record;
        }
    }
    let (baseline, sidecar) = match executor.paired_observation_for_live_qualification() {
        Ok(pair) => pair,
        Err(error) => {
            record["overall"] = json!("OBSERVATION_FAILED_AFTER_WORKSPACE_PHASE");
            record["application_reason"] = json!(format!("{error:?}"));
            return record;
        }
    };
    if sidecar.session_signature() != session {
        record["overall"] = json!("SESSION_CHANGED");
        return record;
    }
    let original_surface = baseline
        .surfaces
        .iter()
        .find(|surface| {
            surface.focus == SurfaceFocus::Focused
                && surface.role == DesktopSurfaceRole::Application
                && sidecar.application_stable_id(surface.surface_id).is_some()
        })
        .map(|surface| surface.surface_id);
    if let Some(original) = original_surface {
        record["initial_application"] = json!({
            "semantic_id": hex(original.as_bytes()),
            "native_stable_id": sidecar.application_stable_id(original),
        });
    }
    let alternate_surface = baseline
        .surfaces
        .iter()
        .find(|surface| {
            Some(surface.surface_id) != original_surface
                && surface.role == DesktopSurfaceRole::Application
                && sidecar.application_stable_id(surface.surface_id).is_some()
        })
        .map(|surface| surface.surface_id);
    if let (Some(original), Some(alternate)) = (original_surface, alternate_surface) {
        let attempt = exercise(
            &mut executor,
            epoch,
            &session,
            ActivationOperation::FocusObservedApplication(alternate),
            4,
        );
        let attempt_pass = attempt
            .as_ref()
            .is_ok_and(|trace| successful(trace, "Applied"));
        record["application_focus_live_execution"] = phase_record(attempt, attempt_pass);
        let restoration = exercise(
            &mut executor,
            epoch,
            &session,
            ActivationOperation::FocusObservedApplication(original),
            5,
        );
        let restoration_pass = restoration.as_ref().is_ok_and(|trace| {
            successful(trace, "Applied")
                || trace.get("outcome").and_then(Value::as_str) == Some("AlreadySatisfied")
        });
        record["application_focus_restoration"] = phase_record(restoration, restoration_pass);
        if !restoration_pass {
            record["overall"] = json!("RESTORATION_FAILED");
            return record;
        }
    }
    let phases = [
        "workspace_live_execution",
        "workspace_restoration",
        "application_focus_live_execution",
        "application_focus_restoration",
        "already_satisfied_control",
    ];
    record["overall"] = json!(if phases
        .iter()
        .all(|phase| record[*phase]["status"] == "PASS")
    {
        "CLOSED_FOR_LOCAL_NATIVE_TYPED_EXECUTION_SCOPE"
    } else if phases
        .iter()
        .any(|phase| record[*phase]["status"] == "FAIL")
    {
        "FAIL"
    } else {
        "PARTIAL_PREREQUISITE_ABSENT"
    });
    record
}

/// Run explicitly with `cargo test -p gnosix-hyprland --features
/// r13-live-qualification --lib live_typed_activation_qualification --
/// --ignored --nocapture`.
/// Never part of an ordinary test run.
#[test]
#[ignore = "mutates local Hyprland focus; R13-01F explicit qualification only"]
fn live_typed_activation_qualification() {
    let record = run_live();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../evidence/runs/gnosix-r13-01f-live-activation-qualification.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&record).expect("serialize receipt"),
    )
    .expect("write live qualification receipt");
    println!(
        "R13-01F receipt={} overall={}",
        path.display(),
        record["overall"]
    );
    assert_ne!(
        record["overall"], "FAIL",
        "live typed activation failed; inspect retained receipt"
    );
    assert_ne!(
        record["overall"], "RESTORATION_FAILED",
        "focus restoration failed; inspect retained receipt"
    );
}
