use gnosix_input_observation::{
    ARBITRATION_SCHEMA_V1, ArbitrationEventV1, ArbitrationRecordV1, CompositorInstanceId,
    EventSequence, InputStateAtBoundary, ProducerProjection, RESYNC_SNAPSHOT_SCHEMA_V1_1,
    ResyncSnapshotV1_1, SubscriptionBoundaryV1,
};
use gnosix_input_runtime_observer::{
    MAX_FRAME_BYTES, RUNTIME_OBSERVATION_SCHEMA_V0, RuntimeHelloV0, RuntimeObserverError,
    observe_reader,
};
use std::io::Cursor;

fn frame<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let payload = serde_json::to_vec(value).unwrap();
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    out
}

fn hello(first_event: u64) -> RuntimeHelloV0 {
    RuntimeHelloV0 {
        schema: RUNTIME_OBSERVATION_SCHEMA_V0.into(),
        boundary: SubscriptionBoundaryV1 {
            schema: ARBITRATION_SCHEMA_V1.into(),
            compositor_instance: CompositorInstanceId(7),
            first_event_sequence: EventSequence(first_event),
            state_generation: 0,
            input_state: InputStateAtBoundary::ActiveOrUnknown,
        },
    }
}

fn invalidation(sequence: u64) -> ProducerProjection {
    ProducerProjection::Record {
        state_generation: 1,
        record: ArbitrationRecordV1::new(
            7,
            sequence,
            None,
            None,
            ArbitrationEventV1::StateInvalidated {
                reason: gnosix_input_observation::CancellationReason::InputStateReset,
            },
        ),
    }
}

fn unknown_snapshot(sequence: u64) -> ProducerProjection {
    ProducerProjection::Snapshot {
        snapshot: ResyncSnapshotV1_1 {
            schema: RESYNC_SNAPSHOT_SCHEMA_V1_1.into(),
            compositor_instance: CompositorInstanceId(7),
            event_sequence: EventSequence(sequence),
            state_generation: 1,
            inventory_complete: false,
            keyboards: vec![],
            pointers: vec![],
            active_pointer_gestures: vec![],
        },
    }
}

#[test]
fn valid_stream_requires_hello_and_replays_projections() {
    let mut bytes = frame(&hello(1));
    bytes.extend(frame(&invalidation(1)));
    bytes.extend(frame(&unknown_snapshot(2)));

    let receipt = observe_reader(Cursor::new(bytes)).unwrap();
    assert_eq!(receipt.compositor_instance, 7);
    assert_eq!(receipt.first_event_sequence, 1);
    assert_eq!(receipt.initial_state_generation, 0);
    assert_eq!(receipt.projections, 2);
    assert_eq!(receipt.snapshots, 1);
    assert_eq!(receipt.all_released_snapshots, 0);
    assert_eq!(receipt.inputs_active_snapshots, 0);
    assert_eq!(receipt.state_unknown_snapshots, 1);
    assert_eq!(receipt.modifier_taps, 0);
    assert!(!receipt.poisoned);
}

#[test]
fn projection_before_hello_is_rejected() {
    let err = observe_reader(Cursor::new(frame(&invalidation(1)))).unwrap_err();
    assert_eq!(err, RuntimeObserverError::InvalidHello);
}

#[test]
fn truncated_frame_is_rejected() {
    let mut bytes = (8_u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(b"{}x");
    let err = observe_reader(Cursor::new(bytes)).unwrap_err();
    assert_eq!(err, RuntimeObserverError::TruncatedFrame);
}

#[test]
fn oversized_frame_is_rejected_before_allocation() {
    let bytes = ((MAX_FRAME_BYTES as u32) + 1).to_be_bytes().to_vec();
    let err = observe_reader(Cursor::new(bytes)).unwrap_err();
    assert_eq!(err, RuntimeObserverError::FrameTooLarge);
}

#[test]
fn adapter_fault_is_propagated_fail_closed() {
    let mut bytes = frame(&hello(1));
    bytes.extend(frame(&invalidation(2)));
    let err = observe_reader(Cursor::new(bytes)).unwrap_err();
    assert!(matches!(err, RuntimeObserverError::Observation(_)));
}

#[cfg(unix)]
#[test]
fn unix_listener_accepts_one_inbound_read_only_stream() {
    use gnosix_input_runtime_observer::observe_listener_once;
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};

    let path = std::env::temp_dir().join(format!(
        "gnosix-input-observer-test-{}-{}.sock",
        std::process::id(),
        std::thread::current().name().unwrap_or("transport")
    ));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || {
        let mut stream = UnixStream::connect(writer_path).unwrap();
        stream.write_all(&frame(&hello(1))).unwrap();
        stream.write_all(&frame(&invalidation(1))).unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
    });

    let receipt = observe_listener_once(&listener).unwrap();
    writer.join().unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(receipt.projections, 1);
    assert_eq!(receipt.modifier_taps, 0);
}
