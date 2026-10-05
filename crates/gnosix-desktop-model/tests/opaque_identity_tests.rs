use gnosix_desktop_model::{OutputHubId, PhysicalOutputDescriptors, SurfaceId, WorkspaceId};
use std::collections::HashSet;

#[test]
fn test_output_hub_id_invariants() {
    let bytes = [42u8; 16];
    let id = OutputHubId::from_bytes(bytes);

    assert_eq!(id.as_bytes(), &bytes);
    assert_eq!(id.into_bytes(), bytes);

    // [0; 16] is ordinary valid bytes, not a special "nil" value
    let zero_bytes = [0u8; 16];
    let zero_id = OutputHubId::from_bytes(zero_bytes);
    assert_eq!(zero_id.as_bytes(), &[0u8; 16]);
    assert_ne!(id, zero_id);

    // Copy, Clone, PartialEq, Eq, Hash
    let id_copy = id;
    assert_eq!(id, id_copy);

    let mut set = HashSet::new();
    set.insert(id);
    set.insert(zero_id);
    assert_eq!(set.len(), 2);
    assert!(set.contains(&id_copy));
}

#[test]
fn test_workspace_id_invariants() {
    let bytes = [101u8; 16];
    let id = WorkspaceId::from_bytes(bytes);

    assert_eq!(id.as_bytes(), &bytes);
    assert_eq!(id.into_bytes(), bytes);

    // [0; 16] is ordinary valid bytes, not a special "nil" value
    let zero_bytes = [0u8; 16];
    let zero_id = WorkspaceId::from_bytes(zero_bytes);
    assert_eq!(zero_id.as_bytes(), &[0u8; 16]);
    assert_ne!(id, zero_id);

    // Copy, Clone, PartialEq, Eq, Hash
    let id_copy = id;
    assert_eq!(id, id_copy);

    let mut set = HashSet::new();
    set.insert(id);
    set.insert(zero_id);
    assert_eq!(set.len(), 2);
    assert!(set.contains(&id_copy));
}

#[test]
fn test_surface_id_invariants() {
    let bytes = [202u8; 16];
    let id = SurfaceId::from_bytes(bytes);
    assert_eq!(id.as_bytes(), &bytes);
    assert_eq!(id.into_bytes(), bytes);

    let zero = SurfaceId::from_bytes([0u8; 16]);
    assert_ne!(id, zero);

    let mut set = HashSet::new();
    set.insert(id);
    set.insert(zero);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_physical_output_descriptors_invariants() {
    let default_desc = PhysicalOutputDescriptors::default();
    assert_eq!(default_desc.make, None);
    assert_eq!(default_desc.model, None);
    assert_eq!(default_desc.serial, None);

    let d1 = PhysicalOutputDescriptors {
        make: Some("Dell Inc.".to_string()),
        model: Some("DELL U2720Q".to_string()),
        serial: Some("ABC1234567".to_string()),
    };
    let d2 = PhysicalOutputDescriptors {
        make: Some("Dell Inc.".to_string()),
        model: Some("DELL U2720Q".to_string()),
        serial: Some("ABC1234567".to_string()),
    };
    let d_diff_serial = PhysicalOutputDescriptors {
        make: Some("Dell Inc.".to_string()),
        model: Some("DELL U2720Q".to_string()),
        serial: Some("XYZ9876543".to_string()),
    };

    assert_eq!(d1, d2);
    assert_ne!(d1, d_diff_serial);

    // Crucial: None != Some("") != Some(" ") (no silent normalization)
    let d_none = PhysicalOutputDescriptors {
        make: None,
        model: None,
        serial: None,
    };
    let d_empty = PhysicalOutputDescriptors {
        make: Some("".to_string()),
        model: Some("".to_string()),
        serial: Some("".to_string()),
    };
    let d_space = PhysicalOutputDescriptors {
        make: Some(" ".to_string()),
        model: Some(" ".to_string()),
        serial: Some(" ".to_string()),
    };

    assert_ne!(d_none, d_empty);
    assert_ne!(d_empty, d_space);
    assert_ne!(d_none, d_space);

    let mut set = HashSet::new();
    set.insert(d1.clone());
    set.insert(d_diff_serial);
    set.insert(d_none);
    set.insert(d_empty);
    set.insert(d_space);
    assert_eq!(set.len(), 5);
}
