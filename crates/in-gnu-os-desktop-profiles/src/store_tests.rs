use super::*;
use crate::{DesktopProfile, DesktopProfileId, OutputProfileSelection};
use gnosix_desktop_model::{
    ConnectorName, DesktopContextGeneration, DesktopOutputProjection, LogicalPoint, LogicalSize,
    OutputHubId, OutputState, OutputTransform, PhysicalOutputDescriptors, PhysicalPixelSize,
    ScaleMilli,
};
use in_gnu_os_output_identity::{
    BindingEvidence, BindingRecord, FilesystemOutputIdentityStore, RegistryAuditEvent,
    RegistryDocument,
};
use in_gnu_os_output_identity_projection::SessionOutputObservation;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn profile_id(byte: u8) -> DesktopProfileId {
    DesktopProfileId::from_bytes([byte; 16])
}

fn hub_id(byte: u8) -> DurableOutputHubId {
    DurableOutputHubId::from_bytes([byte; 16])
}

fn profile_document(revision: u64) -> DesktopProfileDocument {
    DesktopProfileDocument {
        revision,
        profiles: vec![DesktopProfile {
            id: profile_id(1),
            name: "Everyday".into(),
            preferences: Default::default(),
        }],
        ..Default::default()
    }
}

fn selected_document(revision: u64, hub: DurableOutputHubId) -> DesktopProfileDocument {
    let mut document = profile_document(revision);
    document
        .output_profile_selections
        .push(OutputProfileSelection {
            output_hub_id: hub,
            profile_id: profile_id(1),
        });
    document
}

