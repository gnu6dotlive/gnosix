//! Identity-only, crash-conscious filesystem persistence for the R11 registry.
//!
//! The caller supplies a private directory owned by the future `in.gnu.os`
//! authority. This module does not choose a product path, create a service,
//! authorize callers, or connect the registry to GNOS.UX.

use super::{RegistryAuditEvent, RegistryDocument, RegistryError, REGISTRY_SCHEMA_VERSION};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

const CURRENT_FILE: &str = "output-identities-v1.json";
const PREVIOUS_FILE: &str = "output-identities-v1.previous";
const DAMAGED_FILE: &str = "output-identities-v1.damaged";
const LOCK_FILE: &str = "output-identities-v1.lock";
const INITIALIZED_FILE: &str = "output-identities-v1.initialized";
const ISSUED_FILE: &str = "output-identities-v1.issued";
const PENDING_PREFIX: &str = "output-identities-v1.pending.";

static NEXT_PENDING_ID: AtomicU64 = AtomicU64::new(1);

/// Public read-only storage seam for identity consumers.
///
/// Mutation operations are kept on an internal adapter trait so downstream
/// readers cannot commit or restore a registry through this interface.
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{FilesystemOutputIdentityStore, OutputIdentityStore};
/// let store = FilesystemOutputIdentityStore::new("/tmp/identity");
/// store.commit(0, &Default::default());
/// ```
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{FilesystemOutputIdentityStore, OutputIdentityStore};
/// let store = FilesystemOutputIdentityStore::new("/tmp/identity");
/// store.restore_previous("review-reference");
/// ```
pub trait OutputIdentityStore {
    fn load(&self) -> Result<LoadOutcome, StoreError>;
}

pub(crate) trait OutputIdentityStoreMutations: OutputIdentityStore {
    fn commit(
        &self,
        expected_revision: u64,
        document: &RegistryDocument,
    ) -> Result<CommitReceipt, StoreError>;

