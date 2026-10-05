use gnosix_desktop_model::{
    ConnectorName, DesktopContextGeneration, EmptyConnectorNameError, GenerationExhausted,
    LogicalPoint, LogicalSize, OutputTransform, PhysicalPixelSize, ScaleError, ScaleMilli,
};
use std::collections::BTreeSet;
use std::error::Error;

#[test]
fn test_scale_milli_invariants() {
    assert_eq!(ScaleMilli::ONE.as_milli(), 1000);
    assert_eq!(ScaleMilli::from_milli(0), Err(ScaleError::Zero));

    let one = ScaleMilli::from_milli(1000).expect("valid scale milli");
    assert_eq!(one, ScaleMilli::ONE);

    let quarter = ScaleMilli::from_milli(1250).expect("valid scale milli");
    assert_eq!(quarter.as_milli(), 1250);

    assert!(one < quarter);

    let mut set = BTreeSet::new();
    set.insert(quarter);
    set.insert(one);
    let ordered: Vec<ScaleMilli> = set.into_iter().collect();
    assert_eq!(ordered, vec![one, quarter]);
}

#[test]
fn test_scale_error_behavior() {
    let err = ScaleError::Zero;
    assert_eq!(err.to_string(), "Scale milli must be strictly non-zero");
    let dyn_err: &dyn Error = &err;
    assert!(dyn_err.source().is_none());
}

#[test]
fn test_output_transform_variants() {
    assert_eq!(OutputTransform::default(), OutputTransform::Normal);

    let transforms = [
        OutputTransform::Normal,
        OutputTransform::Rotate90,
        OutputTransform::Rotate180,
        OutputTransform::Rotate270,
        OutputTransform::Flipped,
        OutputTransform::Flipped90,
        OutputTransform::Flipped180,
        OutputTransform::Flipped270,
    ];

    let mut set = std::collections::HashSet::new();
    for t in transforms {
        set.insert(t);
    }
    assert_eq!(set.len(), 8);
}

#[test]
fn test_physical_pixel_size() {
    assert!(PhysicalPixelSize::new(0, 1080).is_none());
    assert!(PhysicalPixelSize::new(1920, 0).is_none());
    assert!(PhysicalPixelSize::new(0, 0).is_none());

    let size = PhysicalPixelSize::new(1920, 1080).expect("valid physical size");
    assert_eq!(size.width.get(), 1920);
    assert_eq!(size.height.get(), 1080);
}

#[test]
fn test_logical_point() {
    assert_eq!(LogicalPoint::ORIGIN, LogicalPoint::new(0, 0));

    let point = LogicalPoint::new(-630, 590);
    assert_eq!(point.x, -630);
    assert_eq!(point.y, 590);
}

#[test]
fn test_logical_size() {
    assert!(LogicalSize::new(0, 720).is_none());
    assert!(LogicalSize::new(1280, 0).is_none());
    assert!(LogicalSize::new(0, 0).is_none());

    let size = LogicalSize::new(1280, 720).expect("valid logical size");
    assert_eq!(size.width.get(), 1280);
    assert_eq!(size.height.get(), 720);
}

#[test]
fn test_connector_name() {
    assert_eq!(ConnectorName::new(""), Err(EmptyConnectorNameError));
    assert_eq!(ConnectorName::new("   "), Err(EmptyConnectorNameError));
    assert_eq!(ConnectorName::new("\t\n"), Err(EmptyConnectorNameError));

    let conn = ConnectorName::new("HDMI-A-1").expect("valid connector");
    assert_eq!(conn.as_str(), "HDMI-A-1");

    let conn_spaced = ConnectorName::new("  eDP-1  ").expect("valid connector");
    assert_eq!(conn_spaced.as_str(), "  eDP-1  ");

    let err = EmptyConnectorNameError;
    assert_eq!(err.to_string(), "Connector name cannot be empty");
    let dyn_err: &dyn Error = &err;
    assert!(dyn_err.source().is_none());
}

#[test]
fn test_desktop_context_generation() {
    assert_eq!(
        DesktopContextGeneration::INITIAL,
        DesktopContextGeneration(0)
    );

    let gen0 = DesktopContextGeneration(0);
    let gen1 = gen0.checked_next().expect("increment succeeds");
    assert_eq!(gen1, DesktopContextGeneration(1));
    assert!(gen0 < gen1);

    let gen_max = DesktopContextGeneration(u64::MAX);
    assert_eq!(gen_max.checked_next(), Err(GenerationExhausted));

    let err = GenerationExhausted;
    assert_eq!(
        err.to_string(),
        "DesktopContextGeneration counter exhausted; integer overflow reached"
    );
    let dyn_err: &dyn Error = &err;
    assert!(dyn_err.source().is_none());
}
