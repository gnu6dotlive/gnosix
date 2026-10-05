use crate::{
    output::{OutputHubId, ValidatedOutputCollection},
    spatial::LogicalRect,
    workspace::{ValidatedWorkspaceCollection, WorkspaceId},
};
use thiserror::Error;

/// Opaque logical identifier for a desktop surface.
///
/// Equality is referential identity inside the semantic model only. Allocation,
/// compositor object identity, persistence, and process ownership are out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId([u8; 16]);

impl SurfaceId {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesktopSurfaceRole {
    Application,
    Shell,
    RearPlane,
    Overlay,
    Unclassified,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplicationSurfaceDescriptors {
    pub class: String,
    pub initial_class: String,
    pub title: String,
    pub initial_title: String,
    pub process_id: u32,
    pub xwayland: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceVisibility {
    Visible,
    Hidden,
    #[default]
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceFocus {
    Focused,
    Unfocused,
    #[default]
    Unknown,
}

/// Passive caller-supplied semantic state for a surface.
///
/// Optional output/workspace references express current semantic association.
/// Visibility and focus are observations only. They do not perform routing,
/// placement, activation, persistence, focus mutation, or compositor mutation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SurfaceState {
    pub surface_id: SurfaceId,
    pub role: DesktopSurfaceRole,
    pub output_hub_id: Option<OutputHubId>,
    pub workspace_id: Option<WorkspaceId>,
    pub bounds: LogicalRect,
    pub application: Option<ApplicationSurfaceDescriptors>,
    pub visibility: SurfaceVisibility,
    pub focus: SurfaceFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SurfaceValidationError {
    #[error("duplicate SurfaceId in candidate")]
    DuplicateSurfaceId,
    #[error("surface references an OutputHubId absent from the candidate outputs")]
    UnknownOutput,
    #[error("surface references a WorkspaceId absent from the candidate workspaces")]
    UnknownWorkspace,
    #[error("application surface is missing application descriptors")]
    MissingApplicationDescriptors,
    #[error("non-application surface unexpectedly contains application descriptors")]
    UnexpectedApplicationDescriptors,
    #[error("candidate contains more than one focused surface")]
    MultipleFocused,
}

#[derive(Debug)]
pub struct ValidatedSurfaceCollection {
    surfaces: Vec<SurfaceState>,
}

impl ValidatedSurfaceCollection {
    pub fn from_surfaces(
        surfaces: Vec<SurfaceState>,
        outputs: &ValidatedOutputCollection,
        workspaces: &ValidatedWorkspaceCollection,
    ) -> Result<Self, SurfaceValidationError> {
        let len = surfaces.len();
        let mut focused_count = 0usize;
        for i in 0..len {
            for j in (i + 1)..len {
                if surfaces[i].surface_id == surfaces[j].surface_id {
                    return Err(SurfaceValidationError::DuplicateSurfaceId);
                }
            }

            if surfaces[i]
                .output_hub_id
                .is_some_and(|output_hub_id| !outputs.contains_hub_id(output_hub_id))
            {
                return Err(SurfaceValidationError::UnknownOutput);
            }

            if surfaces[i]
                .workspace_id
                .is_some_and(|workspace_id| !workspaces.contains_workspace_id(workspace_id))
            {
                return Err(SurfaceValidationError::UnknownWorkspace);
            }

            match (surfaces[i].role, surfaces[i].application.is_some()) {
                (DesktopSurfaceRole::Application, false) => {
                    return Err(SurfaceValidationError::MissingApplicationDescriptors);
                }
                (DesktopSurfaceRole::Application, true) => {}
                (_, true) => {
                    return Err(SurfaceValidationError::UnexpectedApplicationDescriptors);
                }
                (_, false) => {}
            }

            if surfaces[i].focus == SurfaceFocus::Focused {
                focused_count += 1;
                if focused_count > 1 {
                    return Err(SurfaceValidationError::MultipleFocused);
                }
            }
        }

        Ok(Self { surfaces })
    }

    pub fn len(&self) -> usize {
        self.surfaces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SurfaceState> {
        self.surfaces.iter()
    }
}

impl PartialEq for ValidatedSurfaceCollection {
    fn eq(&self, other: &Self) -> bool {
        self.surfaces.len() == other.surfaces.len()
            && self.surfaces.iter().all(|a| {
                other
                    .surfaces
                    .iter()
                    .any(|b| a.surface_id == b.surface_id && a == b)
            })
    }
}

impl Eq for ValidatedSurfaceCollection {}
