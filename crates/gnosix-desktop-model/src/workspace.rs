use crate::output::{OutputHubId, ValidatedOutputCollection};
use thiserror::Error;

/// Opaque logical identifier for a workspace.
///
/// Invariant: Fixed-width 128-bit token. Equality asserts referential identity within the
/// model. Makes no claim of uniqueness, collision resistance, allocation, or persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId([u8; 16]);

impl WorkspaceId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("Workspace name cannot be empty")]
pub struct EmptyWorkspaceNameError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceName(String);

impl WorkspaceName {
    pub fn new(name: impl Into<String>) -> Result<Self, EmptyWorkspaceNameError> {
        let name = name.into();
        if name.trim().is_empty() {
            Err(EmptyWorkspaceNameError)
        } else {
            Ok(Self(name))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WorkspaceVisibility {
    #[default]
    Hidden,
    Visible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WorkspaceFocus {
    #[default]
    Unfocused,
    Focused,
}

/// Passive semantic workspace state.
///
/// output_hub_id is an optional caller-supplied current association. Visibility
/// and focus are observations only. This type does not allocate, persist, move,
/// activate, or focus a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceState {
    pub workspace_id: WorkspaceId,
    pub name: WorkspaceName,
    pub output_hub_id: Option<OutputHubId>,
    pub visibility: WorkspaceVisibility,
    pub focus: WorkspaceFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WorkspaceValidationError {
    #[error("duplicate WorkspaceId in candidate")]
    DuplicateWorkspaceId,
    #[error("workspace references an OutputHubId absent from the candidate outputs")]
    UnknownOutput,
    #[error("candidate contains more than one focused workspace")]
    MultipleFocused,
    #[error("output active workspace is absent, hidden, or associated with another output")]
    InvalidOutputActiveWorkspace,
}

#[derive(Debug)]
pub struct ValidatedWorkspaceCollection {
    workspaces: Vec<WorkspaceState>,
}

impl ValidatedWorkspaceCollection {
    pub fn from_workspaces(
        workspaces: Vec<WorkspaceState>,
        outputs: &ValidatedOutputCollection,
    ) -> Result<Self, WorkspaceValidationError> {
        let len = workspaces.len();
        let mut focused_count = 0usize;
        for i in 0..len {
            for j in (i + 1)..len {
                if workspaces[i].workspace_id == workspaces[j].workspace_id {
                    return Err(WorkspaceValidationError::DuplicateWorkspaceId);
                }
            }

            if workspaces[i]
                .output_hub_id
                .is_some_and(|output_hub_id| !outputs.contains_hub_id(output_hub_id))
            {
                return Err(WorkspaceValidationError::UnknownOutput);
            }

            if workspaces[i].focus == WorkspaceFocus::Focused {
                focused_count += 1;
                if focused_count > 1 {
                    return Err(WorkspaceValidationError::MultipleFocused);
                }
            }
        }

        for output in outputs.iter() {
            if let Some(active_workspace_id) = output.active_workspace_id {
                let valid = workspaces.iter().any(|workspace| {
                    workspace.workspace_id == active_workspace_id
                        && workspace.output_hub_id == Some(output.output_hub_id)
                        && workspace.visibility == WorkspaceVisibility::Visible
                });
                if !valid {
                    return Err(WorkspaceValidationError::InvalidOutputActiveWorkspace);
                }
            }
        }

        Ok(Self { workspaces })
    }

    pub fn len(&self) -> usize {
        self.workspaces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.workspaces.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &WorkspaceState> {
        self.workspaces.iter()
    }

    pub fn contains_workspace_id(&self, workspace_id: WorkspaceId) -> bool {
        self.workspaces
            .iter()
            .any(|workspace| workspace.workspace_id == workspace_id)
    }
}

impl PartialEq for ValidatedWorkspaceCollection {
    fn eq(&self, other: &Self) -> bool {
        self.workspaces.len() == other.workspaces.len()
            && self.workspaces.iter().all(|a| {
                other
                    .workspaces
                    .iter()
                    .any(|b| a.workspace_id == b.workspace_id && a == b)
            })
    }
}

impl Eq for ValidatedWorkspaceCollection {}
