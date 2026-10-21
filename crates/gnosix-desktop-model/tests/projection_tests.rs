use gnosix_desktop_model::{
    project_all_output_contexts, project_output_context, ApplicationSurfaceDescriptors,
    ConnectorName, DesktopContextBaseline, DesktopContextGeneration, DesktopSurfaceRole,
    LogicalPoint, LogicalRect, LogicalSize, OutputHubId, OutputState, OutputTransform,
    PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli, SurfaceFocus, SurfaceId,
    SurfaceState, SurfaceVisibility, WorkspaceFocus, WorkspaceId, WorkspaceName, WorkspaceState,
    WorkspaceVisibility,
};

fn output(id: u8, name: &str) -> OutputState {
    OutputState {
        output_hub_id: OutputHubId::from_bytes([id; 16]),
        connector: ConnectorName::new(name).unwrap(),
        descriptors: PhysicalOutputDescriptors::default(),
        physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
        logical_position: LogicalPoint::ORIGIN,
        logical_size: LogicalSize::new(1920, 1080).unwrap(),
        scale: ScaleMilli::from_milli(1000).unwrap(),
        transform: OutputTransform::Normal,
        active_workspace_id: None,
    }
}
fn workspace(id: u8, name: &str, output: Option<OutputHubId>) -> WorkspaceState {
    WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([id; 16]),
        name: WorkspaceName::new(name).unwrap(),
        output_hub_id: output,
        visibility: WorkspaceVisibility::Hidden,
        focus: WorkspaceFocus::Unfocused,
    }
}

fn app_surface(
    id: u8,
    output: Option<OutputHubId>,
    workspace: Option<WorkspaceId>,
) -> SurfaceState {
    SurfaceState {
        surface_id: SurfaceId::from_bytes([id; 16]),
        role: DesktopSurfaceRole::Application,
        output_hub_id: output,
        workspace_id: workspace,
        bounds: LogicalRect::new(LogicalPoint::ORIGIN, LogicalSize::new(640, 480).unwrap()),
        application: Some(ApplicationSurfaceDescriptors {
            class: format!("app-{id}"),
            initial_class: format!("app-{id}"),
            title: format!("title-{id}"),
            initial_title: format!("title-{id}"),
            tags: Vec::new(),
            process_id: u32::from(id),
            xwayland: false,
        }),
        visibility: SurfaceVisibility::Visible,
        focus: SurfaceFocus::Unfocused,
    }
}

fn baseline(
    outputs: Vec<OutputState>,
    workspaces: Vec<WorkspaceState>,
    surfaces: Vec<SurfaceState>,
) -> DesktopContextBaseline {
    let outputs = gnosix_desktop_model::ValidatedOutputCollection::from_outputs(outputs).unwrap();
    let workspaces =
        gnosix_desktop_model::ValidatedWorkspaceCollection::from_workspaces(workspaces, &outputs)
            .unwrap();
    let surfaces = gnosix_desktop_model::ValidatedSurfaceCollection::from_surfaces(
        surfaces,
        &outputs,
        &workspaces,
    )
    .unwrap();

    DesktopContextBaseline {
        generation: DesktopContextGeneration(7),
        outputs,
        workspaces,
        surfaces,
    }
}

#[test]
fn output_projection_contains_only_explicit_output_associations() {
    let out_a = output(1, "DP-1");
    let out_b = output(2, "HDMI-A-1");
    let ws_a = workspace(10, "2", Some(out_a.output_hub_id));
    let ws_b = workspace(11, "1", Some(out_b.output_hub_id));
    let detached_ws = workspace(12, "special:detached", None);
    let ws_a_id = ws_a.workspace_id;
    let view = project_output_context(
        &baseline(
            vec![out_b.clone(), out_a.clone()],
            vec![ws_b, detached_ws, ws_a],
            vec![
                app_surface(20, Some(out_b.output_hub_id), None),
                app_surface(21, None, Some(ws_a_id)),
                app_surface(22, Some(out_a.output_hub_id), Some(ws_a_id)),
            ],
        ),
        out_a.output_hub_id,
    )
    .unwrap();

    assert_eq!(view.generation, DesktopContextGeneration(7));
    assert_eq!(view.output, out_a);
    assert_eq!(view.workspaces.len(), 1);
    assert_eq!(view.workspaces[0].name.as_str(), "2");
    assert_eq!(view.surfaces.len(), 1);
    assert_eq!(view.surfaces[0].surface_id, SurfaceId::from_bytes([22; 16]));
}

#[test]
fn projection_preserves_observed_focus_and_visibility() {
    let out = output(1, "DP-1");
    let mut ws = workspace(10, "1", Some(out.output_hub_id));
    ws.visibility = WorkspaceVisibility::Visible;
    ws.focus = WorkspaceFocus::Focused;

    let mut surface = app_surface(20, Some(out.output_hub_id), Some(ws.workspace_id));
    surface.visibility = SurfaceVisibility::Hidden;
    surface.focus = SurfaceFocus::Focused;

    let view = project_output_context(
        &baseline(vec![out.clone()], vec![ws], vec![surface]),
        out.output_hub_id,
    )
    .unwrap();

    assert_eq!(view.workspaces[0].visibility, WorkspaceVisibility::Visible);
    assert_eq!(view.workspaces[0].focus, WorkspaceFocus::Focused);
    assert_eq!(view.surfaces[0].visibility, SurfaceVisibility::Hidden);
    assert_eq!(view.surfaces[0].focus, SurfaceFocus::Focused);
}

#[test]
fn missing_output_projection_is_absent() {
    let out = output(1, "DP-1");
    let baseline = baseline(vec![out], vec![], vec![]);
    assert!(project_output_context(&baseline, OutputHubId::from_bytes([9; 16])).is_none());
}

#[test]
fn projection_representation_is_stable_across_source_order() {
    let out_a = output(1, "DP-2");
    let out_b = output(2, "HDMI-A-1");
    let ws_a1 = workspace(10, "10", Some(out_a.output_hub_id));
    let ws_a2 = workspace(11, "2", Some(out_a.output_hub_id));
    let surface_a1 = app_surface(20, Some(out_a.output_hub_id), Some(ws_a1.workspace_id));
    let surface_a2 = app_surface(21, Some(out_a.output_hub_id), Some(ws_a2.workspace_id));

    let first = baseline(
        vec![out_a.clone(), out_b.clone()],
        vec![ws_a1.clone(), ws_a2.clone()],
        vec![surface_a1.clone(), surface_a2.clone()],
    );
    let second = baseline(
        vec![out_b, out_a],
        vec![ws_a2, ws_a1],
        vec![surface_a2, surface_a1],
    );

    assert_eq!(
        project_all_output_contexts(&first),
        project_all_output_contexts(&second)
    );
}
