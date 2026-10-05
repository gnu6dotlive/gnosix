use crate::{
    context::{DesktopContextBaseline, DesktopContextCandidate, DesktopContextValidationError},
    output::ValidatedOutputCollection,
    surface::ValidatedSurfaceCollection,
    workspace::ValidatedWorkspaceCollection,
};

/// Exhaustive outcome vocabulary of a desktop semantic transition.
///
/// Carries no persistence, compositor mutation, observation, or writer authority.
#[derive(Debug, PartialEq, Eq)]
pub enum TransitionOutcome {
    /// Candidate was structurally valid and semantically identical to the baseline.
    NoChange,

    /// Candidate was structurally valid and semantically different.
    /// Generation advanced by exactly +1.
    Changed(DesktopContextBaseline),

    /// Candidate failed structural identity or cross-reference validation.
    InvalidCandidate(DesktopContextValidationError),

    /// Candidate changed semantically but generation advancement overflowed.
    GenerationExhausted,
}

/// Pure deterministic reconciliation of caller-supplied desktop semantic state.
pub fn reconcile_context(
    baseline: &DesktopContextBaseline,
    candidate: DesktopContextCandidate,
) -> TransitionOutcome {
    let DesktopContextCandidate {
        outputs,
        workspaces,
        surfaces,
    } = candidate;

    let Some(outputs) = ValidatedOutputCollection::from_outputs(outputs) else {
        return TransitionOutcome::InvalidCandidate(
            DesktopContextValidationError::DuplicateOutputHubId,
        );
    };

    let workspaces = match ValidatedWorkspaceCollection::from_workspaces(workspaces, &outputs) {
        Ok(workspaces) => workspaces,
        Err(error) => {
            return TransitionOutcome::InvalidCandidate(DesktopContextValidationError::Workspace(
                error,
            ));
        }
    };

    let surfaces = match ValidatedSurfaceCollection::from_surfaces(surfaces, &outputs, &workspaces)
    {
        Ok(surfaces) => surfaces,
        Err(error) => {
            return TransitionOutcome::InvalidCandidate(DesktopContextValidationError::Surface(
                error,
            ));
        }
    };

    if outputs == baseline.outputs
        && workspaces == baseline.workspaces
        && surfaces == baseline.surfaces
    {
        return TransitionOutcome::NoChange;
    }

    let Ok(generation) = baseline.generation.checked_next() else {
        return TransitionOutcome::GenerationExhausted;
    };

    TransitionOutcome::Changed(DesktopContextBaseline {
        generation,
        outputs,
        workspaces,
        surfaces,
    })
}
