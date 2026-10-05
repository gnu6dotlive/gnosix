use gnosix_desktop_model::{
    ConnectorName, DesktopContextGeneration, DesktopOutputProjection, LogicalPoint, LogicalSize,
    OutputHubId, OutputState, OutputTransform, PhysicalPixelSize, ScaleMilli,
};
use in_gnu_os_output_identity::{
    FilesystemOutputIdentityStore, LoadOutcome, OutputIdentityReader, OutputIdentityStore,
    OutputObservation, StoreError,
};
use in_gnu_os_output_identity_projection::{
    associate_projection_identity, ProjectionAssociationError, SessionOutputObservation,
};
use std::io;
use std::path::PathBuf;

fn projection(session_id: u8) -> DesktopOutputProjection {
    DesktopOutputProjection {
        generation: DesktopContextGeneration(12),
        output: OutputState {
            output_hub_id: OutputHubId::from_bytes([session_id; 16]),
            connector: ConnectorName::new(format!("DP-{session_id}")).unwrap(),
            active_workspace_id: None,
            descriptors: Default::default(),
            physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
            logical_position: LogicalPoint::ORIGIN,
            logical_size: LogicalSize::new(1920, 1080).unwrap(),
            scale: ScaleMilli::from_milli(1000).unwrap(),
            transform: OutputTransform::Normal,
        },
        workspaces: Vec::new(),
        surfaces: Vec::new(),
    }
}

fn observed(session_id: u8) -> SessionOutputObservation {
    SessionOutputObservation::for_output(
        OutputHubId::from_bytes([session_id; 16]),
        OutputObservation::new(
            [in_gnu_os_output_identity::BindingEvidence::EdidFingerprint(
                [9; 32],
            )],
        ),
    )
}

struct FixedStore(LoadOutcome);

impl OutputIdentityStore for FixedStore {
    fn load(&self) -> Result<LoadOutcome, StoreError> {
        Ok(self.0.clone())
    }
}

struct FailedStore;

impl OutputIdentityStore for FailedStore {
    fn load(&self) -> Result<LoadOutcome, StoreError> {
        Err(StoreError::Io {
            operation: "fixture read",
            path: PathBuf::from("fixture"),
            source: io::Error::other("injected read failure"),
        })
    }
}

#[test]
fn absent_authority_and_uninitialized_store_add_no_association_and_preserve_projection() {
    let projection = projection(3);
    let before = projection.clone();
    let observation = observed(3);
    let absent_store = FixedStore(LoadOutcome::Absent);
    let reader = OutputIdentityReader::new(&absent_store);

    assert_eq!(
        associate_projection_identity(&projection, &observation, None).unwrap(),
        None
    );
    assert_eq!(
        associate_projection_identity(&projection, &observation, Some(&reader)).unwrap(),
        None
    );
    assert_eq!(projection, before);
}

#[test]
fn unbound_registry_adds_no_durable_association() {
    let projection = projection(4);
    let document = serde_json::from_str(
        r#"{"schema_version":1,"revision":1,"next_record_id":1,"output_hubs":[[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]],"bindings":[],"audit":[{"kind":"hub_created","revision":1,"output_hub_id":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"authority_reference":"fixture"}]}"#,
    )
    .unwrap();
    let store = FixedStore(LoadOutcome::Current(document));
    let reader = OutputIdentityReader::new(&store);

    assert_eq!(
        associate_projection_identity(&projection, &observed(4), Some(&reader)).unwrap(),
        None
    );
}

