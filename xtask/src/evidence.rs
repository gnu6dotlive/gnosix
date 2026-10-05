//! Explicit evidence export.
//!
//! Retaining evidence is a deliberate act with a chosen destination, never a
//! side effect of running a check. The earlier Phase 4 scripts wrote straight
//! into tracked paths and truncated their logs on each run, which meant that
//! verifying a commit dirtied the tree, which meant the retained log could
//! never match the state being verified, which produced a commit-per-run
//! cycle. §58 asks for that cycle to be broken permanently, and the way to
//! break it is to make default output go somewhere ignored and make retention
//! an explicit, atomic, non-overwriting operation.
//!
//! The rules, and why each exists:
//!
//! - **Write to a temporary file, then rename.** A reader can never observe a
//!   half-written evidence file, and a crash mid-export cannot corrupt a
//!   previously good one.
//! - **Refuse an existing destination.** Silently overwriting a committed
//!   receipt destroys evidence. Overwriting is possible, but only when asked
//!   for explicitly and it is recorded in the output.
//! - **Never write into a path git is tracking**, unless overwriting was
//!   explicitly requested. This is the check that would have prevented the
//!   self-dirty cycle from recurring by accident.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

/// What an export did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportOutcome {
    Wrote {
        destination: PathBuf,
    },
    /// An explicit overwrite replaced an existing file.
    Overwrote {
        destination: PathBuf,
    },
    /// Nothing to do, because the source was absent.
    Skipped {
        reason: String,
    },
}

impl ExportOutcome {
    pub fn describe(&self) -> String {
        match self {
            Self::Wrote { destination } => format!("wrote {}", destination.display()),
            Self::Overwrote { destination } => {
                format!("overwrote {}", destination.display())
            }
            Self::Skipped { reason } => format!("skipped: {reason}"),
        }
    }
}

/// Whether `path` is tracked by git.
///
/// A dirty-tree invariant is only meaningful if export refuses to land in
/// tracked space without being told to.
pub fn is_tracked(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    Command::new("git")
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(relative)
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Write `contents` to `destination` atomically.
pub fn write_atomic(
    destination: &Path,
    contents: &str,
    overwrite: bool,
) -> Result<ExportOutcome, String> {
    if let Some(parent) = destination.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    if destination.exists() && !overwrite {
        return Err(format!(
            "refusing to overwrite existing evidence at {}; pass --overwrite to replace it",
            destination.display()
        ));
    }
    let existed = destination.exists();

    // Same-directory temporary file, so the rename is atomic: a rename across
    // filesystems is a copy, and a reader could see a partial file.
    let temporary = destination.with_extension(format!(
        "{}.tmp",
        destination
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));

    {
        let mut file = fs::File::create(&temporary)
            .map_err(|error| format!("could not create {}: {error}", temporary.display()))?;
        file.write_all(contents.as_bytes())
            .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("could not flush {}: {error}", temporary.display()))?;
    }

    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("could not rename into {}: {error}", destination.display())
    })?;

    Ok(if existed {
        ExportOutcome::Overwrote {
            destination: destination.to_path_buf(),
        }
    } else {
        ExportOutcome::Wrote {
            destination: destination.to_path_buf(),
        }
    })
}

/// Export a file into a destination directory, honouring the tracking rule.
pub fn export_file(
    root: &Path,
    source: &Path,
    destination_dir: &Path,
    name: &str,
    overwrite: bool,
) -> Result<ExportOutcome, String> {
    if !source.exists() {
        return Ok(ExportOutcome::Skipped {
            reason: format!("{} does not exist", source.display()),
        });
    }
    let destination = destination_dir.join(name);
    if !overwrite && is_tracked(root, &destination) {
        return Err(format!(
            "refusing to write tracked evidence at {}; pass --overwrite to replace it",
            destination.display()
        ));
    }
    let contents = fs::read_to_string(source)
        .map_err(|error| format!("could not read {}: {error}", source.display()))?;
    write_atomic(&destination, &contents, overwrite)
}

/// Export a whole log directory, file by file.
pub fn export_directory(
    root: &Path,
    source_dir: &Path,
    destination_dir: &Path,
    overwrite: bool,
) -> Result<Vec<ExportOutcome>, String> {
    if !source_dir.exists() {
        return Ok(Vec::new());
    }
    let mut outcomes = Vec::new();
    let entries = fs::read_dir(source_dir)
        .map_err(|error| format!("could not read {}: {error}", source_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read entry: {error}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        outcomes.push(export_file(
            root,
            &path,
            destination_dir,
            &name.to_string_lossy(),
            overwrite,
        )?);
    }
    Ok(outcomes)
}
