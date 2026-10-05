use crate::{
    DesktopContextBaseline, DesktopContextGeneration, OutputHubId, OutputState, SurfaceState,
    WorkspaceState,
};

/// Read-only semantic projection of one explicitly associated current output.
///
/// Ordering inside workspaces and surfaces is deterministic representation
/// only. It carries no workspace ordering, z-order, priority or preference
/// semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopOutputProjection {
    pub generation: DesktopContextGeneration,
    pub output: OutputState,
    pub workspaces: Vec<WorkspaceState>,
    pub surfaces: Vec<SurfaceState>,
}

/// Projects one output from a validated desktop semantic baseline.
///
/// Only explicit output_hub_id associations are included. No association is
/// inferred through workspace membership, geometry, titles, classes or names.
pub fn project_output_context(
    baseline: &DesktopContextBaseline,
    output_hub_id: OutputHubId,
) -> Option<DesktopOutputProjection> {
    let output = baseline
        .outputs
        .iter()
        .find(|output| output.output_hub_id == output_hub_id)?
        .clone();

    let mut workspaces = baseline
        .workspaces
        .iter()
        .filter(|workspace| workspace.output_hub_id == Some(output_hub_id))
        .cloned()
        .collect::<Vec<_>>();
    workspaces.sort_by(|left, right| {
        left.name.cmp(&right.name).then_with(|| {
            left.workspace_id
                .as_bytes()
                .cmp(right.workspace_id.as_bytes())
        })
    });

    let mut surfaces = baseline
        .surfaces
        .iter()
        .filter(|surface| surface.output_hub_id == Some(output_hub_id))
        .cloned()
        .collect::<Vec<_>>();
    surfaces.sort_by(|left, right| left.surface_id.as_bytes().cmp(right.surface_id.as_bytes()));

    Some(DesktopOutputProjection {
        generation: baseline.generation,
        output,
        workspaces,
        surfaces,
    })
}
/// Projects every current output using deterministic representation ordering.
///
/// Output ordering is not semantic and MUST NOT be interpreted as primary
/// status or user preference.
pub fn project_all_output_contexts(
    baseline: &DesktopContextBaseline,
) -> Vec<DesktopOutputProjection> {
    let mut output_ids = baseline
        .outputs
        .iter()
        .map(|output| {
            (
                output.connector.as_str().to_owned(),
                *output.output_hub_id.as_bytes(),
                output.output_hub_id,
            )
        })
        .collect::<Vec<_>>();

    output_ids.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    output_ids
        .into_iter()
        .filter_map(|(_, _, output_hub_id)| project_output_context(baseline, output_hub_id))
        .collect()
}
