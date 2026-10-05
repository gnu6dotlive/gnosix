//! The designated `in.gnu.os` profile write authority.
//!
//! This module exists to make one guarantee cheap to state and hard to
//! violate: **`DesktopProfileWriter` is obtainable from exactly one place, and
//! nothing outside this crate can obtain it at all.**
//!
//! The writer's constructor has always been private, which means a caller can
//! only ever be handed the *typed* operations below. That is deliberate. A
//! Settings surface will eventually want "change the text scale on this
//! output", and the failure mode this design refuses is the generic one -
//! `set(key, value)` - because that turns a schema into a stringly-typed
//! write surface and makes every future preference an untested code path.
//!
//! So a mutation is a [`DesiredDesktopState`], the change it implies is a
//! [`ProfileDiff`], and the commit is a [`ProfileMutationReceipt`]. Each is
//! typed, bounded, and inspectable before anything is written.
//!
//! ## Why the diff is computed rather than assumed
//!
//! A caller supplies a *desired state*, not a patch. That is a stronger
//! statement than "change these fields", and it means the authority can say
//! exactly what the change is before making it. A diff that is empty is
//! reported as [`ProfileDiff::Empty`] and writes nothing at all - a Settings
//! surface that re-sends unchanged state is a normal thing for a UI to do,
//! and it must not manufacture a revision for it.
//!
//! ## Why the caller must name the version it observed
//!
//! Every mutation carries a [`ProfileVersion`], the exact revision and
//! content digest the caller read. If the document moved since, the commit
//! fails with [`ProfileStoreError::StaleVersion`] rather than overwriting a
//! change the caller never saw. This is compare-and-swap, and it is why a
//! mutation is not a fire-and-forget write.
//!
//! Nothing here is process authentication. The store's locking, revision and
//! content-hash rules are unchanged; this is the caller-shaped seam above
//! them.

use std::collections::BTreeMap;

use crate::{
    store::ProfileCommitReceipt, AccessibilityPreferences, BarEdgePreference, BarPreferences,
    ColorSchemePreference, DesktopPreferences, DesktopProfile, DesktopProfileDocument,
    DesktopProfileId, DesktopProfileStore, DesktopProfileWriter, MotionPreferences,
    ProfileLoadOutcome, ProfileStoreError, ProfileVersion, ReducedMotionPreference, TextScale,
    VisualPreferences,
};
use in_gnu_os_output_identity::DurableOutputHubId;

/// The complete user-owned preference state for one durable output.
///
/// This is the "explicit desired state" the product contract asks for. It is a
/// total description rather than a patch, so the diff against current state is
/// always computable and never ambiguous.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesiredOutputPreferences {
    pub color_scheme: Option<ColorSchemePreference>,
    pub reduced_motion: Option<ReducedMotionPreference>,
    pub text_scale: Option<TextScale>,
    pub bar_edge: Option<BarEdgePreference>,
}

impl DesiredOutputPreferences {
    fn to_preferences(&self) -> DesktopPreferences {
        DesktopPreferences {
            visual: VisualPreferences {
                color_scheme: self.color_scheme,
            },
            motion: MotionPreferences {
                reduced_motion: self.reduced_motion,
            },
            accessibility: AccessibilityPreferences {
                text_scale: self.text_scale,
            },
            bar: BarPreferences {
                edge: self.bar_edge,
                ..BarPreferences::default()
            },
        }
    }

    fn from_preferences(preferences: &DesktopPreferences) -> Self {
        Self {
            color_scheme: preferences.visual.color_scheme,
            reduced_motion: preferences.motion.reduced_motion,
            text_scale: preferences.accessibility.text_scale,
            bar_edge: preferences.bar.edge,
        }
    }
}

/// What the caller wants the durable profile document to be.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesiredDesktopState {
    /// Profiles that must exist, keyed by their durable id.
    pub profiles: BTreeMap<DesktopProfileId, DesktopProfile>,
    /// Which profile each durable output should use.
    pub output_selections: BTreeMap<DurableOutputHubId, DesktopProfileId>,
}

