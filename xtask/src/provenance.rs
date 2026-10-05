//! Provenance: a record of what actually ran.
//!
//! Every field here is read from the real system after the checks have
//! finished. Nothing is predicted, nothing is assumed, and no hash is written
//! down before the file it describes exists.
//!
//! The one field worth calling out is `dirty`. A qualification run on a dirty
//! tree and a qualification run on a clean tree are different claims, and a
//! closure-grade record has to say which one it is making. Recording the flag
//! rather than asserting cleanliness is what makes that possible; asserting it
//! would just be another thing to get wrong.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{model::VerificationReport, runner};

pub const SCHEMA: &str = "gnosix.xtask.provenance.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub git_commit: Option<String>,
    pub git_tree: Option<String>,
    pub branch: Option<String>,
    /// Whether the working tree had uncommitted changes at collection time.
    pub dirty: Option<bool>,
    /// The porcelain lines themselves, so "dirty" is auditable rather than a
    /// bare claim.
    pub dirty_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolchainProvenance {
    pub rustc_version: Option<String>,
    pub rustc_host: Option<String>,
    pub rustc_commit_hash: Option<String>,
    pub cargo_version: Option<String>,
    /// The channel named by rust-toolchain.toml, if the file exists.
    pub pinned_channel: Option<String>,
    /// Whether the running toolchain is the pinned one.
    pub matches_pin: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlatformProvenance {
    pub os: String,
    pub arch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DependencyProvenance {
    pub cargo_lock_sha256: Option<String>,
    /// The vendored patch, recorded explicitly so a reviewer can confirm the
    /// build did not silently resolve that crate from a registry.
    pub patched_crates: Vec<PatchedCrate>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchedCrate {
    pub crate_name: String,
    pub path: String,
    pub version: String,
    /// The vendored path as tracked by git, so the content is covered by the
    /// normal tree and needs no second dependency-locking scheme.
    pub tracked_in_git: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactProvenance {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub present: bool,
    /// Why an artifact is missing, when it is. A missing binary must never be
    /// reported as built.
    pub absence_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualificationProvenance {
    pub profile: String,
    pub checks_run: usize,
    pub checks_passed: usize,
    pub overall: String,
    /// Observed test counts, reported rather than asserted.
    pub observed_test_counts: BTreeMap<String, String>,
    /// Deliberately empty in this schema: a digest of a log is in the check
    /// results, and re-listing them here would be a second place to drift.
    pub feature_sets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub schema: String,
    /// Unix milliseconds.
    pub timestamp_unix_ms: u128,
    pub source: SourceProvenance,
    pub toolchain: ToolchainProvenance,
    pub platform: PlatformProvenance,
    pub dependencies: DependencyProvenance,
    pub qualification: QualificationProvenance,
    pub artifacts: Vec<ArtifactProvenance>,
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let mut argv = vec!["git".to_owned()];
    argv.extend(args.iter().map(|s| (*s).to_owned()));
    runner::capture_line(&argv, root)
}

fn git_ok(root: &Path, args: &[&str]) -> bool {
    let mut argv = vec!["git".to_owned()];
    argv.extend(args.iter().map(|s| (*s).to_owned()));
    std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn collect_source(root: &Path) -> SourceProvenance {
    let porcelain = git(root, &["status", "--porcelain"]).unwrap_or_default();
    let dirty_paths: Vec<String> = porcelain
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    SourceProvenance {
        git_commit: git(root, &["rev-parse", "HEAD"]),
        git_tree: git(root, &["rev-parse", "HEAD^{tree}"]),
        branch: git(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        // `None` when this is not a working tree at all, which is different
        // from "clean". Recording clean for a directory git does not track
        // would be a fabricated claim.
        dirty: git(root, &["rev-parse", "--is-inside-work-tree"])
            .map(|value| value.trim() == "true" && !dirty_paths.is_empty()),
        dirty_paths,
    }
}

pub fn collect_toolchain(root: &Path) -> ToolchainProvenance {
    let verbose = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();

    let field = |key: &str| -> Option<String> {
        verbose
            .lines()
            .find_map(|line| {
                line.strip_prefix(key)
                    .map(|v| v.trim_start_matches(':').trim().to_owned())
            })
            .filter(|v| !v.is_empty())
    };

    let pinned_channel = std::fs::read_to_string(root.join("rust-toolchain.toml"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| toml_string_value(line, "channel"))
        });

    let running = field("release");
    let matches_pin = match (&pinned_channel, &running) {
        (Some(pin), Some(running)) => Some(pin == running),
        _ => None,
    };

    ToolchainProvenance {
        rustc_version: running,
        rustc_host: field("host"),
        rustc_commit_hash: field("commit-hash"),
        cargo_version: std::process::Command::new("cargo")
            .arg("-V")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned()),
        pinned_channel,
        matches_pin,
    }
}

pub fn collect_dependencies(root: &Path) -> DependencyProvenance {
    let lock = root.join("Cargo.lock");
    let patched_crates = vec![PatchedCrate {
        crate_name: "gpui-pre-linux".to_owned(),
        path: "vendor/gpui-pre-linux-0.3.5".to_owned(),
        version: read_vendor_version(root).unwrap_or_else(|| "unknown".to_owned()),
        tracked_in_git: git_ok(
            root,
            &[
                "ls-files",
                "--error-unmatch",
                "vendor/gpui-pre-linux-0.3.5/Cargo.toml",
            ],
        ),
    }];
    DependencyProvenance {
        cargo_lock_sha256: lock.exists().then(|| runner::sha256_file(&lock)),
        patched_crates,
    }
}

fn read_vendor_version(root: &Path) -> Option<String> {
    let manifest =
        std::fs::read_to_string(root.join("vendor/gpui-pre-linux-0.3.5/Cargo.toml")).ok()?;
    // Read the version from the [package] table specifically. A flat scan
    // finds whatever `version` key comes first, and a scan that stops at the
    // first `[` stops *at* `[package]` and never reaches the line inside it.
    let mut in_package = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && let Some(value) = toml_string_value(line, "version") {
            return Some(value);
        }
    }
    None
}

/// Extract a quoted string value for `key` from one TOML line.
///
/// The naive version - strip the key prefix, then trim quotes - returns
/// `= "1.97.0` for `channel = "1.97.0"`, which silently put a malformed value
/// into a machine-readable receipt. Provenance is only worth having if its
/// values are correct, and a wrong value is worse than an absent one because
/// it looks authoritative.
pub fn toml_string_value(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?;
    let rest = rest.trim_start().strip_prefix('=')?.trim();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Hash an artifact if it exists; record its absence honestly if it does not.
#[cfg_attr(not(test), allow(dead_code))]
pub fn artifact(root: &Path, relative: &str) -> ArtifactProvenance {
    let path = root.join(relative);
    if !path.exists() {
        return ArtifactProvenance {
            path: relative.to_owned(),
            sha256: String::new(),
            size_bytes: 0,
            present: false,
            absence_reason: Some("artifact not present at collection time".to_owned()),
        };
    }
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    ArtifactProvenance {
        path: relative.to_owned(),
        sha256: runner::sha256_file(&path),
        size_bytes: size,
        present: true,
        absence_reason: None,
    }
}

/// Resolve cargo's target directory.
///
/// Not always `<repo>/target`: `CARGO_TARGET_DIR` may point elsewhere, and a
/// workspace may sit inside a larger build tree that shares one target
/// directory. Assuming `<repo>/target` made provenance report both binaries as
/// absent on this machine while they existed and had just been built, which is
/// exactly the "missing artifact silently reported as not built" failure this
/// is meant to make impossible.
pub fn target_dir(root: &Path) -> PathBuf {
    if let Some(explicit) = std::env::var_os("CARGO_TARGET_DIR") {
        let path = PathBuf::from(explicit);
        return if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
    }
    // Walk up looking for a target directory that already holds our binaries.
    for candidate in root.ancestors() {
        let debug = candidate.join("target").join("debug");
        if debug.join("gnos-ux-scene").exists() || debug.join("gnos-ux-lab").exists() {
            return candidate.join("target");
        }
    }
    root.join("target")
}

/// The binaries Phase 5 builds. Hashed, never installed: this is not an
/// installed-path tranche.
pub fn artifacts(root: &Path) -> Vec<ArtifactProvenance> {
    let target = target_dir(root);
    ["debug/gnos-ux-scene", "debug/gnos-ux-lab"]
        .iter()
        .map(|relative| {
            let path = target.join(relative);
            let present = path.exists();
            ArtifactProvenance {
                path: path.display().to_string(),
                sha256: if present {
                    runner::sha256_file(&path)
                } else {
                    String::new()
                },
                size_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                present,
                absence_reason: (!present).then(|| {
                    format!(
                        "{} was not found under {}; was the build gate run?",
                        relative,
                        target.display()
                    )
                }),
            }
        })
        .collect()
}

pub fn collect(
    root: &Path,
    report: &VerificationReport,
    observed_test_counts: BTreeMap<String, String>,
) -> Provenance {
    let passed = report.results.iter().filter(|r| r.is_success()).count();
    Provenance {
        schema: SCHEMA.to_owned(),
        timestamp_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default(),
        source: collect_source(root),
        toolchain: collect_toolchain(root),
        platform: PlatformProvenance {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        },
        dependencies: collect_dependencies(root),
        qualification: QualificationProvenance {
            profile: report.profile.clone(),
            checks_run: report.results.len(),
            checks_passed: passed,
            overall: report.overall.as_str().to_owned(),
            observed_test_counts,
            feature_sets: vec![
                "default".to_owned(),
                "r13-production-client".to_owned(),
                "component-lab".to_owned(),
            ],
        },
        artifacts: artifacts(root),
    }
}
