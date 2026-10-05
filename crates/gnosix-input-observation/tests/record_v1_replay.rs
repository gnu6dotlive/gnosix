use gnosix_input_observation::{
    ArbitrationEventV1, ArbitrationRecordV1, ArbitrationStreamAdapter, BindingRef,
    CancellationReason, DispatchDisposition, PointerGesturePhase, PointerOperation, Propagation,
    SubscriptionBoundaryV1,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Trace {
    name: String,
    boundary: SubscriptionBoundaryV1,
    records: Vec<ArbitrationRecordV1>,
    expected_taps: Vec<gnosix_input_core::arbiter::ModifierTap>,
}

#[test]
fn versioned_record_traces_replay_through_the_qualified_super_arbiter() {
    let traces: Vec<Trace> = serde_json::from_str(include_str!(
        "../fixtures/arbitration-record-v1-traces.json"
    ))
    .expect("v1 record fixture corpus parses");
    assert_eq!(
        traces.len(),
        10,
        "the ten qualified scenarios are translated"
    );

    for trace in traces {
        let mut adapter = ArbitrationStreamAdapter::synchronize(trace.boundary)
            .expect("fixture has a valid synchronization boundary");
        let actual: Vec<_> = trace
            .records
            .into_iter()
            .filter_map(|record| {
                adapter
                    .push(record)
                    .unwrap_or_else(|error| panic!("trace {}: {error:?}", trace.name))
            })
            .collect();
        assert_eq!(actual, trace.expected_taps, "trace {}", trace.name);
    }
}

#[test]
fn an_event_sequence_gap_invalidates_the_stream_without_a_tap() {
    let boundary = SubscriptionBoundaryV1::all_released(7, 11, 1);
    let mut adapter = ArbitrationStreamAdapter::synchronize(boundary).unwrap();
    let press = ArbitrationRecordV1::super_pressed(7, 11, 9, 3);
    assert_eq!(adapter.push(press).unwrap(), None);

    let release_with_gap = ArbitrationRecordV1::super_released(7, 13, 9, 3);
    assert!(adapter.push(release_with_gap).is_err());
    assert!(adapter.is_poisoned());
}

#[test]
fn duplicate_event_sequence_and_wrong_compositor_poison_pending_gesture() {
    for invalid in [
        ArbitrationRecordV1::super_released(7, 11, 9, 3),
        ArbitrationRecordV1::super_released(8, 12, 9, 3),
    ] {
        let mut adapter =
            ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 11, 1))
                .unwrap();
        assert_eq!(
            adapter
                .push(ArbitrationRecordV1::super_pressed(7, 11, 9, 3))
                .unwrap(),
            None
        );
        assert!(adapter.push(invalid).is_err());
        assert!(adapter.is_poisoned());
    }
}

#[test]
fn gesture_sequence_ids_cannot_be_reused_after_completion() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
            .unwrap();
    assert!(
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 1, 10, 3))
            .unwrap()
            .is_none()
    );
    assert!(
        adapter
            .push(ArbitrationRecordV1::super_released(7, 2, 10, 3))
            .unwrap()
            .is_some()
    );
    adapter
        .push(ArbitrationRecordV1::new(
            7,
            3,
            Some(10),
            Some(3),
            ArbitrationEventV1::GestureEnded,
        ))
        .unwrap();
    assert!(
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 4, 10, 3))
            .is_err()
    );
    assert!(adapter.is_poisoned());
}

#[test]
fn transparent_or_failed_binding_claim_still_consumes_super_tap() {
    for (propagation, dispatch) in [
        (Propagation::PassedThrough, DispatchDisposition::Failed),
        (Propagation::Consumed, DispatchDisposition::Succeeded),
    ] {
        let mut adapter =
            ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
                .unwrap();
        let events = [
            ArbitrationEventV1::ModifierPressed {
                side: gnosix_input_observation::SuperKeySide::Left,
                repeat: false,
            },
            ArbitrationEventV1::KeyParticipation {
                code: 25,
                pressed: true,
                repeat: false,
            },
            ArbitrationEventV1::BindingClaimed {
                key_code: 25,
                binding: BindingRef::Unidentified,
                propagation,
                dispatch,
            },
            ArbitrationEventV1::KeyParticipation {
                code: 25,
                pressed: false,
                repeat: false,
            },
            ArbitrationEventV1::ModifierReleased {
                side: gnosix_input_observation::SuperKeySide::Left,
            },
            ArbitrationEventV1::GestureEnded,
        ];
        let taps: Vec<_> = events
            .into_iter()
            .enumerate()
            .filter_map(|(index, event)| {
                adapter
                    .push(ArbitrationRecordV1::new(
                        7,
                        index as u64 + 1,
                        Some(12),
                        Some(3),
                        event,
                    ))
                    .unwrap()
            })
            .collect();
        assert!(taps.is_empty());
    }
}

