//! Tests for the designated profile write authority.
//!
//! The point of this module is a boundary, so most of these tests are about
//! what the authority refuses to do rather than what it performs.

use super::*;

use in_gnu_os_output_identity::DurableOutputHubId;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn hub(byte: u8) -> DurableOutputHubId {
    DurableOutputHubId::from_bytes([byte; 16])
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A profile store over a real R11 identity fixture.
///
/// Output selections are cross-validated against the R11 registry, so a test
/// that selects an output must first have that output bound. The fixture is
/// built through the persisted schema, the same way the store's own tests do,
/// rather than through a private writer.
fn store_with_hubs(hubs: &[DurableOutputHubId]) -> (DesktopProfileStore, Dir) {
    let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "gnosix-profile-authority-{}-{}",
        std::process::id(),
        id
    ));
    let profiles = root.join("profiles");
    let identity_dir = root.join("identity");
    std::fs::create_dir_all(&profiles).expect("create profiles dir");
    std::fs::create_dir_all(&identity_dir).expect("create identity dir");

    let output_hubs: Vec<serde_json::Value> = hubs
        .iter()
        .map(|h| serde_json::to_value(h).expect("hub"))
        .collect();
    let audit: Vec<serde_json::Value> = hubs
        .iter()
        .enumerate()
        .map(|(index, h)| {
            serde_json::json!({
                "kind": "hub_created",
                "revision": index as u64 + 1,
                "output_hub_id": h,
                "authority_reference": format!("fixture-{index}")
            })
        })
        .collect();
    let registry = serde_json::json!({
        "schema_version": 1,
        "revision": hubs.len() as u64,
        "next_record_id": 1,
        "output_hubs": output_hubs,
        "bindings": [],
        "audit": audit
    });
    std::fs::write(
        identity_dir.join("output-identities-v1.json"),
        serde_json::to_vec_pretty(&registry).expect("registry json"),
    )
    .expect("write identity registry");
    std::fs::write(
        identity_dir.join("output-identities-v1.issued"),
        serde_json::to_vec(hubs).expect("issued json"),
    )
    .expect("write issued record");
    std::fs::write(
        identity_dir.join("output-identities-v1.initialized"),
        b"fixture\n",
    )
    .expect("write initialized marker");

    let identity_store =
        in_gnu_os_output_identity::FilesystemOutputIdentityStore::new(&identity_dir);
    (
        DesktopProfileStore::new(&profiles, identity_store),
        Dir(root),
    )
}

fn store_in_temp() -> (DesktopProfileStore, Dir) {
    store_with_hubs(&[hub(1), hub(2), hub(3), hub(4), hub(5), hub(7), hub(8)])
}

fn desired_with_scale(output: DurableOutputHubId, basis_points: u32) -> DesiredDesktopState {
    DesiredDesktopState::new().with_output_preferences(
        output,
        DesiredOutputPreferences {
            text_scale: Some(TextScale::try_from(basis_points).expect("valid text scale")),
            ..Default::default()
        },
    )
}

fn desired_with_bar_edge(
    output: DurableOutputHubId,
    edge: BarEdgePreference,
) -> DesiredDesktopState {
    DesiredDesktopState::new().with_output_preferences(
        output,
        DesiredOutputPreferences {
            bar_edge: Some(edge),
            ..Default::default()
        },
    )
}

// --- the diff --------------------------------------------------------------

#[test]
fn a_desired_state_equal_to_the_current_state_has_an_empty_diff() {
    let document = DesktopProfileDocument::default();
    let desired = desired_with_scale(hub(1), 12_000);
    let seeded = desired.to_document_for_test(&document);

    let diff = diff_document(&seeded, &desired);
    assert!(
        diff.is_empty(),
        "re-sending unchanged state is not a change"
    );
    assert_eq!(diff.summary(), "NO_CHANGE");
}

#[test]
fn a_changed_text_scale_is_named_in_the_diff() {
    let document = DesktopProfileDocument::default();
    let before = desired_with_scale(hub(1), 10_000);
    let after = desired_with_scale(hub(1), 12_000);

    let diff = diff_document(&before.to_document_for_test(&document), &after);
    assert!(!diff.is_empty());
    assert_eq!(diff.summary(), "TEXT_SCALE");
}

