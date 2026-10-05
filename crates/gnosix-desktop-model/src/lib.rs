#![deny(dead_code)]
#![deny(unused_variables)]
#![deny(unused_imports)]
#![deny(unused_must_use)]

pub mod context;
pub mod generation;
pub mod output;
pub mod projection;
pub mod scale;
pub mod spatial;
pub mod surface;
pub mod transition;
pub mod workspace;

pub use context::{DesktopContextBaseline, DesktopContextCandidate, DesktopContextValidationError};
pub use generation::{DesktopContextGeneration, GenerationExhausted};
pub use output::{
    ConnectorName, EmptyConnectorNameError, LogicalPoint, LogicalSize, OutputHubId, OutputState,
    OutputTransform, PhysicalOutputDescriptors, PhysicalPixelSize, ValidatedOutputCollection,
};
pub use projection::{
    project_all_output_contexts, project_output_context, DesktopOutputProjection,
};
pub use scale::{ScaleError, ScaleMilli};
pub use spatial::{classify_spatial_relation, LogicalRect, SpatialRelation};
pub use surface::{
    ApplicationSurfaceDescriptors, DesktopSurfaceRole, SurfaceFocus, SurfaceId, SurfaceState,
    SurfaceValidationError, SurfaceVisibility, ValidatedSurfaceCollection,
};
pub use transition::{reconcile_context, TransitionOutcome};
pub use workspace::{
    EmptyWorkspaceNameError, ValidatedWorkspaceCollection, WorkspaceFocus, WorkspaceId,
    WorkspaceName, WorkspaceState, WorkspaceValidationError, WorkspaceVisibility,
};
