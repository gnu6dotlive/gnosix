use crate::store::OutputIdentityStoreMutations;
use in_gnu_os_output_identity::{
    CandidateIssue, DurableOutputHubId, FilesystemOutputIdentityStore, LoadOutcome,
    OutputIdentityRegistry, OutputIdentityStore, RegistryAuditEvent, RegistryDocument, StoreError,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

struct TempStore {
    root: PathBuf,
    store: FilesystemOutputIdentityStore,
}

impl TempStore {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "gnosix-r11-store-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self {
            store: FilesystemOutputIdentityStore::new(&root),
            root,
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn output(byte: u8) -> DurableOutputHubId {
    DurableOutputHubId::from_bytes([byte; 16])
}

fn document_with_one_hub() -> RegistryDocument {
    let mut registry = OutputIdentityRegistry::new();
    registry
        .register_hub(output(1), 0, "test-provisioning")
        .unwrap();
    registry.document().clone()
}

fn next_document(document: &RegistryDocument) -> RegistryDocument {
    let json = serde_json::to_string(document).unwrap();
    let mut registry = OutputIdentityRegistry::from_json(&json).unwrap();
    registry
        .register_hub(output(2), registry.revision(), "test-provisioning")
        .unwrap();
    registry.document().clone()
}

fn third_document(document: &RegistryDocument) -> RegistryDocument {
    let json = serde_json::to_string(document).unwrap();
    let mut registry = OutputIdentityRegistry::from_json(&json).unwrap();
    registry
        .register_hub(output(3), registry.revision(), "test-provisioning")
        .unwrap();
    registry.document().clone()
}

fn commit_two_generations(temp: &TempStore) -> (RegistryDocument, RegistryDocument) {
    let first = document_with_one_hub();
    temp.store.commit(0, &first).unwrap();
    let second = next_document(&first);
    temp.store.commit(first.revision, &second).unwrap();
    (first, second)
}

#[test]
fn missing_store_is_distinct_from_a_damaged_store() {
    let temp = TempStore::new();
    assert!(matches!(temp.store.load().unwrap(), LoadOutcome::Absent));

    fs::write(temp.file("output-identities-v1.json"), b"broken").unwrap();
    assert!(matches!(
        temp.store.load().unwrap(),
        LoadOutcome::Unrecoverable { issue }
            if issue.current == CandidateIssue::InvalidJson
    ));
}

#[test]
fn commit_persists_a_valid_generation_and_preserves_one_previous_generation() {
    let temp = TempStore::new();
    let (first, second) = commit_two_generations(&temp);
    assert_eq!(
        temp.store.load().unwrap(),
        LoadOutcome::Current(second.clone())
    );

    fs::write(temp.file("output-identities-v1.json"), b"damaged-current").unwrap();
    let LoadOutcome::Recoverable { previous, issue } = temp.store.load().unwrap() else {
        panic!("valid previous generation should be exposed for explicit recovery")
    };
    assert_eq!(previous, first);
    assert_eq!(issue.current, CandidateIssue::InvalidJson);
    assert_eq!(
        fs::read(temp.file("output-identities-v1.json")).unwrap(),
        b"damaged-current"
    );
}

#[test]
fn recovery_requires_an_explicit_reference_and_records_provenance() {
    let temp = TempStore::new();
    let (first, _) = commit_two_generations(&temp);
    fs::write(temp.file("output-identities-v1.json"), b"damaged-current").unwrap();

    let restored = temp.store.restore_previous("operator-review-42").unwrap();
    assert_eq!(restored.recovered_from_revision, first.revision);
    assert_eq!(restored.issue.current, CandidateIssue::InvalidJson);
    assert!(temp.file("output-identities-v1.damaged").exists());
    let LoadOutcome::Current(current) = temp.store.load().unwrap() else {
        panic!("explicit recovery should write a current generation")
    };
    assert_eq!(current.revision, first.revision + 1);
    assert!(matches!(
        current.audit.last(),
        Some(RegistryAuditEvent::RecoveryRestored {
            recovered_from_revision,
            recovery_issue,
            approval_reference,
            ..
        }) if *recovered_from_revision == first.revision
            && recovery_issue == "InvalidJson"
            && approval_reference == "operator-review-42"
    ));
}

#[test]
fn recovered_generation_cannot_reissue_an_id_from_the_lost_current_generation() {
    let temp = TempStore::new();
    let (_first, second) = commit_two_generations(&temp);
    fs::write(temp.file("output-identities-v1.json"), b"damaged-current").unwrap();

    temp.store.restore_previous("explicit-recovery").unwrap();
    let LoadOutcome::Current(recovered) = temp.store.load().unwrap() else {
        panic!("recovery should restore the known-good generation")
    };
    assert!(recovered.output_hubs.contains(&output(1)));
    assert!(!recovered.output_hubs.contains(&output(2)));

    let mut attempted_reuse =
        OutputIdentityRegistry::from_json(&serde_json::to_string(&recovered).unwrap()).unwrap();
    attempted_reuse
        .register_hub(output(2), recovered.revision, "reprovision")
        .unwrap();
    assert!(matches!(
        temp.store
            .commit(recovered.revision, attempted_reuse.document()),
        Err(StoreError::OutputHubIdPreviouslyIssued)
    ));
    assert_eq!(
        second.output_hubs,
        std::collections::BTreeSet::from([output(1), output(2)])
    );
}

#[test]
fn locked_current_snapshot_exposes_only_current_ids_and_revision() {
    let temp = TempStore::new();
    let current = document_with_one_hub();
    temp.store.commit(0, &current).unwrap();

    let snapshot = temp.store.locked_current_snapshot().unwrap();
    assert_eq!(snapshot.revision(), current.revision);
    assert!(snapshot.contains_hub(output(1)));
    assert!(!snapshot.contains_hub(output(2)));
}

#[test]
fn locked_identity_snapshot_holds_the_writer_lock_until_dropped() {
    let temp = TempStore::new();
    let current = document_with_one_hub();
    temp.store.commit(0, &current).unwrap();
    let snapshot = temp.store.locked_current_snapshot().unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.file("output-identities-v1.lock"))
        .unwrap();

    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(snapshot);
    lock.try_lock().unwrap();
}

#[test]
fn corrupt_issued_id_ledger_blocks_cross_document_identity_snapshot() {
    let temp = TempStore::new();
    let current = document_with_one_hub();
    temp.store.commit(0, &current).unwrap();
    fs::write(temp.file("output-identities-v1.issued"), b"not-json").unwrap();

    assert!(matches!(
        temp.store.locked_current_snapshot(),
        Err(StoreError::IssuedLedgerUnavailable)
    ));
    assert_eq!(temp.store.load().unwrap(), LoadOutcome::Current(current));
}

#[test]
fn initialized_legacy_registry_without_issued_ledger_fails_closed_without_migration() {
    let temp = TempStore::new();
    let current = document_with_one_hub();
    fs::write(
        temp.file("output-identities-v1.json"),
        serde_json::to_vec_pretty(&current).unwrap(),
    )
    .unwrap();
    fs::write(
        temp.file("output-identities-v1.initialized"),
        b"legacy-initialized\n",
    )
    .unwrap();

    assert!(matches!(
        temp.store.locked_current_snapshot(),
        Err(StoreError::IssuedLedgerUnavailable)
    ));
    assert!(!temp.file("output-identities-v1.issued").exists());
}

#[test]
fn recovery_without_review_reference_is_rejected_without_mutation() {
    let temp = TempStore::new();
    commit_two_generations(&temp);
    fs::write(temp.file("output-identities-v1.json"), b"damaged-current").unwrap();
    let current_before = fs::read(temp.file("output-identities-v1.json")).unwrap();
    let previous_before = fs::read(temp.file("output-identities-v1.previous")).unwrap();

    assert!(matches!(
        temp.store.restore_previous("  "),
        Err(StoreError::MissingRecoveryReference)
    ));
    assert_eq!(
        fs::read(temp.file("output-identities-v1.json")).unwrap(),
        current_before
    );
    assert_eq!(
        fs::read(temp.file("output-identities-v1.previous")).unwrap(),
        previous_before
    );
}

#[test]
fn future_schema_never_falls_back_or_gets_overwritten() {
    let temp = TempStore::new();
    let (_, second) = commit_two_generations(&temp);
    let current_path = temp.file("output-identities-v1.json");
    let mut current: serde_json::Value =
        serde_json::from_slice(&fs::read(&current_path).unwrap()).unwrap();
    current["schema_version"] = serde_json::json!(2);
    fs::write(&current_path, serde_json::to_vec(&current).unwrap()).unwrap();
    let bytes_before = fs::read(&current_path).unwrap();

    assert!(matches!(
        temp.store.load().unwrap(),
        LoadOutcome::Unrecoverable { issue }
            if issue.current == CandidateIssue::UnsupportedSchema(2)
    ));
    let third = third_document(&second);
    assert!(matches!(
        temp.store.commit(second.revision, &third),
        Err(StoreError::RecoveryRequired(_))
    ));
    assert_eq!(fs::read(current_path).unwrap(), bytes_before);
}

#[test]
fn missing_current_with_previous_and_pending_candidate_requires_review() {
    let temp = TempStore::new();
    let (first, second) = commit_two_generations(&temp);
    fs::remove_file(temp.file("output-identities-v1.json")).unwrap();
    fs::write(
        temp.file("output-identities-v1.pending.simulated"),
        serde_json::to_vec(&second).unwrap(),
    )
    .unwrap();

    let LoadOutcome::Recoverable { previous, issue } = temp.store.load().unwrap() else {
        panic!("missing current with a known-good previous must request recovery")
    };
    assert_eq!(previous, first);
    assert_eq!(issue.current, CandidateIssue::Missing);
    assert_eq!(
        issue.pending_candidates,
        ["output-identities-v1.pending.simulated"]
    );
    assert!(!temp.file("output-identities-v1.json").exists());
}

#[test]
fn orphaned_pending_generation_is_not_mistaken_for_an_absent_store() {
    let temp = TempStore::new();
    fs::write(
        temp.file("output-identities-v1.pending.orphan"),
        serde_json::to_vec(&document_with_one_hub()).unwrap(),
    )
    .unwrap();

    assert!(matches!(
        temp.store.load().unwrap(),
        LoadOutcome::Unrecoverable { issue }
            if issue.current == CandidateIssue::Missing
                && issue.pending_candidates == ["output-identities-v1.pending.orphan"]
    ));
}

#[test]
fn each_commit_retains_the_immediately_previous_known_good_generation() {
    let temp = TempStore::new();
    let (_, second) = commit_two_generations(&temp);
    let third = third_document(&second);
    temp.store.commit(second.revision, &third).unwrap();

    fs::write(temp.file("output-identities-v1.json"), b"damaged-current").unwrap();
    let LoadOutcome::Recoverable { previous, .. } = temp.store.load().unwrap() else {
        panic!("the last successful generation must remain recoverable")
    };
    assert_eq!(previous, second);
}

#[test]
fn initialized_store_with_all_generations_missing_is_not_reported_as_absent() {
    let temp = TempStore::new();
    commit_two_generations(&temp);
    fs::remove_file(temp.file("output-identities-v1.json")).unwrap();
    fs::remove_file(temp.file("output-identities-v1.previous")).unwrap();

    assert!(matches!(
        temp.store.load().unwrap(),
        LoadOutcome::Unrecoverable { issue }
            if issue.initialized_marker
                && issue.current == CandidateIssue::Missing
                && issue.pending_candidates.is_empty()
    ));
}

#[test]
fn independent_store_handles_serialize_compare_and_swap_commits() {
    let temp = TempStore::new();
    let first = document_with_one_hub();
    temp.store.commit(0, &first).unwrap();
    let second_candidate = next_document(&first);
    let mut third_registry =
        OutputIdentityRegistry::from_json(&serde_json::to_string(&first).unwrap()).unwrap();
    third_registry
        .register_hub(output(3), first.revision, "test-provisioning")
        .unwrap();
    let third_candidate = third_registry.document().clone();

    let barrier = Arc::new(Barrier::new(3));
    let first_writer = FilesystemOutputIdentityStore::new(&temp.root);
    let second_writer = FilesystemOutputIdentityStore::new(&temp.root);
    let first_barrier = Arc::clone(&barrier);
    let first_revision = first.revision;
    let first_thread = std::thread::spawn(move || {
        first_barrier.wait();
        first_writer.commit(first_revision, &second_candidate)
    });
    let second_barrier = Arc::clone(&barrier);
    let second_revision = first.revision;
    let second_thread = std::thread::spawn(move || {
        second_barrier.wait();
        second_writer.commit(second_revision, &third_candidate)
    });
    barrier.wait();

    let first_result = first_thread.join().unwrap();
    let second_result = second_thread.join().unwrap();
    assert_ne!(first_result.is_ok(), second_result.is_ok());
    assert!(
        matches!(first_result, Err(StoreError::StaleRevision { .. }))
            || matches!(second_result, Err(StoreError::StaleRevision { .. }))
    );
    assert!(matches!(
        temp.store.load().unwrap(),
        LoadOutcome::Current(document) if document.revision == 2
    ));
}

#[cfg(unix)]
#[test]
fn newly_created_registry_directory_and_files_are_user_private() {
    use std::os::unix::fs::PermissionsExt;

    let parent = std::env::temp_dir().join(format!(
        "gnosix-r11d-private-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&parent).unwrap();
    let directory = parent.join("identity-store");
    let store = FilesystemOutputIdentityStore::new(&directory);
    let document = document_with_one_hub();
    store.commit(0, &document).unwrap();

    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for name in [
        "output-identities-v1.json",
        "output-identities-v1.initialized",
        "output-identities-v1.lock",
    ] {
        assert_eq!(
            fs::metadata(directory.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn stale_revision_and_invalid_document_leave_current_generation_unchanged() {
    let temp = TempStore::new();
    let (first, second) = commit_two_generations(&temp);
    let current_path = temp.file("output-identities-v1.json");
    let bytes_before = fs::read(&current_path).unwrap();
    let third = third_document(&second);

    assert!(matches!(
        temp.store.commit(first.revision, &third),
        Err(StoreError::StaleRevision { .. })
    ));
    let mut invalid = third.clone();
    invalid.schema_version = 99;
    assert!(matches!(
        temp.store.commit(second.revision, &invalid),
        Err(StoreError::InvalidDocument(_))
    ));
    assert_eq!(fs::read(current_path).unwrap(), bytes_before);
}