    /// Explicitly restores the retained previous generation after a
    /// `Recoverable` load. The reference is durable audit metadata, not a
    /// credential. Recovery never occurs implicitly during `load`.
    fn restore_previous(&self, recovery_reference: &str) -> Result<RecoveryReceipt, StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    Absent,
    Current(RegistryDocument),
    Recoverable {
        previous: RegistryDocument,
        issue: RecoveryIssue,
    },
    Unrecoverable {
        issue: RecoveryIssue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryIssue {
    pub current: CandidateIssue,
    pub previous: Option<CandidateIssue>,
    pub initialized_marker: bool,
    /// Names only. Pending files are never promoted automatically.
    pub pending_candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateIssue {
    Missing,
    InvalidJson,
    InvalidDocument(String),
    UnsupportedSchema(u32),
    NotRegularFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    pub revision: u64,
    pub previous_revision: Option<u64>,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReceipt {
    pub revision: u64,
    pub recovered_from_revision: u64,
    pub damaged_candidate_preserved: bool,
    pub issue: RecoveryIssue,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("filesystem operation {operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("registry document is invalid: {0}")]
    InvalidDocument(#[from] RegistryError),
    #[error("store requires explicit recovery: {0:?}")]
    RecoveryRequired(RecoveryIssue),
    #[error("store revision changed: expected {expected}, actual {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error(
        "new registry revision {new_revision} must exceed expected revision {expected_revision}"
    )]
    NonAdvancingRevision {
        expected_revision: u64,
        new_revision: u64,
    },
    #[error("recovery reference must not be empty")]
    MissingRecoveryReference,
    #[error("store has no recoverable previous generation")]
    NotRecoverable,
    #[error("damaged candidate already exists; preserving it without overwrite")]
    DamagedCandidateAlreadyExists,
    #[error("unable to allocate a unique pending generation path")]
    PendingPathExhausted,
    #[error("registry revision space is exhausted")]
    RevisionExhausted,
    #[error("durable OutputHub ID was already issued in an earlier generation")]
    OutputHubIdPreviouslyIssued,
    #[error("issued OutputHub ID ledger is invalid or unavailable")]
    IssuedLedgerUnavailable,
}

/// Filesystem-backed store for one identity-only registry document.
///
/// It keeps one previous known-good generation. Files are written in the
/// target directory, synced, renamed, and followed by a directory sync. A
/// damaged or future-version current file is never treated as absent and is
/// never silently replaced by the previous generation.
#[derive(Debug, Clone)]
pub struct FilesystemOutputIdentityStore {
    directory: PathBuf,
}

/// A validated current identity generation held under the store lock.
/// Keeping this value alive blocks cooperating identity mutations/recovery.
pub struct LockedIdentitySnapshot {
    document: RegistryDocument,
    _lock: File,
}

impl LockedIdentitySnapshot {
    pub fn revision(&self) -> u64 {
        self.document.revision
    }

    pub fn contains_hub(&self, output_hub_id: super::DurableOutputHubId) -> bool {
        self.document.output_hubs.contains(&output_hub_id)
    }
}

impl FilesystemOutputIdentityStore {
    /// The directory is selected and protected by the `in.gnu.os` owner.
    /// `load` does not create it; `commit` creates it with private permissions
    /// when it does not exist.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Load a current identity generation while holding the store lock. The
    /// returned snapshot keeps that lock alive, so a cooperating identity
    /// writer cannot change or recover the registry until the guard is dropped.
    /// It is intended for bounded cross-document validation/commit operations.
    pub fn locked_current_snapshot(&self) -> Result<LockedIdentitySnapshot, StoreError> {
        let lock = self.acquire_writer_lock()?;
        let outcome = self.load_unlocked()?;
        let LoadOutcome::Current(document) = outcome else {
            return Err(StoreError::RecoveryRequired(match outcome {
                LoadOutcome::Recoverable { issue, .. } | LoadOutcome::Unrecoverable { issue } => {
                    issue
                }
                LoadOutcome::Absent => RecoveryIssue {
                    current: CandidateIssue::Missing,
                    previous: Some(CandidateIssue::Missing),
                    initialized_marker: false,
                    pending_candidates: Vec::new(),
                },
                LoadOutcome::Current(_) => unreachable!(),
            }));
        };
        let issued = self.load_issued_ids(Some(&document))?;
        if !document.output_hubs.is_subset(&issued) {
            return Err(StoreError::IssuedLedgerUnavailable);
        }
        Ok(LockedIdentitySnapshot {
            document,
            _lock: lock,
        })
    }

    fn current_path(&self) -> PathBuf {
        self.directory.join(CURRENT_FILE)
    }

    fn previous_path(&self) -> PathBuf {
        self.directory.join(PREVIOUS_FILE)
    }

    fn damaged_path(&self) -> PathBuf {
        self.directory.join(DAMAGED_FILE)
    }

    fn issued_path(&self) -> PathBuf {
        self.directory.join(ISSUED_FILE)
    }

    fn load_issued_ids(
        &self,
        current: Option<&RegistryDocument>,
    ) -> Result<std::collections::BTreeSet<super::DurableOutputHubId>, StoreError> {
        let path = self.issued_path();
        match fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| StoreError::IssuedLedgerUnavailable)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if current.is_none() {
                    return Ok(std::collections::BTreeSet::new());
                }
                Err(StoreError::IssuedLedgerUnavailable)
            }
            Err(_) => Err(StoreError::IssuedLedgerUnavailable),
        }
    }

    fn write_issued_ids(
        &self,
        issued: &std::collections::BTreeSet<super::DurableOutputHubId>,
    ) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec(issued).map_err(|_| StoreError::IssuedLedgerUnavailable)?;
        let path = self.issued_path();
        let pending = self
            .directory
            .join(format!("{ISSUED_FILE}.pending.{}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        set_private_file_mode(&mut options);
        let mut file = options.open(&pending).map_err(|source| StoreError::Io {
            operation: "create issued ID ledger candidate",
            path: pending.clone(),
            source,
        })?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|source| StoreError::Io {
                operation: "write issued ID ledger candidate",
                path: pending.clone(),
                source,
            })?;
        fs::rename(&pending, &path).map_err(|source| StoreError::Io {
            operation: "install issued ID ledger",
            path: path.clone(),
            source,
        })?;
        self.sync_directory()
    }

    fn pending_paths(&self) -> Result<Vec<PathBuf>, StoreError> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(StoreError::Io {
                    operation: "read directory",
                    path: self.directory.clone(),
                    source,
                })
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| StoreError::Io {
                operation: "read directory entry",
                path: self.directory.clone(),
                source,
            })?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(PENDING_PREFIX)
            {
                paths.push(entry.path());
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn acquire_writer_lock(&self) -> Result<File, StoreError> {
        create_private_directory(&self.directory)?;
        let path = self.directory.join(LOCK_FILE);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        set_private_file_mode(&mut options);
        let file = options.open(&path).map_err(|source| StoreError::Io {
            operation: "open writer lock",
            path: path.clone(),
            source,
        })?;
        file.lock().map_err(|source| StoreError::Io {
            operation: "lock writer lock",
            path,
            source,
        })?;
        Ok(file)
    }

    fn write_pending(&self, bytes: &[u8]) -> Result<PathBuf, StoreError> {
        for _ in 0..128 {
            let sequence = NEXT_PENDING_ID.fetch_add(1, Ordering::Relaxed);
            let path = self
                .directory
                .join(format!("{PENDING_PREFIX}{}-{sequence}", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            set_private_file_mode(&mut options);
            match options.open(&path) {
                Ok(mut file) => {
                    if let Err(source) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                        let _ = fs::remove_file(&path);
                        return Err(StoreError::Io {
                            operation: "write and sync pending generation",
                            path,
                            source,
                        });
                    }
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(StoreError::Io {
                        operation: "create pending generation",
                        path,
                        source,
                    })
                }
            }
        }
        Err(StoreError::PendingPathExhausted)
    }

    fn sync_directory(&self) -> Result<(), StoreError> {
        let directory = File::open(&self.directory).map_err(|source| StoreError::Io {
            operation: "open directory for sync",
            path: self.directory.clone(),
            source,
        })?;
        directory.sync_all().map_err(|source| StoreError::Io {
            operation: "sync directory",
            path: self.directory.clone(),
            source,
        })
    }

    fn initialized_marker_exists(&self) -> Result<bool, StoreError> {
        let path = self.directory.join(INITIALIZED_FILE);
        match fs::symlink_metadata(&path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(StoreError::Io {
                operation: "inspect initialized marker",
                path,
                source,
            }),
        }
    }

    fn ensure_initialized_marker(&self) -> Result<(), StoreError> {
        let path = self.directory.join(INITIALIZED_FILE);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        set_private_file_mode(&mut options);
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(b"gnosix-output-identity-registry-v1\n")
                    .and_then(|_| file.sync_all())
                    .map_err(|source| StoreError::Io {
                        operation: "write initialized marker",
                        path: path.clone(),
                        source,
                    })?;
                self.sync_directory()
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(StoreError::RecoveryRequired(RecoveryIssue {
                    current: CandidateIssue::Missing,
                    previous: Some(CandidateIssue::Missing),
                    initialized_marker: true,
                    pending_candidates: self
                        .pending_paths()?
                        .iter()
                        .filter_map(|candidate| {
                            candidate
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                        })
                        .collect(),
                }))
            }
            Err(source) => Err(StoreError::Io {
                operation: "create initialized marker",
                path,
                source,
            }),
        }
    }

