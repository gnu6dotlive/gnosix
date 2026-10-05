//! Read and mutation capabilities for the R11 identity authority.
//!
//! Readers are public and read-only. The writer adapter and its constructor
//! are crate-private: only code inside this identity-authority package can
//! issue mutations. This is a Rust/API access boundary, not authentication of
//! a process or caller at a runtime boundary.

use crate::store::{OutputIdentityStoreMutations, StoreError};
use crate::{
    BindingState, DurableOutputHubId, ExplicitBindingApproval, OutputIdentityRegistry,
    OutputIdentityStore, OutputObservation, RegistryDocument, RegistryError,
};
use std::collections::BTreeSet;
use thiserror::Error;

/// Read-only authority view over a store. It exposes resolution and revision
/// inspection without granting access to mutation methods.
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{authority::IdentityAuthorityWriter, FilesystemOutputIdentityStore};
/// let _writer = IdentityAuthorityWriter {
///     store: FilesystemOutputIdentityStore::new("/tmp/identity"),
///     registry: Default::default(),
/// };
/// ```
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{ExplicitBindingApproval, authority::IdentityAuthorityWriter};
/// let approval = ExplicitBindingApproval::operator("review-1");
/// IdentityAuthorityWriter::open(approval);
/// ```
pub struct OutputIdentityReader<'a> {
    store: &'a dyn OutputIdentityStore,
}

impl<'a> OutputIdentityReader<'a> {
    pub fn new(store: &'a impl OutputIdentityStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<crate::LoadOutcome, StoreError> {
        self.store.load()
    }

    pub fn revision(&self) -> Result<Option<u64>, StoreError> {
        Ok(match self.store.load()? {
            crate::LoadOutcome::Absent | crate::LoadOutcome::Unrecoverable { .. } => None,
            crate::LoadOutcome::Current(document) => Some(document.revision),
            crate::LoadOutcome::Recoverable { previous, .. } => Some(previous.revision),
        })
    }

