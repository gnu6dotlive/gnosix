use gnosix_desktop_model::{
    ApplicationSurfaceDescriptors, ConnectorName, DesktopSurfaceRole, LogicalPoint, LogicalRect,
    LogicalSize, OutputHubId, OutputState, OutputTransform, PhysicalOutputDescriptors,
    PhysicalPixelSize, ScaleMilli, SurfaceFocus, SurfaceId, SurfaceState, SurfaceValidationError,
    SurfaceVisibility, ValidatedOutputCollection, ValidatedSurfaceCollection,
    ValidatedWorkspaceCollection, WorkspaceFocus, WorkspaceId, WorkspaceName, WorkspaceState,
    WorkspaceValidationError, WorkspaceVisibility,
};

fn output(id: u8) -> OutputState {
    OutputState {
        output_hub_id: OutputHubId::from_bytes([id; 16]),
        connector: ConnectorName::new(format!("OUT-{id}")).unwrap(),
        descriptors: PhysicalOutputDescriptors::default(),
        physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
        logical_position: LogicalPoint::new(i32::from(id) * 100, 0),
        logical_size: LogicalSize::new(1920, 1080).unwrap(),
        scale: ScaleMilli::ONE,
        transform: OutputTransform::Normal,
        active_workspace_id: None,
    }
}

fn application_descriptors(id: u8) -> ApplicationSurfaceDescriptors {
    ApplicationSurfaceDescriptors {
        class: format!("org.example.App{id}"),
        initial_class: format!("org.example.App{id}"),
        title: format!("Window {id}"),
        initial_title: format!("Window {id}"),
        tags: Vec::new(),
        process_id: u32::from(id) + 100,
        xwayland: false,
    }
}

fn surface(
    id: u8,
    workspace_id: Option<WorkspaceId>,
    output_hub_id: Option<OutputHubId>,
) -> SurfaceState {
    SurfaceState {
        surface_id: SurfaceId::from_bytes([id; 16]),
        role: DesktopSurfaceRole::Application,
        output_hub_id,
        workspace_id,
        bounds: LogicalRect::new(
            LogicalPoint::new(10, 20),
            LogicalSize::new(800, 600).unwrap(),
        ),
        application: Some(application_descriptors(id)),
        visibility: SurfaceVisibility::Visible,
        focus: SurfaceFocus::Unfocused,
    }
}

#[test]
fn workspace_collection_validates_identity_and_output_references() {
    let outputs = ValidatedOutputCollection::from_outputs(vec![output(1)]).unwrap();
    let hub = OutputHubId::from_bytes([1; 16]);
    let workspace_id = WorkspaceId::from_bytes([7; 16]);

    let valid = ValidatedWorkspaceCollection::from_workspaces(
        vec![WorkspaceState {
            workspace_id,
            name: WorkspaceName::new("test-workspace").unwrap(),
            output_hub_id: Some(hub),
            visibility: WorkspaceVisibility::Visible,
            focus: WorkspaceFocus::Unfocused,
        }],
        &outputs,
    )
    .unwrap();
    assert_eq!(valid.len(), 1);
    assert!(valid.contains_workspace_id(workspace_id));

    let duplicate = WorkspaceState {
        workspace_id,
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: Some(hub),
        visibility: WorkspaceVisibility::Hidden,
        focus: WorkspaceFocus::Unfocused,
    };
    assert_eq!(
        ValidatedWorkspaceCollection::from_workspaces(
            vec![
                WorkspaceState {
                    workspace_id,
                    name: WorkspaceName::new("test-workspace").unwrap(),
                    output_hub_id: Some(hub),
                    visibility: WorkspaceVisibility::Visible,
                    focus: WorkspaceFocus::Unfocused,
                },
                duplicate,
            ],
            &outputs,
        ),
        Err(WorkspaceValidationError::DuplicateWorkspaceId)
    );

    let unknown_output = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([8; 16]),
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: Some(OutputHubId::from_bytes([9; 16])),
        visibility: WorkspaceVisibility::Hidden,
        focus: WorkspaceFocus::Unfocused,
    };
    assert_eq!(
        ValidatedWorkspaceCollection::from_workspaces(vec![unknown_output], &outputs),
        Err(WorkspaceValidationError::UnknownOutput)
    );
}

