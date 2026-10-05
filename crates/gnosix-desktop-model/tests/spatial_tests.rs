use gnosix_desktop_model::{
    classify_spatial_relation, ConnectorName, LogicalPoint, LogicalRect, LogicalSize, OutputHubId,
    OutputState, OutputTransform, PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli,
    SpatialRelation,
};

fn rect(x: i32, y: i32, width: u32, height: u32) -> LogicalRect {
    LogicalRect::new(
        LogicalPoint::new(x, y),
        LogicalSize::new(width, height).unwrap(),
    )
}

#[test]
fn spatial_relations_cover_equal_containment_and_overlap() {
    let outer = rect(-100, 50, 500, 400);
    let same = rect(-100, 50, 500, 400);
    let inner = rect(0, 100, 100, 100);
    let overlap = rect(350, 300, 200, 200);

    assert_eq!(
        classify_spatial_relation(outer, same),
        SpatialRelation::Equal
    );
    assert_eq!(
        classify_spatial_relation(outer, inner),
        SpatialRelation::Contains
    );
    assert_eq!(
        classify_spatial_relation(inner, outer),
        SpatialRelation::ContainedBy
    );
    assert_eq!(
        classify_spatial_relation(outer, overlap),
        SpatialRelation::Overlaps
    );
}

#[test]
fn spatial_relations_distinguish_touching_and_disjoint() {
    let a = rect(0, 0, 100, 100);
    let edge_touch = rect(100, 10, 50, 50);
    let corner_touch = rect(100, 100, 20, 20);
    let disjoint = rect(121, 121, 20, 20);

    assert_eq!(
        classify_spatial_relation(a, edge_touch),
        SpatialRelation::Touches
    );
    assert_eq!(
        classify_spatial_relation(a, corner_touch),
        SpatialRelation::Touches
    );
    assert_eq!(
        classify_spatial_relation(a, disjoint),
        SpatialRelation::Disjoint
    );
}

#[test]
fn output_logical_bounds_preserve_negative_layout_coordinates() {
    let state = OutputState {
        output_hub_id: OutputHubId::from_bytes([1; 16]),
        connector: ConnectorName::new("HDMI-A-1").unwrap(),
        descriptors: PhysicalOutputDescriptors::default(),
        physical_size: PhysicalPixelSize::new(2560, 1440).unwrap(),
        logical_position: LogicalPoint::new(-630, 590),
        logical_size: LogicalSize::new(2560, 1440).unwrap(),
        scale: ScaleMilli::ONE,
        transform: OutputTransform::Normal,
        active_workspace_id: None,
    };

    assert_eq!(state.logical_bounds(), rect(-630, 590, 2560, 1440));
}
