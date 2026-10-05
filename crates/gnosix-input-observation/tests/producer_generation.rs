use gnosix_input_observation::*;

#[test]
fn late_old_generation_input_cannot_complete_a_new_generation_tap() {
    let mut adapter =
        ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 2)).unwrap();
    adapter
        .push_record(2, ArbitrationRecordV1::super_pressed(7, 1, 1, 3))
        .unwrap();
    assert_eq!(
        adapter.push_record(1, ArbitrationRecordV1::super_released(7, 2, 1, 3)),
        Err(ObservationError::InvalidStateGeneration)
    );
    assert!(adapter.is_poisoned());
}

fn snapshot(sequence: u64, generation: u64, complete: bool) -> ResyncSnapshotV1_1 {
    ResyncSnapshotV1_1 {
        schema: RESYNC_SNAPSHOT_SCHEMA_V1_1.into(),
        compositor_instance: CompositorInstanceId(7),
        event_sequence: EventSequence(sequence),
        state_generation: generation,
        inventory_complete: complete,
        keyboards: vec![],
        pointers: vec![],
        active_pointer_gestures: vec![],
    }
}

#[test]
fn invalidation_requires_new_generation_and_complete_snapshot_before_rearm() {
    for reason in [
        CancellationReason::ConfigReload,
        CancellationReason::SessionInactive,
        CancellationReason::KeyboardRemoved,
        CancellationReason::InputStateReset,
    ] {
        let mut adapter =
            ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
                .unwrap();
        adapter
            .push_record(1, ArbitrationRecordV1::super_pressed(7, 1, 1, 3))
            .unwrap();
        adapter
            .push_record(
                2,
                ArbitrationRecordV1::new(
                    7,
                    2,
                    None,
                    None,
                    ArbitrationEventV1::StateInvalidated { reason },
                ),
            )
            .unwrap();
        adapter.push_snapshot(snapshot(3, 2, false)).unwrap();
        assert_eq!(
            adapter
                .push_record(2, ArbitrationRecordV1::super_released(7, 4, 1, 3))
                .unwrap(),
            None
        );
        adapter.push_snapshot(snapshot(5, 2, true)).unwrap();
        adapter
            .push_record(2, ArbitrationRecordV1::super_pressed(7, 6, 2, 3))
            .unwrap();
        assert!(
            adapter
                .push_record(2, ArbitrationRecordV1::super_released(7, 7, 2, 3))
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn ordinary_input_cannot_advance_or_skip_generation() {
    for generation in [0, 3, u64::MAX] {
        let mut adapter =
            ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 2))
                .unwrap();
        assert_eq!(
            adapter.push_record(generation, ArbitrationRecordV1::super_pressed(7, 1, 1, 3)),
            Err(ObservationError::InvalidStateGeneration)
        );
        assert!(adapter.is_poisoned());
    }
}

#[test]
fn invalidation_cannot_reuse_skip_or_overflow_generation() {
    for (current, incoming) in [(2, 2), (2, 4), (u64::MAX, 0)] {
        let mut adapter =
            ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, current))
                .unwrap();
        assert_eq!(
            adapter.push_record(
                incoming,
                ArbitrationRecordV1::new(
                    7,
                    1,
                    None,
                    None,
                    ArbitrationEventV1::StateInvalidated {
                        reason: CancellationReason::ConfigReload
                    }
                )
            ),
            Err(ObservationError::InvalidStateGeneration)
        );
        assert!(adapter.is_poisoned());
    }
}

#[test]
fn stale_or_future_snapshot_cannot_change_generation() {
    for generation in [1, 3] {
        let mut adapter =
            ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::while_inputs_held(7, 1, 2))
                .unwrap();
        assert_eq!(
            adapter.push_snapshot(snapshot(1, generation, true)),
            Err(ObservationError::InvalidStateGeneration)
        );
        assert!(adapter.is_poisoned());
    }
}

#[test]
fn record_gap_duplicate_wrong_instance_remain_fail_closed() {
    for bad in [
        ArbitrationRecordV1::super_released(7, 3, 1, 3),
        ArbitrationRecordV1::super_released(7, 1, 1, 3),
        ArbitrationRecordV1::super_released(8, 2, 1, 3),
    ] {
        let mut adapter =
            ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 2))
                .unwrap();
        adapter
            .push_record(2, ArbitrationRecordV1::super_pressed(7, 1, 1, 3))
            .unwrap();
        assert!(adapter.push_record(2, bad).is_err());
        assert!(adapter.is_poisoned());
        assert_eq!(
            adapter.push_snapshot(snapshot(2, 2, true)),
            Err(ObservationError::Poisoned)
        );
    }
}

#[test]
fn projection_serialization_preserves_explicit_generation_and_records() {
    let projection = ProducerProjection::Record {
        state_generation: 9,
        record: ArbitrationRecordV1::super_pressed(7, 1, 1, 3),
    };
    let json = serde_json::to_string(&projection).unwrap();
    assert_eq!(
        serde_json::from_str::<ProducerProjection>(&json).unwrap(),
        projection
    );
    assert!(
        serde_json::from_str::<ProducerProjection>(&json.replace("\"state_generation\":9,", ""))
            .is_err()
    );
}