    fn existing_reader_lock(&self) -> Result<Option<File>, StoreError> {
        let path = self.directory.join(LOCK_FILE);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(StoreError::Io {
                operation: "inspect writer lock for read",
                path,
                source,
            }),
            Ok(metadata) if !metadata.file_type().is_file() => Err(StoreError::Io {
                operation: "validate writer lock for read",
                path,
                source: io::Error::new(io::ErrorKind::InvalidInput, "lock path is not a file"),
            }),
            Ok(_) => {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .map_err(|source| StoreError::Io {
                        operation: "open writer lock for read",
                        path: path.clone(),
                        source,
                    })?;
                file.lock().map_err(|source| StoreError::Io {
                    operation: "lock store for read",
                    path,
                    source,
                })?;
                Ok(Some(file))
            }
        }
    }

    fn load_unlocked(&self) -> Result<LoadOutcome, StoreError> {
        let current_path = self.current_path();
        let previous_path = self.previous_path();
        let pending_candidates = self
            .pending_paths()?
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>();

        match read_generation(&current_path)? {
            CandidateRead::Valid(document) => Ok(LoadOutcome::Current(document)),
            CandidateRead::Invalid(current_issue) => {
                let previous = read_generation(&previous_path)?;
                let issue = RecoveryIssue {
                    current: current_issue.clone(),
                    previous: previous.issue(),
                    initialized_marker: self.initialized_marker_exists()?,
                    pending_candidates,
                };
                if matches!(current_issue, CandidateIssue::UnsupportedSchema(_)) {
                    return Ok(LoadOutcome::Unrecoverable { issue });
                }
                match previous {
                    CandidateRead::Valid(document) => Ok(LoadOutcome::Recoverable {
                        previous: document,
                        issue,
                    }),
                    CandidateRead::Missing | CandidateRead::Invalid(_) => {
                        Ok(LoadOutcome::Unrecoverable { issue })
                    }
                }
            }
            CandidateRead::Missing => {
                let previous = read_generation(&previous_path)?;
                let issue = RecoveryIssue {
                    current: CandidateIssue::Missing,
                    previous: previous.issue(),
                    initialized_marker: self.initialized_marker_exists()?,
                    pending_candidates,
                };
                match previous {
                    CandidateRead::Valid(document) => Ok(LoadOutcome::Recoverable {
                        previous: document,
                        issue,
                    }),
                    CandidateRead::Invalid(_) => Ok(LoadOutcome::Unrecoverable { issue }),
                    CandidateRead::Missing
                        if issue.pending_candidates.is_empty() && !issue.initialized_marker =>
                    {
                        Ok(LoadOutcome::Absent)
                    }
                    CandidateRead::Missing => Ok(LoadOutcome::Unrecoverable { issue }),
                }
            }
        }
    }

    fn commit_transaction(
        &self,
        expected_revision: u64,
        document: &RegistryDocument,
        after_rotation: impl FnOnce() -> Result<(), StoreError>,
    ) -> Result<CommitReceipt, StoreError> {
        document.validate()?;
        if document.revision <= expected_revision {
            return Err(StoreError::NonAdvancingRevision {
                expected_revision,
                new_revision: document.revision,
            });
        }

        let _lock = self.acquire_writer_lock()?;
        let current = self.load_unlocked()?;
        let (previous_revision, current_exists, current_document) = match current {
            LoadOutcome::Current(current) => {
                if current.revision != expected_revision {
                    return Err(StoreError::StaleRevision {
                        expected: expected_revision,
                        actual: current.revision,
                    });
                }
                (Some(current.revision), true, Some(current))
            }
            LoadOutcome::Absent if expected_revision == 0 => (None, false, None),
            LoadOutcome::Absent => {
                return Err(StoreError::StaleRevision {
                    expected: expected_revision,
                    actual: 0,
                })
            }
            LoadOutcome::Recoverable { issue, .. } | LoadOutcome::Unrecoverable { issue } => {
                return Err(StoreError::RecoveryRequired(issue))
            }
        };

        let current_hubs = current_document
            .as_ref()
            .map(|current| &current.output_hubs);
        if current_hubs.is_some_and(|hubs| !hubs.is_subset(&document.output_hubs)) {
            return Err(StoreError::InvalidDocument(RegistryError::InvalidDocument(
                "normal commit cannot remove a durable OutputHub",
            )));
        }
        let mut issued = self.load_issued_ids(current_document.as_ref())?;
        let current_hubs = current_hubs.cloned().unwrap_or_default();
        if document
            .output_hubs
            .difference(&current_hubs)
            .any(|hub| issued.contains(hub))
        {
            return Err(StoreError::OutputHubIdPreviouslyIssued);
        }
        issued.extend(document.output_hubs.iter().copied());
        if !current_exists {
            self.ensure_initialized_marker()?;
        }
        self.write_issued_ids(&issued)?;

        let bytes = serde_json::to_vec_pretty(document)
            .map_err(|error| StoreError::InvalidDocument(RegistryError::Json(error.to_string())))?;
        let pending_path = self.write_pending(&bytes)?;
        if current_exists {
            let previous_path = self.previous_path();
            match fs::symlink_metadata(&previous_path) {
                Ok(_) => fs::remove_file(&previous_path).map_err(|source| StoreError::Io {
                    operation: "remove older previous generation",
                    path: previous_path.clone(),
                    source,
                })?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(StoreError::Io {
                        operation: "inspect previous generation",
                        path: previous_path,
                        source,
                    })
                }
            }
            self.sync_directory()?;
            fs::rename(self.current_path(), &previous_path).map_err(|source| StoreError::Io {
                operation: "rotate current generation to previous",
                path: previous_path,
                source,
            })?;
            self.sync_directory()?;
        }
        after_rotation()?;

        fs::rename(&pending_path, self.current_path()).map_err(|source| StoreError::Io {
            operation: "install current generation",
            path: self.current_path(),
            source,
        })?;
        self.sync_directory()?;

        Ok(CommitReceipt {
            revision: document.revision,
            previous_revision,
            bytes_written: bytes.len(),
        })
    }
}

