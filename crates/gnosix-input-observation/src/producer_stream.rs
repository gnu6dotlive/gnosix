use crate::*;

/// Generation is explicit in the producer projection rather than inferred
/// from a transport timestamp or added retroactively to historical v1 records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProducerProjection {
    Record {
        state_generation: u64,
        record: ArbitrationRecordV1,
    },
    Snapshot {
        snapshot: ResyncSnapshotV1_1,
    },
}

/// In-process producer projection; no transport or execution authority.
#[derive(Debug)]
pub struct ProducerStreamAdapter {
    inner: ArbitrationStreamAdapter,
    generation: u64,
}

impl ProducerStreamAdapter {
    pub fn push(
        &mut self,
        projection: ProducerProjection,
    ) -> Result<Option<ModifierTap>, ObservationError> {
        match projection {
            ProducerProjection::Record {
                state_generation,
                record,
            } => self.push_record(state_generation, record),
            ProducerProjection::Snapshot { snapshot } => {
                self.push_snapshot(snapshot).map(|()| None)
            }
        }
    }
    pub fn synchronize(boundary: SubscriptionBoundaryV1) -> Result<Self, ObservationError> {
        let generation = boundary.state_generation;
        Ok(Self {
            inner: ArbitrationStreamAdapter::synchronize(boundary)?,
            generation,
        })
    }

    pub fn push_record(
        &mut self,
        generation: u64,
        record: ArbitrationRecordV1,
    ) -> Result<Option<ModifierTap>, ObservationError> {
        if self.inner.is_poisoned() {
            return Err(ObservationError::Poisoned);
        }
        // A discontinuity is the only record permitted to move generations;
        // the following complete snapshot, never an ordinary key event,
        // establishes synchronization in that generation.
        let invalidation = matches!(
            record.event,
            ArbitrationEventV1::StateInvalidated { .. }
                | ArbitrationEventV1::SequenceCancelled { .. }
                | ArbitrationEventV1::DeviceRemoved { .. }
        );
        let expected = if invalidation {
            self.generation.checked_add(1)
        } else {
            Some(self.generation)
        };
        if expected != Some(generation) {
            self.inner.invalidate();
            return Err(ObservationError::InvalidStateGeneration);
        }
        if let ArbitrationEventV1::AllInputsReleased { state_generation } = &record.event
            && *state_generation != generation
        {
            self.inner.invalidate();
            return Err(ObservationError::InvalidStateGeneration);
        }
        self.generation = generation;
        self.inner.push(record)
    }

    pub fn push_snapshot(&mut self, snapshot: ResyncSnapshotV1_1) -> Result<(), ObservationError> {
        if self.inner.is_poisoned() {
            return Err(ObservationError::Poisoned);
        }
        if snapshot.state_generation != self.generation {
            self.inner.invalidate();
            return Err(ObservationError::InvalidStateGeneration);
        }
        self.inner.push_resync_snapshot(snapshot)
    }

    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }
}