#[test]
fn recoverable_and_unrecoverable_authority_states_add_no_durable_association() {
    let projection = projection(8);
    let issue = in_gnu_os_output_identity::RecoveryIssue {
        current: in_gnu_os_output_identity::CandidateIssue::InvalidJson,
        previous: None,
        initialized_marker: true,
        pending_candidates: Vec::new(),
    };
    let recoverable = FixedStore(LoadOutcome::Recoverable {
        previous: in_gnu_os_output_identity::RegistryDocument::default(),
        issue: issue.clone(),
    });
    let unrecoverable = FixedStore(LoadOutcome::Unrecoverable { issue });

    for store in [&recoverable, &unrecoverable] {
        let reader = OutputIdentityReader::new(store);
        assert_eq!(
            associate_projection_identity(&projection, &observed(8), Some(&reader)).unwrap(),
            None
        );
    }
}

#[test]
fn bound_identity_is_additive_and_keeps_the_session_id_unchanged() {
    let projection = projection(5);
    let before = projection.clone();
    let session_id = projection.output.output_hub_id;
    let durable_id = [42; 16];
    let bound_document = serde_json::from_str::<in_gnu_os_output_identity::RegistryDocument>(
        &format!(
            r#"{{"schema_version":1,"revision":2,"next_record_id":2,"output_hubs":[{durable}],"bindings":[{{"record_id":1,"output_hub_id":{durable},"stable_evidence":[{{"kind":"edid_fingerprint","sha256":[{fingerprint}]}}],"make":null,"model":null,"physical_size_mm":null,"connector_history":["DP-5"],"active":true,"approval_reference":"review-1","revision":2}}],"audit":[{{"kind":"hub_created","revision":1,"output_hub_id":{durable},"authority_reference":"provision-1"}},{{"kind":"bound","revision":2,"output_hub_id":{durable},"record_id":1,"approval_reference":"review-1"}}]}}"#,
            durable = serde_json::to_string(&durable_id).unwrap(),
            fingerprint = vec!["9"; 32].join(","),
        ),
    )
    .unwrap();
    let store = FixedStore(LoadOutcome::Current(bound_document));
    let reader = OutputIdentityReader::new(&store);

    let association = associate_projection_identity(&projection, &observed(5), Some(&reader))
        .unwrap()
        .unwrap();

    assert_eq!(association.session_output_hub_id, session_id);
    assert_eq!(association.durable_output_hub_id.into_bytes(), durable_id);
    assert_eq!(association.registry_revision, 2);
    assert_eq!(association.matched_evidence.len(), 1);
    assert_eq!(projection.output.output_hub_id, session_id);
    assert_eq!(projection, before);
}

#[test]
fn bound_observation_cannot_be_attached_to_a_different_session_projection() {
    let projection = projection(6);
    let empty_document = serde_json::from_str::<in_gnu_os_output_identity::RegistryDocument>(
        r#"{"schema_version":1,"revision":0,"next_record_id":1,"output_hubs":[],"bindings":[],"audit":[]}"#,
    )
    .unwrap();
    let store = FixedStore(LoadOutcome::Current(empty_document));
    let reader = OutputIdentityReader::new(&store);

    assert!(matches!(
        associate_projection_identity(&projection, &observed(7), Some(&reader)),
        Err(ProjectionAssociationError::ObservationSessionMismatch)
    ));
}

#[test]
fn failed_identity_read_does_not_publish_an_association() {
    let projection = projection(7);
    let store = FailedStore;
    let reader = OutputIdentityReader::new(&store);

    assert!(matches!(
        associate_projection_identity(&projection, &observed(7), Some(&reader)),
        Err(ProjectionAssociationError::Store(_))
    ));
}

#[test]
fn filesystem_absence_keeps_the_existing_projection_unmodified() {
    let directory =
        std::env::temp_dir().join(format!("gnosix-r11f-projection-{}", std::process::id()));
    let store = FilesystemOutputIdentityStore::new(&directory);
    let reader = OutputIdentityReader::new(&store);
    let projection = projection(9);
    let before = projection.clone();

    assert_eq!(
        associate_projection_identity(&projection, &observed(9), Some(&reader)).unwrap(),
        None
    );
    assert_eq!(projection, before);
    let _ = std::fs::remove_dir_all(directory);
}
