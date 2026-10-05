use gnosix_input_observation::*;

#[test]
fn binding_claim_on_super_itself_precedes_release_and_suppresses_tap() {
    let mut adapter =
        ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1)).unwrap();
    adapter
        .push_record(1, ArbitrationRecordV1::super_pressed(7, 1, 1, 3))
        .unwrap();
    adapter
        .push_record(
            1,
            ArbitrationRecordV1::new(
                7,
                2,
                Some(1),
                Some(3),
                ArbitrationEventV1::BindingClaimed {
                    key_code: 125,
                    binding: BindingRef::Unidentified,
                    propagation: Propagation::Consumed,
                    dispatch: DispatchDisposition::Succeeded,
                },
            ),
        )
        .unwrap();
    assert_eq!(
        adapter
            .push_record(1, ArbitrationRecordV1::super_released(7, 3, 1, 3))
            .unwrap(),
        None
    );
}

#[test]
fn compositor_owned_pointer_gesture_can_begin_without_button_participation() {
    let mut adapter =
        ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::all_released(7, 1, 1)).unwrap();
    assert_eq!(
        adapter
            .push_record(
                1,
                ArbitrationRecordV1::new(
                    7,
                    1,
                    Some(1),
                    None,
                    ArbitrationEventV1::PointerGestureClaimed {
                        phase: PointerGesturePhase::Started,
                        operation: PointerOperation::Other
                    }
                )
            )
            .unwrap(),
        None
    );
    adapter
        .push_record(1, ArbitrationRecordV1::super_pressed(7, 2, 1, 3))
        .unwrap();
    assert_eq!(
        adapter
            .push_record(1, ArbitrationRecordV1::super_released(7, 3, 1, 3))
            .unwrap(),
        None
    );
}
