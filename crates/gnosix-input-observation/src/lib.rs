//! Versioned, compositor-neutral arbitration observations.
//!
//! This crate defines a wire contract and a fail-closed adapter to the
//! qualified pure Super arbiter. It performs no input capture or I/O.

use gnosix_input_core::arbiter::{
    CancelReason, DeviceId, ModifierTap, StateEvent, SuperArbiter, SuperSide,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod producer_stream;
mod resync_snapshot;
pub use producer_stream::{ProducerProjection, ProducerStreamAdapter};

pub use resync_snapshot::{
    KeyboardInputSnapshot, ModifierClass, PointerInputSnapshot, RESYNC_SNAPSHOT_SCHEMA_V1_1,
    ResyncReadiness, ResyncSnapshotV1_1,
};

pub const ARBITRATION_SCHEMA_V1: &str = "gnosix.compositor.arbitration.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompositorInstanceId(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventSequence(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GestureSequenceId(pub u64);

/// Unique only within one compositor instance; no identity across restart is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionDeviceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuperKeySide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputStateAtBoundary {
    AllReleased,
    ActiveOrUnknown,
}

/// A subscriber may only arm new taps after observing an all-released boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionBoundaryV1 {
    pub schema: String,
    pub compositor_instance: CompositorInstanceId,
    pub first_event_sequence: EventSequence,
    pub state_generation: u64,
    pub input_state: InputStateAtBoundary,
}

impl SubscriptionBoundaryV1 {
    pub fn all_released(instance: u128, first_event: u64, generation: u64) -> Self {
        Self {
            schema: ARBITRATION_SCHEMA_V1.into(),
            compositor_instance: CompositorInstanceId(instance),
            first_event_sequence: EventSequence(first_event),
            state_generation: generation,
            input_state: InputStateAtBoundary::AllReleased,
        }
    }