/// Build a current R11 fixture through the persisted schema so these tests use
/// the real locked snapshot parser without exposing the private R11 writer.
fn identity_snapshot(temp: &TempDir, hubs: &[DurableOutputHubId]) {
    let identity_dir = temp.0.join("identity");
    fs::create_dir_all(&identity_dir).unwrap();
    if hubs == [hub_id(1)] {
        fs::write(
            identity_dir.join("output-identities-v1.json"),
            include_str!("../../../evidence/fixtures/gnosix-r12-b2-r11-identity-current-v1.json"),
        )
        .unwrap();
        fs::write(
            identity_dir.join("output-identities-v1.issued"),
            include_str!("../../../evidence/fixtures/gnosix-r12-b2-r11-issued-v1.json"),
        )
        .unwrap();
        fs::write(
            identity_dir.join("output-identities-v1.initialized"),
            b"fixture\n",
        )
        .unwrap();
        let snapshot = FilesystemOutputIdentityStore::new(identity_dir)
            .locked_current_snapshot()
            .unwrap();
        assert_eq!(snapshot.revision(), 1);
        return;
    }
    let output_hubs = hubs
        .iter()
        .map(|hub| serde_json::to_value(hub).unwrap())
        .collect::<Vec<_>>();
    let audit = hubs
        .iter()
        .enumerate()
        .map(|(index, hub)| {
            serde_json::json!({
                "kind": "hub_created",
                "revision": index as u64 + 1,
                "output_hub_id": hub,
                "authority_reference": format!("fixture-{index}")
            })
        })
        .collect::<Vec<_>>();
    let registry = serde_json::json!({
        "schema_version": 1,
        "revision": hubs.len() as u64,
        "next_record_id": 1,
        "output_hubs": output_hubs,
        "bindings": [],
        "audit": audit
    });
    fs::write(
        identity_dir.join("output-identities-v1.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    fs::write(
        identity_dir.join("output-identities-v1.issued"),
        serde_json::to_vec(hubs).unwrap(),
    )
    .unwrap();
    fs::write(
        identity_dir.join("output-identities-v1.initialized"),
        b"fixture\n",
    )
    .unwrap();
    let snapshot = FilesystemOutputIdentityStore::new(identity_dir)
        .locked_current_snapshot()
        .unwrap();
    assert_eq!(snapshot.revision(), hubs.len() as u64);
}

fn store(temp: &TempDir) -> DesktopProfileStore {
    DesktopProfileStore::new(
        temp.0.join("profiles"),
        FilesystemOutputIdentityStore::new(temp.0.join("identity")),
    )
}

fn version(store: &DesktopProfileStore) -> ProfileVersion {
    match store.load().unwrap() {
        ProfileLoadOutcome::Absent => ProfileVersion::Absent,
        ProfileLoadOutcome::Current { version, .. } => version,
        other => panic!("cannot make a write token from {other:?}"),
    }
}

fn commit_current(
    store: &DesktopProfileStore,
    writer: &DesktopProfileWriter,
    document: &DesktopProfileDocument,
) -> Result<ProfileCommitReceipt, ProfileStoreError> {
    writer.commit(&version(store), document)
}

#[test]
fn session_projection_uses_the_profile_stores_bound_r11_authority() {
    let temp = TempDir::new("gnosix-r12-session-projection");
    let identity_dir = temp.0.join("identity");
    fs::create_dir_all(&identity_dir).unwrap();

    let durable_hub = hub_id(7);
    let evidence = BindingEvidence::SerialFingerprint([9; 32]);
    let registry = RegistryDocument {
        schema_version: in_gnu_os_output_identity::REGISTRY_SCHEMA_VERSION,
        revision: 2,
        next_record_id: 2,
        output_hubs: [durable_hub].into_iter().collect(),
        bindings: vec![BindingRecord {
            record_id: 1,
            output_hub_id: durable_hub,
            stable_evidence: [evidence.clone()].into_iter().collect(),
            make: Some("Example".to_owned()),
            model: Some("Panel".to_owned()),
            physical_size_mm: None,
            connector_history: ["DP-1".to_owned()].into_iter().collect(),
            active: true,
            approval_reference: "fixture-bind".to_owned(),
            revision: 2,
        }],
        audit: vec![
            RegistryAuditEvent::HubCreated {
                revision: 1,
                output_hub_id: durable_hub,
                authority_reference: "fixture-create".to_owned(),
            },
            RegistryAuditEvent::Bound {
                revision: 2,
                output_hub_id: durable_hub,
                record_id: 1,
                approval_reference: "fixture-bind".to_owned(),
            },
        ],
    };
    registry.validate().unwrap();
    fs::write(
        identity_dir.join("output-identities-v1.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    fs::write(
        identity_dir.join("output-identities-v1.issued"),
        serde_json::to_vec(&[durable_hub]).unwrap(),
    )
    .unwrap();
    fs::write(
        identity_dir.join("output-identities-v1.initialized"),
        b"fixture\n",
    )
    .unwrap();

    let store = store(&temp);
    let writer = store.open_writer();
    writer
        .commit(&ProfileVersion::Absent, &selected_document(1, durable_hub))
        .unwrap();

    let session_hub = OutputHubId::from_bytes([8; 16]);
    let projection = DesktopOutputProjection {
        generation: DesktopContextGeneration(4),
        output: OutputState {
            output_hub_id: session_hub,
            connector: ConnectorName::new("DP-1").unwrap(),
            descriptors: PhysicalOutputDescriptors {
                make: Some("Example".to_owned()),
                model: Some("Panel".to_owned()),
                serial: Some("opaque-to-r12".to_owned()),
            },
            active_workspace_id: None,
            physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
            logical_position: LogicalPoint::ORIGIN,
            logical_size: LogicalSize::new(1920, 1080).unwrap(),
            scale: ScaleMilli::from_milli(1000).unwrap(),
            transform: OutputTransform::Normal,
        },
        workspaces: Vec::new(),
        surfaces: Vec::new(),
    };
    let observed = SessionOutputObservation::for_output(
        session_hub,
        in_gnu_os_output_identity::OutputObservation::new([evidence])
            .with_connector("DP-1")
            .with_make_model("Example", "Panel"),
    );

    let session = store
        .project_session_output(&projection, &observed)
        .unwrap();
    let association = session.association.expect("durable association");
    assert_eq!(association.durable_output_hub_id, durable_hub);
    assert_eq!(association.registry_revision, 2);
    assert!(matches!(
        session.profile,
        Some(ProfileProjectionOutcome::Selected {
            output_hub_id,
            profile_revision: 1,
            identity_revision: 2,
            ..
        }) if output_hub_id == durable_hub
    ));
}

#[test]
fn absent_store_commits_complete_profile_document_with_revision_one() {
    let temp = TempDir::new("gnosix-r12-profile-store-cas");
    let store = store(&temp);
    assert!(matches!(store.load().unwrap(), ProfileLoadOutcome::Absent));

    let writer = store.open_writer();
    let receipt = commit_current(&store, &writer, &profile_document(1)).unwrap();

    assert_eq!(receipt.revision, 1);
    assert_eq!(receipt.previous_revision, None);
    assert!(
        matches!(store.load().unwrap(), ProfileLoadOutcome::Current { document, .. } if document.revision == 1)
    );
}

#[test]
fn stale_cas_is_rejected_without_overwriting_the_winning_document() {
    let temp = TempDir::new("gnosix-r12-profile-store-stale");
    let store = store(&temp);
    let writer_a = store.open_writer();
    let writer_b = store.open_writer();
    let stale_token = version(&store);
    writer_a.commit(&stale_token, &profile_document(1)).unwrap();
    commit_current(&store, &writer_a, &profile_document(2)).unwrap();

    assert!(matches!(
        writer_b.commit(&stale_token, &profile_document(1)),
        Err(ProfileStoreError::StaleVersion { .. })
    ));
    assert!(
        matches!(store.load().unwrap(), ProfileLoadOutcome::Current { document, .. } if document.revision == 2)
    );
}

#[test]
fn invalid_document_does_not_create_or_mutate_store() {
    let temp = TempDir::new("gnosix-r12-profile-store-invalid");
    let store = store(&temp);
    let mut invalid = profile_document(1);
    invalid.profiles.push(invalid.profiles[0].clone());

    assert!(matches!(
        store
            .open_writer()
            .commit(&ProfileVersion::Absent, &invalid),
        Err(ProfileStoreError::InvalidDocument(
            ProfileError::DuplicateProfile
        ))
    ));
    assert!(matches!(store.load().unwrap(), ProfileLoadOutcome::Absent));
}

#[test]
fn output_selection_requires_live_snapshot_and_known_durable_id() {
    let temp = TempDir::new("gnosix-r12-profile-store-hub-validation");
    let store = store(&temp);
    let selected = selected_document(1, hub_id(2));
    assert!(matches!(
        store
            .open_writer()
            .commit(&ProfileVersion::Absent, &selected),
        Err(ProfileStoreError::Identity(StoreError::RecoveryRequired(_)))
    ));
    identity_snapshot(&temp, &[hub_id(1)]);
    assert!(matches!(
        store
            .open_writer()
            .commit(&ProfileVersion::Absent, &selected),
        Err(ProfileStoreError::UnknownOutputHub(id)) if id == hub_id(2)
    ));
    assert!(matches!(store.load().unwrap(), ProfileLoadOutcome::Absent));
}

#[test]
fn profile_writer_uses_only_its_bound_identity_registry() {
    let temp = TempDir::new("gnosix-r12-profile-store-bound-identity");
    let foreign = TempDir::new("gnosix-r12-profile-store-foreign-identity");
    let store = store(&temp);
    identity_snapshot(&foreign, &[hub_id(2)]);

    assert!(matches!(
        store
            .open_writer()
            .commit(&ProfileVersion::Absent, &selected_document(1, hub_id(2))),
        Err(ProfileStoreError::Identity(StoreError::RecoveryRequired(_)))
    ));
    assert!(matches!(store.load().unwrap(), ProfileLoadOutcome::Absent));
}

#[test]
fn read_only_projection_uses_only_validated_durable_hub_selection() {
    let temp = TempDir::new("gnosix-r12-profile-projection");
    let store = store(&temp);
    identity_snapshot(&temp, &[hub_id(1)]);
    let writer = store.open_writer();
    let mut selected = selected_document(1, hub_id(1));
    selected.profiles[0].preferences.visual.color_scheme = Some(crate::ColorSchemePreference::Dark);
    selected.profiles[0].preferences.bar.edge = Some(crate::BarEdgePreference::Bottom);
    writer.commit(&ProfileVersion::Absent, &selected).unwrap();

    assert!(matches!(
        store.project_output(hub_id(1)).unwrap(),
        ProfileProjectionOutcome::Selected {
            output_hub_id,
            profile_revision: 1,
            identity_revision: 1,
            profile,
        } if output_hub_id == hub_id(1)
            && profile.id == profile_id(1)
            && profile.preferences.visual.color_scheme == Some(crate::ColorSchemePreference::Dark)
            && profile.preferences.bar.edge == Some(crate::BarEdgePreference::Bottom)
    ));
    assert!(matches!(
        store.project_output(hub_id(2)),
        Err(ProfileStoreError::UnknownOutputHub(id)) if id == hub_id(2)
    ));

    let mut next = selected;
    next.revision = 2;
    writer.commit(&version(&store), &next).unwrap();
    fs::write(store.current_path(), b"damaged-current").unwrap();
    assert!(matches!(
        store.project_output(hub_id(1)).unwrap(),
        ProfileProjectionOutcome::RecoveryRequired { .. }
    ));
}

#[test]
fn read_only_projection_distinguishes_absent_document_from_unselected_output() {
    let temp = TempDir::new("gnosix-r12-profile-projection-absent");
    let store = store(&temp);
    identity_snapshot(&temp, &[hub_id(1)]);
    assert!(matches!(
        store.project_output(hub_id(1)).unwrap(),
        ProfileProjectionOutcome::DocumentAbsent {
            output_hub_id: id,
            identity_revision: 1,
        } if id == hub_id(1)
    ));

    let writer = store.open_writer();
    writer
        .commit(&ProfileVersion::Absent, &profile_document(1))
        .unwrap();
    assert!(matches!(
        store.project_output(hub_id(1)).unwrap(),
        ProfileProjectionOutcome::NoSelection {
            output_hub_id,
            profile_revision: 1,
            identity_revision: 1,
        } if output_hub_id == hub_id(1)
    ));
}

#[test]
fn read_only_projection_fails_closed_when_identity_authority_is_absent() {
    let temp = TempDir::new("gnosix-r12-profile-projection-no-identity");
    let store = store(&temp);
    assert!(matches!(
        store.project_output(hub_id(1)),
        Err(ProfileStoreError::Identity(StoreError::RecoveryRequired(_)))
    ));
}

#[test]
fn read_only_projection_does_not_fall_back_when_current_and_previous_are_damaged() {
    let temp = TempDir::new("gnosix-r12-profile-projection-unrecoverable");
    let store = store(&temp);
    identity_snapshot(&temp, &[hub_id(1)]);
    store
        .open_writer()
        .commit(&ProfileVersion::Absent, &profile_document(1))
        .unwrap();
    fs::write(store.current_path(), b"damaged-current").unwrap();

    assert!(matches!(
        store.project_output(hub_id(1)).unwrap(),
        ProfileProjectionOutcome::Unavailable {
            output_hub_id,
            identity_revision: 1,
            ..
        } if output_hub_id == hub_id(1)
    ));
}

#[test]
fn duplicate_output_selection_and_unknown_profile_remain_invalid() {
    let temp = TempDir::new("gnosix-r12-profile-store-duplicates");
    let store = store(&temp);
    let mut duplicate = selected_document(1, hub_id(1));
    duplicate
        .output_profile_selections
        .push(duplicate.output_profile_selections[0]);
    identity_snapshot(&temp, &[hub_id(1)]);

    assert!(matches!(
        store
            .open_writer()
            .commit(&ProfileVersion::Absent, &duplicate),
        Err(ProfileStoreError::InvalidDocument(
            ProfileError::DuplicateOutputSelection
        ))
    ));
    let mut missing_profile = selected_document(1, hub_id(1));
    missing_profile.output_profile_selections[0].profile_id = profile_id(9);
    assert!(matches!(
        store
            .open_writer()
            .commit(&ProfileVersion::Absent, &missing_profile),
        Err(ProfileStoreError::InvalidDocument(
            ProfileError::SelectionReferencesUnknownProfile
        ))
    ));
    assert!(matches!(store.load().unwrap(), ProfileLoadOutcome::Absent));
}

#[test]
fn corrupt_current_is_recoverable_only_as_an_explicit_previous_candidate() {
    let temp = TempDir::new("gnosix-r12-profile-store-recovery");
    let store = store(&temp);
    let writer = store.open_writer();
    commit_current(&store, &writer, &profile_document(1)).unwrap();
    let mut second = profile_document(2);
    second.profiles[0].name = "Night".into();
    commit_current(&store, &writer, &second).unwrap();
    let stale_pre_corruption_version = version(&store);
    fs::write(store.directory.join(CURRENT), b"broken-current").unwrap();

    let ProfileLoadOutcome::Recoverable {
        previous,
        issue,
        token,
    } = store.load().unwrap()
    else {
        panic!("corrupt current must not silently load as current or absent")
    };
    assert_eq!(previous.revision, 1);
    assert_eq!(issue.current, ProfileCandidateIssue::InvalidJson);
    assert_eq!(
        fs::read(store.directory.join(CURRENT)).unwrap(),
        b"broken-current"
    );

    let receipt = writer.recover(&token, "operator-recovery-7").unwrap();
    assert_eq!(receipt.recovered_from_revision, 1);
    assert_eq!(receipt.revision, 2);
    assert!(receipt.damaged_candidate_preserved);
    assert_eq!(receipt.recovery_reference, "operator-recovery-7");
    assert!(
        matches!(store.load().unwrap(), ProfileLoadOutcome::Current { document, .. } if document.revision == 2)
    );
    assert_eq!(fs::read(store.damaged_path()).unwrap(), b"broken-current");
    assert!(matches!(
        writer.commit(&stale_pre_corruption_version, &profile_document(3)),
        Err(ProfileStoreError::StaleVersion { .. })
    ));
}

#[test]
fn recovery_fails_when_no_valid_previous_generation_exists() {
    let temp = TempDir::new("gnosix-r12-profile-store-no-recovery");
    let store = store(&temp);
    fs::create_dir_all(&store.directory).unwrap();
    fs::write(store.initialized_path(), b"initialized\n").unwrap();
    fs::write(store.current_path(), b"broken").unwrap();
    let writer = store.open_writer();
    let token = ProfileRecoveryToken {
        current_bytes: Some(b"broken".to_vec()),
        previous_revision: 0,
        issue: ProfileRecoveryIssue {
            current: ProfileCandidateIssue::InvalidJson,
            previous: Some(ProfileCandidateIssue::Missing),
            initialized_marker: true,
            pending_candidates: Vec::new(),
        },
    };

    assert!(matches!(
        writer.recover(&token, "review"),
        Err(ProfileStoreError::NotRecoverable)
    ));
    assert_eq!(fs::read(store.current_path()).unwrap(), b"broken");
}

#[test]
fn future_schema_is_unrecoverable_and_never_falls_back_or_overwrites() {
    let temp = TempDir::new("gnosix-r12-profile-store-future-schema");
    let store = store(&temp);
    let writer = store.open_writer();
    commit_current(&store, &writer, &profile_document(1)).unwrap();
    commit_current(&store, &writer, &profile_document(2)).unwrap();
    let stale_token = version(&store);
    let future =
        br#"{"schema_version":99,"revision":99,"profiles":[],"output_profile_selections":[]}"#;
    fs::write(store.current_path(), future).unwrap();

    assert!(matches!(
        store.load().unwrap(),
        ProfileLoadOutcome::Unrecoverable {
            issue: ProfileRecoveryIssue {
                current: ProfileCandidateIssue::UnsupportedSchema(99),
                ..
            }
        }
    ));
    assert!(matches!(
        writer.commit(&stale_token, &profile_document(3)),
        Err(ProfileStoreError::RecoveryRequired(_))
    ));
    assert_eq!(fs::read(store.current_path()).unwrap(), future);
}

#[test]
fn recovery_token_is_bound_to_the_observed_corrupt_bytes() {
    let temp = TempDir::new("gnosix-r12-profile-store-stale-recovery");
    let store = store(&temp);
    let writer = store.open_writer();
    commit_current(&store, &writer, &profile_document(1)).unwrap();
    commit_current(&store, &writer, &profile_document(2)).unwrap();
    fs::write(store.current_path(), b"broken-a").unwrap();
    let ProfileLoadOutcome::Recoverable { token, .. } = store.load().unwrap() else {
        panic!("expected a recoverable state")
    };
    fs::write(store.current_path(), b"broken-b").unwrap();

    assert!(matches!(
        writer.recover(&token, "review"),
        Err(ProfileStoreError::StaleRecoveryToken)
    ));
    assert_eq!(fs::read(store.current_path()).unwrap(), b"broken-b");
}

#[test]
fn interrupted_post_rotation_commit_never_appears_as_partial_current() {
    let temp = TempDir::new("gnosix-r12-profile-store-interrupt");
    let store = store(&temp);
    let writer = store.open_writer();
    commit_current(&store, &writer, &profile_document(1)).unwrap();
    let failure = || Err(ProfileStoreError::Serialization);
    let expected = version(&store);

    assert!(writer
        .store
        .commit_transaction(&expected, &profile_document(2), failure)
        .is_err());
    assert!(!store.current_path().exists());
    assert!(
        matches!(store.load().unwrap(), ProfileLoadOutcome::Recoverable { previous, .. } if previous.revision == 1)
    );
}

#[test]
fn two_concurrent_profile_writers_have_one_cas_winner() {
    let temp = TempDir::new("gnosix-r12-profile-store-race");
    let path = temp.0.join("profiles");
    let identity_path = path.with_extension("identity");
    let store_a =
        DesktopProfileStore::new(&path, FilesystemOutputIdentityStore::new(&identity_path));
    let store_b =
        DesktopProfileStore::new(&path, FilesystemOutputIdentityStore::new(&identity_path));
    let barrier = Arc::new(Barrier::new(3));
    let first = {
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            store_a
                .open_writer()
                .commit(&ProfileVersion::Absent, &profile_document(1))
        })
    };
    let second = {
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            store_b
                .open_writer()
                .commit(&ProfileVersion::Absent, &profile_document(1))
        })
    };
    barrier.wait();
    let results = [first.join().unwrap(), second.join().unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ProfileStoreError::StaleVersion { .. })))
            .count(),
        1
    );
    assert!(
        matches!(DesktopProfileStore::new(path.clone(), FilesystemOutputIdentityStore::new(path.with_extension("identity"))).load().unwrap(), ProfileLoadOutcome::Current { document, .. } if document.revision == 1)
    );
}