impl DesiredDesktopState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare one output's desired preferences, creating or updating the
    /// profile it selects.
    ///
    /// The profile is named after the output rather than the other way round,
    /// because a launcher or Settings surface thinks in terms of "this
    /// monitor", not "this opaque profile id".
    pub fn with_output_preferences(
        mut self,
        output: DurableOutputHubId,
        preferences: DesiredOutputPreferences,
    ) -> Self {
        let profile_id = profile_id_for_output(output);
        self.profiles.insert(
            profile_id,
            DesktopProfile {
                id: profile_id,
                name: format!(
                    "output {}",
                    output.into_bytes()[..8]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                ),
                preferences: preferences.to_preferences(),
            },
        );
        self.output_selections.insert(output, profile_id);
        self
    }

    /// Render the desired state as a candidate document.
    fn to_document(&self, current: &DesktopProfileDocument) -> DesktopProfileDocument {
        self.to_document_for_test(current)
    }

    /// The same rendering, named for the crate-internal tests that build a
    /// starting document from a desired state so they can compute a diff
    /// without a filesystem.
    pub(crate) fn to_document_for_test(
        &self,
        current: &DesktopProfileDocument,
    ) -> DesktopProfileDocument {
        let mut document = current.clone();
        document.profiles = self
            .profiles
            .values()
            .cloned()
            .collect::<Vec<DesktopProfile>>();
        document.output_profile_selections = self
            .output_selections
            .iter()
            .map(|(output, profile)| crate::OutputProfileSelection {
                output_hub_id: *output,
                profile_id: *profile,
            })
            .collect();
        document.revision = current.revision.saturating_add(1);
        document
    }
}

/// The durable profile identity for one output's preferences.
///
/// Derived from the output's durable identity, so the same output always
/// resolves to the same profile and two outputs never collide. This is what
/// keeps a preference from silently migrating between monitors.
fn profile_id_for_output(output: DurableOutputHubId) -> DesktopProfileId {
    DesktopProfileId::from_bytes(output.into_bytes())
}

/// One observable difference between current and desired state.
///
/// Named fields rather than a generic key so that a receipt says what changed
/// in terms a person reviewing the log can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileFieldChange {
    ColorScheme,
    ReducedMotion,
    TextScale,
    BarEdge,
    ProfileAdded,
    ProfileRemoved,
    ProfilePreferenceChanged,
    OutputSelection,
}

impl ProfileFieldChange {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ColorScheme => "COLOR_SCHEME",
            Self::ReducedMotion => "REDUCED_MOTION",
            Self::TextScale => "TEXT_SCALE",
            Self::BarEdge => "BAR_EDGE",
            Self::ProfileAdded => "PROFILE_ADDED",
            Self::ProfileRemoved => "PROFILE_REMOVED",
            Self::ProfilePreferenceChanged => "PROFILE_PREFERENCES",
            Self::OutputSelection => "OUTPUT_SELECTION",
        }
    }
}

/// The bounded difference a mutation would make.
///
/// Bounded on purpose: it names categories of change, never arbitrary content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDiff {
    /// Change categories, sorted and deduplicated, so the same intent always
    /// produces the same diff and a receipt is comparable across runs.
    pub changes: Vec<ProfileFieldChange>,
    /// How many profiles the desired state declares.
    pub desired_profile_count: usize,
}

impl ProfileDiff {
    /// Whether applying this would change nothing.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// A stable, sorted rendering for logs and receipts.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "NO_CHANGE".to_owned();
        }
        let codes: Vec<&str> = self.changes.iter().map(ProfileFieldChange::code).collect();
        codes.join(",")
    }
}