#[test]
fn a_changed_bar_edge_is_named_in_the_diff() {
    let document = DesktopProfileDocument::default();
    let before = desired_with_bar_edge(hub(1), BarEdgePreference::Top);
    let after = desired_with_bar_edge(hub(1), BarEdgePreference::Left);

    let diff = diff_document(&before.to_document_for_test(&document), &after);
    assert_eq!(diff.summary(), "BAR_EDGE");
    assert_eq!(diff.changes, [ProfileFieldChange::BarEdge]);
}

#[test]
fn each_preference_is_named_independently() {
    let document = DesktopProfileDocument::default();
    let before = DesiredDesktopState::new().with_output_preferences(
        hub(2),
        DesiredOutputPreferences {
            color_scheme: Some(ColorSchemePreference::Dark),
            reduced_motion: None,
            text_scale: Some(TextScale::try_from(10_000_u32).expect("scale")),
            bar_edge: None,
        },
    );
    let after = DesiredDesktopState::new().with_output_preferences(
        hub(2),
        DesiredOutputPreferences {
            color_scheme: Some(ColorSchemePreference::Light),
            reduced_motion: Some(ReducedMotionPreference::Reduce),
            text_scale: Some(TextScale::try_from(10_000_u32).expect("scale")),
            bar_edge: None,
        },
    );

    let diff = diff_document(&before.to_document_for_test(&document), &after);
    assert_eq!(diff.summary(), "COLOR_SCHEME,REDUCED_MOTION");
    assert!(
        !diff.changes.contains(&ProfileFieldChange::TextScale),
        "an unchanged preference must not appear in the diff"
    );
}

#[test]
fn adding_an_output_is_named_as_a_profile_addition() {
    let document = DesktopProfileDocument::default();
    let before = desired_with_scale(hub(1), 10_000);
    let after = before
        .clone()
        .with_output_preferences(hub(2), DesiredOutputPreferences::default());

    let diff = diff_document(&before.to_document_for_test(&document), &after);
    assert!(diff.changes.contains(&ProfileFieldChange::ProfileAdded));
    assert!(diff.changes.contains(&ProfileFieldChange::OutputSelection));
}

#[test]
fn the_diff_is_deterministic_for_the_same_intent() {
    let document = DesktopProfileDocument::default();
    let before = desired_with_scale(hub(1), 10_000);
    let after = desired_with_scale(hub(1), 12_000);
    let seeded = before.to_document_for_test(&document);

    let first = diff_document(&seeded, &after);
    let second = diff_document(&seeded, &after);
    assert_eq!(first, second, "the same intent must produce the same diff");
}

// --- per-output identity --------------------------------------------------

#[test]
fn each_output_resolves_to_its_own_profile() {
    let left = desired_with_scale(hub(1), 10_000);
    let right = desired_with_scale(hub(2), 20_000);
    let combined = left.clone().with_output_preferences(
        hub(2),
        DesiredOutputPreferences {
            text_scale: Some(TextScale::try_from(20_000_u32).expect("scale")),
            ..Default::default()
        },
    );

    let left_profile = left.output_selections[&hub(1)];
    let right_profile = combined.output_selections[&hub(2)];
    assert_ne!(
        left_profile, right_profile,
        "a preference must not migrate between outputs"
    );
    assert_eq!(right.output_selections[&hub(2)], right_profile);
}

// --- the authority ---------------------------------------------------------

#[test]
fn a_first_mutation_against_an_absent_document_commits() {
    let (store, _dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);
    let expected = authority
        .current_version()
        .expect("version of absent store");

    let desired = desired_with_scale(hub(7), 12_500);
    let receipt = authority
        .apply(&desired, &expected)
        .expect("first mutation applies");

    assert_eq!(receipt.code(), "COMMITTED");
    assert_eq!(receipt.diff().summary(), "PROFILE_ADDED,OUTPUT_SELECTION");
    assert_eq!(
        authority
            .current_version()
            .expect("version after commit")
            .revision(),
        Some(1),
        "the first commit must advance the revision to one"
    );
}