#[test]
fn profile_recovery_fails_closed_if_previous_hub_is_not_in_current_r11() {
    let temp = TempDir::new("gnosix-r12-profile-store-recovery-identity");
    let store = store(&temp);
    let writer = store.open_writer();
    identity_snapshot(&temp, &[hub_id(1)]);
    writer
        .commit(&ProfileVersion::Absent, &selected_document(1, hub_id(1)))
        .unwrap();
    writer
        .commit(&version(&store), &selected_document(2, hub_id(1)))
        .unwrap();
    fs::write(store.current_path(), b"broken-current").unwrap();
    let ProfileLoadOutcome::Recoverable { token, .. } = store.load().unwrap() else {
        panic!("expected recoverable profile generation")
    };
    identity_snapshot(&temp, &[hub_id(2)]);
    assert!(matches!(
        writer.recover(&token, "recovery-after-r11-change"),
        Err(ProfileStoreError::UnknownOutputHub(id)) if id == hub_id(1)
    ));
    assert_eq!(fs::read(store.current_path()).unwrap(), b"broken-current");
}

#[test]
fn persisted_b1_and_r11_fixture_artifacts_are_valid_inputs() {
    let profile = decode_profile_document(include_bytes!(
        "../../../evidence/fixtures/gnosix-r12-b2-profile-current-v1.json"
    ))
    .unwrap();
    assert_eq!(
        profile.schema_version,
        crate::DESKTOP_PROFILE_SCHEMA_VERSION
    );
    profile.validate().unwrap();
    let temp = TempDir::new("gnosix-r12-profile-fixture");
    identity_snapshot(&temp, &[hub_id(1)]);
}