/// Compute the difference between a document and a desired state.
///
/// A pure function, so the diff a caller is shown is the diff that will be
/// applied. Nothing here reads a lock or touches the filesystem.
pub fn diff_document(
    current: &DesktopProfileDocument,
    desired: &DesiredDesktopState,
) -> ProfileDiff {
    let mut changes = Vec::new();

    let current_profiles: BTreeMap<DesktopProfileId, &DesktopProfile> = current
        .profiles
        .iter()
        .map(|profile| (profile.id, profile))
        .collect();

    for (id, profile) in &desired.profiles {
        match current_profiles.get(id) {
            None => changes.push(ProfileFieldChange::ProfileAdded),
            Some(existing) => {
                if DesiredOutputPreferences::from_preferences(&existing.preferences)
                    != DesiredOutputPreferences::from_preferences(&profile.preferences)
                {
                    changes.push(ProfileFieldChange::ProfilePreferenceChanged);
                }
            }
        }
    }

    for id in current_profiles.keys() {
        if !desired.profiles.contains_key(id) {
            changes.push(ProfileFieldChange::ProfileRemoved);
        }
    }

    let current_selections: BTreeMap<DurableOutputHubId, DesktopProfileId> = current
        .output_profile_selections
        .iter()
        .map(|selection| (selection.output_hub_id, selection.profile_id))
        .collect();
    if current_selections != desired.output_selections {
        changes.push(ProfileFieldChange::OutputSelection);
    }

    // Collapse the per-profile preference categories into their named fields so
    // a receipt can say which preference moved, not merely that one did.
    let mut detailed = Vec::new();
    for (id, profile) in &desired.profiles {
        if let Some(existing) = current_profiles.get(id) {
            let before = DesiredOutputPreferences::from_preferences(&existing.preferences);
            let after = DesiredOutputPreferences::from_preferences(&profile.preferences);
            if before.color_scheme != after.color_scheme {
                detailed.push(ProfileFieldChange::ColorScheme);
            }
            if before.reduced_motion != after.reduced_motion {
                detailed.push(ProfileFieldChange::ReducedMotion);
            }
            if before.text_scale != after.text_scale {
                detailed.push(ProfileFieldChange::TextScale);
            }
            if before.bar_edge != after.bar_edge {
                detailed.push(ProfileFieldChange::BarEdge);
            }
        }
    }
    detailed.sort();
    detailed.dedup();
    changes.extend(detailed);
    changes.retain(|change| {
        // The coarse category is redundant once the specific fields are known.
        change != &ProfileFieldChange::ProfilePreferenceChanged
    });
    changes.sort();
    changes.dedup();

    ProfileDiff {
        changes,
        desired_profile_count: desired.profiles.len(),
    }
}

/// What a mutation did, as retained evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMutationOutcome {
    /// The document already matched the desired state. Nothing was written and
    /// no revision was consumed.
    AlreadySatisfied { diff: ProfileDiff },
    /// The document was written atomically.
    Committed {
        diff: ProfileDiff,
        previous: ProfileVersion,
        current: ProfileVersion,
    },
}

/// A mutation receipt.
///
/// This is the durable record of a settings change. It names the diff, both
/// versions, and the outcome, so a later reader can tell what was intended
/// from what happened without reconstructing either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMutationReceipt {
    pub outcome: ProfileMutationOutcome,
}

impl ProfileMutationReceipt {
    pub fn code(&self) -> &'static str {
        match &self.outcome {
            ProfileMutationOutcome::AlreadySatisfied { .. } => "ALREADY_SATISFIED",
            ProfileMutationOutcome::Committed { .. } => "COMMITTED",
        }
    }

    pub fn diff(&self) -> &ProfileDiff {
        match &self.outcome {
            ProfileMutationOutcome::AlreadySatisfied { diff }
            | ProfileMutationOutcome::Committed { diff, .. } => diff,
        }
    }

    /// A single-line rendering suitable for an authority log.
    pub fn summary(&self) -> String {
        match &self.outcome {
            ProfileMutationOutcome::AlreadySatisfied { diff } => {
                format!("outcome=ALREADY_SATISFIED diff={}", diff.summary())
            }
            ProfileMutationOutcome::Committed {
                diff,
                previous,
                current,
            } => format!(
                "outcome=COMMITTED diff={} previous_revision={} current_revision={}",
                diff.summary(),
                previous.revision().unwrap_or_default(),
                current.revision().unwrap_or_default(),
            ),
        }
    }
}

#[cfg(any(test, feature = "r12-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialProfileProvisionPlan {
    pub profile: DesktopProfile,
    pub output_selections: Vec<crate::OutputProfileSelection>,
}

