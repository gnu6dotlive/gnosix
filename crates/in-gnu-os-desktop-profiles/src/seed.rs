//! Hermetic default personal-profile seed and initialization plan builder.

use crate::{
    DesktopPreferences, DesktopProfile, DesktopProfileDocument, DesktopProfileId,
    InitialProfileProvisionPlan, OutputProfileSelection, ProfileStoreError,
    DESKTOP_PROFILE_SCHEMA_VERSION,
};
use in_gnu_os_output_identity::DurableOutputHubId;
use serde::{Deserialize, Serialize};

/// Ratified default Personal profile identifier (`c978753b-6cef-3b26-d05d-bfb692825b2e`).
pub const DEFAULT_PERSONAL_PROFILE_ID_BYTES: [u8; 16] = [
    0xc9, 0x78, 0x75, 0x3b, 0x6c, 0xef, 0x3b, 0x26, 0xd0, 0x5d, 0xbf, 0xb6, 0x92, 0x82, 0x5b, 0x2e,
];

/// Canonical seed template for first-boot profile bootstrapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProfileSeed {
    pub profile: DesktopProfile,
}

impl Default for DesktopProfileSeed {
    fn default() -> Self {
        Self {
            profile: DesktopProfile {
                id: DesktopProfileId::from_bytes(DEFAULT_PERSONAL_PROFILE_ID_BYTES),
                name: "Personal".to_string(),
                preferences: DesktopPreferences::default(),
            },
        }
    }
}

impl DesktopProfileSeed {
    pub fn new(profile: DesktopProfile) -> Self {
        Self { profile }
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_json_file(path: impl AsRef<std::path::Path>) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Converts this seed into an `InitialProfileProvisionPlan` associating
    /// the provided output hubs with this profile.
    pub fn into_provision_plan(
        self,
        output_hubs: impl IntoIterator<Item = DurableOutputHubId>,
    ) -> InitialProfileProvisionPlan {
        let profile_id = self.profile.id;
        let output_selections = output_hubs
            .into_iter()
            .map(|output_hub_id| OutputProfileSelection {
                output_hub_id,
                profile_id,
            })
            .collect();
        InitialProfileProvisionPlan {
            profile: self.profile,
            output_selections,
        }
    }

    /// Creates a validated Gen-1 `DesktopProfileDocument` for the given outputs.
    pub fn into_document(
        self,
        output_hubs: impl IntoIterator<Item = DurableOutputHubId>,
    ) -> Result<DesktopProfileDocument, ProfileStoreError> {
        let plan = self.into_provision_plan(output_hubs);
        let document = DesktopProfileDocument {
            schema_version: DESKTOP_PROFILE_SCHEMA_VERSION,
            revision: 1,
            profiles: vec![plan.profile],
            output_profile_selections: plan.output_selections,
        };
        document.validate()?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_seed_into_document_produces_valid_gen1() {
        let seed = DesktopProfileSeed::default();
        let hub = DurableOutputHubId::from_bytes([7; 16]);
        let doc = seed.into_document(vec![hub]).unwrap();
        assert_eq!(doc.schema_version, DESKTOP_PROFILE_SCHEMA_VERSION);
        assert_eq!(doc.revision, 1);
        assert_eq!(doc.profiles.len(), 1);
        assert_eq!(doc.profiles[0].name, "Personal");
        assert_eq!(doc.output_profile_selections.len(), 1);
        assert_eq!(doc.output_profile_selections[0].output_hub_id, hub);
        assert_eq!(
            doc.output_profile_selections[0].profile_id,
            doc.profiles[0].id
        );
    }

    #[test]
    fn seed_json_roundtrip() {
        let seed = DesktopProfileSeed::default();
        let serialized = serde_json::to_string_pretty(&seed).unwrap();
        let deserialized = DesktopProfileSeed::from_json_str(&serialized).unwrap();
        assert_eq!(seed, deserialized);
    }

    #[test]
    fn default_template_file_loads_and_validates() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("share/gnosix/profiles/default-profile-seed.json");
        let seed =
            DesktopProfileSeed::from_json_file(&path).expect("failed to load default template");
        let hub = DurableOutputHubId::from_bytes([1; 16]);
        let doc = seed
            .into_document(vec![hub])
            .expect("failed to convert template to document");
        assert_eq!(doc.profiles[0].name, "Personal");
        assert_eq!(doc.revision, 1);
    }
}