#[test]
fn workspace_collection_equality_is_unordered() {
    let outputs = ValidatedOutputCollection::from_outputs(vec![output(1)]).unwrap();
    let hub = OutputHubId::from_bytes([1; 16]);
    let a = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([1; 16]),
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: Some(hub),
        visibility: WorkspaceVisibility::Visible,
        focus: WorkspaceFocus::Unfocused,
    };
    let b = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([2; 16]),
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: None,
        visibility: WorkspaceVisibility::Hidden,
        focus: WorkspaceFocus::Unfocused,
    };

    let ab = ValidatedWorkspaceCollection::from_workspaces(vec![a.clone(), b.clone()], &outputs)
        .unwrap();
    let ba = ValidatedWorkspaceCollection::from_workspaces(vec![b, a], &outputs).unwrap();
    assert_eq!(ab, ba);
}

#[test]
fn surface_collection_validates_identity_and_cross_references() {
    let hub = OutputHubId::from_bytes([1; 16]);
    let outputs = ValidatedOutputCollection::from_outputs(vec![output(1)]).unwrap();
    let workspace_id = WorkspaceId::from_bytes([7; 16]);
    let workspaces = ValidatedWorkspaceCollection::from_workspaces(
        vec![WorkspaceState {
            workspace_id,
            name: WorkspaceName::new("test-workspace").unwrap(),
            output_hub_id: Some(hub),
            visibility: WorkspaceVisibility::Visible,
            focus: WorkspaceFocus::Unfocused,
        }],
        &outputs,
    )
    .unwrap();

    let good = surface(1, Some(workspace_id), Some(hub));
    let collection =
        ValidatedSurfaceCollection::from_surfaces(vec![good.clone()], &outputs, &workspaces)
            .unwrap();
    assert_eq!(collection.len(), 1);

    assert_eq!(
        ValidatedSurfaceCollection::from_surfaces(
            vec![good.clone(), good.clone()],
            &outputs,
            &workspaces,
        ),
        Err(SurfaceValidationError::DuplicateSurfaceId)
    );

    let bad_output = surface(
        2,
        Some(workspace_id),
        Some(OutputHubId::from_bytes([99; 16])),
    );
    assert_eq!(
        ValidatedSurfaceCollection::from_surfaces(vec![bad_output], &outputs, &workspaces),
        Err(SurfaceValidationError::UnknownOutput)
    );

    let bad_workspace = surface(3, Some(WorkspaceId::from_bytes([99; 16])), Some(hub));
    assert_eq!(
        ValidatedSurfaceCollection::from_surfaces(vec![bad_workspace], &outputs, &workspaces),
        Err(SurfaceValidationError::UnknownWorkspace)
    );
}

#[test]
fn focused_workspace_and_surface_are_unique_in_a_candidate() {
    let hub = OutputHubId::from_bytes([1; 16]);
    let outputs = ValidatedOutputCollection::from_outputs(vec![output(1)]).unwrap();

    let focused_a = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([1; 16]),
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: Some(hub),
        visibility: WorkspaceVisibility::Visible,
        focus: WorkspaceFocus::Focused,
    };
    let focused_b = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([2; 16]),
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: Some(hub),
        visibility: WorkspaceVisibility::Visible,
        focus: WorkspaceFocus::Focused,
    };
    assert_eq!(
        ValidatedWorkspaceCollection::from_workspaces(vec![focused_a.clone(), focused_b], &outputs,),
        Err(WorkspaceValidationError::MultipleFocused)
    );

    let workspaces =
        ValidatedWorkspaceCollection::from_workspaces(vec![focused_a.clone()], &outputs).unwrap();
    let mut surface_a = surface(1, Some(focused_a.workspace_id), Some(hub));
    surface_a.focus = SurfaceFocus::Focused;
    let mut surface_b = surface(2, Some(focused_a.workspace_id), Some(hub));
    surface_b.focus = SurfaceFocus::Focused;
    assert_eq!(
        ValidatedSurfaceCollection::from_surfaces(
            vec![surface_a, surface_b],
            &outputs,
            &workspaces,
        ),
        Err(SurfaceValidationError::MultipleFocused)
    );
}