#[cfg(any(test, feature = "r12-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitialProfileProvisionStatus {
    Provisioned,
    AlreadyProvisioned,
}

#[cfg(any(test, feature = "r12-production-provisioner"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialProfileProvisionReceipt {
    pub status: InitialProfileProvisionStatus,
    pub version: ProfileVersion,
}

#[cfg(any(test, feature = "r12-production-provisioner"))]
#[derive(Debug, thiserror::Error)]
pub enum InitialProfileProvisionError {
    #[error("existing R12 state diverges from the approved initial plan at revision {revision}")]
    DivergentExistingState { revision: u64 },
    #[error("R12 store requires explicit recovery before provisioning: {0:?}")]
    RecoveryRequired(crate::ProfileRecoveryIssue),
    #[error("independent R12 readback did not match the approved initial state")]
    IndependentVerificationFailed,
    #[error(transparent)]
    Store(#[from] ProfileStoreError),
}

#[cfg(any(test, feature = "r12-production-provisioner"))]
fn initial_profile_document(
    plan: &InitialProfileProvisionPlan,
) -> Result<DesktopProfileDocument, ProfileStoreError> {
    if plan
        .output_selections
        .iter()
        .any(|selection| selection.profile_id != plan.profile.id)
    {
        return Err(ProfileStoreError::InvalidDocument(
            crate::ProfileError::SelectionReferencesUnknownProfile,
        ));
    }
    let document = DesktopProfileDocument {
        schema_version: crate::DESKTOP_PROFILE_SCHEMA_VERSION,
        revision: 1,
        profiles: vec![plan.profile.clone()],
        output_profile_selections: plan.output_selections.clone(),
    };
    document.validate()?;
    Ok(document)
}

#[cfg(any(test, feature = "r12-production-provisioner"))]
pub fn provision_initial_profile(
    store: &DesktopProfileStore,
    plan: &InitialProfileProvisionPlan,
) -> Result<InitialProfileProvisionReceipt, InitialProfileProvisionError> {
    let expected_document = initial_profile_document(plan)?;
    match store.load()? {
        ProfileLoadOutcome::Absent => {}
        ProfileLoadOutcome::Current { document, version } => {
            if document == expected_document {
                return Ok(InitialProfileProvisionReceipt {
                    status: InitialProfileProvisionStatus::AlreadyProvisioned,
                    version,
                });
            }
            return Err(InitialProfileProvisionError::DivergentExistingState {
                revision: document.revision,
            });
        }
        ProfileLoadOutcome::Recoverable { issue, .. }
        | ProfileLoadOutcome::Unrecoverable { issue } => {
            return Err(InitialProfileProvisionError::RecoveryRequired(issue));
        }
    }

    let commit = store
        .open_writer()
        .commit(&ProfileVersion::Absent, &expected_document)?;
    let verified = match store.load_for_identity()? {
        ProfileLoadOutcome::Current { document, version } => {
            document == expected_document && version == commit.version
        }
        _ => false,
    };
    if !verified {
        return Err(InitialProfileProvisionError::IndependentVerificationFailed);
    }
    Ok(InitialProfileProvisionReceipt {
        status: InitialProfileProvisionStatus::Provisioned,
        version: commit.version,
    })
}

/// The designated write authority.
///
/// ```compile_fail
/// use in_gnu_os_desktop_profiles::DesktopProfileAuthority;
/// let issuer = DesktopProfileAuthority::issue;
/// ```
///
/// Legacy desired-state implementation retained for hermetic qualification.
/// Its issuer is crate-private and test-only. The production host uses the
/// feature-gated bounded `BarProfileWriter`; neither path exposes the raw writer.
#[derive(Debug)]
pub struct DesktopProfileAuthority {
    store: DesktopProfileStore,
}

impl DesktopProfileAuthority {
    /// Issue the write authority for a store.
    ///
    /// The designation is by construction: this is the sole issuer, and the
    /// writer it uses is unreachable except through the typed operations on
    /// this type.
    #[cfg(test)]
    pub(crate) fn issue(store: DesktopProfileStore) -> Self {
        Self { store }
    }

