use gnosix_desktop_model::{
    ConnectorName, LogicalPoint, LogicalSize, OutputHubId, OutputState, OutputTransform,
    PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli,
};
use std::collections::HashSet;

fn sample_output_state() -> OutputState {
    OutputState {
        output_hub_id: OutputHubId::from_bytes([1; 16]),
        connector: ConnectorName::new("DP-1").unwrap(),
        descriptors: PhysicalOutputDescriptors {
            make: Some("Dell".to_string()),
            model: Some("U2720Q".to_string()),
            serial: Some("SN123".to_string()),
        },
        physical_size: PhysicalPixelSize::new(3840, 2160).unwrap(),
        logical_position: LogicalPoint::ORIGIN,
        logical_size: LogicalSize::new(1920, 1080).unwrap(),
        scale: ScaleMilli::from_milli(2000).unwrap(),
        transform: OutputTransform::Normal,
        active_workspace_id: None,
    }
}

#[test]
fn test_output_state_equality_and_preservation() {
    let raw_hub = [42u8; 16];
    let mut state = sample_output_state();
    state.output_hub_id = OutputHubId::from_bytes(raw_hub);

    // Byte-for-byte caller-supplied ID preservation
    assert_eq!(state.output_hub_id.as_bytes(), &raw_hub);
    assert_eq!(state.output_hub_id.into_bytes(), raw_hub);

    // Identical composites are structurally equal
    let clone = state.clone();
    assert_eq!(state, clone);
}

#[test]
fn test_output_state_single_field_deltas() {
    let base = sample_output_state();

    // 1. Delta in output_hub_id
    let mut d_hub = base.clone();
    d_hub.output_hub_id = OutputHubId::from_bytes([2; 16]);
    assert_ne!(base, d_hub);

    // 2. Delta in connector
    let mut d_conn = base.clone();
    d_conn.connector = ConnectorName::new("HDMI-A-1").unwrap();
    assert_ne!(base, d_conn);

    // 3. Delta in descriptors
    let mut d_desc = base.clone();
    d_desc.descriptors.model = Some("U2723QE".to_string());
    assert_ne!(base, d_desc);

    // 4. Delta in physical_size
    let mut d_phys = base.clone();
    d_phys.physical_size = PhysicalPixelSize::new(2560, 1440).unwrap();
    assert_ne!(base, d_phys);

    // 5. Delta in logical_position
    let mut d_pos = base.clone();
    d_pos.logical_position = LogicalPoint::new(-630, 590);
    assert_ne!(base, d_pos);

    // 6. Delta in logical_size
    let mut d_log = base.clone();
    d_log.logical_size = LogicalSize::new(2560, 1440).unwrap();
    assert_ne!(base, d_log);

    // 7. Delta in scale
    let mut d_scale = base.clone();
    d_scale.scale = ScaleMilli::from_milli(1500).unwrap();
    assert_ne!(base, d_scale);

    // 8. Delta in transform
    let mut d_trans = base.clone();
    d_trans.transform = OutputTransform::Rotate90;
    assert_ne!(base, d_trans);
}

#[test]
fn test_output_state_zero_normalization() {
    let base = sample_output_state();

    // Leading/trailing whitespace in descriptors is NOT normalized
    let mut with_spaces = base.clone();
    with_spaces.descriptors.make = Some(" Dell ".to_string());
    assert_ne!(base, with_spaces);

    // Case differences in descriptors are NOT folded
    let mut uppercase = base.clone();
    uppercase.descriptors.make = Some("DELL".to_string());
    assert_ne!(base, uppercase);

    // None != Some("") in descriptors
    let mut empty_string = base.clone();
    empty_string.descriptors.serial = Some("".to_string());
    let mut none_serial = base.clone();
    none_serial.descriptors.serial = None;
    assert_ne!(empty_string, none_serial);
}

#[test]
fn test_output_state_hash_convenience() {
    let s1 = sample_output_state();
    let s2 = sample_output_state();

    let mut set = HashSet::new();
    assert!(set.insert(s1.clone()));
    assert!(!set.insert(s2.clone())); // already present due to exact equality
    assert!(set.contains(&s1));
    assert!(set.contains(&s2));

    let mut different = sample_output_state();
    different.transform = OutputTransform::Rotate180;
    assert!(!set.contains(&different));
    assert!(set.insert(different));
    assert_eq!(set.len(), 2);
}
