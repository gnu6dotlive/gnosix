use in_gnu_os_desktop_profiles::{
    DesktopProfileId, DesktopProfileSeed, DesktopProfileStore, bar_writer::BarProfileWriter,
};
use in_gnu_os_output_identity::{FilesystemOutputIdentityStore, OutputIdentityStore};
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (profiles, identities, profile_id) = deployment_config(&args)?;
    let identity_store = FilesystemOutputIdentityStore::new(identities);
    let store = DesktopProfileStore::new(profiles, identity_store.clone());

    bootstrap_if_absent(&store, &identity_store, profile_id)?;

    let writer = BarProfileWriter::new(store, profile_id, *uuid::Uuid::new_v4().as_bytes());
    writer
        .read()
        .map_err(|e| format!("authority cannot start: {e}"))?;
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
    ])
    .map_err(|e| e.to_string())?;
    let _connection = in_gnu_os_profile_service::serve(writer).map_err(|e| e.to_string())?;
    // Blocking signal iterator; no perpetual polling loop or compositor dependency.
    let _ = signals.forever().next();
    Ok(())
}

fn resolve_seed() -> DesktopProfileSeed {
    for candidate in [
        "/usr/share/gnosix/profiles/default-profile-seed.json",
        "share/gnosix/profiles/default-profile-seed.json",
    ] {
        if let Ok(seed) = DesktopProfileSeed::from_json_file(candidate) {
            return seed;
        }
    }
    DesktopProfileSeed::default()
}

