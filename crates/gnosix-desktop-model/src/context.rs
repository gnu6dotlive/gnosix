use crate::{
    generation::DesktopContextGeneration,
    output::{OutputState, ValidatedOutputCollection},
    surface::{SurfaceState, SurfaceValidationError, ValidatedSurfaceCollection},
    workspace::{ValidatedWorkspaceCollection, WorkspaceState, WorkspaceValidationError},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DesktopContextValidationError {
    #[error("duplicate OutputHubId in candidate")]
    DuplicateOutputHubId,
    #[error(transparent)]
    Workspace(#[from] WorkspaceValidationError),
    #[error(transparent)]
    Surface(#[from] SurfaceValidationError),
}

/// Structurally valid desktop semantic comparison baseline.
///
/// Invariants:
/// - Represents a verified baseline at a specific generation.
/// - Outputs, workspaces, and surfaces have validated identity uniqueness.
/// - Workspace/surface cross-references resolve inside this same baseline.
/// - Does not attest to prior history, persistence, compositor state, or writer authority.
#[derive(Debug, PartialEq, Eq)]
pub struct DesktopContextBaseline {
    pub generation: DesktopContextGeneration,
    pub outputs: ValidatedOutputCollection,
    pub workspaces: ValidatedWorkspaceCollection,
    pub surfaces: ValidatedSurfaceCollection,
}

impl DesktopContextBaseline {
    pub fn empty() -> Self {
        let outputs =
            ValidatedOutputCollection::from_outputs(Vec::new()).expect("empty outputs are valid");
        let workspaces = ValidatedWorkspaceCollection::from_workspaces(Vec::new(), &outputs)
            .expect("empty workspaces are valid");
        let surfaces = ValidatedSurfaceCollection::from_surfaces(Vec::new(), &outputs, &workspaces)
            .expect("empty surfaces are valid");

        Self {
            generation: DesktopContextGeneration::INITIAL,
            outputs,
            workspaces,
            surfaces,
        }
    }
}

/// Raw, unvalidated candidate proposed for reconciliation.
///
/// Invariants:
/// - Retains caller-supplied vectors preserving multiplicity and order.
/// - Carries NO generation token.
/// - Implements NO equality trait because vector order is non-semantic.
/// - Does not attest to observation completeness or external authority.
#[derive(Debug, Clone)]
pub struct DesktopContextCandidate {
    pub outputs: Vec<OutputState>,
    pub workspaces: Vec<WorkspaceState>,
    pub surfaces: Vec<SurfaceState>,
}