#[test]
fn validated_load_rejects_a_dangling_id_after_identity_recovery() {
    let temp = TempDir::new("gnosix-r12-profile-store-dangling");
    let store = store(&temp);
    identity_snapshot(&temp, &[hub_id(1)]);
    store
        .open_writer()
        .commit(&ProfileVersion::Absent, &selected_document(1, hub_id(1)))
        .unwrap();
    identity_snapshot(&temp, &[hub_id(2)]);
    assert!(matches!(
        store.load_for_identity(),
        Err(ProfileStoreError::UnknownOutputHub(id)) if id == hub_id(1)
    ));
}

#[test]
fn legacy_v1_profile_loads_as_schema2_compat_view_and_first_commit_upgrades() {
    let temp = TempDir::new("gnosix-r12-profile-store-v1-upgrade");
    let store = store(&temp);
    fs::create_dir_all(temp.0.join("profiles")).unwrap();
    let legacy = br#"{
      "schema_version": 1,
      "revision": 7,
      "profiles": [{
        "id": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
        "name": "Personal",
        "preferences": { "bar": { "edge": "left" } }
      }],
      "output_profile_selections": []
    }"#;
    fs::write(store.current_path(), legacy).unwrap();
    fs::write(store.initialized_path(), b"legacy-v1\n").unwrap();

    let (mut document, version) = match store.load().unwrap() {
        ProfileLoadOutcome::Current { document, version } => (document, version),
        other => panic!("legacy v1 did not load: {other:?}"),
    };
    assert_eq!(document.schema_version, 2);
    assert_eq!(document.revision, 7);
    assert_eq!(
        document.profiles[0].preferences.bar.edge,
        Some(crate::BarEdgePreference::Left)
    );
    assert_eq!(
        document.profiles[0].preferences.bar.overrides(),
        crate::BarPreferenceOverrides::default()
    );
    assert_eq!(version.revision(), Some(7));

    document.revision = 8;
    document.profiles[0].preferences.bar.persistent = Some(true);
    store.open_writer().commit(&version, &document).unwrap();
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(store.current_path()).unwrap()).unwrap();
    assert_eq!(persisted["schema_version"], 2);
    assert_eq!(
        persisted["profiles"][0]["preferences"]["bar"]["persistent"],
        true
    );
}

#[test]
fn legacy_v1_cannot_smuggle_donor11_fields_under_the_old_schema_number() {
    let temp = TempDir::new("gnosix-r12-profile-store-v1-smuggle");
    let store = store(&temp);
    fs::create_dir_all(temp.0.join("profiles")).unwrap();
    let invalid_legacy = br#"{
      "schema_version": 1,
      "revision": 1,
      "profiles": [{
        "id": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
        "name": "Personal",
        "preferences": { "bar": { "persistent": true } }
      }],
      "output_profile_selections": []
    }"#;
    fs::write(store.current_path(), invalid_legacy).unwrap();
    fs::write(store.initialized_path(), b"legacy-v1\n").unwrap();

    assert!(matches!(
        store.load().unwrap(),
        ProfileLoadOutcome::Unrecoverable {
            issue: ProfileRecoveryIssue {
                current: ProfileCandidateIssue::InvalidJson,
                ..
            }
        }
    ));
}