    /// The current durable document, for reading and for computing a diff.
    pub fn current_document(&self) -> Result<DesktopProfileDocument, ProfileStoreError> {
        Ok(self.readable()?.0)
    }

    /// The version a caller must echo back to authorise a mutation.
    pub fn current_version(&self) -> Result<ProfileVersion, ProfileStoreError> {
        Ok(self.readable()?.1)
    }

    /// Load for reading, refusing a document that is not plainly readable.
    ///
    /// A store that needs recovery is *not* readable for the purpose of
    /// computing a diff, because the document on disk is damaged and the
    /// previous candidate is the thing worth keeping. Returning
    /// `RecoveryRequired` here means a Settings surface cannot compute a diff
    /// against damaged state and then write over the recoverable copy.
    fn readable(&self) -> Result<(DesktopProfileDocument, ProfileVersion), ProfileStoreError> {
        self.store.load()?.readable()
    }

    /// Compute the diff a desired state would make, without writing anything.
    pub fn preview(&self, desired: &DesiredDesktopState) -> Result<ProfileDiff, ProfileStoreError> {
        let (current, _) = self.readable()?;
        Ok(diff_document(&current, desired))
    }

    /// Apply a desired state, guarded by the version the caller observed.
    ///
    /// The order matters and is deliberate: read, diff, decide, then write. A
    /// mutation that would change nothing returns without touching the store,
    /// so re-sending unchanged state is free and cannot consume a revision.
    pub fn apply(
        &self,
        desired: &DesiredDesktopState,
        expected: &ProfileVersion,
    ) -> Result<ProfileMutationReceipt, ProfileStoreError> {
        let (current, current_version) = self.readable()?;
        if expected != &current_version {
            return Err(ProfileStoreError::StaleVersion {
                expected: expected.clone(),
                actual: current_version,
            });
        }
        let diff = diff_document(&current, desired);

        if diff.is_empty() {
            return Ok(ProfileMutationReceipt {
                outcome: ProfileMutationOutcome::AlreadySatisfied { diff },
            });
        }

        let document = desired.to_document(&current);
        document.validate()?;

        // The only path to the raw writer in the whole crate.
        let writer: DesktopProfileWriter = self.store.open_writer();
        let commit: ProfileCommitReceipt = writer.commit(expected, &document)?;

        Ok(ProfileMutationReceipt {
            outcome: ProfileMutationOutcome::Committed {
                diff,
                previous: expected.clone(),
                current: commit.version,
            },
        })
    }

    /// The writer, for the recovery path only.
    ///
    /// Recovery installs the previous known-good candidate, which is not a
    /// desired-state mutation and therefore has no diff. It is exposed as a
    /// distinct operation rather than folded into [`Self::apply`], so a
    /// rollback is never expressible as an ordinary settings change.
    pub fn recover(
        &self,
        token: &crate::store::ProfileRecoveryToken,
        recovery_reference: &str,
    ) -> Result<crate::store::ProfileRecoveryReceipt, ProfileStoreError> {
        let writer: DesktopProfileWriter = self.store.open_writer();
        writer.recover(token, recovery_reference)
    }
}

impl ProfileLoadOutcome {
    /// The document and version, when the state is plainly readable.
    ///
    /// `Absent` reads as the empty document with no version, which is what a
    /// first mutation expects. `Recoverable` and `Unrecoverable` are refused:
    /// both mean the current file is damaged and the previous candidate is the
    /// only trustworthy content, so a diff computed here would be a diff
    /// against the wrong thing.
    fn readable(self) -> Result<(DesktopProfileDocument, ProfileVersion), ProfileStoreError> {
        match self {
            Self::Absent => Ok((DesktopProfileDocument::default(), ProfileVersion::Absent)),
            Self::Current { document, version } => Ok((document, version)),
            Self::Recoverable { issue, .. } => Err(ProfileStoreError::RecoveryRequired(issue)),
            Self::Unrecoverable { issue } => Err(ProfileStoreError::RecoveryRequired(issue)),
        }
    }
}
