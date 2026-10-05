//! Designated R12 host seam. Only bounded Bar preferences on one existing
//! selected profile can change. The raw writer and whole-document mutation never escape.
use crate::*;
use std::collections::HashMap;
const CAPACITY: usize = 1024;

#[derive(Clone, PartialEq, Eq)]
enum BarMutation {
    Edge(SetBarEdge),
    Overrides(SetBarPreferenceOverrides),
}
impl BarMutation {
    fn request_id(&self) -> [u8; 16] {
        match self {
            Self::Edge(request) => request.request_id,
            Self::Overrides(request) => request.request_id,
        }
    }
    fn epoch(&self) -> [u8; 16] {
        match self {
            Self::Edge(request) => request.epoch,
            Self::Overrides(request) => request.epoch,
        }
    }
    fn profile_id(&self) -> DesktopProfileId {
        match self {
            Self::Edge(request) => request.profile_id,
            Self::Overrides(request) => request.profile_id,
        }
    }
}

/// This is the only production issuer seam, enabled by the authority host.
/// It never returns its store or raw writer.
/// ```compile_fail
/// use in_gnu_os_desktop_profiles::bar_writer::BarProfileWriter;
/// fn leak(writer: BarProfileWriter) { let store = writer.store; }
/// ```
pub struct BarProfileWriter {
    store: DesktopProfileStore,
    profile_id: DesktopProfileId,
    epoch: [u8; 16],
    completed: HashMap<[u8; 16], (BarMutation, BarWriteReceipt)>,
}
impl BarProfileWriter {
    pub fn new(store: DesktopProfileStore, profile_id: DesktopProfileId, epoch: [u8; 16]) -> Self {
        Self {
            store,
            profile_id,
            epoch,
            completed: HashMap::new(),
        }
    }
    pub fn read(&self) -> Result<BarProfileSnapshot, ProfileStoreError> {
        self.store
            .selected_bar_snapshot(self.profile_id, self.epoch)
    }
    pub fn apply(&mut self, request: SetBarEdge) -> BarWriteReceipt {
        self.apply_mutation(BarMutation::Edge(request))
    }
    pub fn apply_overrides(&mut self, request: SetBarPreferenceOverrides) -> BarWriteReceipt {
        self.apply_mutation(BarMutation::Overrides(request))
    }
    fn apply_mutation(&mut self, mutation: BarMutation) -> BarWriteReceipt {
        let request_id = mutation.request_id();
        let refusal = |code| BarWriteReceipt {
            request_id,
            code,
            version: None,
        };
        if mutation.epoch() != self.epoch {
            return refusal(BarWriteCode::AuthorityUnavailable);
        }
        if let Some((old, receipt)) = self.completed.get(&request_id) {
            return if old == &mutation {
                receipt.clone()
            } else {
                refusal(BarWriteCode::Invalid)
            };
        }
        if self.completed.len() >= CAPACITY {
            return refusal(BarWriteCode::AuthorityUnavailable);
        }
        let receipt = if mutation.profile_id() != self.profile_id {
            refusal(BarWriteCode::Invalid)
        } else {
            let result = match &mutation {
                BarMutation::Edge(request) => self.apply_edge_uncached(request),
                BarMutation::Overrides(request) => self.apply_overrides_uncached(request),
            };
            match result {
                Ok((code, version)) => BarWriteReceipt {
                    request_id,
                    code,
                    version: Some(version),
                },
                Err(ProfileStoreError::StaleVersion { .. }) => refusal(BarWriteCode::Conflict),
                Err(ProfileStoreError::UnknownSelectedProfile(_)) => refusal(BarWriteCode::Invalid),
                Err(ProfileStoreError::RecoveryRequired(_) | ProfileStoreError::Identity(_)) => {
                    refusal(BarWriteCode::AuthorityUnavailable)
                }
                // Includes uncertain I/O completion. Never replay an automatic retry.
                Err(_) => refusal(BarWriteCode::Failed),
            }
        };
        self.completed
            .insert(request_id, (mutation, receipt.clone()));
        receipt
    }
    fn load_expected(
        &self,
        expected: &ProfileVersion,
    ) -> Result<(DesktopProfileDocument, ProfileVersion), ProfileStoreError> {
        let snapshot = self.read()?;
        if &snapshot.version != expected {
            return Err(ProfileStoreError::StaleVersion {
                expected: expected.clone(),
                actual: snapshot.version,
            });
        }
        let ProfileLoadOutcome::Current { document, version } = self.store.load()? else {
            return Err(ProfileStoreError::UnknownSelectedProfile(self.profile_id));
        };
        if &version != expected {
            return Err(ProfileStoreError::StaleVersion {
                expected: expected.clone(),
                actual: version,
            });
        }
        Ok((document, version))
    }
    fn profile_mut<'a>(
        &self,
        document: &'a mut DesktopProfileDocument,
    ) -> Result<&'a mut DesktopProfile, ProfileStoreError> {
        document
            .profiles
            .iter_mut()
            .find(|profile| profile.id == self.profile_id)
            .ok_or(ProfileStoreError::UnknownSelectedProfile(self.profile_id))
    }
    fn commit_changed(
        &self,
        expected: &ProfileVersion,
        mut document: DesktopProfileDocument,
    ) -> Result<ProfileVersion, ProfileStoreError> {
        document.revision = document
            .revision
            .checked_add(1)
            .ok_or(ProfileStoreError::RevisionExhausted)?;
        Ok(self
            .store
            .open_writer()
            .commit(expected, &document)?
            .version)
    }
    fn apply_edge_uncached(
        &self,
        request: &SetBarEdge,
    ) -> Result<(BarWriteCode, ProfileVersion), ProfileStoreError> {
        let (mut document, version) = self.load_expected(&request.expected)?;
        let profile = self.profile_mut(&mut document)?;
        if profile.preferences.bar.edge == Some(request.edge) {
            return Ok((BarWriteCode::AlreadySatisfied, version));
        }
        profile.preferences.bar.edge = Some(request.edge);
        let version = self.commit_changed(&request.expected, document)?;
        Ok((BarWriteCode::Applied, version))
    }
    fn apply_overrides_uncached(
        &self,
        request: &SetBarPreferenceOverrides,
    ) -> Result<(BarWriteCode, ProfileVersion), ProfileStoreError> {
        let (mut document, version) = self.load_expected(&request.expected)?;
        let profile = self.profile_mut(&mut document)?;
        if profile.preferences.bar.overrides() == request.preferences {
            return Ok((BarWriteCode::AlreadySatisfied, version));
        }
        profile.preferences.bar.set_overrides(request.preferences);
        let version = self.commit_changed(&request.expected, document)?;
        Ok((BarWriteCode::Applied, version))
    }
}
