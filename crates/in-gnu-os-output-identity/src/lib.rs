//! Durable logical OutputHub identity and conservative output binding policy.
//!
//! This crate provides the domain/policy core and identity-only filesystem
//! store for the future `in.gnu.os` authority. It does not allocate identifiers,
//! observe hardware, authorize callers, or communicate with GNOS.UX. The
//! authority adapter owns those responsibilities and selects/protects the
//! store directory. GNOS.UX and `gnosix-desktop-model` remain consumers.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

/// Caller/authority allocated durable logical identity. This type never
/// generates IDs; its 128-bit value is unrelated to compositor/session IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DurableOutputHubId([u8; 16]);

impl DurableOutputHubId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Stable descriptor fingerprints supplied by an observation adapter.
/// Fingerprints are opaque here; raw serial/EDID values need not be persisted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "sha256", rename_all = "snake_case")]
pub enum BindingEvidence {
    EdidFingerprint([u8; 32]),
    SerialFingerprint([u8; 32]),
}

impl BindingEvidence {
    fn same_kind(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::EdidFingerprint(_), Self::EdidFingerprint(_))
                | (Self::SerialFingerprint(_), Self::SerialFingerprint(_))
        )
    }
}

/// Current, ephemeral observation. Connector/model/dimensions are supporting
/// context only; they cannot independently establish durable identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputObservation {
    stable_evidence: BTreeSet<BindingEvidence>,
    make: Option<String>,
    model: Option<String>,
    physical_size_mm: Option<(u32, u32)>,
    connector: Option<String>,
}