    pub fn while_inputs_held(instance: u128, first_event: u64, generation: u64) -> Self {
        Self {
            input_state: InputStateAtBoundary::ActiveOrUnknown,
            ..Self::all_released(instance, first_event, generation)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputDeviceKind {
    Keyboard,
    Pointer,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Propagation {
    Consumed,
    PassedThrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchDisposition {
    NotInvoked,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "identity", content = "value", rename_all = "snake_case")]
pub enum BindingRef {
    /// Stable only within one compositor instance and configuration contract.
    Stable(u64),
    /// Valid only for one configuration generation.
    Ephemeral(u64),
    Unidentified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerOperation {
    MoveWindow,
    ResizeWindow,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerGesturePhase {
    Started,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    ConfigReload,
    SessionInactive,
    KeyboardRemoved,
    PointerRemoved,
    CompositorShutdown,
    InputStateReset,
    SequenceSuperseded,
    LostRelease,
    EventGap,
    UnknownFuture(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ArbitrationEventV1 {
    ModifierPressed {
        side: SuperKeySide,
        repeat: bool,
    },
    ModifierReleased {
        side: SuperKeySide,
    },
    KeyParticipation {
        code: u32,
        pressed: bool,
        repeat: bool,
    },
    PointerParticipation {
        button: u32,
        pressed: bool,
    },
    BindingClaimed {
        key_code: u32,
        binding: BindingRef,
        propagation: Propagation,
        dispatch: DispatchDisposition,
    },
    PointerGestureClaimed {
        phase: PointerGesturePhase,
        operation: PointerOperation,
    },
    GestureEnded,
    SequenceCancelled {
        reason: CancellationReason,
    },
    StateInvalidated {
        reason: CancellationReason,
    },
    DeviceRemoved {
        kind: InputDeviceKind,
    },
    /// The producer has authoritatively observed all relevant inputs released.
    AllInputsReleased {
        state_generation: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArbitrationRecordV1 {
    pub schema: String,
    pub compositor_instance: CompositorInstanceId,
    pub event_sequence: EventSequence,
    pub gesture_sequence: Option<GestureSequenceId>,
    pub device: Option<SessionDeviceId>,
    pub event: ArbitrationEventV1,
}

impl ArbitrationRecordV1 {
    pub fn new(
        instance: u128,
        event_sequence: u64,
        gesture_sequence: Option<u64>,
        device: Option<u64>,
        event: ArbitrationEventV1,
    ) -> Self {
        Self {
            schema: ARBITRATION_SCHEMA_V1.into(),
            compositor_instance: CompositorInstanceId(instance),
            event_sequence: EventSequence(event_sequence),
            gesture_sequence: gesture_sequence.map(GestureSequenceId),
            device: device.map(SessionDeviceId),
            event,
        }
    }

    pub fn super_pressed(instance: u128, seq: u64, gesture: u64, device: u64) -> Self {
        Self::new(
            instance,
            seq,
            Some(gesture),
            Some(device),
            ArbitrationEventV1::ModifierPressed {
                side: SuperKeySide::Left,
                repeat: false,
            },
        )
    }

    pub fn super_released(instance: u128, seq: u64, gesture: u64, device: u64) -> Self {
        Self::new(
            instance,
            seq,
            Some(gesture),
            Some(device),
            ArbitrationEventV1::ModifierReleased {
                side: SuperKeySide::Left,
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservationError {
    UnsupportedSchema,
    SequenceGap {
        expected: EventSequence,
        received: EventSequence,
    },
    WrongCompositorInstance,
    MissingGestureSequence,
    UnexpectedGestureSequence,
    GestureSequenceReused(GestureSequenceId),
    GestureSequenceNotIncreasing(GestureSequenceId),
    UnknownGestureSequence(GestureSequenceId),
    InvalidStateGeneration,
    Poisoned,
    SequenceExhausted,
}

/// Validates a single ordered compositor stream before passing facts to the arbiter.
#[derive(Debug)]
pub struct ArbitrationStreamAdapter {
    instance: CompositorInstanceId,
    expected_event: EventSequence,
    state_generation: u64,
    requires_new_generation: bool,
    synchronized: bool,
    poisoned: bool,
    active_gestures: BTreeSet<GestureSequenceId>,
    pointer_owners: BTreeMap<GestureSequenceId, usize>,
    used_gestures: BTreeSet<GestureSequenceId>,
    last_gesture: Option<GestureSequenceId>,
    arbiter: SuperArbiter,
}

impl ArbitrationStreamAdapter {
    pub fn synchronize(boundary: SubscriptionBoundaryV1) -> Result<Self, ObservationError> {
        if boundary.schema != ARBITRATION_SCHEMA_V1 {
            return Err(ObservationError::UnsupportedSchema);
        }
        Ok(Self {
            instance: boundary.compositor_instance,
            expected_event: boundary.first_event_sequence,
            state_generation: boundary.state_generation,
            requires_new_generation: false,
            synchronized: boundary.input_state == InputStateAtBoundary::AllReleased,
            poisoned: false,
            active_gestures: BTreeSet::new(),
            pointer_owners: BTreeMap::new(),
            used_gestures: BTreeSet::new(),
            last_gesture: None,
            arbiter: SuperArbiter::new(),
        })
    }

    /// Applies an ordered compositor-logical snapshot. Unknown/active state
    /// cancels pending gestures; only complete neutrality can re-arm them.
    /// No physical hardware release or producer implementation is implied.
    pub fn push_resync_snapshot(
        &mut self,
        snapshot: ResyncSnapshotV1_1,
    ) -> Result<(), ObservationError> {
        if self.poisoned {
            return Err(ObservationError::Poisoned);
        }
        let result = (|| {
            if snapshot.schema != RESYNC_SNAPSHOT_SCHEMA_V1_1 {
                return Err(ObservationError::UnsupportedSchema);
            }
            if snapshot.compositor_instance != self.instance {
                return Err(ObservationError::WrongCompositorInstance);
            }
            if snapshot.event_sequence != self.expected_event {
                return Err(ObservationError::SequenceGap {
                    expected: self.expected_event,
                    received: snapshot.event_sequence,
                });
            }
            if snapshot.state_generation < self.state_generation
                || (self.requires_new_generation
                    && snapshot.state_generation == self.state_generation)
            {
                return Err(ObservationError::InvalidStateGeneration);
            }
            self.expected_event.0 = self
                .expected_event
                .0
                .checked_add(1)
                .ok_or(ObservationError::SequenceExhausted)?;
            self.state_generation = snapshot.state_generation;
            self.requires_new_generation = false;
            self.cancel(CancelReason::BackendResync);
            self.active_gestures.clear();
            self.pointer_owners.clear();
            self.synchronized = snapshot.readiness() == ResyncReadiness::AllReleased;
            if self.synchronized {
                self.arbiter.advance(StateEvent::AllReleased);
            }
            Ok(())
        })();
        if result.is_err() {
            self.invalidate();
        }
        result
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn push(
        &mut self,
        record: ArbitrationRecordV1,
    ) -> Result<Option<ModifierTap>, ObservationError> {
        if self.poisoned {
            return Err(ObservationError::Poisoned);
        }
        let check = self.validate_envelope(&record);
        if let Err(error) = check {
            self.invalidate();
            return Err(error);
        }
        self.expected_event.0 = match self.expected_event.0.checked_add(1) {
            Some(next) => next,
            None => {
                self.invalidate();
                return Err(ObservationError::SequenceExhausted);
            }
        };

        if !self.synchronized
            && !matches!(record.event, ArbitrationEventV1::AllInputsReleased { .. })
        {
            // Keep transport ordering, but ignore every input fact until the
            // producer establishes a fresh all-released synchronization point.
            return Ok(None);
        }

        let result = (|| {
            match record.event {
                ArbitrationEventV1::ModifierPressed { side, repeat } => {
                    let gesture = self.begin_or_validate_gesture(record.gesture_sequence)?;
                    let _ = gesture;
                    let result = self.arbiter.advance(StateEvent::SuperDown {
                        device: device_id(record.device),
                        side: super_side(side),
                        repeat,
                    });
                    if !self.pointer_owners.is_empty() {
                        self.arbiter.advance(StateEvent::PointerDrag);
                    }
                    Ok(result)
                }
                ArbitrationEventV1::ModifierReleased { side } => {
                    self.require_active_gesture(record.gesture_sequence)?;
                    Ok(self.arbiter.advance(StateEvent::SuperUp {
                        device: device_id(record.device),
                        side: super_side(side),
                    }))
                }
                ArbitrationEventV1::KeyParticipation {
                    code,
                    pressed,
                    repeat,
                } => {
                    if pressed {
                        self.begin_or_validate_gesture(record.gesture_sequence)?;
                        Ok(self.arbiter.advance(StateEvent::KeyDown {
                            device: device_id(record.device),
                            code,
                            repeat,
                        }))
                    } else {
                        self.require_active_gesture(record.gesture_sequence)?;
                        Ok(self.arbiter.advance(StateEvent::KeyUp {
                            device: device_id(record.device),
                            code,
                        }))
                    }
                }
                ArbitrationEventV1::PointerParticipation { button, pressed } => {
                    if pressed {
                        self.begin_or_validate_gesture(record.gesture_sequence)?;
                    } else {
                        self.require_active_gesture(record.gesture_sequence)?;
                    }
                    Ok(self.arbiter.advance(if pressed {
                        StateEvent::PointerDown {
                            device: device_id(record.device),
                            button,
                        }
                    } else {
                        StateEvent::PointerUp {
                            device: device_id(record.device),
                            button,
                        }
                    }))
                }
                ArbitrationEventV1::BindingClaimed { .. } => {
                    self.require_active_gesture(record.gesture_sequence)?;
                    // A binding may claim Super itself at release. Preserve
                    // consumption without inventing a second key-down.
                    self.arbiter.binding_claimed();
                    Ok(None)
                }
                ArbitrationEventV1::PointerGestureClaimed { phase, .. } => match phase {
                    PointerGesturePhase::Started => {
                        let gesture = self.begin_or_validate_gesture(record.gesture_sequence)?;
                        *self.pointer_owners.entry(gesture).or_default() += 1;
                        Ok(self.arbiter.advance(StateEvent::PointerDrag))
                    }
                    PointerGesturePhase::Ended => {
                        let gesture = self.require_active_gesture(record.gesture_sequence)?;
                        self.end_pointer_owner(gesture)?;
                        Ok(None)
                    }
                    PointerGesturePhase::Cancelled => {
                        let gesture = self.require_active_gesture(record.gesture_sequence)?;
                        self.end_pointer_owner(gesture)?;
                        self.cancel(CancelReason::LostRelease);
                        self.synchronized = false;
                        self.active_gestures.clear();
                        self.pointer_owners.clear();
                        Ok(None)
                    }
                },
                ArbitrationEventV1::GestureEnded => {
                    let gesture = self.require_active_gesture(record.gesture_sequence)?;
                    self.active_gestures.remove(&gesture);
                    Ok(None)
                }
                ArbitrationEventV1::SequenceCancelled { reason } => {
                    self.require_active_gesture(record.gesture_sequence)?;
                    self.cancel(cancel_reason(reason));
                    self.synchronized = false;
                    self.requires_new_generation = true;
                    self.active_gestures.clear();
                    self.pointer_owners.clear();
                    Ok(None)
                }
                ArbitrationEventV1::StateInvalidated { reason } => {
                    if record.gesture_sequence.is_some() {
                        self.invalidate();
                        return Err(ObservationError::UnexpectedGestureSequence);
                    }
                    self.cancel(cancel_reason(reason));
                    self.synchronized = false;
                    self.requires_new_generation = true;
                    self.active_gestures.clear();
                    self.pointer_owners.clear();
                    Ok(None)
                }
                ArbitrationEventV1::DeviceRemoved { .. } => {
                    self.cancel(CancelReason::LostRelease);
                    self.synchronized = false;
                    self.requires_new_generation = true;
                    self.active_gestures.clear();
                    self.pointer_owners.clear();
                    Ok(None)
                }
                ArbitrationEventV1::AllInputsReleased { state_generation } => {
                    if record.gesture_sequence.is_some()
                        || state_generation <= self.state_generation
                    {
                        self.invalidate();
                        return Err(ObservationError::InvalidStateGeneration);
                    }
                    self.state_generation = state_generation;
                    self.requires_new_generation = false;
                    self.active_gestures.clear();
                    self.pointer_owners.clear();
                    self.arbiter.advance(StateEvent::AllReleased);
                    self.synchronized = true;
                    Ok(None)
                }
            }
        })();
        if result.is_err() {
            self.invalidate();
        }
        result
    }

    fn validate_envelope(&mut self, record: &ArbitrationRecordV1) -> Result<(), ObservationError> {
        if record.schema != ARBITRATION_SCHEMA_V1 {
            return Err(ObservationError::UnsupportedSchema);
        }
        if record.compositor_instance != self.instance {
            return Err(ObservationError::WrongCompositorInstance);
        }
        if record.event_sequence != self.expected_event {
            return Err(ObservationError::SequenceGap {
                expected: self.expected_event,
                received: record.event_sequence,
            });
        }
        Ok(())
    }

    fn begin_or_validate_gesture(
        &mut self,
        gesture: Option<GestureSequenceId>,
    ) -> Result<GestureSequenceId, ObservationError> {
        let gesture = gesture.ok_or(ObservationError::MissingGestureSequence)?;
        if self.active_gestures.contains(&gesture) {
            return Ok(gesture);
        }
        if self.used_gestures.contains(&gesture) {
            return Err(ObservationError::GestureSequenceReused(gesture));
        }
        if gesture.0 == 0 || self.last_gesture.is_some_and(|last| gesture <= last) {
            return Err(ObservationError::GestureSequenceNotIncreasing(gesture));
        }
        self.used_gestures.insert(gesture);
        self.last_gesture = Some(gesture);
        self.active_gestures.insert(gesture);
        Ok(gesture)
    }

    fn end_pointer_owner(&mut self, gesture: GestureSequenceId) -> Result<(), ObservationError> {
        let owners = self
            .pointer_owners
            .get_mut(&gesture)
            .ok_or(ObservationError::UnknownGestureSequence(gesture))?;
        *owners -= 1;
        if *owners == 0 {
            self.pointer_owners.remove(&gesture);
        }
        Ok(())
    }

    fn require_active_gesture(
        &self,
        gesture: Option<GestureSequenceId>,
    ) -> Result<GestureSequenceId, ObservationError> {
        let gesture = gesture.ok_or(ObservationError::MissingGestureSequence)?;
        if self.active_gestures.contains(&gesture) {
            Ok(gesture)
        } else {
            Err(ObservationError::UnknownGestureSequence(gesture))
        }
    }

    fn cancel(&mut self, reason: CancelReason) {
        self.arbiter.advance(StateEvent::Cancel(reason));
    }

    fn invalidate(&mut self) {
        self.cancel(CancelReason::BackendResync);
        self.poisoned = true;
        self.synchronized = false;
        self.active_gestures.clear();
        self.pointer_owners.clear();
    }
}

fn device_id(device: Option<SessionDeviceId>) -> DeviceId {
    // Zero is the conservative unknown-device bucket: it still participates in
    // cross-device key/pointer suppression, but is never a persistent identity.
    DeviceId(device.map_or(0, |value| value.0))
}

fn super_side(side: SuperKeySide) -> SuperSide {
    match side {
        SuperKeySide::Left => SuperSide::Left,
        SuperKeySide::Right => SuperSide::Right,
    }
}

fn cancel_reason(reason: CancellationReason) -> CancelReason {
    match reason {
        CancellationReason::ConfigReload => CancelReason::Reload,
        CancellationReason::SessionInactive | CancellationReason::CompositorShutdown => {
            CancelReason::SessionLost
        }
        CancellationReason::LostRelease
        | CancellationReason::KeyboardRemoved
        | CancellationReason::PointerRemoved => CancelReason::LostRelease,
        CancellationReason::InputStateReset
        | CancellationReason::SequenceSuperseded
        | CancellationReason::EventGap
        | CancellationReason::UnknownFuture(_) => CancelReason::BackendResync,
    }
}