impl OutputIdentityStore for FilesystemOutputIdentityStore {
    fn load(&self) -> Result<LoadOutcome, StoreError> {
        let _lock = self.existing_reader_lock()?;
        self.load_unlocked()
    }
}

impl OutputIdentityStoreMutations for FilesystemOutputIdentityStore {
    fn commit(
        &self,
        expected_revision: u64,
        document: &RegistryDocument,
    ) -> Result<CommitReceipt, StoreError> {
        self.commit_transaction(expected_revision, document, || Ok(()))
    }

    fn restore_previous(&self, recovery_reference: &str) -> Result<RecoveryReceipt, StoreError> {
        if recovery_reference.trim().is_empty() {
            return Err(StoreError::MissingRecoveryReference);
        }
        let _lock = self.acquire_writer_lock()?;
        let issued = self.load_issued_ids(None)?;
        if !self.issued_path().exists() {
            return Err(StoreError::IssuedLedgerUnavailable);
        }
        let (mut restored, issue) = match self.load_unlocked()? {
            LoadOutcome::Recoverable { previous, issue } => (previous, issue),
            _ => return Err(StoreError::NotRecoverable),
        };
        if !restored.output_hubs.is_subset(&issued) {
            return Err(StoreError::IssuedLedgerUnavailable);
        }

        let recovered_from_revision = restored.revision;
        let revision = recovered_from_revision
            .checked_add(1)
            .ok_or(StoreError::RevisionExhausted)?;
        restored.revision = revision;
        restored.audit.push(RegistryAuditEvent::RecoveryRestored {
            revision,
            recovered_from_revision,
            recovery_issue: format!("{:?}", issue.current),
            approval_reference: recovery_reference.to_owned(),
        });
        restored.validate()?;
        let bytes = serde_json::to_vec_pretty(&restored)
            .map_err(|error| StoreError::InvalidDocument(RegistryError::Json(error.to_string())))?;
        let pending_path = self.write_pending(&bytes)?;
        let current_path = self.current_path();
        let damaged_path = self.damaged_path();
        let damaged_candidate_preserved = match fs::symlink_metadata(&current_path) {
            Ok(_) => {
                match fs::symlink_metadata(&damaged_path) {
                    Ok(_) => return Err(StoreError::DamagedCandidateAlreadyExists),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(source) => {
                        return Err(StoreError::Io {
                            operation: "inspect damaged candidate path",
                            path: damaged_path,
                            source,
                        })
                    }
                }
                fs::rename(&current_path, &damaged_path).map_err(|source| StoreError::Io {
                    operation: "preserve damaged current generation",
                    path: damaged_path.clone(),
                    source,
                })?;
                self.sync_directory()?;
                true
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A previous valid generation is still retained, but there
                // were no current bytes to preserve.
                false
            }
            Err(source) => {
                return Err(StoreError::Io {
                    operation: "inspect current generation before recovery",
                    path: current_path,
                    source,
                })
            }
        };
        fs::rename(&pending_path, self.current_path()).map_err(|source| StoreError::Io {
            operation: "install explicitly recovered generation",
            path: self.current_path(),
            source,
        })?;
        self.sync_directory()?;

        Ok(RecoveryReceipt {
            revision,
            recovered_from_revision,
            damaged_candidate_preserved,
            issue,
        })
    }
}