#[test]
fn late_binding_outcome_does_not_invent_a_second_key_press() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
            .unwrap();
    let events = [
        ArbitrationEventV1::ModifierPressed {
            side: gnosix_input_observation::SuperKeySide::Left,
            repeat: false,
        },
        ArbitrationEventV1::KeyParticipation {
            code: 25,
            pressed: true,
            repeat: false,
        },
        ArbitrationEventV1::KeyParticipation {
            code: 25,
            pressed: false,
            repeat: false,
        },
        ArbitrationEventV1::BindingClaimed {
            key_code: 25,
            binding: BindingRef::Unidentified,
            propagation: Propagation::PassedThrough,
            dispatch: DispatchDisposition::Succeeded,
        },
        ArbitrationEventV1::ModifierReleased {
            side: gnosix_input_observation::SuperKeySide::Left,
        },
        ArbitrationEventV1::GestureEnded,
        ArbitrationEventV1::ModifierPressed {
            side: gnosix_input_observation::SuperKeySide::Left,
            repeat: false,
        },
        ArbitrationEventV1::ModifierReleased {
            side: gnosix_input_observation::SuperKeySide::Left,
        },
    ];
    let taps: Vec<_> = events
        .into_iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let gesture = if index < 6 { 12 } else { 13 };
            adapter
                .push(ArbitrationRecordV1::new(
                    7,
                    index as u64 + 1,
                    Some(gesture),
                    Some(3),
                    event,
                ))
                .unwrap()
        })
        .collect();
    assert_eq!(taps.len(), 1);
}

#[test]
fn pointer_gesture_claim_after_super_suppresses_tap_without_motion_records() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
            .unwrap();
    let events = [
        ArbitrationEventV1::ModifierPressed {
            side: gnosix_input_observation::SuperKeySide::Left,
            repeat: false,
        },
        ArbitrationEventV1::PointerGestureClaimed {
            phase: PointerGesturePhase::Started,
            operation: PointerOperation::MoveWindow,
        },
        ArbitrationEventV1::ModifierReleased {
            side: gnosix_input_observation::SuperKeySide::Left,
        },
    ];
    let taps: Vec<_> = events
        .into_iter()
        .enumerate()
        .filter_map(|(index, event)| {
            adapter
                .push(ArbitrationRecordV1::new(
                    7,
                    index as u64 + 1,
                    Some(5),
                    Some(8),
                    event,
                ))
                .unwrap()
        })
        .collect();
    assert!(taps.is_empty());
}

#[test]
fn cancellation_or_device_removal_requires_a_fresh_all_released_boundary() {
    for invalidation in [
        ArbitrationEventV1::SequenceCancelled {
            reason: CancellationReason::LostRelease,
        },
        ArbitrationEventV1::DeviceRemoved {
            kind: gnosix_input_observation::InputDeviceKind::Keyboard,
        },
    ] {
        let mut adapter =
            ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
                .unwrap();
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 1, 10, 3))
            .unwrap();
        adapter
            .push(ArbitrationRecordV1::new(
                7,
                2,
                Some(10),
                Some(3),
                invalidation,
            ))
            .unwrap();
        assert_eq!(
            adapter
                .push(ArbitrationRecordV1::super_released(7, 3, 10, 3))
                .unwrap(),
            None
        );
        adapter
            .push(ArbitrationRecordV1::new(
                7,
                4,
                None,
                None,
                ArbitrationEventV1::AllInputsReleased {
                    state_generation: 2,
                },
            ))
            .unwrap();
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 5, 11, 3))
            .unwrap();
        assert!(
            adapter
                .push(ArbitrationRecordV1::super_released(7, 6, 11, 3))
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn subscription_while_super_is_held_cannot_reinterpret_release_as_tap() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::while_inputs_held(7, 20, 4))
            .unwrap();
    assert_eq!(
        adapter
            .push(ArbitrationRecordV1::super_released(7, 20, 2, 1))
            .unwrap(),
        None
    );
    assert!(!adapter.is_poisoned());
}