#[test]
fn application_descriptors_match_surface_role() {
    let outputs = ValidatedOutputCollection::from_outputs(vec![output(1)]).unwrap();
    let workspaces = ValidatedWorkspaceCollection::from_workspaces(vec![], &outputs).unwrap();
    let hub = OutputHubId::from_bytes([1; 16]);

    let mut missing = surface(1, None, Some(hub));
    missing.application = None;
    assert_eq!(
        ValidatedSurfaceCollection::from_surfaces(vec![missing], &outputs, &workspaces),
        Err(SurfaceValidationError::MissingApplicationDescriptors)
    );

    let mut unexpected = surface(2, None, Some(hub));
    unexpected.role = DesktopSurfaceRole::Shell;
    assert_eq!(
        ValidatedSurfaceCollection::from_surfaces(vec![unexpected], &outputs, &workspaces),
        Err(SurfaceValidationError::UnexpectedApplicationDescriptors)
    );
}

#[test]
fn surface_collection_equality_is_unordered_and_role_sensitive() {
    let outputs = ValidatedOutputCollection::from_outputs(vec![output(1)]).unwrap();
    let workspaces = ValidatedWorkspaceCollection::from_workspaces(vec![], &outputs).unwrap();

    let mut a = surface(1, None, Some(OutputHubId::from_bytes([1; 16])));
    a.role = DesktopSurfaceRole::Shell;
    a.application = None;
    let mut b = surface(2, None, Some(OutputHubId::from_bytes([1; 16])));
    b.role = DesktopSurfaceRole::RearPlane;
    b.application = None;

    let ab = ValidatedSurfaceCollection::from_surfaces(
        vec![a.clone(), b.clone()],
        &outputs,
        &workspaces,
    )
    .unwrap();
    let ba = ValidatedSurfaceCollection::from_surfaces(
        vec![b.clone(), a.clone()],
        &outputs,
        &workspaces,
    )
    .unwrap();
    assert_eq!(ab, ba);

    b.role = DesktopSurfaceRole::Overlay;
    let changed =
        ValidatedSurfaceCollection::from_surfaces(vec![a, b], &outputs, &workspaces).unwrap();
    assert_ne!(ab, changed);
}

#[test]
fn output_active_workspace_must_be_visible_and_owned_by_that_output() {
    let mut first = output(1);
    let second = output(2);
    first.active_workspace_id = Some(WorkspaceId::from_bytes([11; 16]));
    let outputs = ValidatedOutputCollection::from_outputs(vec![first.clone(), second]).unwrap();
    let active = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([11; 16]),
        name: WorkspaceName::new("11").unwrap(),
        output_hub_id: Some(first.output_hub_id),
        visibility: WorkspaceVisibility::Visible,
        focus: WorkspaceFocus::Unfocused,
    };
    assert!(ValidatedWorkspaceCollection::from_workspaces(vec![active.clone()], &outputs).is_ok());
    let mut wrong_output = active.clone();
    wrong_output.output_hub_id = Some(OutputHubId::from_bytes([2; 16]));
    assert!(matches!(
        ValidatedWorkspaceCollection::from_workspaces(vec![wrong_output], &outputs),
        Err(WorkspaceValidationError::InvalidOutputActiveWorkspace)
    ));
    let mut hidden = active;
    hidden.visibility = WorkspaceVisibility::Hidden;
    assert!(matches!(
        ValidatedWorkspaceCollection::from_workspaces(vec![hidden], &outputs),
        Err(WorkspaceValidationError::InvalidOutputActiveWorkspace)
    ));
}
