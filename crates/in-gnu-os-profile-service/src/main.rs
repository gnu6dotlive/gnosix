use in_gnu_os_desktop_profiles::{
    DesktopProfileId, DesktopProfileStore, bar_writer::BarProfileWriter,
};
use in_gnu_os_output_identity::FilesystemOutputIdentityStore;
use std::path::PathBuf;
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (profiles, identities, profile_id) = deployment_config(&args)?;
    let writer = BarProfileWriter::new(
        DesktopProfileStore::new(profiles, FilesystemOutputIdentityStore::new(identities)),
        profile_id,
        *uuid::Uuid::new_v4().as_bytes(),
    );
    // No bootstrap, profile creation, R11 mutation, or implicit recovery.
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
}
