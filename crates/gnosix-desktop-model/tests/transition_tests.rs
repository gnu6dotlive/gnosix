use gnosix_desktop_model::{
    ApplicationSurfaceDescriptors, ConnectorName, DesktopContextBaseline, DesktopContextCandidate,
    DesktopContextGeneration, DesktopContextValidationError, DesktopSurfaceRole, LogicalPoint,
    LogicalRect, LogicalSize, OutputHubId, OutputState, OutputTransform, PhysicalOutputDescriptors,
    PhysicalPixelSize, ScaleMilli, SurfaceFocus, SurfaceId, SurfaceState, SurfaceValidationError,
    SurfaceVisibility, TransitionOutcome, ValidatedOutputCollection, ValidatedSurfaceCollection,
    ValidatedWorkspaceCollection, WorkspaceFocus, WorkspaceId, WorkspaceName, WorkspaceState,
    WorkspaceValidationError, WorkspaceVisibility,
};

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

fn sample_output(id_byte: u8, connector: &str) -> OutputState {
    OutputState {
        output_hub_id: OutputHubId::from_bytes([id_byte; 16]),
        connector: ConnectorName::new(connector).unwrap(),
        descriptors: PhysicalOutputDescriptors {
            make: Some("Vendor".to_string()),
            model: Some("Display".to_string()),
            serial: Some(format!("SN-{id_byte}")),
        },
        physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
        logical_position: LogicalPoint::new(i32::from(id_byte) * 100, 0),
        logical_size: LogicalSize::new(1920, 1080).unwrap(),
        scale: ScaleMilli::from_milli(1000).unwrap(),
        transform: OutputTransform::Normal,
        active_workspace_id: None,
    }
}

fn baseline_from_outputs(outputs: Vec<OutputState>) -> DesktopContextBaseline {
    let outputs = ValidatedOutputCollection::from_outputs(outputs).unwrap();
    let workspaces = ValidatedWorkspaceCollection::from_workspaces(vec![], &outputs).unwrap();
    let surfaces =
        ValidatedSurfaceCollection::from_surfaces(vec![], &outputs, &workspaces).unwrap();

    DesktopContextBaseline {
        generation: DesktopContextGeneration::INITIAL,
        outputs,
        workspaces,
        surfaces,
    }
}

fn empty_candidate(outputs: Vec<OutputState>) -> DesktopContextCandidate {
    DesktopContextCandidate {
        outputs,
        workspaces: Vec::<WorkspaceState>::new(),
        surfaces: Vec::<SurfaceState>::new(),
    }
}

#[test]
fn test_transition_outcome_variants_and_equality() {
    let base1 = baseline_from_outputs(vec![sample_output(1, "DP-1")]);
    let base2 = baseline_from_outputs(vec![sample_output(1, "DP-1")]);

    let outcome_no_change = TransitionOutcome::NoChange;
    let outcome_invalid =
        TransitionOutcome::InvalidCandidate(DesktopContextValidationError::DuplicateOutputHubId);
    let outcome_exhausted = TransitionOutcome::GenerationExhausted;
    let outcome_changed1 = TransitionOutcome::Changed(base1);
    let outcome_changed2 = TransitionOutcome::Changed(base2);

    assert_eq!(outcome_no_change, TransitionOutcome::NoChange);
    assert_eq!(
        outcome_invalid,
        TransitionOutcome::InvalidCandidate(DesktopContextValidationError::DuplicateOutputHubId)
    );
    assert_eq!(outcome_exhausted, TransitionOutcome::GenerationExhausted);
    assert_eq!(outcome_changed1, outcome_changed2);

    assert_ne!(outcome_no_change, outcome_invalid);
    assert_ne!(outcome_no_change, outcome_exhausted);
    assert_ne!(outcome_no_change, outcome_changed1);
}

#[test]
fn test_reconcile_context_invalid_duplicate_output() {
    let baseline = baseline_from_outputs(vec![sample_output(1, "DP-1")]);
    let out = sample_output(1, "DP-1");

    let outcome =
        gnosix_desktop_model::reconcile_context(&baseline, empty_candidate(vec![out.clone(), out]));

    assert_eq!(
        outcome,
        TransitionOutcome::InvalidCandidate(DesktopContextValidationError::DuplicateOutputHubId)
    );
}

#[test]
fn test_reconcile_context_no_change_with_reordered_outputs() {
    let out1 = sample_output(1, "DP-1");
    let out2 = sample_output(2, "HDMI-A-1");
    let baseline = baseline_from_outputs(vec![out1.clone(), out2.clone()]);

    let outcome =
        gnosix_desktop_model::reconcile_context(&baseline, empty_candidate(vec![out2, out1]));

    assert_eq!(outcome, TransitionOutcome::NoChange);
    assert_eq!(baseline.generation, DesktopContextGeneration::INITIAL);
}

#[test]
fn test_reconcile_context_changed_output_delta() {
    let baseline = baseline_from_outputs(vec![sample_output(1, "DP-1")]);
    let mut changed = sample_output(1, "DP-1");
    changed.scale = ScaleMilli::from_milli(1250).unwrap();

    let outcome =
        gnosix_desktop_model::reconcile_context(&baseline, empty_candidate(vec![changed]));

    match outcome {
        TransitionOutcome::Changed(next) => {
            assert_eq!(next.generation, DesktopContextGeneration(1));
            assert_eq!(next.outputs.len(), 1);
            assert!(next.workspaces.is_empty());
            assert!(next.surfaces.is_empty());
        }
        other => panic!("expected Changed, got {other:?}"),
    }
}