    pub fn resolve(
        &self,
        observation: &OutputObservation,
    ) -> Result<IdentityReadState, StoreError> {
        Ok(match self.store.load()? {
            crate::LoadOutcome::Absent => IdentityReadState::Absent,
            crate::LoadOutcome::Current(document) => {
                let registry = OutputIdentityRegistry::from_document(document)
                    .map_err(StoreError::InvalidDocument)?;
                IdentityReadState::Current {
                    revision: registry.revision(),
                    binding: registry.resolve(observation),
                }
            }
            crate::LoadOutcome::Recoverable { previous, issue } => IdentityReadState::Recoverable {
                previous_revision: previous.revision,
                issue,
            },
            crate::LoadOutcome::Unrecoverable { issue } => {
                IdentityReadState::Unrecoverable { issue }
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityReadState {
    Absent,
    Current {
        revision: u64,
        binding: BindingState,
    },
    Recoverable {
        previous_revision: u64,
        issue: crate::RecoveryIssue,
    },
    Unrecoverable {
        issue: crate::RecoveryIssue,
    },
}

/// Opaque authority-side mutation capability. Its issuer is private to the
/// authority module, so downstream readers cannot construct it. A future
/// runtime must still authenticate its callers separately.
pub struct IdentityAuthorityWriter {
    store: crate::FilesystemOutputIdentityStore,
    registry: OutputIdentityRegistry,
}

#[derive(Debug, Error)]
pub enum AuthorityWriteError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Policy(#[from] RegistryError),
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialOutputBinding {
    pub output_hub_id: DurableOutputHubId,
    pub observation: OutputObservation,
    pub authority_reference: String,
    pub approval_reference: String,
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialIdentityProvisionPlan {
    pub outputs: Vec<InitialOutputBinding>,
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitialIdentityProvisionStatus {
    Provisioned,
    Resumed { from_revision: u64 },
    AlreadyProvisioned,
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialIdentityProvisionReceipt {
    pub status: InitialIdentityProvisionStatus,
    pub final_revision: u64,
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
#[derive(Debug, Error)]
pub enum InitialIdentityProvisionError {
    #[error("existing R11 state diverges from the approved initial plan at revision {revision}")]
    DivergentExistingState { revision: u64 },
    #[error("R11 store requires explicit recovery before provisioning: {0:?}")]
    RecoveryRequired(crate::RecoveryIssue),
    #[error("approved initial R11 plan is invalid: {0}")]
    InvalidPlan(RegistryError),
    #[error("independent R11 readback did not match the approved final state")]
    IndependentVerificationFailed,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Authority(#[from] AuthorityWriteError),
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
fn initial_identity_prefixes(
    plan: &InitialIdentityProvisionPlan,
) -> Result<Vec<RegistryDocument>, InitialIdentityProvisionError> {
    let mut registry = OutputIdentityRegistry::new();
    let mut prefixes = vec![registry.document().clone()];
    for output in &plan.outputs {
        let revision = registry.revision();
        registry
            .register_hub(
                output.output_hub_id,
                revision,
                output.authority_reference.clone(),
            )
            .map_err(InitialIdentityProvisionError::InvalidPlan)?;
        prefixes.push(registry.document().clone());
        let revision = registry.revision();
        registry
            .bind(
                output.output_hub_id,
                &output.observation,
                revision,
                ExplicitBindingApproval::operator(output.approval_reference.clone()),
            )
            .map_err(InitialIdentityProvisionError::InvalidPlan)?;
        prefixes.push(registry.document().clone());
    }
    Ok(prefixes)
}

#[cfg(any(test, feature = "r11-production-provisioner"))]
pub fn provision_initial_identity(
    store: crate::FilesystemOutputIdentityStore,
    plan: &InitialIdentityProvisionPlan,
) -> Result<InitialIdentityProvisionReceipt, InitialIdentityProvisionError> {
    let prefixes = initial_identity_prefixes(plan)?;
    let final_document = prefixes.last().expect("initial prefix set is never empty");
    let prefix_index = match store.load()? {
        crate::LoadOutcome::Absent => 0,
        crate::LoadOutcome::Current(document) => prefixes
            .iter()
            .position(|candidate| candidate == &document)
            .ok_or(InitialIdentityProvisionError::DivergentExistingState {
                revision: document.revision,
            })?,
        crate::LoadOutcome::Recoverable { issue, .. }
        | crate::LoadOutcome::Unrecoverable { issue } => {
            return Err(InitialIdentityProvisionError::RecoveryRequired(issue));
        }
    };
    if prefix_index == prefixes.len() - 1 {
        return Ok(InitialIdentityProvisionReceipt {
            status: InitialIdentityProvisionStatus::AlreadyProvisioned,
            final_revision: final_document.revision,
        });
    }

    let mut writer = IdentityAuthorityWriter::open(store.clone())?;
    for (index, output) in plan.outputs.iter().enumerate() {
        let register_prefix = index * 2 + 1;
        if prefix_index < register_prefix {
            let revision = writer.revision();
            writer.register_hub(
                output.output_hub_id,
                revision,
                output.authority_reference.clone(),
            )?;
        }
        let bind_prefix = index * 2 + 2;
        if prefix_index < bind_prefix {
            let revision = writer.revision();
            writer.bind(
                output.output_hub_id,
                &output.observation,
                revision,
                ExplicitBindingApproval::operator(output.approval_reference.clone()),
            )?;
        }
    }

    let verified = match store.load()? {
        crate::LoadOutcome::Current(document) => document == *final_document,
        _ => false,
    };
    if !verified {
        return Err(InitialIdentityProvisionError::IndependentVerificationFailed);
    }
    Ok(InitialIdentityProvisionReceipt {
        status: if prefix_index == 0 {
            InitialIdentityProvisionStatus::Provisioned
        } else {
            InitialIdentityProvisionStatus::Resumed {
                from_revision: prefix_index as u64,
            }
        },
        final_revision: final_document.revision,
    })
}

impl IdentityAuthorityWriter {
    /// Only the authority adapter inside this package can issue the writer.
    #[allow(dead_code)] // Internal issuance seam for the future in.gnu.os authority adapter.
    fn open(store: crate::FilesystemOutputIdentityStore) -> Result<Self, AuthorityWriteError> {
        let registry = match store.load()? {
            crate::LoadOutcome::Absent => OutputIdentityRegistry::new(),
            crate::LoadOutcome::Current(document) => {
                OutputIdentityRegistry::from_document(document)?
            }
            crate::LoadOutcome::Recoverable { issue, .. }
            | crate::LoadOutcome::Unrecoverable { issue } => {
                return Err(StoreError::RecoveryRequired(issue).into())
            }
        };
        Ok(Self { store, registry })
    }

    pub fn revision(&self) -> u64 {
        self.registry.revision()
    }

    pub fn resolve(&self, observation: &OutputObservation) -> BindingState {
        self.registry.resolve(observation)
    }

    pub fn document(&self) -> &RegistryDocument {
        self.registry.document()
    }

    pub fn register_hub(
        &mut self,
        output_hub_id: DurableOutputHubId,
        expected_revision: u64,
        authority_reference: impl Into<String>,
    ) -> Result<u64, AuthorityWriteError> {
        self.update(|registry| {
            registry.register_hub(output_hub_id, expected_revision, authority_reference)
        })
    }

    pub fn bind(
        &mut self,
        output_hub_id: DurableOutputHubId,
        observation: &OutputObservation,
        expected_revision: u64,
        approval: ExplicitBindingApproval,
    ) -> Result<u64, AuthorityWriteError> {
        self.update(|registry| {
            registry.bind(output_hub_id, observation, expected_revision, approval)
        })
    }

    pub fn rebind(
        &mut self,
        from_output_hub_id: DurableOutputHubId,
        to_output_hub_id: DurableOutputHubId,
        observation: &OutputObservation,
        expected_revision: u64,
        approval: ExplicitBindingApproval,
    ) -> Result<u64, AuthorityWriteError> {
        self.update(|registry| {
            registry.rebind(
                from_output_hub_id,
                to_output_hub_id,
                observation,
                expected_revision,
                approval,
            )
        })
    }

    pub fn resolve_ambiguity(
        &mut self,
        selected_output_hub_id: DurableOutputHubId,
        observation: &OutputObservation,
        expected_candidates: &BTreeSet<DurableOutputHubId>,
        expected_revision: u64,
        approval: ExplicitBindingApproval,
    ) -> Result<u64, AuthorityWriteError> {
        self.update(|registry| {
            registry.approve_ambiguous_binding(
                selected_output_hub_id,
                observation,
                expected_candidates,
                expected_revision,
                approval,
            )
        })
    }

    pub fn recover(
        &mut self,
        recovery_reference: &str,
    ) -> Result<crate::RecoveryReceipt, AuthorityWriteError> {
        let receipt = self.store.restore_previous(recovery_reference)?;
        let crate::LoadOutcome::Current(document) = self.store.load()? else {
            return Err(StoreError::NotRecoverable.into());
        };
        self.registry = OutputIdentityRegistry::from_document(document)?;
        Ok(receipt)
    }

    fn update(
        &mut self,
        mutate: impl FnOnce(&mut OutputIdentityRegistry) -> Result<u64, RegistryError>,
    ) -> Result<u64, AuthorityWriteError> {
        let mut candidate = self.registry.clone();
        let revision = mutate(&mut candidate)?;
        self.store
            .commit(self.registry.revision(), candidate.document())?;
        self.registry = candidate;
        Ok(revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BindingEvidence, FilesystemOutputIdentityStore};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

    struct TempStore {
        path: PathBuf,
        store: FilesystemOutputIdentityStore,
    }

    impl TempStore {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "gnosix-r11e-authority-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self {
                store: FilesystemOutputIdentityStore::new(&path),
                path,
            }
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn id(byte: u8) -> DurableOutputHubId {
        DurableOutputHubId::from_bytes([byte; 16])
    }

    fn observation() -> OutputObservation {
        OutputObservation::new([BindingEvidence::EdidFingerprint([7; 32])]).with_connector("DP-2")
    }

    #[test]
    fn writer_mutates_then_commits_and_reader_resolves_the_durable_revision() {
        let temp = TempStore::new();
        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        assert_eq!(writer.register_hub(id(1), 0, "provisioning").unwrap(), 1);
        assert_eq!(
            writer
                .bind(
                    id(1),
                    &observation(),
                    1,
                    ExplicitBindingApproval::operator("review-1")
                )
                .unwrap(),
            2
        );
        assert_eq!(writer.revision(), 2);
        assert!(matches!(
            writer.resolve(&observation()),
            BindingState::Bound { output_hub_id, .. } if output_hub_id == id(1)
        ));
        let reader = OutputIdentityReader::new(&temp.store);
        assert_eq!(reader.revision().unwrap(), Some(2));
        assert!(matches!(
            reader.resolve(&observation()).unwrap(),
            IdentityReadState::Current { revision: 2, binding: BindingState::Bound { output_hub_id, .. } }
                if output_hub_id == id(1)
        ));
    }

    #[test]
    fn failed_policy_or_disk_mutation_does_not_advance_writer_memory() {
        let temp = TempStore::new();
        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        assert!(writer
            .bind(
                id(1),
                &observation(),
                0,
                ExplicitBindingApproval::operator("x")
            )
            .is_err());
        assert_eq!(writer.revision(), 0);
        assert!(matches!(
            temp.store.load().unwrap(),
            crate::LoadOutcome::Absent
        ));
        writer.register_hub(id(1), 0, "provisioning").unwrap();
        fs::create_dir(temp.path.join("output-identities-v1.previous")).unwrap();
        assert!(writer.register_hub(id(2), 1, "provisioning").is_err());
        assert_eq!(writer.revision(), 1);
        assert!(!writer.document().output_hubs.contains(&id(2)));
        assert!(matches!(
            temp.store.load().unwrap(),
            crate::LoadOutcome::Current(document) if document.revision == 1
        ));
    }

    #[test]
    fn only_writer_adapter_can_perform_explicit_recovery() {
        let temp = TempStore::new();
        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        writer.register_hub(id(1), 0, "provisioning").unwrap();
        writer.register_hub(id(2), 1, "provisioning").unwrap();
        fs::write(temp.path.join("output-identities-v1.json"), b"broken").unwrap();
        assert!(matches!(
            OutputIdentityReader::new(&temp.store)
                .resolve(&observation())
                .unwrap(),
            IdentityReadState::Recoverable {
                previous_revision: 1,
                ..
            }
        ));
        let receipt = writer.recover("review-recovery-1").unwrap();
        assert_eq!(receipt.revision, 2);
        assert_eq!(writer.revision(), 2);
        assert_eq!(
            writer.document().audit.last().unwrap(),
            &crate::RegistryAuditEvent::RecoveryRestored {
                revision: 2,
                recovered_from_revision: 1,
                recovery_issue: "InvalidJson".into(),
                approval_reference: "review-recovery-1".into(),
            }
        );
    }

    #[test]
    fn stale_writer_cannot_commit_over_a_newer_writer_revision() {
        let temp = TempStore::new();
        let mut first =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        let mut stale =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        first.register_hub(id(1), 0, "provisioning").unwrap();
        assert!(matches!(
            stale.register_hub(id(2), 0, "provisioning"),
            Err(AuthorityWriteError::Store(StoreError::StaleRevision {
                expected: 0,
                actual: 1,
            }))
        ));
        assert_eq!(stale.revision(), 0);
        assert_eq!(first.revision(), 1);
        assert!(matches!(
            temp.store.load().unwrap(),
            crate::LoadOutcome::Current(document)
                if document.revision == 1 && document.output_hubs.contains(&id(1))
        ));
    }

    #[test]
    fn writer_rebind_is_committed_before_its_in_memory_state_advances() {
        let temp = TempStore::new();
        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        writer.register_hub(id(1), 0, "provisioning").unwrap();
        writer.register_hub(id(2), 1, "provisioning").unwrap();
        writer
            .bind(
                id(1),
                &observation(),
                2,
                ExplicitBindingApproval::operator("bind-1"),
            )
            .unwrap();
        assert_eq!(
            writer
                .rebind(
                    id(1),
                    id(2),
                    &observation(),
                    3,
                    ExplicitBindingApproval::operator("rebind-1"),
                )
                .unwrap(),
            4
        );
        assert!(matches!(
            OutputIdentityReader::new(&temp.store)
                .resolve(&observation())
                .unwrap(),
            IdentityReadState::Current {
                revision: 4,
                binding: BindingState::Bound { output_hub_id, .. }
            } if output_hub_id == id(2)
        ));
    }

    #[test]
    fn writer_ambiguity_resolution_requires_exact_candidates_and_persists_audit() {
        let temp = TempStore::new();
        let mut registry = OutputIdentityRegistry::new();
        registry.register_hub(id(1), 0, "provisioning").unwrap();
        registry.register_hub(id(2), 1, "provisioning").unwrap();
        registry
            .bind(
                id(1),
                &observation(),
                2,
                ExplicitBindingApproval::operator("bind-1"),
            )
            .unwrap();
        temp.store.commit(0, registry.document()).unwrap();
        let mut json: serde_json::Value =
            serde_json::from_str(&registry.to_json().unwrap()).unwrap();
        let mut duplicate = json["bindings"][0].clone();
        duplicate["record_id"] = serde_json::json!(2);
        duplicate["output_hub_id"] = serde_json::to_value(id(2)).unwrap();
        duplicate["revision"] = serde_json::json!(4);
        duplicate["approval_reference"] = serde_json::json!("recovered-duplicate-claim");
        json["bindings"].as_array_mut().unwrap().push(duplicate);
        json["audit"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "kind": "bound",
                "revision": 4,
                "output_hub_id": serde_json::to_value(id(2)).unwrap(),
                "record_id": 2,
                "approval_reference": "recovered-duplicate-claim"
            }));
        json["revision"] = serde_json::json!(4);
        json["next_record_id"] = serde_json::json!(3);
        let ambiguous_document: RegistryDocument = serde_json::from_value(json).unwrap();
        ambiguous_document.validate().unwrap();
        temp.store.commit(3, &ambiguous_document).unwrap();

        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        let candidates: BTreeSet<_> = [id(1), id(2)].into_iter().collect();
        assert_eq!(
            writer.resolve(&observation()),
            BindingState::Ambiguous {
                candidates: candidates.clone()
            }
        );
        assert!(writer
            .resolve_ambiguity(
                id(1),
                &observation(),
                &BTreeSet::from([id(1)]),
                4,
                ExplicitBindingApproval::operator("ambiguity-review"),
            )
            .is_err());
        assert_eq!(writer.revision(), 4);
        assert_eq!(
            writer
                .resolve_ambiguity(
                    id(1),
                    &observation(),
                    &candidates,
                    4,
                    ExplicitBindingApproval::operator("ambiguity-review"),
                )
                .unwrap(),
            5
        );
        assert!(matches!(
            OutputIdentityReader::new(&temp.store)
                .resolve(&observation())
                .unwrap(),
            IdentityReadState::Current {
                revision: 5,
                binding: BindingState::Bound { output_hub_id, .. }
            } if output_hub_id == id(1)
        ));
    }

    fn initial_plan() -> InitialIdentityProvisionPlan {
        InitialIdentityProvisionPlan {
            outputs: vec![
                InitialOutputBinding {
                    output_hub_id: id(1),
                    observation: OutputObservation::new([BindingEvidence::SerialFingerprint(
                        [11; 32],
                    )])
                    .with_make_model("Maker A", "Model A")
                    .with_physical_size_mm(700, 390)
                    .with_connector("HDMI-A-1"),
                    authority_reference: "PERSONAL-PROFILE-01:hub:hdmi".into(),
                    approval_reference: "PERSONAL-PROFILE-01:bind:hdmi".into(),
                },
                InitialOutputBinding {
                    output_hub_id: id(2),
                    observation: OutputObservation::new([BindingEvidence::SerialFingerprint(
                        [22; 32],
                    )])
                    .with_make_model("Maker B", "Model B")
                    .with_physical_size_mm(700, 390)
                    .with_connector("DP-2"),
                    authority_reference: "PERSONAL-PROFILE-01:hub:dp".into(),
                    approval_reference: "PERSONAL-PROFILE-01:bind:dp".into(),
                },
            ],
        }
    }

    #[test]
    fn initial_provisioner_creates_and_independently_resolves_an_absent_store() {
        let temp = TempStore::new();
        let receipt = provision_initial_identity(
            FilesystemOutputIdentityStore::new(&temp.path),
            &initial_plan(),
        )
        .unwrap();
        assert_eq!(receipt.status, InitialIdentityProvisionStatus::Provisioned);
        assert_eq!(receipt.final_revision, 4);
        let reader = OutputIdentityReader::new(&temp.store);
        for output in &initial_plan().outputs {
            assert!(matches!(
                reader.resolve(&output.observation).unwrap(),
                IdentityReadState::Current { revision: 4, binding: BindingState::Bound { output_hub_id, .. } }
                    if output_hub_id == output.output_hub_id
            ));
        }
    }

    #[test]
    fn initial_provisioner_resumes_only_an_exact_plan_prefix() {
        let temp = TempStore::new();
        let plan = initial_plan();
        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        writer
            .register_hub(
                plan.outputs[0].output_hub_id,
                0,
                plan.outputs[0].authority_reference.clone(),
            )
            .unwrap();
        writer
            .bind(
                plan.outputs[0].output_hub_id,
                &plan.outputs[0].observation,
                1,
                ExplicitBindingApproval::operator(plan.outputs[0].approval_reference.clone()),
            )
            .unwrap();
        let receipt =
            provision_initial_identity(FilesystemOutputIdentityStore::new(&temp.path), &plan)
                .unwrap();
        assert_eq!(
            receipt.status,
            InitialIdentityProvisionStatus::Resumed { from_revision: 2 }
        );
        assert_eq!(receipt.final_revision, 4);
    }

    #[test]
    fn initial_provisioner_refuses_divergent_existing_state_without_mutation() {
        let temp = TempStore::new();
        let mut writer =
            IdentityAuthorityWriter::open(FilesystemOutputIdentityStore::new(&temp.path)).unwrap();
        writer.register_hub(id(9), 0, "other-plan").unwrap();
        let before = match temp.store.load().unwrap() {
            crate::LoadOutcome::Current(doc) => doc,
            _ => unreachable!(),
        };
        assert!(matches!(
            provision_initial_identity(
                FilesystemOutputIdentityStore::new(&temp.path),
                &initial_plan()
            ),
            Err(InitialIdentityProvisionError::DivergentExistingState { revision: 1 })
        ));
        let after = match temp.store.load().unwrap() {
            crate::LoadOutcome::Current(doc) => doc,
            _ => unreachable!(),
        };
        assert_eq!(after, before);
    }

    #[test]
    fn initial_provisioner_is_idempotent_after_exact_completion() {
        let temp = TempStore::new();
        let plan = initial_plan();
        provision_initial_identity(FilesystemOutputIdentityStore::new(&temp.path), &plan).unwrap();
        let second =
            provision_initial_identity(FilesystemOutputIdentityStore::new(&temp.path), &plan)
                .unwrap();
        assert_eq!(
            second.status,
            InitialIdentityProvisionStatus::AlreadyProvisioned
        );
        assert_eq!(second.final_revision, 4);
    }

    #[test]
    fn initial_provisioner_refuses_recovery_state() {
        let temp = TempStore::new();
        let plan = initial_plan();
        provision_initial_identity(FilesystemOutputIdentityStore::new(&temp.path), &plan).unwrap();
        fs::write(temp.path.join("output-identities-v1.json"), b"broken").unwrap();
        assert!(matches!(
            provision_initial_identity(FilesystemOutputIdentityStore::new(&temp.path), &plan),
            Err(InitialIdentityProvisionError::RecoveryRequired(_))
        ));
    }
}
