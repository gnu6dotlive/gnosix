//! Read-only association between durable output identity and an existing
//! session-scoped desktop projection.
//!
//! This adapter adds optional durable metadata beside a
//! [`DesktopOutputProjection`]. It never changes the projection, allocates or
//! replaces its session [`OutputHubId`], or grants mutation authority.

use gnosix_desktop_model::{DesktopOutputProjection, OutputHubId};
use in_gnu_os_output_identity::{
    BindingEvidence, BindingState, DurableOutputHubId, IdentityReadState, OutputIdentityReader,
    OutputObservation, StoreError,
};
use std::collections::BTreeSet;

/// Current output identity observation explicitly paired with the session
/// output from which it was collected.
///
/// The observation adapter remains responsible for ensuring the evidence was
/// collected from this output. The session ID check prevents accidentally
/// attaching a separately resolved result to a different projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOutputObservation {
    session_output_hub_id: OutputHubId,
    observation: OutputObservation,
}

impl SessionOutputObservation {
    pub fn for_output(session_output_hub_id: OutputHubId, observation: OutputObservation) -> Self {
        Self {
            session_output_hub_id,
            observation,
        }
    }

    pub const fn session_output_hub_id(&self) -> OutputHubId {
        self.session_output_hub_id
    }

    pub fn observation(&self) -> &OutputObservation {
        &self.observation
    }
}

/// Additive, read-only link between a current session output and a durable
/// logical OutputHub. The two identifiers intentionally have distinct types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableSessionOutputAssociation {
    pub session_output_hub_id: OutputHubId,
    pub durable_output_hub_id: DurableOutputHubId,
    pub registry_revision: u64,
    pub matched_evidence: BTreeSet<BindingEvidence>,
}

#[derive(Debug)]
pub enum ProjectionAssociationError {
    ObservationSessionMismatch,
    Store(StoreError),
}

impl From<StoreError> for ProjectionAssociationError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Resolves optional durable metadata for an existing session projection.
///
/// `reader = None`, `Absent`, recovery states, and every non-`Bound`
/// state return no association. The caller retains the original projection
/// unchanged in all cases. A `Bound` association is returned only when the
/// observation was explicitly paired with that projection's session ID.
///
/// A durable identity cannot be substituted for the session identifier:
///
/// ```compile_fail
/// use gnosix_desktop_model::OutputHubId;
/// use in_gnu_os_output_identity::DurableOutputHubId;
/// let durable = DurableOutputHubId::from_bytes([1; 16]);
/// let _: OutputHubId = durable;
/// ```
pub fn associate_projection_identity(
    projection: &DesktopOutputProjection,
    observed: &SessionOutputObservation,
    reader: Option<&OutputIdentityReader<'_>>,
) -> Result<Option<DurableSessionOutputAssociation>, ProjectionAssociationError> {
    let Some(reader) = reader else {
        return Ok(None);
    };

    if observed.session_output_hub_id != projection.output.output_hub_id {
        return Err(ProjectionAssociationError::ObservationSessionMismatch);
    }

    let state = reader.resolve(observed.observation())?;
    Ok(association_from_state(projection, state))
}

fn association_from_state(
    projection: &DesktopOutputProjection,
    state: IdentityReadState,
) -> Option<DurableSessionOutputAssociation> {
    let IdentityReadState::Current {
        revision,
        binding:
            BindingState::Bound {
                output_hub_id,
                matched_evidence,
            },
    } = state
    else {
        return None;
    };

    Some(DurableSessionOutputAssociation {
        session_output_hub_id: projection.output.output_hub_id,
        durable_output_hub_id: output_hub_id,
        registry_revision: revision,
        matched_evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnosix_desktop_model::{
        ConnectorName, DesktopContextGeneration, LogicalPoint, LogicalSize, OutputState,
        OutputTransform, PhysicalPixelSize, ScaleMilli,
    };
    use in_gnu_os_output_identity::{BindingConflictReason, RecoveryIssue, UnboundReason};

    fn projection() -> DesktopOutputProjection {
        DesktopOutputProjection {
            generation: DesktopContextGeneration(1),
            output: OutputState {
                output_hub_id: OutputHubId::from_bytes([3; 16]),
                connector: ConnectorName::new("DP-3").unwrap(),
                active_workspace_id: None,
                descriptors: Default::default(),
                physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
                logical_position: LogicalPoint::ORIGIN,
                logical_size: LogicalSize::new(1920, 1080).unwrap(),
                scale: ScaleMilli::from_milli(1000).unwrap(),
                transform: OutputTransform::Normal,
            },
            workspaces: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn every_non_bound_read_state_maps_to_no_association() {
        let projection = projection();
        let recovery = RecoveryIssue {
            current: in_gnu_os_output_identity::CandidateIssue::InvalidJson,
            previous: None,
            initialized_marker: true,
            pending_candidates: Vec::new(),
        };
        let states = [
            IdentityReadState::Absent,
            IdentityReadState::Current {
                revision: 1,
                binding: BindingState::Unbound {
                    reason: UnboundReason::NoMatchingBinding,
                },
            },
            IdentityReadState::Current {
                revision: 1,
                binding: BindingState::Ambiguous {
                    candidates: BTreeSet::from([
                        DurableOutputHubId::from_bytes([1; 16]),
                        DurableOutputHubId::from_bytes([2; 16]),
                    ]),
                },
            },
            IdentityReadState::Current {
                revision: 1,
                binding: BindingState::DescriptorConflict {
                    candidates: BTreeSet::from([DurableOutputHubId::from_bytes([1; 16])]),
                    reason: BindingConflictReason::StrongDescriptorDisagreement,
                },
            },
            IdentityReadState::Recoverable {
                previous_revision: 1,
                issue: recovery.clone(),
            },
            IdentityReadState::Unrecoverable { issue: recovery },
        ];

        for state in states {
            assert_eq!(association_from_state(&projection, state), None);
        }
    }
}
