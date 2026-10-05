use in_gnu_os_output_identity::{
    BindingState, DurableOutputHubId, FilesystemOutputIdentityStore, IdentityReadState,
    OutputIdentityReader, OutputObservation,
};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

#[test]
fn external_consumer_can_read_revision_and_resolve_without_writer_access() {
    let directory = std::env::temp_dir().join(format!(
        "gnosix-r11e-reader-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    let store = FilesystemOutputIdentityStore::new(&directory);
    let reader = OutputIdentityReader::new(&store);

    assert!(matches!(
        reader.load().unwrap(),
        in_gnu_os_output_identity::LoadOutcome::Absent
    ));
    assert_eq!(reader.revision().unwrap(), None);
    assert_eq!(
        reader.resolve(&OutputObservation::new([])).unwrap(),
        IdentityReadState::Absent
    );

    fs::create_dir(&directory).unwrap();
    let id = DurableOutputHubId::from_bytes([1; 16]);
    let document = serde_json::json!({
        "schema_version": 1,
        "revision": 1,
        "next_record_id": 1,
        "output_hubs": [id],
        "bindings": [],
        "audit": [{
            "kind": "hub_created",
            "revision": 1,
            "output_hub_id": id,
            "authority_reference": "fixture"
        }]
    });
    fs::write(
        directory.join("output-identities-v1.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        reader.load().unwrap(),
        in_gnu_os_output_identity::LoadOutcome::Current(_)
    ));
    assert_eq!(reader.revision().unwrap(), Some(1));
    assert!(matches!(
        reader.resolve(&OutputObservation::new([])).unwrap(),
        IdentityReadState::Current {
            revision: 1,
            binding: BindingState::Unbound {
                reason: in_gnu_os_output_identity::UnboundReason::NoStableEvidence
            }
        }
    ));
    fs::remove_dir_all(directory).unwrap();
}
