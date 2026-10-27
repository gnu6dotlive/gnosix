//! GNOSIX Hyprland projection boundary.
//!
//! The default build is a library boundary. The opt-in R14 host feature adds
//! the separately supervised in.gnu.os activation process; it does not add a
//! caller transport or process-authentication claim.

pub mod activation_execution;
#[cfg(feature = "r13-production-dbus")]
pub mod caller_auth;
pub mod keyboard_lock;
mod observation_transport;
#[cfg(feature = "r13-production-dbus")]
pub mod production_dbus;
#[cfg(feature = "r14-production-host")]
pub mod production_host;
pub mod semantic;
pub mod topology;

use std::{collections::HashMap, env, process::Command};

use gnos_ux_surface::{
    compositor::{CompositorIntentSource, ResolvedCompositorIntent},
    envelope::{LogicalPx, RuntimeId, SpaceReservationIntent, SurfaceEdge},
    scene::{MonitorSceneId, SceneEpoch},
};

pub const SUPPORTED_HYPRLAND_VERSION: &str = "0.56.2";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectionSlot {
    SurfaceReservation,
    QualificationBorderSize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectionKey {
    pub scene_id: MonitorSceneId,
    pub slot: ProjectionSlot,
    pub runtime_target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceDecision {
    Fresh,
    AlreadyAccepted,
    Stale,
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FenceEntry {
    epoch: SceneEpoch,
    request_token: i64,
}
#[derive(Default)]
pub struct IntentFence {
    accepted: HashMap<ProjectionKey, FenceEntry>,
}

impl IntentFence {
    pub fn evaluate(
        &self,
        key: &ProjectionKey,
        epoch: SceneEpoch,
        request_token: i64,
    ) -> FenceDecision {
        let Some(previous) = self.accepted.get(key) else {
            return FenceDecision::Fresh;
        };
        if epoch < previous.epoch {
            FenceDecision::Stale
        } else if epoch == previous.epoch {
            if request_token == previous.request_token {
                FenceDecision::AlreadyAccepted
            } else {
                FenceDecision::Conflict
            }
        } else {
            FenceDecision::Fresh
        }
    }

    pub fn commit(&mut self, key: ProjectionKey, epoch: SceneEpoch, request_token: i64) {
        self.accepted.insert(
            key,
            FenceEntry {
                epoch,
                request_token,
            },
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceReservationPlan {
    pub source: CompositorIntentSource,
    pub output: Option<RuntimeId>,
    pub edge: SurfaceEdge,
    pub exclusive_zone: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceProjectionError {
    AutoReservationUnsupported,
    ReservationOutOfRange,
}
pub fn surface_reservation_plan(
    intent: &ResolvedCompositorIntent,
) -> Result<SurfaceReservationPlan, SurfaceProjectionError> {
    let exclusive_zone = match intent.reservation {
        SpaceReservationIntent::Overlay => 0,
        SpaceReservationIntent::Reserve(LogicalPx(value)) => {
            i32::try_from(value).map_err(|_| SurfaceProjectionError::ReservationOutOfRange)?
        }
        SpaceReservationIntent::Auto => {
            return Err(SurfaceProjectionError::AutoReservationUnsupported);
        }
    };

    Ok(SurfaceReservationPlan {
        source: intent.source,
        output: intent.output.clone(),
        edge: intent.edge,
        exclusive_zone,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HyprlandGlobalMutation {
    QualificationBorderSize(i64),
}

impl HyprlandGlobalMutation {
    pub const fn slot(self) -> ProjectionSlot {
        match self {
            Self::QualificationBorderSize(_) => ProjectionSlot::QualificationBorderSize,
        }
    }

    pub const fn requested_value(self) -> i64 {
        match self {
            Self::QualificationBorderSize(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalProjectionIntent {
    pub source: CompositorIntentSource,
    pub mutation: HyprlandGlobalMutation,
    pub expected_before: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationOutcome {
    Applied,
    AlreadySatisfied,
    Stale,
    Conflict,
    Unsupported,
    TransportError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationOutcome {
    NotPerformed,
    Matched,
    Mismatch,
    TransportError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollbackOutcome {
    Restored,
    AlreadyRestored,
    Conflict,
    Unsupported,
    TransportError,
    ObservationMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionReceipt {
    pub source: CompositorIntentSource,
    pub slot: ProjectionSlot,
    pub runtime_target: String,
    pub requested_state: i64,
    pub observed_before: Option<i64>,
    pub application_outcome: ApplicationOutcome,
    pub observed_after: Option<i64>,
    pub observation_outcome: ObservationOutcome,
    pub rollback_outcome: Option<RollbackOutcome>,
    pub observed_final: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendError {
    UnsupportedVersion,
    Transport,
    Parse,
}

pub trait HyprlandRuntimeBackend {
    fn runtime_target(&self) -> &str;
    fn version(&self) -> Result<String, BackendError>;
    fn observe(&self, mutation: HyprlandGlobalMutation) -> Result<i64, BackendError>;
    fn apply(&self, mutation: HyprlandGlobalMutation) -> Result<(), BackendError>;
}

#[derive(Default)]
pub struct ProjectionBoundary {
    fence: IntentFence,
}
impl ProjectionBoundary {
    pub fn apply_global<B: HyprlandRuntimeBackend>(
        &mut self,
        backend: &B,
        intent: GlobalProjectionIntent,
    ) -> ProjectionReceipt {
        let requested = intent.mutation.requested_value();
        let key = ProjectionKey {
            scene_id: intent.source.scene_id,
            slot: intent.mutation.slot(),
            runtime_target: backend.runtime_target().to_owned(),
        };

        match self
            .fence
            .evaluate(&key, intent.source.scene_epoch, requested)
        {
            FenceDecision::Stale => {
                return receipt(backend, intent, None, ApplicationOutcome::Stale, None);
            }
            FenceDecision::Conflict => {
                return receipt(backend, intent, None, ApplicationOutcome::Conflict, None);
            }
            FenceDecision::AlreadyAccepted => {
                return match backend.observe(intent.mutation) {
                    Ok(observed) if observed == requested => observed_receipt(
                        backend,
                        intent,
                        Some(observed),
                        ApplicationOutcome::AlreadySatisfied,
                        Some(observed),
                        ObservationOutcome::Matched,
                    ),
                    Ok(observed) => observed_receipt(
                        backend,
                        intent,
                        Some(observed),
                        ApplicationOutcome::Conflict,
                        Some(observed),
                        ObservationOutcome::Mismatch,
                    ),
                    Err(_) => observed_receipt(
                        backend,
                        intent,
                        None,
                        ApplicationOutcome::TransportError,
                        None,
                        ObservationOutcome::TransportError,
                    ),
                };
            }
            FenceDecision::Fresh => {}
        }

        match backend.version() {
            Ok(version) if version == SUPPORTED_HYPRLAND_VERSION => {}
            Ok(_) => {
                return receipt(backend, intent, None, ApplicationOutcome::Unsupported, None);
            }
            Err(_) => {
                return receipt(
                    backend,
                    intent,
                    None,
                    ApplicationOutcome::TransportError,
                    None,
                );
            }
        }
        let before = match backend.observe(intent.mutation) {
            Ok(value) => value,
            Err(_) => {
                return receipt(
                    backend,
                    intent,
                    None,
                    ApplicationOutcome::TransportError,
                    None,
                );
            }
        };

        if intent
            .expected_before
            .is_some_and(|expected| expected != before)
        {
            return receipt(
                backend,
                intent,
                Some(before),
                ApplicationOutcome::Conflict,
                Some(before),
            );
        }

        if before == requested {
            self.fence.commit(key, intent.source.scene_epoch, requested);
            return observed_receipt(
                backend,
                intent,
                Some(before),
                ApplicationOutcome::AlreadySatisfied,
                Some(before),
                ObservationOutcome::Matched,
            );
        }

        if backend.apply(intent.mutation).is_err() {
            return receipt(
                backend,
                intent,
                Some(before),
                ApplicationOutcome::TransportError,
                None,
            );
        }

        match backend.observe(intent.mutation) {
            Ok(after) if after == requested => {
                self.fence.commit(key, intent.source.scene_epoch, requested);
                observed_receipt(
                    backend,
                    intent,
                    Some(before),
                    ApplicationOutcome::Applied,
                    Some(after),
                    ObservationOutcome::Matched,
                )
            }
            Ok(after) => observed_receipt(
                backend,
                intent,
                Some(before),
                ApplicationOutcome::Applied,
                Some(after),
                ObservationOutcome::Mismatch,
            ),
            Err(_) => observed_receipt(
                backend,
                intent,
                Some(before),
                ApplicationOutcome::Applied,
                None,
                ObservationOutcome::TransportError,
            ),
        }
    }
    pub fn rollback_global<B: HyprlandRuntimeBackend>(
        &self,
        backend: &B,
        receipt: &mut ProjectionReceipt,
    ) {
        match backend.version() {
            Ok(version) if version == SUPPORTED_HYPRLAND_VERSION => {}
            Ok(_) => {
                receipt.rollback_outcome = Some(RollbackOutcome::Unsupported);
                return;
            }
            Err(_) => {
                receipt.rollback_outcome = Some(RollbackOutcome::TransportError);
                return;
            }
        }

        let Some(before) = receipt.observed_before else {
            receipt.rollback_outcome = Some(RollbackOutcome::Conflict);
            return;
        };

        let mutation = mutation_for_slot(receipt.slot, receipt.requested_state);
        let current = match backend.observe(mutation) {
            Ok(value) => value,
            Err(_) => {
                receipt.rollback_outcome = Some(RollbackOutcome::TransportError);
                return;
            }
        };
        if current == before {
            receipt.rollback_outcome = Some(RollbackOutcome::AlreadyRestored);
            receipt.observed_final = Some(current);
            return;
        }
        if current != receipt.requested_state {
            receipt.rollback_outcome = Some(RollbackOutcome::Conflict);
            receipt.observed_final = Some(current);
            return;
        }

        let restore = mutation_for_slot(receipt.slot, before);
        if backend.apply(restore).is_err() {
            receipt.rollback_outcome = Some(RollbackOutcome::TransportError);
            return;
        }

        match backend.observe(restore) {
            Ok(final_value) => {
                receipt.observed_final = Some(final_value);
                receipt.rollback_outcome = Some(if final_value == before {
                    RollbackOutcome::Restored
                } else {
                    RollbackOutcome::ObservationMismatch
                });
            }
            Err(_) => {
                receipt.rollback_outcome = Some(RollbackOutcome::TransportError);
            }
        }
    }
}

fn receipt<B: HyprlandRuntimeBackend>(
    backend: &B,
    intent: GlobalProjectionIntent,
    observed_before: Option<i64>,
    application_outcome: ApplicationOutcome,
    observed_after: Option<i64>,
) -> ProjectionReceipt {
    ProjectionReceipt {
        source: intent.source,
        slot: intent.mutation.slot(),
        runtime_target: backend.runtime_target().to_owned(),
        requested_state: intent.mutation.requested_value(),
        observed_before,
        application_outcome,
        observed_after,
        observation_outcome: ObservationOutcome::NotPerformed,
        rollback_outcome: None,
        observed_final: None,
    }
}

fn observed_receipt<B: HyprlandRuntimeBackend>(
    backend: &B,
    intent: GlobalProjectionIntent,
    observed_before: Option<i64>,
    application_outcome: ApplicationOutcome,
    observed_after: Option<i64>,
    observation_outcome: ObservationOutcome,
) -> ProjectionReceipt {
    let mut receipt = receipt(
        backend,
        intent,
        observed_before,
        application_outcome,
        observed_after,
    );
    receipt.observation_outcome = observation_outcome;
    receipt
}

fn mutation_for_slot(slot: ProjectionSlot, value: i64) -> HyprlandGlobalMutation {
    match slot {
        ProjectionSlot::QualificationBorderSize => {
            HyprlandGlobalMutation::QualificationBorderSize(value)
        }
        ProjectionSlot::SurfaceReservation => {
            unreachable!("surface reservation is not a Hyprland-global mutation")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeOutputIdentity {
    pub session_signature: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedRuntimeOutput {
    pub session_signature: String,
    pub runtime_monitor_id: i64,
    pub name: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub refresh_millihz: u32,
    pub scale_milli: u32,
    pub transform: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputTargetError {
    StaleSession,
    Missing,
    Ambiguous,
}

pub fn validate_runtime_output_target(
    target: &RuntimeOutputIdentity,
    observed: &[ObservedRuntimeOutput],
) -> Result<ObservedRuntimeOutput, OutputTargetError> {
    if observed
        .iter()
        .any(|output| output.session_signature != target.session_signature)
    {
        return Err(OutputTargetError::StaleSession);
    }

    let matches = observed
        .iter()
        .filter(|output| output.name == target.name)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(OutputTargetError::Missing),
        [output] => Ok((*output).clone()),
        _ => Err(OutputTargetError::Ambiguous),
    }
}

pub struct HyprctlLuaBackend {
    instance_signature: String,
}

impl HyprctlLuaBackend {
    pub fn from_env() -> Result<Self, BackendError> {
        let instance_signature =
            env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| BackendError::Transport)?;
        Self::for_instance_signature(&instance_signature)
    }

    pub(crate) fn for_instance_signature(instance_signature: &str) -> Result<Self, BackendError> {
        if instance_signature.trim().is_empty() {
            return Err(BackendError::Transport);
        }
        Ok(Self {
            instance_signature: instance_signature.to_owned(),
        })
    }

    fn run(&self, mode: &str, lua: &str) -> Result<String, BackendError> {
        let output = Command::new("hyprctl")
            .args(["-i", &self.instance_signature, mode, lua])
            .output()
            .map_err(|_| BackendError::Transport)?;
        if !output.status.success() {
            return Err(BackendError::Transport);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    pub fn observe_outputs(&self) -> Result<Vec<ObservedRuntimeOutput>, BackendError> {
        let output = observation_transport::read_observation(
            &self.instance_signature,
            observation_transport::ReadQuery::Outputs,
        )
        .map_err(|_| BackendError::Transport)?;
        let value: serde_json::Value =
            serde_json::from_slice(&output).map_err(|_| BackendError::Parse)?;
        let monitors = value.as_array().ok_or(BackendError::Parse)?;
        monitors
            .iter()
            .map(|monitor| {
                let runtime_monitor_id = monitor
                    .get("id")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or(BackendError::Parse)?;
                let name = monitor
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(BackendError::Parse)?;
                if name.trim().is_empty() {
                    return Err(BackendError::Parse);
                }
                let x = monitor
                    .get("x")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or(BackendError::Parse)?;
                let y = monitor
                    .get("y")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or(BackendError::Parse)?;
                let width = monitor
                    .get("width")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or(BackendError::Parse)?;
                let height = monitor
                    .get("height")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or(BackendError::Parse)?;
                if width <= 0 || height <= 0 {
                    return Err(BackendError::Parse);
                }
                let refresh = monitor
                    .get("refreshRate")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or(BackendError::Parse)?;
                if !refresh.is_finite() || refresh <= 0.0 {
                    return Err(BackendError::Parse);
                }
                let refresh_millihz = (refresh * 1000.0).round();
                if !(1.0..=u32::MAX as f64).contains(&refresh_millihz) {
                    return Err(BackendError::Parse);
                }
                let scale = monitor
                    .get("scale")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or(BackendError::Parse)?;
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(BackendError::Parse);
                }
                let scale_milli = (scale * 1000.0).round();
                if !(1.0..=u32::MAX as f64).contains(&scale_milli) {
                    return Err(BackendError::Parse);
                }
                let transform = monitor
                    .get("transform")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or(BackendError::Parse)?;
                Ok(ObservedRuntimeOutput {
                    session_signature: self.instance_signature.clone(),
                    runtime_monitor_id,
                    name: name.to_owned(),
                    x,
                    y,
                    width,
                    height,
                    refresh_millihz: refresh_millihz as u32,
                    scale_milli: scale_milli as u32,
                    transform,
                })
            })
            .collect()
    }
}

impl HyprlandRuntimeBackend for HyprctlLuaBackend {
    fn runtime_target(&self) -> &str {
        &self.instance_signature
    }

    fn version(&self) -> Result<String, BackendError> {
        self.run("repl", "return hl.version()")
    }

    fn observe(&self, mutation: HyprlandGlobalMutation) -> Result<i64, BackendError> {
        let key = match mutation {
            HyprlandGlobalMutation::QualificationBorderSize(_) => "general.border_size",
        };
        self.run(
            "repl",
            &format!("local v=hl.get_config(\"{key}\"); return tostring(v)"),
        )?
        .parse::<i64>()
        .map_err(|_| BackendError::Parse)
    }

    fn apply(&self, mutation: HyprlandGlobalMutation) -> Result<(), BackendError> {
        let lua = match mutation {
            HyprlandGlobalMutation::QualificationBorderSize(value) => {
                format!("hl.config({{general={{border_size={value}}}}})")
            }
        };
        self.run("eval", &lua)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct FakeBackend {
        target: &'static str,
        version: &'static str,
        value: Cell<i64>,
    }

    impl HyprlandRuntimeBackend for FakeBackend {
        fn runtime_target(&self) -> &str {
            self.target
        }
        fn version(&self) -> Result<String, BackendError> {
            Ok(self.version.to_owned())
        }
        fn observe(&self, _: HyprlandGlobalMutation) -> Result<i64, BackendError> {
            Ok(self.value.get())
        }
        fn apply(&self, mutation: HyprlandGlobalMutation) -> Result<(), BackendError> {
            self.value.set(mutation.requested_value());
            Ok(())
        }
    }

    fn source(epoch: u64) -> CompositorIntentSource {
        CompositorIntentSource {
            scene_id: MonitorSceneId::new(1),
            scene_epoch: SceneEpoch::new(epoch),
        }
    }

    fn global(epoch: u64, value: i64, expected_before: Option<i64>) -> GlobalProjectionIntent {
        GlobalProjectionIntent {
            source: source(epoch),
            mutation: HyprlandGlobalMutation::QualificationBorderSize(value),
            expected_before,
        }
    }

    fn backend(value: i64) -> FakeBackend {
        FakeBackend {
            target: "session-a",
            version: SUPPORTED_HYPRLAND_VERSION,
            value: Cell::new(value),
        }
    }
    #[test]
    fn later_epoch_fences_earlier_epoch() {
        let backend = backend(4);
        let mut boundary = ProjectionBoundary::default();

        assert_eq!(
            boundary
                .apply_global(&backend, global(11, 5, Some(4)))
                .application_outcome,
            ApplicationOutcome::Applied
        );
        assert_eq!(
            boundary
                .apply_global(&backend, global(10, 6, Some(5)))
                .application_outcome,
            ApplicationOutcome::Stale
        );
        assert_eq!(backend.value.get(), 5);
    }

    #[test]
    fn equal_epoch_same_request_is_idempotent() {
        let backend = backend(4);
        let mut boundary = ProjectionBoundary::default();
        boundary.apply_global(&backend, global(11, 5, Some(4)));

        assert_eq!(
            boundary
                .apply_global(&backend, global(11, 5, Some(4)))
                .application_outcome,
            ApplicationOutcome::AlreadySatisfied
        );
        assert_eq!(backend.value.get(), 5);
    }

    #[test]
    fn conditional_rollback_refuses_diverged_state() {
        let backend = backend(4);
        let mut boundary = ProjectionBoundary::default();
        let mut receipt = boundary.apply_global(&backend, global(1, 5, Some(4)));
        backend.value.set(7);
        boundary.rollback_global(&backend, &mut receipt);

        assert_eq!(receipt.rollback_outcome, Some(RollbackOutcome::Conflict));
        assert_eq!(backend.value.get(), 7);
    }
    #[test]
    fn rollback_restores_captured_state() {
        let backend = backend(4);
        let mut boundary = ProjectionBoundary::default();
        let mut receipt = boundary.apply_global(&backend, global(1, 5, Some(4)));
        boundary.rollback_global(&backend, &mut receipt);

        assert_eq!(receipt.rollback_outcome, Some(RollbackOutcome::Restored));
        assert_eq!(receipt.observed_final, Some(4));
    }

    #[test]
    fn unsupported_version_never_mutates() {
        let backend = FakeBackend {
            target: "session-a",
            version: "0.57.0",
            value: Cell::new(4),
        };
        let mut boundary = ProjectionBoundary::default();
        let receipt = boundary.apply_global(&backend, global(1, 5, Some(4)));

        assert_eq!(receipt.application_outcome, ApplicationOutcome::Unsupported);
        assert_eq!(backend.value.get(), 4);
    }

    #[test]
    fn runtime_output_validation_rejects_stale_missing_and_ambiguous_identity() {
        let target = RuntimeOutputIdentity {
            session_signature: "session-a".into(),
            name: "DP-1".into(),
        };
        let one = vec![ObservedRuntimeOutput {
            session_signature: "session-a".into(),
            runtime_monitor_id: 1,
            name: "DP-1".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            refresh_millihz: 60_000,
            scale_milli: 1000,
            transform: 0,
        }];
        assert_eq!(
            validate_runtime_output_target(&target, &one),
            Ok(one[0].clone())
        );

        let stale = vec![ObservedRuntimeOutput {
            session_signature: "session-b".into(),
            runtime_monitor_id: 1,
            name: "DP-1".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            refresh_millihz: 60_000,
            scale_milli: 1000,
            transform: 0,
        }];
        assert_eq!(
            validate_runtime_output_target(&target, &stale),
            Err(OutputTargetError::StaleSession)
        );
        let missing = vec![ObservedRuntimeOutput {
            session_signature: "session-a".into(),
            runtime_monitor_id: 0,
            name: "HDMI-A-1".into(),
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            refresh_millihz: 60_000,
            scale_milli: 1000,
            transform: 0,
        }];
        assert_eq!(
            validate_runtime_output_target(&target, &missing),
            Err(OutputTargetError::Missing)
        );

        let ambiguous = vec![one[0].clone(), one[0].clone()];
        assert_eq!(
            validate_runtime_output_target(&target, &ambiguous),
            Err(OutputTargetError::Ambiguous)
        );
    }

    #[test]
    fn surface_plan_rejects_auto_and_maps_reserve() {
        use gnos_ux_surface::compositor::ResolvedCompositorIntent;
        use gnos_ux_surface::envelope::KeyboardIntent;

        let intent = ResolvedCompositorIntent {
            source: source(1),
            edge: SurfaceEdge::Top,
            keyboard: KeyboardIntent::None,
            reservation: SpaceReservationIntent::Reserve(LogicalPx(44)),
            margins: [LogicalPx(0); 4],
            output: Some(RuntimeId::new("DP-2")),
        };
        let plan = surface_reservation_plan(&intent).expect("plan");
        assert_eq!(plan.exclusive_zone, 44);
        assert_eq!(plan.output, Some(RuntimeId::new("DP-2")));

        let mut auto = intent;
        auto.reservation = SpaceReservationIntent::Auto;
        assert_eq!(
            surface_reservation_plan(&auto),
            Err(SurfaceProjectionError::AutoReservationUnsupported)
        );
    }
}
