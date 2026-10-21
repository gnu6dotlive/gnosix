//! Test harness only: provision one profile into an ISOLATED R12 store.
//!
//! Both directories are required, absolute and distinct; nothing defaults to
//! a real user path. The identity directory must already hold the
//! repository's R11 fixture identity (hub `[1; 16]`); the profile selects it.
//! Prints the profile id the authority is then started with.
use in_gnu_os_desktop_profiles::{
    provision_initial_profile, DesktopProfile, DesktopProfileId, DesktopProfileStore,
    InitialProfileProvisionPlan, OutputProfileSelection,
};
use in_gnu_os_output_identity::{DurableOutputHubId, FilesystemOutputIdentityStore};
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [flag_p, profiles, flag_i, identities] = args.as_slice() else {
        return Err("usage: --profile-directory ABSOLUTE --identity-directory ABSOLUTE".into());
    };
    if flag_p != "--profile-directory" || flag_i != "--identity-directory" {
        return Err("usage: --profile-directory ABSOLUTE --identity-directory ABSOLUTE".into());
    }
    let (profiles, identities) = (PathBuf::from(profiles), PathBuf::from(identities));
    if !profiles.is_absolute() || !identities.is_absolute() || profiles == identities {
        return Err("distinct absolute directories required".into());
    }
    let profile_id = DesktopProfileId::from_bytes([0x5a; 16]);
    let store = DesktopProfileStore::new(profiles, FilesystemOutputIdentityStore::new(identities));
    let receipt = provision_initial_profile(
        &store,
        &InitialProfileProvisionPlan {
            profile: DesktopProfile {
                id: profile_id,
                name: "Harness".into(),
                preferences: Default::default(),
            },
            output_selections: vec![OutputProfileSelection {
                output_hub_id: DurableOutputHubId::from_bytes([1; 16]),
                profile_id,
            }],
        },
    )
    .map_err(|e| e.to_string())?;
    println!(
        "PRIVATE_PROFILE status={:?} profile_id={}",
        receipt.status,
        "5a".repeat(16)
    );
    Ok(())
}
