//! Synthetic state events, not desktop event injection.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SuperSide {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelReason {
    SessionLost,
    Reload,
    ProviderLost,
    LostRelease,
    BackendResync,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GestureState {
    SuperPending,
    ChordConsumed,
    PointerChordConsumed,
    HoldConsumed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateEvent {
    SuperDown {
        device: DeviceId,
        side: SuperSide,
        repeat: bool,
    },
    SuperUp {
        device: DeviceId,
        side: SuperSide,
    },
    KeyDown {
        device: DeviceId,
        code: u32,
        repeat: bool,
    },
    KeyUp {
        device: DeviceId,
        code: u32,
    },
    PointerDown {
        device: DeviceId,
        button: u32,
    },
    PointerUp {
        device: DeviceId,
        button: u32,
    },
    PointerDrag,
    HoldCommitted,
    Cancel(CancelReason),
    /// Only a backend-confirmed all-keys-up resynchronization may rearm.
    AllReleased,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierTap {
    pub device: DeviceId,
    pub side: SuperSide,
}

#[derive(Default, Debug)]
pub struct SuperArbiter {
    pending: BTreeMap<(DeviceId, SuperSide), GestureState>,
    keys: BTreeSet<(DeviceId, u32)>,
    buttons: BTreeSet<(DeviceId, u32)>,
    cancelled: bool,
}
impl SuperArbiter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn states(&self) -> Vec<GestureState> {
        self.pending.values().copied().collect()
    }
    /// A compositor binding claim consumes pending modifier taps without
    /// inventing another key press (including a binding on Super itself).
    pub fn binding_claimed(&mut self) {
        self.consume(GestureState::ChordConsumed);
    }
    fn consume(&mut self, state: GestureState) {
        for pending in self.pending.values_mut() {
            if *pending == GestureState::SuperPending {
                *pending = state;
            }
        }
    }
    pub fn advance(&mut self, event: StateEvent) -> Option<ModifierTap> {
        match event {
            StateEvent::SuperDown {
                device,
                side,
                repeat,
            } => {
                if repeat || self.pending.contains_key(&(device, side)) {
                    return None;
                }
                let state = if self.cancelled {
                    GestureState::Cancelled
                } else if !self.pending.is_empty() || !self.keys.is_empty() {
                    GestureState::ChordConsumed
                } else if !self.buttons.is_empty() {
                    GestureState::PointerChordConsumed
                } else {
                    GestureState::SuperPending
                };
                if !self.pending.is_empty() {
                    self.consume(GestureState::ChordConsumed);
                }
                self.pending.insert((device, side), state);
            }
            StateEvent::SuperUp { device, side } => {
                if self.pending.remove(&(device, side)) == Some(GestureState::SuperPending)
                    && !self.cancelled
                {
                    return Some(ModifierTap { device, side });
                }
            }
            StateEvent::KeyDown { device, code, .. } => {
                self.keys.insert((device, code));
                self.consume(GestureState::ChordConsumed);
            }
            StateEvent::KeyUp { device, code } => {
                self.keys.remove(&(device, code));
            }
            StateEvent::PointerDown { device, button } => {
                self.buttons.insert((device, button));
                self.consume(GestureState::PointerChordConsumed);
            }
            StateEvent::PointerUp { device, button } => {
                self.buttons.remove(&(device, button));
            }
            StateEvent::PointerDrag => self.consume(GestureState::PointerChordConsumed),
            StateEvent::HoldCommitted => self.consume(GestureState::HoldConsumed),
            StateEvent::Cancel(_) => {
                self.cancelled = true;
                self.consume(GestureState::Cancelled);
            }
            StateEvent::AllReleased => {
                self.pending.clear();
                self.keys.clear();
                self.buttons.clear();
                self.cancelled = false;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const D: DeviceId = DeviceId(1);
    fn down() -> StateEvent {
        StateEvent::SuperDown {
            device: D,
            side: SuperSide::Left,
            repeat: false,
        }
    }
    fn up() -> StateEvent {
        StateEvent::SuperUp {
            device: D,
            side: SuperSide::Left,
        }
    }
    fn taps(events: Vec<StateEvent>) -> Vec<ModifierTap> {
        let mut a = SuperArbiter::new();
        events.into_iter().filter_map(|e| a.advance(e)).collect()
    }
    #[test]
    fn isolated_release_emits_once_and_duplicate_up_is_inert() {
        assert_eq!(
            taps(vec![down(), up(), up()]),
            vec![ModifierTap {
                device: D,
                side: SuperSide::Left
            }]
        );
    }
    #[test]
    fn rapid_taps_remain_individual_without_repeat_activation() {
        assert_eq!(taps(vec![down(), down(), up(), down(), up()]).len(), 2);
    }
    #[test]
    fn key_chord_and_screenshot_chord_suppress_tap() {
        for codes in [vec![25], vec![42, 31]] {
            let mut events = vec![down()];
            for code in &codes {
                events.push(StateEvent::KeyDown {
                    device: D,
                    code: *code,
                    repeat: false,
                });
            }
            for code in &codes {
                events.push(StateEvent::KeyUp {
                    device: D,
                    code: *code,
                });
            }
            events.push(up());
            assert!(taps(events).is_empty());
        }
    }
    #[test]
    fn pointer_button_and_drag_never_emit_tap() {
        for use_event in [
            StateEvent::PointerDown {
                device: D,
                button: 272,
            },
            StateEvent::PointerDrag,
        ] {
            assert!(taps(vec![down(), use_event, up()]).is_empty());
        }
    }
    #[test]
    fn unrelated_modifier_already_held_suppresses_tap() {
        assert!(
            taps(vec![
                StateEvent::KeyDown {
                    device: D,
                    code: 42,
                    repeat: false
                },
                down(),
                up()
            ])
            .is_empty()
        );
    }
    #[test]
    fn repeat_only_super_down_cannot_create_a_gesture() {
        assert!(
            taps(vec![
                StateEvent::SuperDown {
                    device: D,
                    side: SuperSide::Left,
                    repeat: true
                },
                up()
            ])
            .is_empty()
        );
    }
    #[test]
    fn repeated_keydown_and_duplicate_keyup_after_consumption_do_not_repeat_or_restore_tap() {
        let mut arbiter = SuperArbiter::new();
        assert_eq!(arbiter.advance(down()), None);
        assert_eq!(
            arbiter.advance(StateEvent::KeyDown {
                device: D,
                code: 25,
                repeat: false
            }),
            None
        );
        assert_eq!(
            arbiter.advance(StateEvent::KeyDown {
                device: D,
                code: 25,
                repeat: true
            }),
            None
        );
        assert_eq!(
            arbiter.advance(StateEvent::KeyUp {
                device: D,
                code: 25
            }),
            None
        );
        assert_eq!(
            arbiter.advance(StateEvent::KeyUp {
                device: D,
                code: 25
            }),
            None
        );
        assert_eq!(arbiter.advance(up()), None);
    }
    #[test]
    fn cancellation_requires_observed_all_released_before_rearming() {
        for reason in [
            CancelReason::SessionLost,
            CancelReason::Reload,
            CancelReason::ProviderLost,
            CancelReason::LostRelease,
            CancelReason::BackendResync,
        ] {
            assert_eq!(
                taps(vec![
                    down(),
                    StateEvent::Cancel(reason),
                    up(),
                    down(),
                    up(),
                    StateEvent::AllReleased,
                    down(),
                    up()
                ])
                .len(),
                1
            );
        }
    }
    #[test]
    fn lost_release_cannot_be_reinterpreted_as_a_fresh_tap() {
        assert!(
            taps(vec![
                down(),
                down(),
                StateEvent::Cancel(CancelReason::LostRelease),
                up()
            ])
            .is_empty()
        );
    }
    #[test]
    fn left_and_right_are_distinct_and_overlapping_supers_are_consumed() {
        let right_down = StateEvent::SuperDown {
            device: D,
            side: SuperSide::Right,
            repeat: false,
        };
        let right_up = StateEvent::SuperUp {
            device: D,
            side: SuperSide::Right,
        };
        assert_eq!(
            taps(vec![right_down.clone(), right_up.clone()])[0].side,
            SuperSide::Right
        );
        assert!(taps(vec![down(), right_down, up(), right_up]).is_empty());
    }
    #[test]
    fn chord_on_another_keyboard_consumes_pending_super() {
        assert!(
            taps(vec![
                down(),
                StateEvent::KeyDown {
                    device: DeviceId(2),
                    code: 25,
                    repeat: false
                },
                up()
            ])
            .is_empty()
        );
    }
    #[test]
    fn hold_is_consumption_only_without_an_invented_hold_action() {
        assert!(taps(vec![down(), StateEvent::HoldCommitted, up()]).is_empty());
    }
}
