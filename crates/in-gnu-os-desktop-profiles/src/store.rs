//! Crash-conscious persistence for the R12-B1 profile document.
//!
//! The profile file has its own directory and CAS revision. Cross-document
//! checks use an R11 locked identity snapshot, which is kept alive throughout
//! the profile commit/recovery. This module has no renderer or runtime state.

use crate::{DesktopProfile, DesktopProfileDocument, OutputProfileResolution, ProfileError};
use gnosix_desktop_model::DesktopOutputProjection;
use in_gnu_os_output_identity::{
    DurableOutputHubId, FilesystemOutputIdentityStore, LockedIdentitySnapshot,
    OutputIdentityReader, StoreError,
};
use in_gnu_os_output_identity_projection::{
    associate_projection_identity, DurableSessionOutputAssociation, ProjectionAssociationError,
    SessionOutputObservation,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

const CURRENT: &str = "desktop-profiles-v1.json";
const PREVIOUS: &str = "desktop-profiles-v1.previous";
const DAMAGED: &str = "desktop-profiles-v1.damaged";
const LOCK: &str = "desktop-profiles-v1.lock";
const INITIALIZED: &str = "desktop-profiles-v1.initialized";
const PENDING_PREFIX: &str = "desktop-profiles-v1.pending.";
static NEXT_PENDING: AtomicU64 = AtomicU64::new(1);

/// Filesystem location for exactly one R12-B1 document. It is publicly
/// readable; mutation is available only through an authority-issued writer.
///
/// ```compile_fail
/// use in_gnu_os_desktop_profiles::DesktopProfileStore;
/// let store = DesktopProfileStore::new("/tmp/profiles", FilesystemOutputIdentityStore::new("/tmp/identity"));
/// store.open_writer();
/// ```
///
/// ```compile_fail
/// use in_gnu_os_desktop_profiles::DesktopProfileWriter;
/// let _writer = DesktopProfileWriter { store: todo!() };
/// ```
#[derive(Debug)]
pub struct DesktopProfileStore {
    directory: PathBuf,
    identity_store: FilesystemOutputIdentityStore,
}

impl DesktopProfileStore {
    /// The future `in.gnu.os` owner chooses and protects this directory.
    pub fn new(
        directory: impl Into<PathBuf>,
        identity_store: FilesystemOutputIdentityStore,
    ) -> Self {
        Self {
            directory: directory.into(),
            identity_store,
        }
    }

    #[cfg(any(test, feature = "r12-production-writer"))]
    pub(crate) fn selected_bar_snapshot(
        &self,
        profile_id: crate::DesktopProfileId,
        epoch: [u8; 16],
    ) -> Result<crate::BarProfileSnapshot, ProfileStoreError> {
        let identity = self.identity_store.locked_current_snapshot()?;
        let (document, version) = match self.load()? {
            ProfileLoadOutcome::Current { document, version } => (document, version),
            ProfileLoadOutcome::Recoverable { issue, .. }
            | ProfileLoadOutcome::Unrecoverable { issue } => {
                return Err(ProfileStoreError::RecoveryRequired(issue))
            }
            ProfileLoadOutcome::Absent => {
                return Err(ProfileStoreError::UnknownSelectedProfile(profile_id))
            }
        };
        validate_identity_refs(&document, Some(&identity))?;
        if !document
            .output_profile_selections
            .iter()
            .any(|s| s.profile_id == profile_id)
        {
            return Err(ProfileStoreError::UnknownSelectedProfile(profile_id));
        }
        let profile = document
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or(ProfileStoreError::UnknownSelectedProfile(profile_id))?;
        Ok(crate::BarProfileSnapshot {
            epoch,
            profile_id,
            version,
            identity_revision: identity.revision(),
            edge: profile.preferences.bar.edge,
            preferences: profile.preferences.bar.overrides(),
        })
    }

    pub fn load(&self) -> Result<ProfileLoadOutcome, ProfileStoreError> {
        let _lock = self.existing_reader_lock()?;
        self.load_unlocked()
    }

    /// Load a document for a consumer that is about to resolve output
    /// selections. A live R11 snapshot is required for every referenced Hub.
    pub fn load_for_identity(&self) -> Result<ProfileLoadOutcome, ProfileStoreError> {
        let outcome = self.load()?;
        let needs_identity = match &outcome {
            ProfileLoadOutcome::Current { document, .. } => {
                !document.output_profile_selections.is_empty()
            }
            ProfileLoadOutcome::Recoverable { previous, .. } => {
                !previous.output_profile_selections.is_empty()
            }
            ProfileLoadOutcome::Absent | ProfileLoadOutcome::Unrecoverable { .. } => false,
        };
        let identity = if needs_identity {
            Some(self.identity_store.locked_current_snapshot()?)
        } else {
            None
        };
        match &outcome {
            ProfileLoadOutcome::Current { document, .. } => {
                validate_identity_refs(document, identity.as_ref())?
            }
            ProfileLoadOutcome::Recoverable { previous, .. } => {
                validate_identity_refs(previous, identity.as_ref())?
            }
            ProfileLoadOutcome::Absent | ProfileLoadOutcome::Unrecoverable { .. } => {}
        }
        Ok(outcome)
    }

    /// Build a read-only point-in-time profile projection for a durable Hub.
    /// The Hub must exist in this store's bound current R11 registry. No
    /// connector, session ID, or fallback identity is accepted.
    pub fn project_output(
        &self,
        output_hub_id: DurableOutputHubId,
    ) -> Result<ProfileProjectionOutcome, ProfileStoreError> {
        // Keep R11 -> profile lock order and the identity generation stable
        // while selecting and copying the profile value.
        let identity = self.identity_store.locked_current_snapshot()?;
        if !identity.contains_hub(output_hub_id) {
            return Err(ProfileStoreError::UnknownOutputHub(output_hub_id));
        }
        let identity_revision = identity.revision();
        Ok(match self.load()? {
            ProfileLoadOutcome::Absent => ProfileProjectionOutcome::DocumentAbsent {
                output_hub_id,
                identity_revision,
            },
            ProfileLoadOutcome::Current { document, .. } => {
                validate_identity_refs(&document, Some(&identity))?;
                match document.resolve_output(output_hub_id)? {
                    OutputProfileResolution::NoSelection => ProfileProjectionOutcome::NoSelection {
                        output_hub_id,
                        profile_revision: document.revision,
                        identity_revision,
                    },
                    OutputProfileResolution::Selected { profile, .. } => {
                        ProfileProjectionOutcome::Selected {
                            output_hub_id,
                            profile_revision: document.revision,
                            identity_revision,
                            profile: profile.clone(),
                        }
                    }
                }
            }
            ProfileLoadOutcome::Recoverable { issue, .. } => {
                ProfileProjectionOutcome::RecoveryRequired {
                    output_hub_id,
                    identity_revision,
                    issue,
                }
            }
            ProfileLoadOutcome::Unrecoverable { issue } => ProfileProjectionOutcome::Unavailable {
                output_hub_id,
                identity_revision,
                issue,
            },
        })
    }

    /// Resolve one current session output through this profile store's own
    /// bound R11 identity authority, then project its R12 profile if and only if
    /// a durable binding exists.
    ///
    /// The caller cannot pair an association from one R11 store with a profile
    /// store bound to another. A revision change between association and profile
    /// projection remains visible in the returned point-in-time revisions and
    /// must be rejected by downstream effective-preference resolution.
    pub fn project_session_output(
        &self,
        projection: &DesktopOutputProjection,
        observed: &SessionOutputObservation,
    ) -> Result<SessionProfileProjection, SessionProfileProjectionError> {
        let reader = OutputIdentityReader::new(&self.identity_store);
        let association = associate_projection_identity(projection, observed, Some(&reader))
            .map_err(SessionProfileProjectionError::Association)?;
        let profile = match association.as_ref() {
            Some(association) => Some(
                self.project_output(association.durable_output_hub_id)
                    .map_err(SessionProfileProjectionError::Profile)?,
            ),
            None => None,
        };
        Ok(SessionProfileProjection {
            association,
            profile,
        })
    }

    #[allow(dead_code)] // Internal issuance seam for the future in.gnu.os profile authority.
    /// The sole route to the crate-internal writer.
    ///
    /// `pub(crate)` rather than `pub`: the designated authority in
    /// [`crate::authority`] must be able to issue a writer, and nothing
    /// outside this crate may obtain one. The designation is by
    /// construction - there is exactly one caller in the tree.
    pub(crate) fn open_writer(&self) -> DesktopProfileWriter {
        DesktopProfileWriter {
            store: Self {
                directory: self.directory.clone(),
                identity_store: self.identity_store.clone(),
            },
        }
    }

    fn current_path(&self) -> PathBuf {
        self.directory.join(CURRENT)
    }

    fn previous_path(&self) -> PathBuf {
        self.directory.join(PREVIOUS)
    }

    fn damaged_path(&self) -> PathBuf {
        self.directory.join(DAMAGED)
    }

    fn initialized_path(&self) -> PathBuf {
        self.directory.join(INITIALIZED)
    }

    fn create_lock(&self) -> Result<File, ProfileStoreError> {
        ensure_private_directory(&self.directory)?;
        let path = self.directory.join(LOCK);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        set_private_file_mode(&mut options);
        let file = options
            .open(&path)
            .map_err(|source| ProfileStoreError::Io {
                operation: "open writer lock",
                path: path.clone(),
                source,
            })?;
        file.lock().map_err(|source| ProfileStoreError::Io {
            operation: "lock profile store",
            path,
            source,
        })?;
        Ok(file)
    }

    fn existing_reader_lock(&self) -> Result<Option<File>, ProfileStoreError> {
        let path = self.directory.join(LOCK);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ProfileStoreError::Io {
                operation: "inspect reader lock",
                path,
                source,
            }),
            Ok(metadata) if !metadata.file_type().is_file() => Err(ProfileStoreError::Io {
                operation: "validate reader lock",
                path,
                source: io::Error::new(io::ErrorKind::InvalidInput, "lock path is not a file"),
            }),
            Ok(_) => {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .map_err(|source| ProfileStoreError::Io {
                        operation: "open reader lock",
                        path: path.clone(),
                        source,
                    })?;
                file.lock().map_err(|source| ProfileStoreError::Io {
                    operation: "lock profile store for read",
                    path,
                    source,
                })?;
                Ok(Some(file))
            }
        }
    }

    fn load_unlocked(&self) -> Result<ProfileLoadOutcome, ProfileStoreError> {
        let pending = self.pending_candidates()?;
        let has_pending = !pending.is_empty();
        match read_candidate(&self.current_path())? {
            CandidateRead::Valid(document, bytes) => Ok(ProfileLoadOutcome::Current {
                version: ProfileVersion::current(document.revision, &bytes),
                document,
            }),
            CandidateRead::Invalid(current_issue, current_bytes) => {
                let previous = read_candidate(&self.previous_path())?;
                let issue = self.issue(current_issue.clone(), previous.issue(), pending)?;
                if matches!(current_issue, ProfileCandidateIssue::UnsupportedSchema(_)) {
                    return Ok(ProfileLoadOutcome::Unrecoverable { issue });
                }
                match previous {
                    CandidateRead::Valid(document, _) => {
                        let token = ProfileRecoveryToken {
                            current_bytes,
                            previous_revision: document.revision,
                            issue: issue.clone(),
                        };
                        Ok(ProfileLoadOutcome::Recoverable {
                            previous: document,
                            issue,
                            token,
                        })
                    }
                    CandidateRead::Missing => Ok(ProfileLoadOutcome::Unrecoverable { issue }),
                    CandidateRead::Invalid(_, _) => Ok(ProfileLoadOutcome::Unrecoverable { issue }),
                }
            }
            CandidateRead::Missing => {
                let previous = read_candidate(&self.previous_path())?;
                let issue =
                    self.issue(ProfileCandidateIssue::Missing, previous.issue(), pending)?;
                match previous {
                    CandidateRead::Valid(document, _) => {
                        let token = ProfileRecoveryToken {
                            current_bytes: None,
                            previous_revision: document.revision,
                            issue: issue.clone(),
                        };
                        Ok(ProfileLoadOutcome::Recoverable {
                            previous: document,
                            issue,
                            token,
                        })
                    }
                    CandidateRead::Invalid(_, _) => Ok(ProfileLoadOutcome::Unrecoverable { issue }),
                    CandidateRead::Missing if !has_pending && !issue.initialized_marker => {
                        Ok(ProfileLoadOutcome::Absent)
                    }
                    CandidateRead::Missing => Ok(ProfileLoadOutcome::Unrecoverable { issue }),
                }
            }
        }
    }

    fn issue(
        &self,
        current: ProfileCandidateIssue,
        previous: Option<ProfileCandidateIssue>,
        pending_candidates: Vec<String>,
    ) -> Result<ProfileRecoveryIssue, ProfileStoreError> {
        let initialized_marker = match fs::symlink_metadata(self.initialized_path()) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(ProfileStoreError::Io {
                    operation: "inspect initialized marker",
                    path: self.initialized_path(),
                    source,
                })
            }
        };
        Ok(ProfileRecoveryIssue {
            current,
            previous,
            initialized_marker,
            pending_candidates,
        })
    }

    fn pending_candidates(&self) -> Result<Vec<String>, ProfileStoreError> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(ProfileStoreError::Io {
                    operation: "read profile directory",
                    path: self.directory.clone(),
                    source,
                })
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| ProfileStoreError::Io {
                operation: "read profile directory entry",
                path: self.directory.clone(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(PENDING_PREFIX) {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    fn write_pending(&self, bytes: &[u8]) -> Result<PathBuf, ProfileStoreError> {
        for _ in 0..128 {
            let id = NEXT_PENDING.fetch_add(1, Ordering::Relaxed);
            let path = self
                .directory
                .join(format!("{PENDING_PREFIX}{}-{id}", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            set_private_file_mode(&mut options);
            match options.open(&path) {
                Ok(mut file) => {
                    if let Err(source) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                        let _ = fs::remove_file(&path);
                        return Err(ProfileStoreError::Io {
                            operation: "write and sync profile candidate",
                            path,
                            source,
                        });
                    }
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(ProfileStoreError::Io {
                        operation: "create profile candidate",
                        path,
                        source,
                    })
                }
            }
        }
        Err(ProfileStoreError::PendingPathExhausted)
    }

    fn sync_directory(&self) -> Result<(), ProfileStoreError> {
        let directory = File::open(&self.directory).map_err(|source| ProfileStoreError::Io {
            operation: "open profile directory for sync",
            path: self.directory.clone(),
            source,
        })?;
        directory
            .sync_all()
            .map_err(|source| ProfileStoreError::Io {
                operation: "sync profile directory",
                path: self.directory.clone(),
                source,
            })
    }

    fn ensure_initialized(&self) -> Result<(), ProfileStoreError> {
        let path = self.initialized_path();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        set_private_file_mode(&mut options);
        let mut file = options
            .open(&path)
            .map_err(|source| ProfileStoreError::Io {
                operation: "create initialized marker",
                path: path.clone(),
                source,
            })?;
        file.write_all(b"gnosix-desktop-profile-document-v1\n")
            .and_then(|_| file.sync_all())
            .map_err(|source| ProfileStoreError::Io {
                operation: "write initialized marker",
                path: path.clone(),
                source,
            })?;
        self.sync_directory()
    }

    fn commit_transaction(
        &self,
        expected: &ProfileVersion,
        document: &DesktopProfileDocument,
        after_rotation: impl FnOnce() -> Result<(), ProfileStoreError>,
    ) -> Result<ProfileCommitReceipt, ProfileStoreError> {
        document.validate()?;
        let identity = if document.output_profile_selections.is_empty() {
            None
        } else {
            Some(self.identity_store.locked_current_snapshot()?)
        };
        validate_identity_refs(document, identity.as_ref())?;
        let expected_revision = expected.revision().unwrap_or(0);
        let next = expected_revision
            .checked_add(1)
            .ok_or(ProfileStoreError::RevisionExhausted)?;
        if document.revision != next {
            return Err(ProfileStoreError::NonAdvancingRevision {
                expected: expected_revision,
                candidate: document.revision,
            });
        }

        let _lock = self.create_lock()?;
        let (previous_revision, current_exists) = match self.load_unlocked()? {
            ProfileLoadOutcome::Current {
                document: current,
                version,
            } => {
                if expected != &version {
                    return Err(ProfileStoreError::StaleVersion {
                        expected: expected.clone(),
                        actual: version,
                    });
                }
                (Some(current.revision), true)
            }
            ProfileLoadOutcome::Absent if matches!(expected, ProfileVersion::Absent) => {
                (None, false)
            }
            ProfileLoadOutcome::Absent => {
                return Err(ProfileStoreError::StaleVersion {
                    expected: expected.clone(),
                    actual: ProfileVersion::Absent,
                })
            }
            ProfileLoadOutcome::Recoverable { issue, .. }
            | ProfileLoadOutcome::Unrecoverable { issue } => {
                return Err(ProfileStoreError::RecoveryRequired(issue))
            }
        };

        if !current_exists {
            self.ensure_initialized()?;
        }
        let bytes =
            serde_json::to_vec_pretty(document).map_err(|_| ProfileStoreError::Serialization)?;
        let pending = self.write_pending(&bytes)?;
        if current_exists {
            let previous_path = self.previous_path();
            remove_file_if_exists(&previous_path, "remove older profile generation")?;
            self.sync_directory()?;
            fs::rename(self.current_path(), &previous_path).map_err(|source| {
                ProfileStoreError::Io {
                    operation: "rotate profile generation",
                    path: previous_path,
                    source,
                }
            })?;
            self.sync_directory()?;
        }
        after_rotation()?;
        fs::rename(&pending, self.current_path()).map_err(|source| ProfileStoreError::Io {
            operation: "install current profile generation",
            path: self.current_path(),
            source,
        })?;
        self.sync_directory()?;
        Ok(ProfileCommitReceipt {
            revision: document.revision,
            previous_revision,
            bytes_written: bytes.len(),
            identity_revision: identity.as_ref().map(LockedIdentitySnapshot::revision),
            version: ProfileVersion::current(document.revision, &bytes),
        })
    }
}

/// Opaque writer capability. Its constructor is private to this crate; a
/// future `in.gnu.os` adapter must be the sole issuer. This is not process
/// authentication.
pub struct DesktopProfileWriter {
    store: DesktopProfileStore,
}

impl DesktopProfileWriter {
    pub fn commit(
        &self,
        expected: &ProfileVersion,
        document: &DesktopProfileDocument,
    ) -> Result<ProfileCommitReceipt, ProfileStoreError> {
        self.store.commit_transaction(expected, document, || Ok(()))
    }

    /// Explicitly installs the previous valid profile candidate. The supplied
    /// token must match the exact current bytes observed by `load`.
    pub fn recover(
        &self,
        token: &ProfileRecoveryToken,
        recovery_reference: &str,
    ) -> Result<ProfileRecoveryReceipt, ProfileStoreError> {
        if recovery_reference.trim().is_empty() {
            return Err(ProfileStoreError::MissingRecoveryReference);
        }
        // Keep identity -> profile lock order consistent with commit, and hold
        // the R11 generation stable through validation and recovery install.
        let identity_required = match self.store.load()? {
            ProfileLoadOutcome::Recoverable { previous, .. } => {
                !previous.output_profile_selections.is_empty()
            }
            _ => return Err(ProfileStoreError::NotRecoverable),
        };
        let identity = if identity_required {
            Some(self.store.identity_store.locked_current_snapshot()?)
        } else {
            None
        };
        let _lock = self.store.create_lock()?;
        let (previous, issue, current_bytes) = match self.store.load_unlocked()? {
            ProfileLoadOutcome::Recoverable {
                previous,
                issue,
                token: observed,
            } => (previous, issue, observed.current_bytes),
            _ => return Err(ProfileStoreError::NotRecoverable),
        };
        if token.issue != issue
            || token.previous_revision != previous.revision
            || token.current_bytes != current_bytes
        {
            return Err(ProfileStoreError::StaleRecoveryToken);
        }
        validate_identity_refs(&previous, identity.as_ref())?;
        let revision = previous
            .revision
            .checked_add(1)
            .ok_or(ProfileStoreError::RevisionExhausted)?;
        let mut recovered = previous.clone();
        recovered.revision = revision;
        recovered.validate()?;
        let bytes =
            serde_json::to_vec_pretty(&recovered).map_err(|_| ProfileStoreError::Serialization)?;
        let pending = self.store.write_pending(&bytes)?;
        let current_path = self.store.current_path();
        let damaged_path = self.store.damaged_path();
        let damaged_preserved = match fs::symlink_metadata(&current_path) {
            Ok(_) => {
                if fs::symlink_metadata(&damaged_path).is_ok() {
                    return Err(ProfileStoreError::DamagedCandidateAlreadyExists);
                }
                fs::rename(&current_path, &damaged_path).map_err(|source| {
                    ProfileStoreError::Io {
                        operation: "preserve damaged profile candidate",
                        path: damaged_path,
                        source,
                    }
                })?;
                self.store.sync_directory()?;
                true
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(ProfileStoreError::Io {
                    operation: "inspect damaged profile candidate",
                    path: current_path,
                    source,
                })
            }
        };
        fs::rename(&pending, self.store.current_path()).map_err(|source| {
            ProfileStoreError::Io {
                operation: "install explicitly recovered profile generation",
                path: self.store.current_path(),
                source,
            }
        })?;
        self.store.sync_directory()?;
        Ok(ProfileRecoveryReceipt {
            revision,
            recovered_from_revision: previous.revision,
            damaged_candidate_preserved: damaged_preserved,
            recovery_reference: recovery_reference.to_owned(),
            issue,
            identity_revision: identity.as_ref().map(LockedIdentitySnapshot::revision),
            version: ProfileVersion::current(recovered.revision, &bytes),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileLoadOutcome {
    Absent,
    Current {
        document: DesktopProfileDocument,
        version: ProfileVersion,
    },
    Recoverable {
        previous: DesktopProfileDocument,
        issue: ProfileRecoveryIssue,
        token: ProfileRecoveryToken,
    },
    Unrecoverable {
        issue: ProfileRecoveryIssue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProfileProjection {
    pub association: Option<DurableSessionOutputAssociation>,
    pub profile: Option<ProfileProjectionOutcome>,
}

#[derive(Debug)]
pub enum SessionProfileProjectionError {
    Association(ProjectionAssociationError),
    Profile(ProfileStoreError),
}

impl std::fmt::Display for SessionProfileProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Association(error) => {
                write!(formatter, "output identity association failed: {error:?}")
            }
            Self::Profile(error) => write!(formatter, "profile projection failed: {error}"),
        }
    }
}

impl std::error::Error for SessionProfileProjectionError {}

/// Read-only per-output preference result. `Selected` contains stored
/// preferences as authored; it does not merge system observations or apply
/// them to GNOS.UX/Gnoshow. Recovery candidates are never projected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileProjectionOutcome {
    DocumentAbsent {
        output_hub_id: DurableOutputHubId,
        identity_revision: u64,
    },
    NoSelection {
        output_hub_id: DurableOutputHubId,
        profile_revision: u64,
        identity_revision: u64,
    },
    Selected {
        output_hub_id: DurableOutputHubId,
        profile_revision: u64,
        identity_revision: u64,
        profile: DesktopProfile,
    },
    RecoveryRequired {
        output_hub_id: DurableOutputHubId,
        identity_revision: u64,
        issue: ProfileRecoveryIssue,
    },
    Unavailable {
        output_hub_id: DurableOutputHubId,
        identity_revision: u64,
        issue: ProfileRecoveryIssue,
    },
}

/// Optimistic compare-and-swap token. Current versions pair the B1 document
/// revision with SHA-256 of the exact persisted bytes, preventing ABA after
/// explicit recovery installs a different document at the same revision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProfileVersion {
    Absent,
    Current { revision: u64, etag: [u8; 32] },
}

impl ProfileVersion {
    fn current(revision: u64, bytes: &[u8]) -> Self {
        Self::Current {
            revision,
            etag: Sha256::digest(bytes).into(),
        }
    }

    pub fn revision(&self) -> Option<u64> {
        match self {
            Self::Absent => None,
            Self::Current { revision, .. } => Some(*revision),
        }
    }

    pub fn etag_hex(&self) -> Option<String> {
        match self {
            Self::Absent => None,
            Self::Current { etag, .. } => {
                Some(etag.iter().map(|byte| format!("{byte:02x}")).collect())
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProfileRecoveryToken {
    current_bytes: Option<Vec<u8>>,
    previous_revision: u64,
    issue: ProfileRecoveryIssue,
}

impl std::fmt::Debug for ProfileRecoveryToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfileRecoveryToken")
            .field("current_candidate_present", &self.current_bytes.is_some())
            .field("previous_revision", &self.previous_revision)
            .field("issue", &self.issue)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRecoveryIssue {
    pub current: ProfileCandidateIssue,
    pub previous: Option<ProfileCandidateIssue>,
    pub initialized_marker: bool,
    pub pending_candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileCandidateIssue {
    Missing,
    InvalidJson,
    InvalidDocument(ProfileError),
    UnsupportedSchema(u32),
    NotRegularFile,
}

impl ProfileCandidateIssue {
    fn from_document_error(error: ProfileError) -> Self {
        match error {
            ProfileError::UnsupportedSchema(version) => Self::UnsupportedSchema(version),
            other => Self::InvalidDocument(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCommitReceipt {
    pub revision: u64,
    pub previous_revision: Option<u64>,
    pub bytes_written: usize,
    pub identity_revision: Option<u64>,
    pub version: ProfileVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRecoveryReceipt {
    pub revision: u64,
    pub recovered_from_revision: u64,
    pub damaged_candidate_preserved: bool,
    pub recovery_reference: String,
    pub issue: ProfileRecoveryIssue,
    pub identity_revision: Option<u64>,
    pub version: ProfileVersion,
}

#[derive(Debug, Error)]
pub enum ProfileStoreError {
    #[error("filesystem operation {operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("profile document is invalid: {0}")]
    InvalidDocument(#[from] ProfileError),
    #[error("OutputHub identity operation failed: {0}")]
    Identity(#[from] StoreError),
    #[error("a locked current identity snapshot is required for output selections")]
    MissingIdentitySnapshot,
    #[error("profile selection references an OutputHub absent from the locked R11 snapshot")]
    UnknownOutputHub(DurableOutputHubId),
    #[error("profile is absent or not selected: {0:?}")]
    UnknownSelectedProfile(crate::DesktopProfileId),
    #[error("profile store requires explicit recovery: {0:?}")]
    RecoveryRequired(ProfileRecoveryIssue),
    #[error("profile version changed: expected {expected:?}, actual {actual:?}")]
    StaleVersion {
        expected: ProfileVersion,
        actual: ProfileVersion,
    },
    #[error("candidate profile revision {candidate} must equal expected revision {expected} + 1")]
    NonAdvancingRevision { expected: u64, candidate: u64 },
    #[error("profile revision space is exhausted")]
    RevisionExhausted,
    #[error("recovery reference must not be empty")]
    MissingRecoveryReference,
    #[error("profile store has no recoverable previous candidate")]
    NotRecoverable,
    #[error("current profile bytes changed since the recovery token was observed")]
    StaleRecoveryToken,
    #[error("damaged profile candidate already exists and will not be overwritten")]
    DamagedCandidateAlreadyExists,
    #[error("unable to allocate a unique pending profile path")]
    PendingPathExhausted,
    #[error("profile serialization failed")]
    Serialization,
}

enum CandidateRead {
    Missing,
    Valid(DesktopProfileDocument, Vec<u8>),
    Invalid(ProfileCandidateIssue, Option<Vec<u8>>),
}

impl CandidateRead {
    fn issue(&self) -> Option<ProfileCandidateIssue> {
        match self {
            Self::Missing => Some(ProfileCandidateIssue::Missing),
            Self::Valid(_, _) => None,
            Self::Invalid(issue, _) => Some(issue.clone()),
        }
    }
}

fn read_candidate(path: &Path) -> Result<CandidateRead, ProfileStoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CandidateRead::Missing),
        Err(source) => {
            return Err(ProfileStoreError::Io {
                operation: "inspect profile candidate",
                path: path.to_owned(),
                source,
            })
        }
    };
    if !metadata.file_type().is_file() {
        return Ok(CandidateRead::Invalid(
            ProfileCandidateIssue::NotRegularFile,
            None,
        ));
    }
    let bytes = fs::read(path).map_err(|source| ProfileStoreError::Io {
        operation: "read profile candidate",
        path: path.to_owned(),
        source,
    })?;
    let document = match decode_profile_document(&bytes) {
        Ok(document) => document,
        Err(issue) => return Ok(CandidateRead::Invalid(issue, Some(bytes))),
    };
    Ok(CandidateRead::Valid(document, bytes))
}

fn decode_profile_document(bytes: &[u8]) -> Result<DesktopProfileDocument, ProfileCandidateIssue> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ProfileCandidateIssue::InvalidJson)?;
    let schema = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(ProfileCandidateIssue::InvalidJson)?;

    match schema {
        1 => {
            if legacy_v1_contains_post_v1_bar_fields(&value) {
                return Err(ProfileCandidateIssue::InvalidJson);
            }
            let mut document: DesktopProfileDocument =
                serde_json::from_value(value).map_err(|_| ProfileCandidateIssue::InvalidJson)?;
            document.schema_version = crate::DESKTOP_PROFILE_SCHEMA_VERSION;
            document
                .validate()
                .map_err(ProfileCandidateIssue::from_document_error)?;
            Ok(document)
        }
        crate::DESKTOP_PROFILE_SCHEMA_VERSION => {
            let document: DesktopProfileDocument =
                serde_json::from_value(value).map_err(|_| ProfileCandidateIssue::InvalidJson)?;
            document
                .validate()
                .map_err(ProfileCandidateIssue::from_document_error)?;
            Ok(document)
        }
        other => Err(ProfileCandidateIssue::UnsupportedSchema(other)),
    }
}

fn legacy_v1_contains_post_v1_bar_fields(value: &serde_json::Value) -> bool {
    value
        .get("profiles")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|profiles| {
            profiles.iter().any(|profile| {
                profile
                    .get("preferences")
                    .and_then(|preferences| preferences.get("bar"))
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|bar| bar.keys().any(|key| key != "edge"))
            })
        })
}

fn validate_identity_refs(
    document: &DesktopProfileDocument,
    identity: Option<&LockedIdentitySnapshot>,
) -> Result<(), ProfileStoreError> {
    document.validate()?;
    if document.output_profile_selections.is_empty() {
        return Ok(());
    }
    let identity = identity.ok_or(ProfileStoreError::MissingIdentitySnapshot)?;
    for selection in &document.output_profile_selections {
        if !identity.contains_hub(selection.output_hub_id) {
            return Err(ProfileStoreError::UnknownOutputHub(selection.output_hub_id));
        }
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path, operation: &'static str) -> Result<(), ProfileStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => fs::remove_file(path).map_err(|source| ProfileStoreError::Io {
            operation,
            path: path.to_owned(),
            source,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ProfileStoreError::Io {
            operation: "inspect profile generation",
            path: path.to_owned(),
            source,
        }),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), ProfileStoreError> {
    fs::create_dir_all(path).map_err(|source| ProfileStoreError::Io {
        operation: "create profile store directory",
        path: path.to_owned(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            ProfileStoreError::Io {
                operation: "set private profile store directory mode",
                path: path.to_owned(),
                source,
            }
        })?;
    }
    Ok(())
}

fn set_private_file_mode(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
