use gnosix_input_observation::*;

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
fn resume_unknown_then_authoritative_neutral_snapshot_rearms_only_new_gestures() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1))
            .unwrap();
    assert_eq!(
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 1, 1, 3))
            .unwrap(),
        None
    );
    adapter.push_resync_snapshot(snapshot(2, 2, false)).unwrap();
    assert_eq!(
        adapter
            .push(ArbitrationRecordV1::super_released(7, 3, 1, 3))
            .unwrap(),
        None
    );
    adapter.push_resync_snapshot(snapshot(4, 2, true)).unwrap();
    assert_eq!(
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 5, 2, 3))
            .unwrap(),
        None
    );
    assert!(
        adapter
            .push(ArbitrationRecordV1::super_released(7, 6, 2, 3))
            .unwrap()
            .is_some()
    );
}

#[test]
fn old_generation_snapshot_never_rearms_after_invalidation() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::while_inputs_held(7, 1, 4))
            .unwrap();
    assert!(adapter.push_resync_snapshot(snapshot(1, 3, true)).is_err());
    assert!(adapter.is_poisoned());
}

#[test]
fn active_snapshot_cannot_rearm_and_bad_order_or_instance_poison_the_stream() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::while_inputs_held(7, 1, 1))
            .unwrap();
    let mut active = snapshot(1, 2, true);
    active.active_pointer_gestures.push(GestureSequenceId(8));
    adapter.push_resync_snapshot(active).unwrap();
    assert_eq!(
        adapter
            .push(ArbitrationRecordV1::super_pressed(7, 2, 1, 3))
            .unwrap(),
        None
    );
    assert_eq!(
        adapter
            .push(ArbitrationRecordV1::super_released(7, 3, 1, 3))
            .unwrap(),
        None
    );
    for bad in [snapshot(5, 2, true), {
        let mut s = snapshot(4, 2, true);
        s.compositor_instance = CompositorInstanceId(8);
        s
    }] {
        let mut fresh = ArbitrationStreamAdapter::synchronize(
            SubscriptionBoundaryV1::while_inputs_held(7, 4, 2),
        )
        .unwrap();
        assert!(fresh.push_resync_snapshot(bad).is_err());
        assert!(fresh.is_poisoned());
    }
}

#[test]
fn compiled_cpp_inventory_helper_output_replays_without_uncertain_taps() {
    #[derive(serde::Deserialize)]
    struct Sample {
        case: String,
        generation: u64,
        readiness: ResyncReadiness,
    }
    let samples: Vec<Sample> = include_str!("../fixtures/resume-inventory-spike.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(samples.len(), 9);
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::while_inputs_held(7, 1, 1))
            .unwrap();
    let mut sequence = 1;
    for (index, sample) in samples.iter().enumerate() {
        let mut projected = snapshot(
            sequence,
            sample.generation,
            sample.readiness != ResyncReadiness::StateUnknown,
        );
        if sample.readiness == ResyncReadiness::InputsActive {
            projected
                .active_pointer_gestures
                .push(GestureSequenceId(900));
        }
        assert_eq!(projected.readiness(), sample.readiness, "{}", sample.case);
        adapter.push_resync_snapshot(projected).unwrap();
        sequence += 1;
        assert_eq!(
            adapter
                .push(ArbitrationRecordV1::super_pressed(
                    7,
                    sequence,
                    index as u64 + 1,
                    3
                ))
                .unwrap(),
            None
        );
        sequence += 1;
        let tap = adapter
            .push(ArbitrationRecordV1::super_released(
                7,
                sequence,
                index as u64 + 1,
                3,
            ))
            .unwrap();
        sequence += 1;
        assert_eq!(
            tap.is_some(),
            sample.readiness == ResyncReadiness::AllReleased,
            "{}",
            sample.case
        );
    }
}

#[test]
fn previous_generation_cannot_rearm_after_explicit_state_invalidation() {
    let mut adapter =
        ArbitrationStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 4))
            .unwrap();
    adapter
        .push(ArbitrationRecordV1::new(
            7,
            1,
            None,
            None,
            ArbitrationEventV1::StateInvalidated {
                reason: CancellationReason::ConfigReload,
            },
        ))
        .unwrap();
    assert!(adapter.push_resync_snapshot(snapshot(2, 4, true)).is_err());
    assert!(adapter.is_poisoned());
}
