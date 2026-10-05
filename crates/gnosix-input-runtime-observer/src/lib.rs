//! Receipt-only runtime observer for Gnosix compositor input projections.
//!
//! The observer reads a one-way framed stream and feeds the neutral
//! `gnosix-input-observation` adapter. It has no compositor command surface.

use gnosix_input_observation::{
    ObservationError, ProducerProjection, ProducerStreamAdapter, ResyncReadiness,
    SubscriptionBoundaryV1,
};
use serde::{Deserialize, Serialize};
use std::io::Read;

pub const RUNTIME_OBSERVATION_SCHEMA_V0: &str = "gnosix.input.runtime-observation.v0";
pub const MAX_FRAME_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeHelloV0 {
    pub schema: String,
    pub boundary: SubscriptionBoundaryV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeObservationReceipt {
    pub compositor_instance: u128,
    pub first_event_sequence: u64,
    pub initial_state_generation: u64,
    pub projections: u64,
    pub snapshots: u64,
    pub all_released_snapshots: u64,
    pub inputs_active_snapshots: u64,
    pub state_unknown_snapshots: u64,
    pub modifier_taps: u64,
    pub poisoned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeObserverError {
    Io(String),
    InvalidHello,
    InvalidProjection,
    TruncatedFrame,
    FrameTooLarge,
    Observation(ObservationError),
}

impl From<ObservationError> for RuntimeObserverError {
    fn from(value: ObservationError) -> Self {
        Self::Observation(value)
    }
}

pub fn observe_reader(
    mut reader: impl Read,
) -> Result<RuntimeObservationReceipt, RuntimeObserverError> {
    let hello_bytes = read_frame(&mut reader)?.ok_or(RuntimeObserverError::InvalidHello)?;
    let hello: RuntimeHelloV0 =
        serde_json::from_slice(&hello_bytes).map_err(|_| RuntimeObserverError::InvalidHello)?;
    if hello.schema != RUNTIME_OBSERVATION_SCHEMA_V0 {
        return Err(RuntimeObserverError::InvalidHello);
    }

    let mut adapter = ProducerStreamAdapter::synchronize(hello.boundary.clone())?;
    let mut projections = 0_u64;
    let mut snapshots = 0_u64;
    let mut all_released_snapshots = 0_u64;
    let mut inputs_active_snapshots = 0_u64;
    let mut state_unknown_snapshots = 0_u64;
    let mut modifier_taps = 0_u64;

    while let Some(payload) = read_frame(&mut reader)? {
        let projection: ProducerProjection = serde_json::from_slice(&payload)
            .map_err(|_| RuntimeObserverError::InvalidProjection)?;
        if let ProducerProjection::Snapshot { snapshot } = &projection {
            snapshots += 1;
            match snapshot.readiness() {
                ResyncReadiness::AllReleased => all_released_snapshots += 1,
                ResyncReadiness::InputsActive => inputs_active_snapshots += 1,
                ResyncReadiness::StateUnknown => state_unknown_snapshots += 1,
            }
        }
        if adapter.push(projection)?.is_some() {
            modifier_taps += 1;
        }
        projections += 1;
    }

    Ok(RuntimeObservationReceipt {
        compositor_instance: hello.boundary.compositor_instance.0,
        first_event_sequence: hello.boundary.first_event_sequence.0,
        initial_state_generation: hello.boundary.state_generation,
        projections,
        snapshots,
        all_released_snapshots,
        inputs_active_snapshots,
        state_unknown_snapshots,
        modifier_taps,
        poisoned: adapter.is_poisoned(),
    })
}

fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, RuntimeObserverError> {
    let mut header = [0_u8; 4];
    let mut read = 0;
    while read < header.len() {
        match reader.read(&mut header[read..]) {
            Ok(0) if read == 0 => return Ok(None),
            Ok(0) => return Err(RuntimeObserverError::TruncatedFrame),
            Ok(n) => read += n,
            Err(error) => return Err(RuntimeObserverError::Io(error.to_string())),
        }
    }

    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(RuntimeObserverError::FrameTooLarge);
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => RuntimeObserverError::TruncatedFrame,
            _ => RuntimeObserverError::Io(error.to_string()),
        })?;
    Ok(Some(payload))
}

#[cfg(unix)]
pub fn observe_listener_once(
    listener: &std::os::unix::net::UnixListener,
) -> Result<RuntimeObservationReceipt, RuntimeObserverError> {
    let (stream, _) = listener
        .accept()
        .map_err(|error| RuntimeObserverError::Io(error.to_string()))?;
    observe_reader(stream)
}
