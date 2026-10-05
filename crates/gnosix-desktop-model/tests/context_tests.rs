use gnosix_desktop_model::{
    ConnectorName, DesktopContextBaseline, DesktopContextCandidate, DesktopContextGeneration,
    LogicalPoint, LogicalSize, OutputHubId, OutputState, OutputTransform,
    PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli, SurfaceState,
    ValidatedOutputCollection, ValidatedSurfaceCollection, ValidatedWorkspaceCollection,
    WorkspaceState,
};

fn sample_output(id_byte: u8, connector_name: &str) -> OutputState {
    OutputState {
        output_hub_id: OutputHubId::from_bytes([id_byte; 16]),
        connector: ConnectorName::new(connector_name).unwrap(),
        descriptors: PhysicalOutputDescriptors {
            make: Some("Vendor".to_string()),
            model: Some("Display".to_string()),
            serial: Some(format!("SN-{id_byte}")),
        },
        physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
        logical_position: LogicalPoint::ORIGIN,
        logical_size: LogicalSize::new(1920, 1080).unwrap(),
        scale: ScaleMilli::from_milli(1000).unwrap(),
        transform: OutputTransform::Normal,
        active_workspace_id: None,
    }
}

fn baseline(
    outputs: Vec<OutputState>,
    generation: DesktopContextGeneration,
) -> DesktopContextBaseline {
    let outputs = ValidatedOutputCollection::from_outputs(outputs).unwrap();
    let workspaces = ValidatedWorkspaceCollection::from_workspaces(vec![], &outputs).unwrap();
    let surfaces =
        ValidatedSurfaceCollection::from_surfaces(vec![], &outputs, &workspaces).unwrap();

    DesktopContextBaseline {
        generation,
        outputs,
        workspaces,
        surfaces,
    }
}

#[test]
fn test_baseline_empty_and_non_empty() {
    let empty = baseline(vec![], DesktopContextGeneration::INITIAL);
    assert!(empty.outputs.is_empty());
    assert!(empty.workspaces.is_empty());
    assert!(empty.surfaces.is_empty());

    let non_empty = baseline(
        vec![sample_output(1, "DP-1")],
        DesktopContextGeneration::INITIAL,
    );
    assert_eq!(non_empty.outputs.len(), 1);
    assert!(non_empty.workspaces.is_empty());
    assert!(non_empty.surfaces.is_empty());
}

#[test]
fn test_baseline_equality_semantics() {
    let out_a = sample_output(1, "DP-1");
    let out_b = sample_output(2, "HDMI-A-1");

    let base1 = baseline(
        vec![out_a.clone(), out_b.clone()],
        DesktopContextGeneration::INITIAL,
    );
    let reordered = baseline(
        vec![out_b.clone(), out_a.clone()],
        DesktopContextGeneration::INITIAL,
    );
    let next_generation = baseline(vec![out_a, out_b], DesktopContextGeneration(1));

    assert_eq!(base1, reordered);
    assert_ne!(base1, next_generation);
}

#[test]
fn test_candidate_preserves_raw_vectors() {
    let out_a = sample_output(1, "DP-1");
    let out_b = sample_output(2, "HDMI-A-1");

    let empty = DesktopContextCandidate {
        outputs: vec![],
        workspaces: Vec::<WorkspaceState>::new(),
        surfaces: Vec::<SurfaceState>::new(),
    };
    assert!(empty.outputs.is_empty());
    assert!(empty.workspaces.is_empty());
    assert!(empty.surfaces.is_empty());

    let duplicate = DesktopContextCandidate {
        outputs: vec![out_a.clone(), out_a.clone(), out_b.clone()],
        workspaces: Vec::<WorkspaceState>::new(),
        surfaces: Vec::<SurfaceState>::new(),
    };
    assert_eq!(duplicate.outputs.len(), 3);
    assert_eq!(duplicate.outputs[0], out_a);
    assert_eq!(duplicate.outputs[1], out_a);
    assert_eq!(duplicate.outputs[2], out_b);
}