#[test]
fn test_reconcile_context_generation_exhausted() {
    let mut baseline = baseline_from_outputs(vec![sample_output(1, "DP-1")]);
    baseline.generation = DesktopContextGeneration(u64::MAX);

    let mut changed = sample_output(1, "DP-1");
    changed.logical_position = LogicalPoint::new(77, -22);

    let outcome =
        gnosix_desktop_model::reconcile_context(&baseline, empty_candidate(vec![changed]));
    assert_eq!(outcome, TransitionOutcome::GenerationExhausted);
}

#[test]
fn test_reconcile_context_changed_workspace_delta() {
    let output = sample_output(1, "DP-1");
    let hub = output.output_hub_id;
    let baseline = baseline_from_outputs(vec![output.clone()]);
    let workspace = WorkspaceState {
        workspace_id: WorkspaceId::from_bytes([7; 16]),
        name: WorkspaceName::new("test-workspace").unwrap(),
        output_hub_id: Some(hub),
        visibility: WorkspaceVisibility::Visible,
        focus: WorkspaceFocus::Unfocused,
    };

    let candidate = DesktopContextCandidate {
        outputs: vec![output],
        workspaces: vec![workspace],
        surfaces: vec![],
    };

    let outcome = gnosix_desktop_model::reconcile_context(&baseline, candidate);
    match outcome {
        TransitionOutcome::Changed(next) => {
            assert_eq!(next.generation, DesktopContextGeneration(1));
            assert_eq!(next.workspaces.len(), 1);
            assert!(next.surfaces.is_empty());
        }
        other => panic!("expected Changed, got {other:?}"),
    }
}

#[test]
fn test_reconcile_context_changed_surface_delta() {
    let output = sample_output(1, "DP-1");
    let hub = output.output_hub_id;
    let workspace_id = WorkspaceId::from_bytes([7; 16]);
    let outputs = ValidatedOutputCollection::from_outputs(vec![output.clone()]).unwrap();
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
    let surfaces =
        ValidatedSurfaceCollection::from_surfaces(vec![], &outputs, &workspaces).unwrap();
    let baseline = DesktopContextBaseline {
        generation: DesktopContextGeneration::INITIAL,
        outputs,
        workspaces,
        surfaces,
    };

    let candidate = DesktopContextCandidate {
        outputs: vec![output],
        workspaces: vec![WorkspaceState {
            workspace_id,
            name: WorkspaceName::new("test-workspace").unwrap(),
            output_hub_id: Some(hub),
            visibility: WorkspaceVisibility::Visible,
            focus: WorkspaceFocus::Unfocused,
        }],
        surfaces: vec![SurfaceState {
            surface_id: SurfaceId::from_bytes([8; 16]),
            role: DesktopSurfaceRole::Application,
            output_hub_id: Some(hub),
            workspace_id: Some(workspace_id),
            bounds: LogicalRect::new(
                LogicalPoint::new(20, 30),
                LogicalSize::new(640, 480).unwrap(),
            ),
            application: Some(application_descriptors(8)),
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Unfocused,
        }],
    };

    let outcome = gnosix_desktop_model::reconcile_context(&baseline, candidate);
    match outcome {
        TransitionOutcome::Changed(next) => {
            assert_eq!(next.generation, DesktopContextGeneration(1));
            assert_eq!(next.surfaces.len(), 1);
        }
        other => panic!("expected Changed, got {other:?}"),
    }
}

#[test]
fn test_reconcile_context_reports_cross_reference_errors() {
    let output = sample_output(1, "DP-1");
    let baseline = baseline_from_outputs(vec![output.clone()]);

    let bad_workspace = DesktopContextCandidate {
        outputs: vec![output.clone()],
        workspaces: vec![WorkspaceState {
            workspace_id: WorkspaceId::from_bytes([7; 16]),
            name: WorkspaceName::new("test-workspace").unwrap(),
            output_hub_id: Some(OutputHubId::from_bytes([99; 16])),
            visibility: WorkspaceVisibility::Visible,
            focus: WorkspaceFocus::Unfocused,
        }],
        surfaces: vec![],
    };
    assert_eq!(
        gnosix_desktop_model::reconcile_context(&baseline, bad_workspace),
        TransitionOutcome::InvalidCandidate(DesktopContextValidationError::Workspace(
            WorkspaceValidationError::UnknownOutput
        ))
    );

    let bad_surface = DesktopContextCandidate {
        outputs: vec![output],
        workspaces: vec![],
        surfaces: vec![SurfaceState {
            surface_id: SurfaceId::from_bytes([8; 16]),
            role: DesktopSurfaceRole::Application,
            output_hub_id: Some(OutputHubId::from_bytes([1; 16])),
            workspace_id: Some(WorkspaceId::from_bytes([88; 16])),
            bounds: LogicalRect::new(LogicalPoint::ORIGIN, LogicalSize::new(320, 200).unwrap()),
            application: Some(application_descriptors(8)),
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Unfocused,
        }],
    };
    assert_eq!(
        gnosix_desktop_model::reconcile_context(&baseline, bad_surface),
        TransitionOutcome::InvalidCandidate(DesktopContextValidationError::Surface(
            SurfaceValidationError::UnknownWorkspace
        ))
    );
}
