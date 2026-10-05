use gnosix_desktop_model::{
    ConnectorName, LogicalPoint, LogicalSize, OutputHubId, OutputState, OutputTransform,
    PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli, ValidatedOutputCollection,
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

#[test]
fn test_collection_empty_and_singleton() {
    // 1. Empty vector is a valid collection (0 outputs)
    let empty = ValidatedOutputCollection::from_outputs(Vec::new()).expect("empty vec is valid");
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.iter().count(), 0);

    // 2. Singleton is a valid collection
    let single_out = sample_output(1, "DP-1");
    let single = ValidatedOutputCollection::from_outputs(vec![single_out.clone()])
        .expect("singleton is valid");
    assert!(!single.is_empty());
    assert_eq!(single.len(), 1);
    let items: Vec<&OutputState> = single.iter().collect();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], &single_out);
}

#[test]
fn test_collection_duplicate_rejection() {
    let out_a = sample_output(1, "DP-1");
    let out_b = sample_output(2, "HDMI-A-1");

    // Identical duplicate: same OutputHubId and same attributes
    assert!(
        ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_a.clone()]).is_none(),
        "identical duplicate hub must be rejected"
    );

    // Divergent duplicate: same OutputHubId, divergent attributes
    let mut out_a_divergent = out_a.clone();
    out_a_divergent.connector = ConnectorName::new("DP-2").unwrap();
    assert!(
        ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_a_divergent]).is_none(),
        "divergent duplicate hub must be rejected"
    );

    // Duplicate embedded in multi-element input
    assert!(
        ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_b.clone(), out_a.clone()])
            .is_none(),
        "duplicate embedded in list must be rejected"
    );

    // Multiple distinct hubs succeed
    let distinct = ValidatedOutputCollection::from_outputs(vec![
        sample_output(1, "DP-1"),
        sample_output(2, "DP-2"),
        sample_output(3, "HDMI-A-1"),
    ])
    .expect("distinct hubs must be accepted");
    assert_eq!(distinct.len(), 3);
}

#[test]
fn test_collection_preservation_and_iteration() {
    let out_a = sample_output(10, "DP-1");
    let out_b = sample_output(20, "DP-2");
    let col = ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_b.clone()])
        .expect("valid distinct collection");

    assert_eq!(col.len(), 2);
    let items: Vec<&OutputState> = col.iter().collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], &out_a);
    assert_eq!(items[1], &out_b);
}

#[test]
fn test_collection_unordered_equality() {
    let out_a = sample_output(1, "DP-1");
    let out_b = sample_output(2, "DP-2");
    let out_c = sample_output(3, "HDMI-A-1");

    let col_ab1 = ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_b.clone()])
        .expect("valid AB");
    let col_ab2 = ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_b.clone()])
        .expect("valid AB");
    let col_ba = ValidatedOutputCollection::from_outputs(vec![out_b.clone(), out_a.clone()])
        .expect("valid BA");

    // Reflexivity
    assert_eq!(col_ab1, col_ab1);

    // Sequence equality: [A, B] == [A, B]
    assert_eq!(col_ab1, col_ab2);

    // Order-independent equality: [A, B] == [B, A]
    assert_eq!(col_ab1, col_ba);
    assert_eq!(col_ba, col_ab1);

    // Cardinality delta: [A, B] != [A]
    let col_a =
        ValidatedOutputCollection::from_outputs(vec![out_a.clone()]).expect("valid singleton");
    assert_ne!(col_ab1, col_a);
    assert_ne!(col_a, col_ab1);

    // Membership delta: [A, B] != [A, C]
    let col_ac = ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_c.clone()])
        .expect("valid AC");
    assert_ne!(col_ab1, col_ac);

    // Attribute delta on same hub: [A, B] != [A, B_modified]
    let mut out_b_modified = out_b.clone();
    out_b_modified.scale = ScaleMilli::from_milli(1500).unwrap();
    let col_ab_mod = ValidatedOutputCollection::from_outputs(vec![out_a.clone(), out_b_modified])
        .expect("valid AB_mod");
    assert_ne!(col_ab1, col_ab_mod);

    // Empty collections equal each other
    let empty1 = ValidatedOutputCollection::from_outputs(Vec::new()).unwrap();
    let empty2 = ValidatedOutputCollection::from_outputs(Vec::new()).unwrap();
    assert_eq!(empty1, empty2);
    assert_ne!(empty1, col_a);
}
