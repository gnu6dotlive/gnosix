use gnosix_input_observation::*;

#[derive(serde::Deserialize)]
struct Trace {
    case: String,
    expected_taps: usize,
    projections: Vec<ProducerProjection>,
}

fn traces() -> Vec<Trace> {
    include_str!("../fixtures/complete-producer-spike.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn adapter() -> ProducerStreamAdapter {
    ProducerStreamAdapter::synchronize(SubscriptionBoundaryV1::while_inputs_held(7, 1, 1)).unwrap()
}

#[test]
fn complete_cpp_producer_traces_reach_the_real_super_arbiter() {
    let traces = traces();
    assert_eq!(traces.len(), 35);
    for trace in traces {
        let expected = match trace.case.as_str() {
            "isolated_super"
            | "reload_during_super_then_fresh_tap"
            | "pointer_removed_during_super_then_fresh_tap"
            | "session_resume_barrier_then_fresh_tap"
            | "repeat_and_duplicate_release"
            | "long_hold_no_hold_action"
            | "dnd_cancel_then_resync"
            | "old_generation_timer_claim"
            | "native_trackpad_outlives_raw_end" => 1,
            "left_right_super" => 2,
            "rapid_taps" => 3,
            "super_w"
            | "super_shift_s"
            | "super_drag"
            | "super_resize"
            | "keyboard_removed_during_super"
            | "resume_without_barrier"
            | "incomplete_inventory"
            | "old_generation_release_ticket"
            | "overlapping_super"
            | "cross_keyboard_chord"
            | "super_release_binding"
            | "transparent_failed_binding"
            | "matched_not_invoked"
            | "dnd_retains_ownership_after_wayland_drop"
            | "swipe"
            | "pinch"
            | "trackpad_hold"
            | "trackpad_cancel"
            | "modifier_only_active_snapshot"
            | "unknown_owner_snapshot"
            | "window_cancel"
            | "unknown_release_during_super"
            | "pointer_release_binding"
            | "pointer_removed_native_trackpad_stays_active" => 0,
            name => panic!("unreviewed producer scenario: {name}"),
        };
        assert_eq!(trace.expected_taps, expected, "{}", trace.case);
        let mut adapter = adapter();
        let mut taps = 0;
        for (index, projection) in trace.projections.into_iter().enumerate() {
            taps += usize::from(
                adapter
                    .push(projection)
                    .unwrap_or_else(|error| panic!("{} record {}: {error:?}", trace.case, index))
                    .is_some(),
            );
        }
        assert!(!adapter.is_poisoned(), "{}", trace.case);
        assert_eq!(taps, expected, "{}", trace.case);
    }
}

#[test]
fn complete_producer_stream_faults_poison_pending_tap_before_release() {
    let tap = traces()
        .into_iter()
        .find(|t| t.case == "isolated_super")
        .unwrap();
    for fault in [
        "gap",
        "duplicate",
        "out_of_order",
        "old_generation",
        "wrong_instance",
    ] {
        let mut projections = tap.projections.clone();
        let (index, generation, record) = projections
            .iter_mut()
            .enumerate()
            .find_map(|(i, p)| match p {
                ProducerProjection::Record {
                    state_generation,
                    record,
                } if matches!(record.event, ArbitrationEventV1::ModifierReleased { .. }) => {
                    Some((i, state_generation, record))
                }
                _ => None,
            })
            .unwrap();
        match fault {
            "gap" => record.event_sequence.0 += 1,
            "duplicate" => record.event_sequence.0 -= 1,
            "out_of_order" => record.event_sequence.0 = 1,
            "old_generation" => *generation -= 1,
            "wrong_instance" => record.compositor_instance = CompositorInstanceId(8),
            _ => unreachable!(),
        }
        let mut adapter = adapter();
        for projection in projections[..index].iter().cloned() {
            assert_eq!(adapter.push(projection).unwrap(), None);
        }
        assert!(adapter.push(projections[index].clone()).is_err(), "{fault}");
        assert!(adapter.is_poisoned(), "{fault}");
        for projection in projections[index + 1..].iter().cloned() {
            assert_eq!(adapter.push(projection), Err(ObservationError::Poisoned));
        }
    }
}