impl OutputObservation {
    pub fn new(evidence: impl IntoIterator<Item = BindingEvidence>) -> Self {
        Self {
            stable_evidence: evidence.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn with_make_model(mut self, make: impl Into<String>, model: impl Into<String>) -> Self {
        self.make = Some(make.into());
        self.model = Some(model.into());
        self
    }

    pub fn with_physical_size_mm(mut self, width: u32, height: u32) -> Self {
        self.physical_size_mm = Some((width, height));
        self
    }

    pub fn with_connector(mut self, connector: impl Into<String>) -> Self {
        self.connector = Some(connector.into());
        self
    }
}

/// Evidence captured when an authority explicitly binds an observed output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingRecord {
    pub record_id: u64,
    pub output_hub_id: DurableOutputHubId,
    pub stable_evidence: BTreeSet<BindingEvidence>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub physical_size_mm: Option<(u32, u32)>,
    /// Historical context only; never sufficient for a Bound result.
    pub connector_history: BTreeSet<String>,
    pub active: bool,
    pub approval_reference: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegistryAuditEvent {
    HubCreated {
        revision: u64,
        output_hub_id: DurableOutputHubId,
        authority_reference: String,
    },
    Bound {
        revision: u64,
        output_hub_id: DurableOutputHubId,
        record_id: u64,
        approval_reference: String,
    },
    Rebound {
        revision: u64,
        from_output_hub_id: DurableOutputHubId,
        to_output_hub_id: DurableOutputHubId,
        retired_record_ids: BTreeSet<u64>,
        record_id: u64,
        approval_reference: String,
    },
    AmbiguityResolved {
        revision: u64,
        candidates: BTreeSet<DurableOutputHubId>,
        selected_output_hub_id: DurableOutputHubId,
        retired_record_ids: BTreeSet<u64>,
        approval_reference: String,
    },
    RecoveryRestored {
        revision: u64,
        recovered_from_revision: u64,
        recovery_issue: String,
        approval_reference: String,
    },
}

/// Versioned durable document owned/written only by the `in.gnu.os` authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDocument {
    pub schema_version: u32,
    pub revision: u64,
    pub next_record_id: u64,
    pub output_hubs: BTreeSet<DurableOutputHubId>,
    pub bindings: Vec<BindingRecord>,
    pub audit: Vec<RegistryAuditEvent>,
}

impl Default for RegistryDocument {
    fn default() -> Self {
        Self {
            schema_version: REGISTRY_SCHEMA_VERSION,
            revision: 0,
            next_record_id: 1,
            output_hubs: BTreeSet::new(),
            bindings: Vec::new(),
            audit: Vec::new(),
        }
    }
}

impl RegistryDocument {
    pub fn validate(&self) -> Result<(), RegistryError> {
        validate_document(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitBindingApproval {
    reference: String,
}

impl ExplicitBindingApproval {
    /// Records the authority's decision reference; this value is not an
    /// authentication token and does not authorize an untrusted caller.
    pub fn operator(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingConflictReason {
    ConnectorHistoryIsNotIdentity,
    PreviouslyBoundEvidenceWasRetired,
    StrongDescriptorDisagreement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnboundReason {
    NoStableEvidence,
    NoMatchingBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingState {
    Bound {
        output_hub_id: DurableOutputHubId,
        matched_evidence: BTreeSet<BindingEvidence>,
    },
    Ambiguous {
        candidates: BTreeSet<DurableOutputHubId>,
    },
    Unbound {
        reason: UnboundReason,
    },
    DescriptorConflict {
        candidates: BTreeSet<DurableOutputHubId>,
        reason: BindingConflictReason,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("unsupported OutputHub identity schema version {0}")]
    UnsupportedSchema(u32),
    #[error("registry document is structurally invalid: {0}")]
    InvalidDocument(&'static str),
    #[error("registry revision changed: expected {expected}, actual {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("OutputHub identity is already registered")]
    DuplicateHub,
    #[error("OutputHub identity is not registered")]
    UnknownHub,
    #[error("logical OutputHub already has an active physical-output binding")]
    HubAlreadyBound,
    #[error("binding requires at least one stable descriptor fingerprint")]
    MissingStableEvidence,
    #[error("stable descriptor evidence is already actively bound to another identity")]
    EvidenceAlreadyBound,
    #[error("explicit rebind source has no active binding matching the observation")]
    RebindSourceNotFound,
    #[error("the observation does not currently resolve to the expected ambiguous candidate set")]
    AmbiguousCandidatesChanged,
    #[error("the selected OutputHub is not one of the ambiguous candidates")]
    SelectedHubNotCandidate,
    #[error("the observation is not currently ambiguous across at least two OutputHubs")]
    NotAmbiguous,
    #[error("approval reference must not be empty")]
    MissingApprovalReference,
    #[error("record identifier space is exhausted")]
    RecordIdExhausted,
    #[error("registry revision space is exhausted")]
    RevisionExhausted,
    #[error("JSON encoding/decoding failed: {0}")]
    Json(String),
}

/// In-memory policy state. Public consumers can parse, inspect, and resolve a
/// registry; mutations are available only to the crate's authority adapter.
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{DurableOutputHubId, OutputIdentityRegistry};
/// let mut reader = OutputIdentityRegistry::new();
/// reader.register_hub(DurableOutputHubId::from_bytes([1; 16]), 0, "audit-ref");
/// ```
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{ExplicitBindingApproval, OutputIdentityRegistry};
/// let mut reader = OutputIdentityRegistry::new();
/// reader.bind(todo!(), &todo!(), 0, ExplicitBindingApproval::operator("review"));
/// ```
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{ExplicitBindingApproval, OutputIdentityRegistry};
/// let mut reader = OutputIdentityRegistry::new();
/// reader.rebind(todo!(), todo!(), &todo!(), 0, ExplicitBindingApproval::operator("review"));
/// ```
///
/// ```compile_fail
/// use in_gnu_os_output_identity::{ExplicitBindingApproval, OutputIdentityRegistry};
/// let mut reader = OutputIdentityRegistry::new();
/// reader.approve_ambiguous_binding(todo!(), &todo!(), &Default::default(), 0, ExplicitBindingApproval::operator("review"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputIdentityRegistry {
    document: RegistryDocument,
}

impl OutputIdentityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json(json: &str) -> Result<Self, RegistryError> {
        let document: RegistryDocument =
            serde_json::from_str(json).map_err(|error| RegistryError::Json(error.to_string()))?;
        if document.schema_version != REGISTRY_SCHEMA_VERSION {
            return Err(RegistryError::UnsupportedSchema(document.schema_version));
        }
        document.validate()?;
        Ok(Self { document })
    }

    pub fn to_json(&self) -> Result<String, RegistryError> {
        self.document.validate()?;
        serde_json::to_string_pretty(&self.document)
            .map_err(|error| RegistryError::Json(error.to_string()))
    }

    pub fn revision(&self) -> u64 {
        self.document.revision
    }

    pub fn document(&self) -> &RegistryDocument {
        &self.document
    }

    pub(crate) fn from_document(document: RegistryDocument) -> Result<Self, RegistryError> {
        document.validate()?;
        Ok(Self { document })
    }

    pub(crate) fn register_hub(
        &mut self,
        output_hub_id: DurableOutputHubId,
        expected_revision: u64,
        authority_reference: impl Into<String>,
    ) -> Result<u64, RegistryError> {
        self.check_revision(expected_revision)?;
        let authority_reference = authority_reference.into();
        if authority_reference.trim().is_empty() {
            return Err(RegistryError::MissingApprovalReference);
        }
        if self.document.output_hubs.contains(&output_hub_id) {
            return Err(RegistryError::DuplicateHub);
        }
        let revision = self.next_revision()?;
        self.document.output_hubs.insert(output_hub_id);
        self.document.revision = revision;
        self.document.audit.push(RegistryAuditEvent::HubCreated {
            revision,
            output_hub_id,
            authority_reference,
        });
        Ok(revision)
    }

    pub(crate) fn bind(
        &mut self,
        output_hub_id: DurableOutputHubId,
        observation: &OutputObservation,
        expected_revision: u64,
        approval: ExplicitBindingApproval,
    ) -> Result<u64, RegistryError> {
        self.check_revision(expected_revision)?;
        self.ensure_hub(output_hub_id)?;
        self.ensure_approval(&approval)?;
        self.ensure_stable_evidence(observation)?;
        if self
            .document
            .bindings
            .iter()
            .any(|record| record.active && record.output_hub_id == output_hub_id)
        {
            return Err(RegistryError::HubAlreadyBound);
        }
        if self.evidence_has_active_binding(observation, None) {
            return Err(RegistryError::EvidenceAlreadyBound);
        }
        let revision = self.next_revision()?;
        let record_id = self.next_record_id()?;
        self.document.bindings.push(binding_record(
            record_id,
            output_hub_id,
            observation,
            approval.reference.clone(),
            revision,
        ));
        self.document.revision = revision;
        self.document.audit.push(RegistryAuditEvent::Bound {
            revision,
            output_hub_id,
            record_id,
            approval_reference: approval.reference,
        });
        Ok(revision)
    }

    /// Explicitly replaces a prior physical-output binding. The source logical
    /// identity is named by the caller; connector history only locates its old
    /// record and never establishes identity on its own.
    pub(crate) fn rebind(
        &mut self,
        from_output_hub_id: DurableOutputHubId,
        to_output_hub_id: DurableOutputHubId,
        observation: &OutputObservation,
        expected_revision: u64,
        approval: ExplicitBindingApproval,
    ) -> Result<u64, RegistryError> {
        self.check_revision(expected_revision)?;
        self.ensure_hub(from_output_hub_id)?;
        self.ensure_hub(to_output_hub_id)?;
        self.ensure_approval(&approval)?;
        self.ensure_stable_evidence(observation)?;
        if self
            .document
            .bindings
            .iter()
            .any(|record| record.active && record.output_hub_id == to_output_hub_id)
        {
            return Err(RegistryError::HubAlreadyBound);
        }
        if self.evidence_has_active_binding(observation, Some(from_output_hub_id)) {
            return Err(RegistryError::EvidenceAlreadyBound);
        }

        let source_records: Vec<usize> = self
            .document
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                record.active
                    && record.output_hub_id == from_output_hub_id
                    && (observation
                        .connector
                        .as_ref()
                        .is_some_and(|connector| record.connector_history.contains(connector))
                        || !record
                            .stable_evidence
                            .is_disjoint(&observation.stable_evidence))
            })
            .map(|(index, _)| index)
            .collect();
        if source_records.is_empty() {
            return Err(RegistryError::RebindSourceNotFound);
        }

        let revision = self.next_revision()?;
        let record_id = self.document.next_record_id;
        let next_record_id = record_id
            .checked_add(1)
            .ok_or(RegistryError::RecordIdExhausted)?;
        let retired_record_ids = source_records
            .iter()
            .map(|index| self.document.bindings[*index].record_id)
            .collect();
        for index in source_records {
            self.document.bindings[index].active = false;
        }
        self.document.next_record_id = next_record_id;
        self.document.bindings.push(binding_record(
            record_id,
            to_output_hub_id,
            observation,
            approval.reference.clone(),
            revision,
        ));
        self.document.revision = revision;
        self.document.audit.push(RegistryAuditEvent::Rebound {
            revision,
            from_output_hub_id,
            to_output_hub_id,
            retired_record_ids,
            record_id,
            approval_reference: approval.reference,
        });
        Ok(revision)
    }

    /// Records an explicit operator choice when descriptor evidence currently
    /// resolves to multiple logical OutputHubs. The caller must echo the
    /// candidate set it reviewed; if it changed, no mutation occurs.
    pub(crate) fn approve_ambiguous_binding(
        &mut self,
        selected_output_hub_id: DurableOutputHubId,
        observation: &OutputObservation,
        expected_candidates: &BTreeSet<DurableOutputHubId>,
        expected_revision: u64,
        approval: ExplicitBindingApproval,
    ) -> Result<u64, RegistryError> {
        self.check_revision(expected_revision)?;
        self.ensure_hub(selected_output_hub_id)?;
        self.ensure_approval(&approval)?;
        self.ensure_stable_evidence(observation)?;
        if expected_candidates.len() < 2 {
            return Err(RegistryError::NotAmbiguous);
        }
        if !expected_candidates.contains(&selected_output_hub_id) {
            return Err(RegistryError::SelectedHubNotCandidate);
        }
        let actual_candidates = match self.resolve(observation) {
            BindingState::Ambiguous { candidates } => candidates,
            _ => return Err(RegistryError::NotAmbiguous),
        };
        if actual_candidates != *expected_candidates {
            return Err(RegistryError::AmbiguousCandidatesChanged);
        }
        if actual_candidates.len() < 2 {
            return Err(RegistryError::NotAmbiguous);
        }

        let revision = self.next_revision()?;
        let mut retired_record_ids = BTreeSet::new();
        for record in &mut self.document.bindings {
            if record.active
                && record.output_hub_id != selected_output_hub_id
                && actual_candidates.contains(&record.output_hub_id)
                && !record
                    .stable_evidence
                    .is_disjoint(&observation.stable_evidence)
            {
                record.active = false;
                retired_record_ids.insert(record.record_id);
            }
        }
        self.document.revision = revision;
        self.document
            .audit
            .push(RegistryAuditEvent::AmbiguityResolved {
                revision,
                candidates: actual_candidates,
                selected_output_hub_id,
                retired_record_ids,
                approval_reference: approval.reference,
            });
        Ok(revision)
    }

    pub fn resolve(&self, observation: &OutputObservation) -> BindingState {
        let matching_active: Vec<&BindingRecord> = self
            .document
            .bindings
            .iter()
            .filter(|record| {
                record.active
                    && !record
                        .stable_evidence
                        .is_disjoint(&observation.stable_evidence)
            })
            .collect();
        let active_candidates: BTreeSet<_> = matching_active
            .iter()
            .map(|record| record.output_hub_id)
            .collect();

        let retired_candidates: BTreeSet<_> = self
            .document
            .bindings
            .iter()
            .filter(|record| {
                !record.active
                    && !record
                        .stable_evidence
                        .is_disjoint(&observation.stable_evidence)
            })
            .map(|record| record.output_hub_id)
            .collect();
        if active_candidates.is_empty() && !retired_candidates.is_empty() {
            return BindingState::DescriptorConflict {
                candidates: retired_candidates,
                reason: BindingConflictReason::PreviouslyBoundEvidenceWasRetired,
            };
        }

        let mut conflict_candidates = BTreeSet::new();
        let mut conflict_reason = BindingConflictReason::StrongDescriptorDisagreement;
        for record in &matching_active {
            if descriptor_disagrees(record, observation) {
                conflict_candidates.insert(record.output_hub_id);
            }
        }
        if let Some(connector) = &observation.connector {
            for record in self
                .document
                .bindings
                .iter()
                .filter(|record| record.active && record.connector_history.contains(connector))
            {
                if !active_candidates.contains(&record.output_hub_id) {
                    conflict_candidates.insert(record.output_hub_id);
                    if observation.stable_evidence.is_empty() && active_candidates.is_empty() {
                        conflict_reason = BindingConflictReason::ConnectorHistoryIsNotIdentity;
                    }
                }
            }
        }
        if !conflict_candidates.is_empty() {
            return BindingState::DescriptorConflict {
                candidates: conflict_candidates,
                reason: conflict_reason,
            };
        }

        if active_candidates.len() > 1 {
            return BindingState::Ambiguous {
                candidates: active_candidates,
            };
        }
        if let Some(output_hub_id) = active_candidates.iter().next().copied() {
            let matched_evidence = matching_active
                .iter()
                .filter(|record| record.output_hub_id == output_hub_id)
                .flat_map(|record| {
                    record
                        .stable_evidence
                        .intersection(&observation.stable_evidence)
                        .cloned()
                })
                .collect();
            return BindingState::Bound {
                output_hub_id,
                matched_evidence,
            };
        }

        if observation.stable_evidence.is_empty() {
            if let Some(connector) = &observation.connector {
                let connector_candidates: BTreeSet<_> = self
                    .document
                    .bindings
                    .iter()
                    .filter(|record| record.connector_history.contains(connector))
                    .map(|record| record.output_hub_id)
                    .collect();
                if !connector_candidates.is_empty() {
                    return BindingState::DescriptorConflict {
                        candidates: connector_candidates,
                        reason: BindingConflictReason::ConnectorHistoryIsNotIdentity,
                    };
                }
            }
            return BindingState::Unbound {
                reason: UnboundReason::NoStableEvidence,
            };
        }

        BindingState::Unbound {
            reason: UnboundReason::NoMatchingBinding,
        }
    }

    fn check_revision(&self, expected_revision: u64) -> Result<(), RegistryError> {
        if self.document.revision != expected_revision {
            return Err(RegistryError::StaleRevision {
                expected: expected_revision,
                actual: self.document.revision,
            });
        }
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, RegistryError> {
        self.document
            .revision
            .checked_add(1)
            .ok_or(RegistryError::RevisionExhausted)
    }

    fn next_record_id(&mut self) -> Result<u64, RegistryError> {
        let record_id = self.document.next_record_id;
        self.document.next_record_id = record_id
            .checked_add(1)
            .ok_or(RegistryError::RecordIdExhausted)?;
        Ok(record_id)
    }

    fn ensure_hub(&self, output_hub_id: DurableOutputHubId) -> Result<(), RegistryError> {
        if self.document.output_hubs.contains(&output_hub_id) {
            Ok(())
        } else {
            Err(RegistryError::UnknownHub)
        }
    }

    fn ensure_approval(&self, approval: &ExplicitBindingApproval) -> Result<(), RegistryError> {
        if approval.reference.trim().is_empty() {
            Err(RegistryError::MissingApprovalReference)
        } else {
            Ok(())
        }
    }

    fn ensure_stable_evidence(&self, observation: &OutputObservation) -> Result<(), RegistryError> {
        if observation.stable_evidence.is_empty() {
            Err(RegistryError::MissingStableEvidence)
        } else {
            Ok(())
        }
    }

    fn evidence_has_active_binding(
        &self,
        observation: &OutputObservation,
        except_hub: Option<DurableOutputHubId>,
    ) -> bool {
        self.document.bindings.iter().any(|record| {
            record.active
                && Some(record.output_hub_id) != except_hub
                && !record
                    .stable_evidence
                    .is_disjoint(&observation.stable_evidence)
        })
    }
}

fn binding_record(
    record_id: u64,
    output_hub_id: DurableOutputHubId,
    observation: &OutputObservation,
    approval_reference: String,
    revision: u64,
) -> BindingRecord {
    BindingRecord {
        record_id,
        output_hub_id,
        stable_evidence: observation.stable_evidence.clone(),
        make: observation.make.clone(),
        model: observation.model.clone(),
        physical_size_mm: observation.physical_size_mm,
        connector_history: observation.connector.iter().cloned().collect(),
        active: true,
        approval_reference,
        revision,
    }
}

fn descriptor_disagrees(record: &BindingRecord, observation: &OutputObservation) -> bool {
    let evidence_conflict = record.stable_evidence.iter().any(|stored| {
        observation
            .stable_evidence
            .iter()
            .any(|observed| stored.same_kind(observed) && stored != observed)
    });
    let make_conflict = matches!((&record.make, &observation.make), (Some(a), Some(b)) if a != b);
    let model_conflict =
        matches!((&record.model, &observation.model), (Some(a), Some(b)) if a != b);
    let physical_size_conflict = matches!(
        (record.physical_size_mm, observation.physical_size_mm),
        (Some(a), Some(b)) if a != b
    );
    evidence_conflict || make_conflict || model_conflict || physical_size_conflict
}

fn validate_document(document: &RegistryDocument) -> Result<(), RegistryError> {
    if document.schema_version != REGISTRY_SCHEMA_VERSION {
        return Err(RegistryError::UnsupportedSchema(document.schema_version));
    }
    let mut record_ids = BTreeSet::new();
    let mut active_hubs = BTreeSet::new();
    let mut largest_record_id = 0;
    for record in &document.bindings {
        if !document.output_hubs.contains(&record.output_hub_id) {
            return Err(RegistryError::InvalidDocument(
                "binding refers to an unknown OutputHub",
            ));
        }
        if record.stable_evidence.is_empty() {
            return Err(RegistryError::InvalidDocument(
                "binding has no stable descriptor evidence",
            ));
        }
        if !record_ids.insert(record.record_id) {
            return Err(RegistryError::InvalidDocument(
                "duplicate binding record ID",
            ));
        }
        if record.active && !active_hubs.insert(record.output_hub_id) {
            return Err(RegistryError::InvalidDocument(
                "logical OutputHub has more than one active physical binding",
            ));
        }
        largest_record_id = largest_record_id.max(record.record_id);
    }
    if document.next_record_id <= largest_record_id {
        return Err(RegistryError::InvalidDocument(
            "next binding record ID does not advance past existing records",
        ));
    }
    let audit_len = u64::try_from(document.audit.len())
        .map_err(|_| RegistryError::InvalidDocument("audit is larger than revision space"))?;
    if audit_len != document.revision {
        return Err(RegistryError::InvalidDocument(
            "audit must contain exactly one event for every committed revision",
        ));
    }
    let mut created_hubs = BTreeSet::new();
    let mut record_origins = BTreeMap::new();
    let mut audited_active = BTreeMap::new();
    for (index, event) in document.audit.iter().enumerate() {
        let expected_revision = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(RegistryError::InvalidDocument("audit revision overflow"))?;
        if audit_revision(event) != expected_revision {
            return Err(RegistryError::InvalidDocument(
                "audit revisions must be contiguous and ordered",
            ));
        }
        match event {
            RegistryAuditEvent::HubCreated {
                output_hub_id,
                authority_reference,
                ..
            } => {
                if authority_reference.trim().is_empty()
                    || !document.output_hubs.contains(output_hub_id)
                    || !created_hubs.insert(*output_hub_id)
                {
                    return Err(RegistryError::InvalidDocument(
                        "invalid or duplicate OutputHub allocation audit event",
                    ));
                }
            }
            RegistryAuditEvent::Bound {
                output_hub_id,
                record_id,
                approval_reference,
                ..
            } => {
                if approval_reference.trim().is_empty()
                    || !created_hubs.contains(output_hub_id)
                    || record_origins
                        .insert(
                            *record_id,
                            (*output_hub_id, expected_revision, approval_reference),
                        )
                        .is_some()
                {
                    return Err(RegistryError::InvalidDocument(
                        "invalid or duplicate binding audit event",
                    ));
                }
                audited_active.insert(*record_id, true);
            }
            RegistryAuditEvent::Rebound {
                from_output_hub_id,
                to_output_hub_id,
                retired_record_ids,
                record_id,
                approval_reference,
                ..
            } => {
                if approval_reference.trim().is_empty()
                    || from_output_hub_id == to_output_hub_id
                    || !created_hubs.contains(from_output_hub_id)
                    || !created_hubs.contains(to_output_hub_id)
                    || retired_record_ids.is_empty()
                    || retired_record_ids.iter().any(|retired_id| {
                        !matches!(audited_active.get(retired_id), Some(true))
                            || record_origins
                                .get(retired_id)
                                .is_none_or(|(hub_id, _, _)| hub_id != from_output_hub_id)
                    })
                    || record_origins
                        .insert(
                            *record_id,
                            (*to_output_hub_id, expected_revision, approval_reference),
                        )
                        .is_some()
                {
                    return Err(RegistryError::InvalidDocument(
                        "invalid or duplicate rebind audit event",
                    ));
                }
                for retired_id in retired_record_ids {
                    audited_active.insert(*retired_id, false);
                }
                audited_active.insert(*record_id, true);
            }
            RegistryAuditEvent::AmbiguityResolved {
                candidates,
                selected_output_hub_id,
                retired_record_ids,
                approval_reference,
                ..
            } => {
                if approval_reference.trim().is_empty()
                    || candidates.len() < 2
                    || !candidates.contains(selected_output_hub_id)
                    || candidates.iter().any(|id| !created_hubs.contains(id))
                    || retired_record_ids.is_empty()
                    || retired_record_ids.iter().any(|retired_id| {
                        !matches!(audited_active.get(retired_id), Some(true))
                            || record_origins.get(retired_id).is_none_or(|(hub_id, _, _)| {
                                hub_id == selected_output_hub_id || !candidates.contains(hub_id)
                            })
                    })
                {
                    return Err(RegistryError::InvalidDocument(
                        "invalid ambiguity-resolution audit event",
                    ));
                }
                for retired_id in retired_record_ids {
                    audited_active.insert(*retired_id, false);
                }
            }
            RegistryAuditEvent::RecoveryRestored {
                recovered_from_revision,
                recovery_issue,
                approval_reference,
                ..
            } => {
                if approval_reference.trim().is_empty()
                    || recovery_issue.trim().is_empty()
                    || recovered_from_revision.checked_add(1) != Some(expected_revision)
                {
                    return Err(RegistryError::InvalidDocument(
                        "invalid recovery provenance event",
                    ));
                }
            }
        }
    }
    if created_hubs != document.output_hubs {
        return Err(RegistryError::InvalidDocument(
            "every OutputHub must have one allocation audit event",
        ));
    }
    if record_origins.len() != document.bindings.len()
        || document.bindings.iter().any(|record| {
            !matches!(
                record_origins.get(&record.record_id),
                Some((output_hub_id, revision, approval))
                    if *output_hub_id == record.output_hub_id
                        && *revision == record.revision
                        && approval.as_str() == record.approval_reference.as_str()
            ) || audited_active.get(&record.record_id) != Some(&record.active)
        })
    {
        return Err(RegistryError::InvalidDocument(
            "binding records must match their unique creation audit events",
        ));
    }
    Ok(())
}

fn audit_revision(event: &RegistryAuditEvent) -> u64 {
    match event {
        RegistryAuditEvent::HubCreated { revision, .. }
        | RegistryAuditEvent::Bound { revision, .. }
        | RegistryAuditEvent::Rebound { revision, .. }
        | RegistryAuditEvent::AmbiguityResolved { revision, .. }
        | RegistryAuditEvent::RecoveryRestored { revision, .. } => *revision,
    }
}

mod store;
pub use store::*;

pub mod authority;
pub use authority::{IdentityReadState, OutputIdentityReader};

#[cfg(test)]
extern crate self as in_gnu_os_output_identity;

#[cfg(test)]
#[path = "registry_tests.rs"]
mod registry_tests;

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;
