//! Candidate compositor-owned resynchronization snapshot for arbitration v1.1.
//!
//! This is a neutral wire shape only. No producer, transport, or live backend
//! exists yet. Arbitration v1 remains unchanged and keeps its compact
//! `AllInputsReleased` consumer event.

use super::{CompositorInstanceId, EventSequence, GestureSequenceId, SessionDeviceId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const RESYNC_SNAPSHOT_SCHEMA_V1_1: &str = "gnosix.compositor.input-state-snapshot.v1_1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierClass {
    Shift,
    Control,
    Alt,
    Super,
    Meta,
    Hyper,
    LevelThree,
    Other(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyboardInputSnapshot {
    pub device: SessionDeviceId,
    /// False means the producer cannot currently vouch for this device state.
    pub state_known: bool,
    pub pressed_keycodes: Vec<u32>,
    pub active_modifiers: Vec<ModifierClass>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerInputSnapshot {
    pub device: SessionDeviceId,
    /// False means the producer cannot currently vouch for this device state.
    pub state_known: bool,
    pub pressed_button_codes: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReadiness {
    /// No compositor-observed logical input is active in this generation.
    /// This does not assert physical hardware release across backend suspend.
    AllReleased,
    InputsActive,
    StateUnknown,
}

/// An ordered, compositor-scoped candidate snapshot for a future v1.1 stream.
///
/// `inventory_complete` covers all participating keyboard and pointer devices;
/// an incomplete inventory is never interpreted as an empty one. Device IDs
/// are scoped to `compositor_instance`. Active gesture IDs correlate with
/// earlier arbitration records and are not persistent identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResyncSnapshotV1_1 {
    pub schema: String,
    pub compositor_instance: CompositorInstanceId,
    pub event_sequence: EventSequence,
    /// Monotonic for one compositor instance. Loss/reset invalidates the old
    /// generation; snapshots from earlier generations cannot re-arm a consumer.
    pub state_generation: u64,
    pub inventory_complete: bool,
    pub keyboards: Vec<KeyboardInputSnapshot>,
    pub pointers: Vec<PointerInputSnapshot>,
    pub active_pointer_gestures: Vec<GestureSequenceId>,
}

impl ResyncSnapshotV1_1 {
    pub fn readiness(&self) -> ResyncReadiness {
        if self.schema != RESYNC_SNAPSHOT_SCHEMA_V1_1 || !self.inventory_complete {
            return ResyncReadiness::StateUnknown;
        }

        let keyboard_ids: BTreeSet<_> = self.keyboards.iter().map(|state| state.device).collect();
        let pointer_ids: BTreeSet<_> = self.pointers.iter().map(|state| state.device).collect();
        if keyboard_ids.len() != self.keyboards.len()
            || pointer_ids.len() != self.pointers.len()
            || self.keyboards.iter().any(|state| !state.state_known)
            || self.pointers.iter().any(|state| !state.state_known)
        {
            return ResyncReadiness::StateUnknown;
        }

        let keyboard_active = self
            .keyboards
            .iter()
            .any(|state| !state.pressed_keycodes.is_empty() || !state.active_modifiers.is_empty());
        let pointer_active = self
            .pointers
            .iter()
            .any(|state| !state.pressed_button_codes.is_empty());
        if keyboard_active || pointer_active || !self.active_pointer_gestures.is_empty() {
            ResyncReadiness::InputsActive
        } else {
            ResyncReadiness::AllReleased
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ARBITRATION_SCHEMA_V1;

    fn released_snapshot() -> ResyncSnapshotV1_1 {
        ResyncSnapshotV1_1 {
            schema: RESYNC_SNAPSHOT_SCHEMA_V1_1.into(),
            compositor_instance: CompositorInstanceId(17),
            event_sequence: EventSequence(91),
            state_generation: 4,
            inventory_complete: true,
            keyboards: vec![KeyboardInputSnapshot {
                device: SessionDeviceId(1),
                state_known: true,
                pressed_keycodes: vec![],
                active_modifiers: vec![],
            }],
            pointers: vec![PointerInputSnapshot {
                device: SessionDeviceId(2),
                state_known: true,
                pressed_button_codes: vec![],
            }],
            active_pointer_gestures: vec![],
        }
    }

    #[test]
    fn complete_known_empty_snapshot_is_the_only_all_released_state() {
        let snapshot = released_snapshot();
        assert_eq!(snapshot.readiness(), ResyncReadiness::AllReleased);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<ResyncSnapshotV1_1>(&json).unwrap(),
            snapshot
        );
    }

    #[test]
    fn pressed_key_or_modifier_is_active() {
        let mut snapshot = released_snapshot();
        snapshot.keyboards[0].pressed_keycodes.push(125);
        assert_eq!(snapshot.readiness(), ResyncReadiness::InputsActive);

        snapshot.keyboards[0].pressed_keycodes.clear();
        snapshot.keyboards[0]
            .active_modifiers
            .push(ModifierClass::Super);
        assert_eq!(snapshot.readiness(), ResyncReadiness::InputsActive);
    }

    #[test]
    fn pressed_pointer_button_or_active_gesture_is_active() {
        let mut snapshot = released_snapshot();
        snapshot.pointers[0].pressed_button_codes.push(0x110);
        assert_eq!(snapshot.readiness(), ResyncReadiness::InputsActive);

        snapshot.pointers[0].pressed_button_codes.clear();
        snapshot.active_pointer_gestures.push(GestureSequenceId(8));
        assert_eq!(snapshot.readiness(), ResyncReadiness::InputsActive);
    }

    #[test]
    fn incomplete_unknown_duplicate_or_wrong_schema_snapshot_fails_closed() {
        let mut snapshot = released_snapshot();
        snapshot.inventory_complete = false;
        assert_eq!(snapshot.readiness(), ResyncReadiness::StateUnknown);

        snapshot = released_snapshot();
        snapshot.keyboards[0].state_known = false;
        assert_eq!(snapshot.readiness(), ResyncReadiness::StateUnknown);

        snapshot = released_snapshot();
        snapshot.keyboards.push(snapshot.keyboards[0].clone());
        assert_eq!(snapshot.readiness(), ResyncReadiness::StateUnknown);

        snapshot = released_snapshot();
        snapshot.schema = ARBITRATION_SCHEMA_V1.into();
        assert_eq!(snapshot.readiness(), ResyncReadiness::StateUnknown);
    }
}