enum CandidateRead {
    Missing,
    Valid(RegistryDocument),
    Invalid(CandidateIssue),
}

impl CandidateRead {
    fn issue(&self) -> Option<CandidateIssue> {
        match self {
            Self::Missing => Some(CandidateIssue::Missing),
            Self::Valid(_) => None,
            Self::Invalid(issue) => Some(issue.clone()),
        }
    }
}

fn read_generation(path: &Path) -> Result<CandidateRead, StoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CandidateRead::Missing),
        Err(source) => {
            return Err(StoreError::Io {
                operation: "inspect registry generation",
                path: path.to_path_buf(),
                source,
            })
        }
    };
    if !metadata.file_type().is_file() {
        return Ok(CandidateRead::Invalid(CandidateIssue::NotRegularFile));
    }
    let bytes = fs::read(path).map_err(|source| StoreError::Io {
        operation: "read registry generation",
        path: path.to_path_buf(),
        source,
    })?;
    let document: RegistryDocument = match serde_json::from_slice(&bytes) {
        Ok(document) => document,
        Err(_) => return Ok(CandidateRead::Invalid(CandidateIssue::InvalidJson)),
    };
    if document.schema_version != REGISTRY_SCHEMA_VERSION {
        return Ok(CandidateRead::Invalid(CandidateIssue::UnsupportedSchema(
            document.schema_version,
        )));
    }
    match document.validate() {
        Ok(()) => Ok(CandidateRead::Valid(document)),
        Err(error) => Ok(CandidateRead::Invalid(CandidateIssue::InvalidDocument(
            error.to_string(),
        ))),
    }
}