fn bootstrap_if_absent(
    store: &DesktopProfileStore,
    identity_store: &FilesystemOutputIdentityStore,
    profile_id: DesktopProfileId,
) -> Result<(), String> {
    match store
        .load()
        .map_err(|e| format!("cannot inspect profile store state: {e}"))?
    {
        in_gnu_os_desktop_profiles::ProfileLoadOutcome::Current { .. } => Ok(()),
        in_gnu_os_desktop_profiles::ProfileLoadOutcome::Absent => {
            let output_hubs: Vec<_> = match identity_store.load() {
                Ok(in_gnu_os_output_identity::LoadOutcome::Current(doc)) => {
                    doc.output_hubs.into_iter().collect()
                }
                _ => Vec::new(),
            };
            let mut seed = resolve_seed();
            seed.profile.id = profile_id;
            let plan = seed.into_provision_plan(output_hubs);
            in_gnu_os_desktop_profiles::provision_initial_profile(store, &plan)
                .map_err(|e| format!("initial profile bootstrap failed: {e}"))?;
            Ok(())
        }
        in_gnu_os_desktop_profiles::ProfileLoadOutcome::Recoverable { issue, .. }
        | in_gnu_os_desktop_profiles::ProfileLoadOutcome::Unrecoverable { issue } => Err(format!(
            "store requires recovery before bootstrap: {issue:?}"
        )),
    }
}
fn deployment_config(args: &[String]) -> Result<(PathBuf, PathBuf, DesktopProfileId), String> {
    if args.len() != 6
        || args[0] != "--profile-directory"
        || args[2] != "--identity-directory"
        || args[4] != "--profile-id"
    {
        return Err("required deployment configuration: --profile-directory ABSOLUTE --identity-directory ABSOLUTE --profile-id HEX32".into());
    }
    let profiles = PathBuf::from(&args[1]);
    let identities = PathBuf::from(&args[3]);
    if !profiles.is_absolute() || !identities.is_absolute() || profiles == identities {
        return Err("distinct absolute authority store directories required".into());
    }
    let id = &args[5];
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid profile id".into());
    }
    let mut bytes = [0; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&id[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok((profiles, identities, DesktopProfileId::from_bytes(bytes)))
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("in.gnu.os profile authority: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args() -> Vec<String> {
        [
            "--profile-directory",
            "/isolated/profiles",
            "--identity-directory",
            "/isolated/identities",
            "--profile-id",
            "01010101010101010101010101010101",
        ]
        .map(str::to_owned)
        .to_vec()
    }
    #[test]
    fn deployment_has_no_implicit_paths_profile_or_bootstrap() {
        assert!(deployment_config(&[]).is_err());
        let config = deployment_config(&args()).unwrap();
        assert_eq!(config.2, DesktopProfileId::from_bytes([1; 16]));
        for (index, value) in [
            (1, "relative"),
            (3, "relative"),
            (5, "not_a_profile_id"),
            (0, "--command"),
            (3, "/isolated/profiles"),
        ] {
            let mut invalid = args();
            invalid[index] = value.into();
            assert!(deployment_config(&invalid).is_err());
        }
    }

    struct TestDir(PathBuf);
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn bootstrap_initializes_absent_store_with_identities_and_is_idempotent() {
        let unique = format!(
            "gnosix-profiled-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let _guard = TestDir(root.clone());
        let profiles_dir = root.join("profiles");
        let identities_dir = root.join("identities");
        std::fs::create_dir_all(&profiles_dir).unwrap();
        std::fs::create_dir_all(&identities_dir).unwrap();

        let hub = in_gnu_os_output_identity::DurableOutputHubId::from_bytes([5; 16]);
        let reg_doc = serde_json::json!({
            "schema_version": 1,
            "revision": 1,
            "next_record_id": 1,
            "output_hubs": [hub],
            "bindings": [],
            "audit": [
                {
                    "kind": "hub_created",
                    "revision": 1,
                    "output_hub_id": hub,
                    "authority_reference": "fixture-0"
                }
            ]
        });
        std::fs::write(
            identities_dir.join("output-identities-v1.json"),
            serde_json::to_vec_pretty(&reg_doc).unwrap(),
        )
        .unwrap();
        std::fs::write(
            identities_dir.join("output-identities-v1.initialized"),
            b"gnosix-output-identity-document-v1\n",
        )
        .unwrap();
        std::fs::write(
            identities_dir.join("output-identities-v1.issued"),
            serde_json::to_vec_pretty(&[hub]).unwrap(),
        )
        .unwrap();

        let identity_store = FilesystemOutputIdentityStore::new(&identities_dir);
        let store = DesktopProfileStore::new(&profiles_dir, identity_store.clone());
        let profile_id = DesktopProfileId::from_bytes([42; 16]);

        assert!(matches!(
            store.load().unwrap(),
            in_gnu_os_desktop_profiles::ProfileLoadOutcome::Absent
        ));

        bootstrap_if_absent(&store, &identity_store, profile_id).unwrap();

        let outcome = store.load().unwrap();
        let in_gnu_os_desktop_profiles::ProfileLoadOutcome::Current { document, version } = outcome
        else {
            panic!("expected current profile after bootstrap, got {outcome:?}");
        };
        assert_eq!(document.revision, 1);
        assert_eq!(document.profiles.len(), 1);
        assert_eq!(document.profiles[0].id, profile_id);
        assert_eq!(document.profiles[0].name, "Personal");
        assert_eq!(document.output_profile_selections.len(), 1);
        assert_eq!(document.output_profile_selections[0].output_hub_id, hub);
        assert_eq!(document.output_profile_selections[0].profile_id, profile_id);

        // BarProfileWriter can now read the initialized snapshot
        let writer = BarProfileWriter::new(
            DesktopProfileStore::new(&profiles_dir, identity_store.clone()),
            profile_id,
            *uuid::Uuid::new_v4().as_bytes(),
        );
        assert!(writer.read().is_ok());

        // Idempotency: second run does not error or modify
        bootstrap_if_absent(&store, &identity_store, profile_id).unwrap();
        let second_outcome = store.load().unwrap();
        let in_gnu_os_desktop_profiles::ProfileLoadOutcome::Current {
            document: doc2,
            version: ver2,
        } = second_outcome
        else {
            panic!("expected current profile on second call");
        };
        assert_eq!(doc2, document);
        assert_eq!(ver2, version);
    }
}