#[test]
fn re_applying_the_same_desired_state_writes_nothing() {
    let (store, _dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);
    let desired = desired_with_scale(hub(7), 12_500);

    let first = authority
        .apply(&desired, &authority.current_version().expect("version"))
        .expect("first apply");
    assert_eq!(first.code(), "COMMITTED");

    let second = authority
        .apply(&desired, &authority.current_version().expect("version"))
        .expect("second apply");
    assert_eq!(
        second.code(),
        "ALREADY_SATISFIED",
        "a UI re-sending unchanged state must not consume a revision"
    );
    assert_eq!(
        authority
            .current_version()
            .expect("version after second apply")
            .revision(),
        Some(1),
        "no revision may be consumed by a no-op mutation"
    );
}

#[test]
fn a_mutation_authorised_against_a_stale_version_is_refused() {
    let (store, _dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);
    let desired = desired_with_scale(hub(7), 12_500);

    // Observe a version, then let someone else advance the document.
    let observed = authority.current_version().expect("version");
    authority
        .apply(&desired, &authority.current_version().expect("version"))
        .expect("someone else commits");

    // The first caller now acts on a version it no longer holds.
    let error = authority
        .apply(&desired_with_scale(hub(8), 11_000), &observed)
        .expect_err("a stale authorisation must not overwrite");
    assert!(
        matches!(
            error,
            ProfileStoreError::StaleVersion { .. } | ProfileStoreError::NonAdvancingRevision { .. }
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn concurrent_bar_edge_writers_use_profile_cas_and_do_not_last_write_win() {
    let (store_a, dir) = store_in_temp();
    let identity =
        in_gnu_os_output_identity::FilesystemOutputIdentityStore::new(dir.0.join("identity"));
    let store_b = DesktopProfileStore::new(dir.0.join("profiles"), identity);
    let first = DesktopProfileAuthority::issue(store_a);
    let stale = DesktopProfileAuthority::issue(store_b);
    let start = first.current_version().unwrap();
    let stale_start = stale.current_version().unwrap();
    assert_eq!(start, stale_start);

    first
        .apply(
            &desired_with_bar_edge(hub(7), BarEdgePreference::Right),
            &start,
        )
        .expect("first edge update commits");
    let error = stale
        .apply(
            &desired_with_bar_edge(hub(7), BarEdgePreference::Bottom),
            &stale_start,
        )
        .expect_err("second writer with stale profile CAS must conflict");
    assert!(matches!(error, ProfileStoreError::StaleVersion { .. }));

    let document = first.current_document().unwrap();
    assert_eq!(
        document.profiles[0].preferences.bar.edge,
        Some(BarEdgePreference::Right)
    );
}

#[test]
fn explicit_recovery_retains_the_last_known_bar_edge() {
    let (store, dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);
    authority
        .apply(
            &desired_with_bar_edge(hub(7), BarEdgePreference::Top),
            &authority.current_version().unwrap(),
        )
        .expect("first profile generation");
    authority
        .apply(
            &desired_with_bar_edge(hub(7), BarEdgePreference::Bottom),
            &authority.current_version().unwrap(),
        )
        .expect("second profile generation");

    let reloaded = DesktopProfileStore::new(
        dir.0.join("profiles"),
        in_gnu_os_output_identity::FilesystemOutputIdentityStore::new(dir.0.join("identity")),
    );
    assert!(matches!(
        reloaded.load().unwrap(),
        ProfileLoadOutcome::Current { document, .. }
            if document.profiles[0].preferences.bar.edge == Some(BarEdgePreference::Bottom)
    ));

    let current_path = dir.0.join("profiles/desktop-profiles-v1.json");
    std::fs::write(&current_path, b"{ damaged").unwrap();
    let store = DesktopProfileStore::new(
        dir.0.join("profiles"),
        in_gnu_os_output_identity::FilesystemOutputIdentityStore::new(dir.0.join("identity")),
    );
    let ProfileLoadOutcome::Recoverable {
        previous, token, ..
    } = store.load().unwrap()
    else {
        panic!("expected recoverable previous profile generation");
    };
    assert_eq!(
        previous.profiles[0].preferences.bar.edge,
        Some(BarEdgePreference::Top)
    );
    store
        .open_writer()
        .recover(&token, "donor-09-test")
        .unwrap();
    let ProfileLoadOutcome::Current { document, .. } = store.load().unwrap() else {
        panic!("explicit recovery should install the known-good generation");
    };
    assert_eq!(
        document.profiles[0].preferences.bar.edge,
        Some(BarEdgePreference::Top)
    );
}

#[test]
fn a_preview_reports_the_diff_without_writing_anything() {
    let (store, _dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);

    let before = authority.current_version().expect("version").revision();
    let diff = authority
        .preview(&desired_with_scale(hub(3), 13_000))
        .expect("preview");
    assert!(!diff.is_empty());
    assert_eq!(
        authority.current_version().expect("version").revision(),
        before,
        "a preview must not mutate the document"
    );
}

#[test]
fn the_receipt_names_both_versions() {
    let (store, _dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);
    let expected = authority.current_version().expect("version");

    let receipt = authority
        .apply(&desired_with_scale(hub(4), 10_500), &expected)
        .expect("apply");
    let summary = receipt.summary();

    assert!(summary.contains("outcome=COMMITTED"), "{summary}");
    assert!(summary.contains("current_revision=1"), "{summary}");
    assert!(summary.contains("PROFILE_ADDED"), "{summary}");
}

// --- the boundary ----------------------------------------------------------

#[test]
fn the_authority_exposes_no_way_to_obtain_the_raw_writer() {
    // A compile-time statement would be stronger, but this pins the intent in
    // a form a reviewer reads: the public surface of the designated authority
    // is the typed operations below, and nothing returns a
    // DesktopProfileWriter.
    //
    // If someone adds a getter, this test still compiles, which is exactly
    // why the guarantee belongs in the type signatures too - the writer's
    // constructor is private and `open_writer` is `pub(crate)`.
    let (store, _dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);

    // Everything reachable from outside the crate goes through these.
    let _ = authority.current_version();
    let _ = authority.preview(&DesiredDesktopState::new());
}

#[test]
fn a_damaged_store_refuses_a_mutation_rather_than_computing_a_diff_against_it() {
    // Writing a desired state over a document that needs recovery would
    // discard the recoverable candidate, so a damaged store is not readable
    // for diff purposes.
    let (store, dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);

    // Establish a valid document.
    authority
        .apply(
            &desired_with_scale(hub(5), 10_000),
            &authority.current_version().expect("version"),
        )
        .expect("seed");

    // Corrupt the stored file.
    let path = dir.0.join("profiles").join("desktop-profiles-v1.json");
    let existing = std::fs::read_to_string(&path).expect("read seeded profile");
    std::fs::write(&path, existing.replace('{', "{{")).expect("corrupt profile");

    let error = authority
        .current_document()
        .expect_err("a damaged store must not present a readable document");
    assert!(
        matches!(error, ProfileStoreError::RecoveryRequired(_)),
        "unexpected error: {error:?}"
    );

    let error = authority.apply(
        &desired_with_scale(hub(5), 20_000),
        &authority
            .current_version()
            .unwrap_or(ProfileVersion::Absent),
    );
    assert!(
        error.is_err(),
        "a mutation against a damaged store must not proceed"
    );
}

#[test]
fn bounded_bar_writer_preserves_other_profiles_and_preferences() {
    let (store, dir) = store_in_temp();
    let authority = DesktopProfileAuthority::issue(store);
    let desired = desired_with_scale(hub(1), 12500)
        .with_output_preferences(hub(2), DesiredOutputPreferences::default());
    authority.apply(&desired, &ProfileVersion::Absent).unwrap();
    let before = authority.current_document().unwrap();
    let mut writer = crate::bar_writer::BarProfileWriter::new(
        DesktopProfileStore::new(
            dir.0.join("profiles"),
            in_gnu_os_output_identity::FilesystemOutputIdentityStore::new(dir.0.join("identity")),
        ),
        DesktopProfileId::from_bytes([1; 16]),
        [8; 16],
    );
    let snapshot = writer.read().unwrap();
    let request = crate::SetBarEdge {
        request_id: [9; 16],
        epoch: snapshot.epoch,
        profile_id: snapshot.profile_id,
        expected: snapshot.version.clone(),
        edge: BarEdgePreference::Right,
    };
    let result = writer.apply(request.clone());
    assert_eq!(result.code, crate::BarWriteCode::Applied);
    assert_eq!(writer.apply(request.clone()), result);
    let mut changed = request.clone();
    changed.edge = BarEdgePreference::Left;
    assert_eq!(writer.apply(changed).code, crate::BarWriteCode::Invalid);
    let mut stale = request;
    stale.request_id = [10; 16];
    assert_eq!(writer.apply(stale).code, crate::BarWriteCode::Conflict);
    let mut after = authority.current_document().unwrap();
    assert_eq!(
        after.profiles[0].preferences.bar.edge,
        Some(BarEdgePreference::Right)
    );
    after.profiles[0].preferences.bar.edge = before.profiles[0].preferences.bar.edge;
    after.revision = before.revision;
    assert_eq!(after, before);
}

fn bounded_fixture() -> (crate::bar_writer::BarProfileWriter, Dir) {
    let (store, dir) = store_in_temp();
    DesktopProfileAuthority::issue(store)
        .apply(&desired_with_scale(hub(1), 12500), &ProfileVersion::Absent)
        .unwrap();
    (reopen_bar_writer(&dir, [8; 16]), dir)
}
fn reopen_bar_writer(dir: &Dir, epoch: [u8; 16]) -> crate::bar_writer::BarProfileWriter {
    crate::bar_writer::BarProfileWriter::new(
        DesktopProfileStore::new(
            dir.0.join("profiles"),
            in_gnu_os_output_identity::FilesystemOutputIdentityStore::new(dir.0.join("identity")),
        ),
        DesktopProfileId::from_bytes([1; 16]),
        epoch,
    )
}
fn bar_request(writer: &crate::bar_writer::BarProfileWriter) -> crate::SetBarEdge {
    let snapshot = writer.read().unwrap();
    crate::SetBarEdge {
        request_id: [9; 16],
        epoch: snapshot.epoch,
        profile_id: snapshot.profile_id,
        expected: snapshot.version,
        edge: BarEdgePreference::Bottom,
    }
}
#[test]
fn bounded_writer_two_instances_preserve_exact_revision_and_digest_cas() {
    let (mut first, dir) = bounded_fixture();
    let mut second = reopen_bar_writer(&dir, [8; 16]);
    let request = bar_request(&first);
    assert_eq!(
        first.apply(request.clone()).code,
        crate::BarWriteCode::Applied
    );
    assert_eq!(second.apply(request).code, crate::BarWriteCode::Conflict);
    let mut request = bar_request(&first);
    request.request_id = [10; 16];
    if let ProfileVersion::Current { etag, .. } = &mut request.expected {
        etag[0] ^= 1;
    }
    let before = first.read().unwrap();
    assert_eq!(first.apply(request).code, crate::BarWriteCode::Conflict);
    assert_eq!(first.read().unwrap(), before);
}
#[test]
fn bounded_writer_rejects_other_profile_and_prior_host_epoch_without_mutation() {
    let (mut writer, _dir) = bounded_fixture();
    let before = writer.read().unwrap();
    let mut request = bar_request(&writer);
    request.profile_id = DesktopProfileId::from_bytes([2; 16]);
    assert_eq!(writer.apply(request).code, crate::BarWriteCode::Invalid);
    let mut request = bar_request(&writer);
    request.epoch = [7; 16];
    assert_eq!(
        writer.apply(request).code,
        crate::BarWriteCode::AuthorityUnavailable
    );
    assert_eq!(writer.read().unwrap(), before);
}
#[test]
fn bounded_writer_restart_preserves_durable_edge_and_refuses_old_epoch_replay() {
    let (mut writer, dir) = bounded_fixture();
    let request = bar_request(&writer);
    let receipt = writer.apply(request.clone());
    assert_eq!(receipt.code, crate::BarWriteCode::Applied);
    drop(writer);
    let mut restarted = reopen_bar_writer(&dir, [10; 16]);
    let snapshot = restarted.read().unwrap();
    assert_eq!(snapshot.edge, Some(BarEdgePreference::Bottom));
    assert_eq!(Some(snapshot.version), receipt.version);
    assert_eq!(
        restarted.apply(request).code,
        crate::BarWriteCode::AuthorityUnavailable
    );
    let request = bar_request(&restarted);
    let result = restarted.apply(request);
    assert_eq!(result.code, crate::BarWriteCode::AlreadySatisfied);
    assert_eq!(result.version, receipt.version);
}
#[test]
fn bounded_writer_recovery_refusal_never_replaces_corrupt_bytes() {
    let (mut writer, dir) = bounded_fixture();
    let request = bar_request(&writer);
    let path = dir
        .0
        .join("profiles")
        .join(DESKTOP_PROFILE_DOCUMENT_FILENAME);
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(writer.read().is_err());
    assert_eq!(
        writer.apply(request).code,
        crate::BarWriteCode::AuthorityUnavailable
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
}
#[test]
fn bounded_writer_invalid_r11_cannot_produce_normal_projection_or_commit() {
    let (mut writer, dir) = bounded_fixture();
    let request = bar_request(&writer);
    let path = dir
        .0
        .join("profiles")
        .join(DESKTOP_PROFILE_DOCUMENT_FILENAME);
    let before = std::fs::read(&path).unwrap();
    std::fs::write(dir.0.join("identity/output-identities-v1.json"), b"corrupt").unwrap();
    assert!(writer.read().is_err());
    assert_eq!(
        writer.apply(request).code,
        crate::BarWriteCode::AuthorityUnavailable
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn bounded_writer_replay_capacity_fails_closed_without_eviction() {
    let (mut writer, _dir) = bounded_fixture();
    let first = bar_request(&writer);
    let receipt = writer.apply(first.clone());
    for i in 0_u128..1023 {
        let mut request = bar_request(&writer);
        request.request_id = i.to_le_bytes();
        assert_eq!(
            writer.apply(request).code,
            crate::BarWriteCode::AlreadySatisfied
        );
    }
    let mut request = bar_request(&writer);
    request.request_id = [17; 16];
    assert_eq!(
        writer.apply(request).code,
        crate::BarWriteCode::AuthorityUnavailable
    );
    assert_eq!(writer.apply(first), receipt);
}

fn initial_profile_plan() -> InitialProfileProvisionPlan {
    let profile_id = DesktopProfileId::from_bytes([42; 16]);
    InitialProfileProvisionPlan {
        profile: DesktopProfile {
            id: profile_id,
            name: "Personal".into(),
            preferences: DesktopPreferences::default(),
        },
        output_selections: vec![
            OutputProfileSelection {
                output_hub_id: hub(1),
                profile_id,
            },
            OutputProfileSelection {
                output_hub_id: hub(2),
                profile_id,
            },
        ],
    }
}

#[test]
fn initial_profile_provisioner_commits_absent_store_and_projects_both_outputs() {
    let (store, _dir) = store_with_hubs(&[hub(1), hub(2)]);
    let receipt = provision_initial_profile(&store, &initial_profile_plan()).unwrap();
    assert_eq!(receipt.status, InitialProfileProvisionStatus::Provisioned);
    assert_eq!(receipt.version.revision(), Some(1));
    for output in [hub(1), hub(2)] {
        assert!(matches!(
            store.project_output(output).unwrap(),
            ProfileProjectionOutcome::Selected { profile_revision: 1, profile, .. }
                if profile.id == DesktopProfileId::from_bytes([42; 16])
                    && profile.name == "Personal"
                    && profile.preferences == DesktopPreferences::default()
        ));
    }
}

#[test]
fn initial_profile_provisioner_is_idempotent_for_exact_existing_document() {
    let (store, _dir) = store_with_hubs(&[hub(1), hub(2)]);
    let plan = initial_profile_plan();
    provision_initial_profile(&store, &plan).unwrap();
    let second = provision_initial_profile(&store, &plan).unwrap();
    assert_eq!(
        second.status,
        InitialProfileProvisionStatus::AlreadyProvisioned
    );
    assert_eq!(second.version.revision(), Some(1));
}

#[test]
fn initial_profile_provisioner_refuses_divergent_current_document_without_mutation() {
    let (store, _dir) = store_with_hubs(&[hub(1), hub(2)]);
    let other_id = DesktopProfileId::from_bytes([9; 16]);
    let divergent = DesktopProfileDocument {
        schema_version: DESKTOP_PROFILE_SCHEMA_VERSION,
        revision: 1,
        profiles: vec![DesktopProfile {
            id: other_id,
            name: "Other".into(),
            preferences: DesktopPreferences::default(),
        }],
        output_profile_selections: vec![OutputProfileSelection {
            output_hub_id: hub(1),
            profile_id: other_id,
        }],
    };
    store
        .open_writer()
        .commit(&ProfileVersion::Absent, &divergent)
        .unwrap();
    let before = store.load().unwrap();
    assert!(matches!(
        provision_initial_profile(&store, &initial_profile_plan()),
        Err(InitialProfileProvisionError::DivergentExistingState { revision: 1 })
    ));
    assert_eq!(store.load().unwrap(), before);
}

#[test]
fn initial_profile_provisioner_refuses_recovery_state() {
    let (store, dir) = store_with_hubs(&[hub(1), hub(2)]);
    let plan = initial_profile_plan();
    provision_initial_profile(&store, &plan).unwrap();
    let second = DesktopProfileDocument {
        revision: 2,
        ..Default::default()
    };
    let current_version = match store.load().unwrap() {
        ProfileLoadOutcome::Current { version, .. } => version,
        other => panic!("expected current profile before recovery fixture, got {other:?}"),
    };
    store
        .open_writer()
        .commit(&current_version, &second)
        .unwrap();
    std::fs::write(
        dir.0
            .join("profiles")
            .join(DESKTOP_PROFILE_DOCUMENT_FILENAME),
        b"broken",
    )
    .unwrap();
    assert!(matches!(
        provision_initial_profile(&store, &plan),
        Err(InitialProfileProvisionError::RecoveryRequired(_))
    ));
}

fn donor_overrides_request(
    writer: &crate::bar_writer::BarProfileWriter,
    request_id: [u8; 16],
    preferences: BarPreferenceOverrides,
) -> crate::SetBarPreferenceOverrides {
    let snapshot = writer.read().unwrap();
    crate::SetBarPreferenceOverrides {
        request_id,
        epoch: snapshot.epoch,
        profile_id: snapshot.profile_id,
        expected: snapshot.version,
        preferences,
    }
}

#[test]
fn bounded_writer_applies_and_clears_only_the_five_donor_overrides() {
    let (mut writer, _dir) = bounded_fixture();
    let edge = bar_request(&writer);
    assert_eq!(writer.apply(edge).code, crate::BarWriteCode::Applied);

    let desired = BarPreferenceOverrides {
        persistent: Some(false),
        show_on_hover: Some(true),
        workspace_count: Some(WorkspaceDisplayCount::try_from(7_i64).unwrap()),
        workspace_display: Some(WorkspaceDisplayMode::Icons),
        tray_compact: Some(true),
    };
    let request = donor_overrides_request(&writer, [10; 16], desired);
    let receipt = writer.apply_overrides(request.clone());
    assert_eq!(receipt.code, crate::BarWriteCode::Applied);
    let snapshot = writer.read().unwrap();
    assert_eq!(snapshot.edge, Some(BarEdgePreference::Bottom));
    assert_eq!(snapshot.preferences, desired);
    assert_eq!(writer.apply_overrides(request), receipt);

    let clear = donor_overrides_request(&writer, [11; 16], BarPreferenceOverrides::default());
    assert_eq!(
        writer.apply_overrides(clear).code,
        crate::BarWriteCode::Applied
    );
    let snapshot = writer.read().unwrap();
    assert_eq!(snapshot.edge, Some(BarEdgePreference::Bottom));
    assert_eq!(snapshot.preferences, BarPreferenceOverrides::default());
}

#[test]
fn donor_override_batch_uses_the_same_profile_cas_and_cross_operation_replay_domain() {
    let (mut first, dir) = bounded_fixture();
    let mut second = reopen_bar_writer(&dir, [8; 16]);
    let desired = BarPreferenceOverrides {
        persistent: Some(true),
        ..Default::default()
    };
    let request = donor_overrides_request(&first, [12; 16], desired);
    assert_eq!(
        first.apply_overrides(request.clone()).code,
        crate::BarWriteCode::Applied
    );
    assert_eq!(
        second.apply_overrides(request).code,
        crate::BarWriteCode::Conflict
    );

    let edge = bar_request(&first);
    assert_eq!(first.apply(edge).code, crate::BarWriteCode::Applied);
    let collision = donor_overrides_request(&first, [9; 16], BarPreferenceOverrides::default());
    assert_eq!(
        first.apply_overrides(collision).code,
        crate::BarWriteCode::Invalid
    );
}