fn create_private_directory(path: &Path) -> Result<(), StoreError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    set_private_directory_mode(&mut builder);
    builder
        .create(path)
        .or_else(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() {
                Ok(())
            } else {
                Err(error)
            }
        })
        .map_err(|source| StoreError::Io {
            operation: "create registry directory",
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(unix)]
fn set_private_directory_mode(builder: &mut fs::DirBuilder) {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
}

#[cfg(not(unix))]
fn set_private_directory_mode(_builder: &mut fs::DirBuilder) {}

#[cfg(unix)]
fn set_private_file_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_file_mode(_options: &mut OpenOptions) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputIdentityRegistry;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn interruption_after_rotation_leaves_previous_generation_recoverable() {
        let directory = std::env::temp_dir().join(format!(
            "gnosix-r11d-interruption-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let store = FilesystemOutputIdentityStore::new(&directory);

        let mut first_registry = OutputIdentityRegistry::new();
        first_registry
            .register_hub(
                crate::DurableOutputHubId::from_bytes([1; 16]),
                0,
                "test-provisioning",
            )
            .unwrap();
        let first = first_registry.document().clone();
        store.commit(0, &first).unwrap();

        let mut second_registry =
            OutputIdentityRegistry::from_json(&serde_json::to_string(&first).unwrap()).unwrap();
        second_registry
            .register_hub(
                crate::DurableOutputHubId::from_bytes([2; 16]),
                first.revision,
                "test-provisioning",
            )
            .unwrap();
        let second = second_registry.document().clone();
        let interruption = StoreError::Io {
            operation: "simulated process interruption",
            path: directory.clone(),
            source: io::Error::other("injected after current-to-previous rotation"),
        };

        assert!(matches!(
            store.commit_transaction(first.revision, &second, || Err(interruption)),
            Err(StoreError::Io {
                operation: "simulated process interruption",
                ..
            })
        ));
        assert!(!directory.join(CURRENT_FILE).exists());
        assert!(directory.join(PREVIOUS_FILE).exists());
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Recoverable { previous, issue }
                if previous == first
                    && issue.current == CandidateIssue::Missing
                    && issue.pending_candidates.len() == 1
        ));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn interruption_during_first_commit_is_not_reported_as_absent() {
        let directory = std::env::temp_dir().join(format!(
            "gnosix-r11d-first-commit-interruption-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let store = FilesystemOutputIdentityStore::new(&directory);
        let mut registry = OutputIdentityRegistry::new();
        registry
            .register_hub(
                crate::DurableOutputHubId::from_bytes([1; 16]),
                0,
                "test-provisioning",
            )
            .unwrap();
        let document = registry.document().clone();
        let interruption = StoreError::Io {
            operation: "simulated first-commit interruption",
            path: directory.clone(),
            source: io::Error::other("injected before first generation install"),
        };

        assert!(store
            .commit_transaction(0, &document, || Err(interruption))
            .is_err());
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Unrecoverable { issue }
                if issue.initialized_marker
                    && issue.current == CandidateIssue::Missing
                    && issue.pending_candidates.len() == 1
        ));
        assert!(matches!(
            store.commit(0, &document),
            Err(StoreError::RecoveryRequired(_))
        ));

        fs::remove_dir_all(directory).unwrap();
    }
}
