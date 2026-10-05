//! Versioned profile schema, durable store, and read-only profile projection
//! for in.gnu.os.
//!
//! This crate does not choose production paths, allocate identifiers, observe
//! outputs, authenticate callers, or communicate with renderers. Its document
//! is separate from the R11 output-identity registry. GNOS.UX and Gnoshow are
//! consumers of read-only profile projections; neither is a writer.
//!
//! Only durable user preference belongs here. Runtime scene state, session IDs,
//! semantic generations, focus, health, and solver state have no representation.

use in_gnu_os_output_identity::DurableOutputHubId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

mod authority;
mod bar_write_contract;
pub use bar_write_contract::*;
#[cfg(test)]
mod authority_tests;
#[cfg(any(test, feature = "r12-production-writer"))]
pub mod bar_writer;
mod store;
pub use authority::*;
pub use store::*;

/// Current logical schema. The generation filename remains stable so CAS/rotation
/// history is not split across two authority files during the v1 -> v2 upgrade.
pub const DESKTOP_PROFILE_SCHEMA_VERSION: u32 = 2;

/// The profile document has a distinct namespace from output-identities-v1.json.
pub const DESKTOP_PROFILE_DOCUMENT_FILENAME: &str = "desktop-profiles-v1.json";

/// Authority-allocated durable identity for one desktop profile.
///
/// This is deliberately not a session ID and does not allocate itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DesktopProfileId([u8; 16]);

impl DesktopProfileId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// A user's explicit color-scheme preference. FollowSystem is a preference
/// instruction; it is not the system-observed color-scheme value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSchemePreference {
    FollowSystem,
    Light,
    Dark,
}

/// Motion preference stored in a profile.
///
/// None on DesktopPreferences::reduced_motion means no profile override; it
/// does not claim that the system preference is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReducedMotionPreference {
    FollowSystem,
    Reduce,
}

/// Durable cardinal placement intent for the product Bar. The wire names are
/// explicit and stable; these enum discriminants are never persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarEdgePreference {
    Top,
    Right,
    Bottom,
    Left,
}

/// Text scale in basis points: 10,000 means 1.0x.
///
/// The positive scalar is a durable user preference, independent of compositor
/// output scale. It does not imply automatic large-text preference discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct TextScale(u32);

impl TextScale {
    pub const fn basis_points(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for TextScale {
    type Error = TextScaleError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(TextScaleError(value))
        } else {
            Ok(Self(value))
        }
    }
}

impl From<TextScale> for u32 {
    fn from(value: TextScale) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("text scale must be a positive basis-point value; got {0}")]
pub struct TextScaleError(pub u32);

/// Number of ordinary workspace slots requested for Bar presentation. This is
/// presentation capacity only; it does not create, activate, or identify workspaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct WorkspaceDisplayCount(u8);

impl WorkspaceDisplayCount {
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<i64> for WorkspaceDisplayCount {
    type Error = WorkspaceDisplayCountError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if (1..=20).contains(&value) {
            Ok(Self(value as u8))
        } else {
            Err(WorkspaceDisplayCountError(value))
        }
    }
}

impl From<WorkspaceDisplayCount> for i64 {
    fn from(value: WorkspaceDisplayCount) -> Self {
        i64::from(value.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("workspace display count must be in 1..=20; got {0}")]
pub struct WorkspaceDisplayCountError(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceDisplayMode {
    Shapes,
    Text,
    Icons,
}

/// The five DONOR-02 Bar preference contracts. `None` always means no durable
/// override; no donor default is imported by this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarPreferenceOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_on_hover: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_count: Option<WorkspaceDisplayCount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_display: Option<WorkspaceDisplayMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_compact: Option<bool>,
}

/// User-owned preferences only. System observations are supplied separately
/// when a future authority resolves an effective read-only projection.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopPreferences {
    #[serde(default)]
    pub visual: VisualPreferences,
    #[serde(default)]
    pub motion: MotionPreferences,
    #[serde(default)]
    pub accessibility: AccessibilityPreferences,
    #[serde(default)]
    pub bar: BarPreferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<BarEdgePreference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_on_hover: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_count: Option<WorkspaceDisplayCount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_display: Option<WorkspaceDisplayMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_compact: Option<bool>,
}

impl BarPreferences {
    pub const fn overrides(&self) -> BarPreferenceOverrides {
        BarPreferenceOverrides {
            persistent: self.persistent,
            show_on_hover: self.show_on_hover,
            workspace_count: self.workspace_count,
            workspace_display: self.workspace_display,
            tray_compact: self.tray_compact,
        }
    }

    pub fn set_overrides(&mut self, overrides: BarPreferenceOverrides) {
        self.persistent = overrides.persistent;
        self.show_on_hover = overrides.show_on_hover;
        self.workspace_count = overrides.workspace_count;
        self.workspace_display = overrides.workspace_display;
        self.tray_compact = overrides.tray_compact;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<ColorSchemePreference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduced_motion: Option<ReducedMotionPreference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessibilityPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_scale: Option<TextScale>,
}

/// Durable user profile. Name is descriptive, not an identity or selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProfile {
    pub id: DesktopProfileId,
    pub name: String,
    #[serde(default)]
    pub preferences: DesktopPreferences,
}

/// Explicit assignment from one durable logical output to a durable profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputProfileSelection {
    pub output_hub_id: DurableOutputHubId,
    pub profile_id: DesktopProfileId,
}

/// Identity/configuration document for profile definitions and durable output
/// assignments. It is never merged into the R11 identity registry document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProfileDocument {
    pub schema_version: u32,
    pub revision: u64,
    pub profiles: Vec<DesktopProfile>,
    pub output_profile_selections: Vec<OutputProfileSelection>,
}

impl Default for DesktopProfileDocument {
    fn default() -> Self {
        Self {
            schema_version: DESKTOP_PROFILE_SCHEMA_VERSION,
            revision: 0,
            profiles: Vec::new(),
            output_profile_selections: Vec::new(),
        }
    }
}

impl DesktopProfileDocument {
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.schema_version != DESKTOP_PROFILE_SCHEMA_VERSION {
            return Err(ProfileError::UnsupportedSchema(self.schema_version));
        }

        let mut profile_ids = BTreeSet::new();
        for profile in &self.profiles {
            if !profile_ids.insert(profile.id) {
                return Err(ProfileError::DuplicateProfile);
            }
            if profile.name.trim().is_empty() {
                return Err(ProfileError::EmptyProfileName);
            }
        }

        let mut output_ids = BTreeSet::new();
        for selection in &self.output_profile_selections {
            if !output_ids.insert(selection.output_hub_id) {
                return Err(ProfileError::DuplicateOutputSelection);
            }
            if !profile_ids.contains(&selection.profile_id) {
                return Err(ProfileError::SelectionReferencesUnknownProfile);
            }
        }

        Ok(())
    }

    /// Resolve one durable logical output to its selected profile.
    ///
    /// NoSelection deliberately leaves the caller's existing defaults intact.
    /// The function accepts only durable output identity, never connector names,
    /// session IDs, workspace/focus state, or geometry.
    pub fn resolve_output(
        &self,
        output_hub_id: DurableOutputHubId,
    ) -> Result<OutputProfileResolution<'_>, ProfileError> {
        self.validate()?;
        match self
            .output_profile_selections
            .iter()
            .find(|selection| selection.output_hub_id == output_hub_id)
        {
            None => Ok(OutputProfileResolution::NoSelection),
            Some(selection) => {
                let profile = self
                    .profiles
                    .iter()
                    .find(|profile| profile.id == selection.profile_id)
                    .ok_or(ProfileError::SelectionReferencesUnknownProfile)?;
                Ok(OutputProfileResolution::Selected {
                    profile_id: selection.profile_id,
                    profile,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputProfileResolution<'a> {
    NoSelection,
    Selected {
        profile_id: DesktopProfileId,
        profile: &'a DesktopProfile,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProfileError {
    #[error("unsupported desktop profile schema version {0}")]
    UnsupportedSchema(u32),
    #[error("duplicate profile identity")]
    DuplicateProfile,
    #[error("duplicate durable output profile selection")]
    DuplicateOutputSelection,
    #[error("profile name must not be empty")]
    EmptyProfileName,
    #[error("output selection references an unknown profile")]
    SelectionReferencesUnknownProfile,
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_gnu_os_output_identity::DurableOutputHubId;

    fn id(byte: u8) -> DesktopProfileId {
        DesktopProfileId::from_bytes([byte; 16])
    }

    fn output(byte: u8) -> DurableOutputHubId {
        DurableOutputHubId::from_bytes([byte; 16])
    }

    fn profile(profile_id: DesktopProfileId, name: &str) -> DesktopProfile {
        DesktopProfile {
            id: profile_id,
            name: name.to_owned(),
            preferences: DesktopPreferences::default(),
        }
    }

    #[test]
    fn empty_document_is_valid_and_has_no_output_selection() {
        let document = DesktopProfileDocument::default();

        assert_eq!(document.validate(), Ok(()));
        assert_eq!(
            document.resolve_output(output(1)),
            Ok(OutputProfileResolution::NoSelection)
        );
    }

    #[test]
    fn output_selection_resolves_only_by_durable_output_identity() {
        let profile_id = id(2);
        let mut document = DesktopProfileDocument::default();
        document.profiles.push(profile(profile_id, "Studio"));
        document
            .output_profile_selections
            .push(OutputProfileSelection {
                output_hub_id: output(1),
                profile_id,
            });

        assert!(matches!(
            document.resolve_output(output(1)),
            Ok(OutputProfileResolution::Selected {
                profile_id: selected,
                profile: DesktopProfile { name, .. }
            }) if selected == profile_id && name == "Studio"
        ));
        assert_eq!(
            document.resolve_output(output(3)),
            Ok(OutputProfileResolution::NoSelection)
        );
    }

    #[test]
    fn selection_cannot_reference_a_missing_profile() {
        let mut document = DesktopProfileDocument::default();
        document
            .output_profile_selections
            .push(OutputProfileSelection {
                output_hub_id: output(1),
                profile_id: id(9),
            });

        assert_eq!(
            document.validate(),
            Err(ProfileError::SelectionReferencesUnknownProfile)
        );
    }

    #[test]
    fn duplicate_profile_ids_are_rejected() {
        let mut document = DesktopProfileDocument::default();
        document.profiles.push(profile(id(1), "Studio"));
        document.profiles.push(profile(id(1), "Travel"));

        assert_eq!(document.validate(), Err(ProfileError::DuplicateProfile));
    }

    #[test]
    fn duplicate_output_selections_are_rejected() {
        let profile_id = id(1);
        let mut document = DesktopProfileDocument::default();
        document.profiles.push(profile(profile_id, "Studio"));
        document.output_profile_selections.extend([
            OutputProfileSelection {
                output_hub_id: output(1),
                profile_id,
            },
            OutputProfileSelection {
                output_hub_id: output(1),
                profile_id,
            },
        ]);

        assert_eq!(
            document.validate(),
            Err(ProfileError::DuplicateOutputSelection)
        );
    }

    #[test]
    fn profile_name_must_contain_non_whitespace_text() {
        let mut document = DesktopProfileDocument::default();
        document.profiles.push(profile(id(1), "  \t "));

        assert_eq!(document.validate(), Err(ProfileError::EmptyProfileName));
    }

    #[test]
    fn future_schema_is_rejected_without_downgrade() {
        let document = DesktopProfileDocument {
            schema_version: DESKTOP_PROFILE_SCHEMA_VERSION + 1,
            ..DesktopProfileDocument::default()
        };

        assert_eq!(
            document.validate(),
            Err(ProfileError::UnsupportedSchema(
                DESKTOP_PROFILE_SCHEMA_VERSION + 1
            ))
        );
    }

    #[test]
    fn text_scale_is_positive_and_uses_a_relative_basis_point_unit() {
        assert_eq!(TextScale::try_from(1).unwrap().basis_points(), 1);
        assert_eq!(TextScale::try_from(20_000).unwrap().basis_points(), 20_000);
        assert_eq!(TextScale::try_from(40_000).unwrap().basis_points(), 40_000);
        assert!(TextScale::try_from(0).is_err());
    }

    #[test]
    fn profile_document_round_trips_without_session_or_runtime_state() {
        let profile_id = id(2);
        let mut document = DesktopProfileDocument::default();
        document.profiles.push(DesktopProfile {
            id: profile_id,
            name: "Studio".into(),
            preferences: DesktopPreferences {
                visual: VisualPreferences {
                    color_scheme: Some(ColorSchemePreference::Dark),
                },
                motion: MotionPreferences {
                    reduced_motion: Some(ReducedMotionPreference::Reduce),
                },
                accessibility: AccessibilityPreferences {
                    text_scale: Some(TextScale::try_from(12_500u32).unwrap()),
                },
                bar: BarPreferences {
                    edge: Some(BarEdgePreference::Left),
                    ..BarPreferences::default()
                },
            },
        });
        document
            .output_profile_selections
            .push(OutputProfileSelection {
                output_hub_id: output(1),
                profile_id,
            });

        let encoded = serde_json::to_vec(&document).unwrap();
        let decoded: DesktopProfileDocument = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(decoded, document);
        for forbidden in [b"workspace".as_slice(), b"focused", b"pointer", b"health"] {
            assert!(!encoded.windows(forbidden.len()).any(|w| w == forbidden));
        }
    }

    #[test]
    fn schema_rejects_unknown_fields() {
        let json = r#"{
            "schema_version": 1,
            "revision": 0,
            "profiles": [],
            "output_profile_selections": [],
            "current_workspace": 12
        }"#;

        assert!(serde_json::from_str::<DesktopProfileDocument>(json).is_err());
    }

    #[test]
    fn historical_profile_without_bar_preference_remains_valid() {
        let json = r#"{
            "schema_version": 1,
            "revision": 4,
            "profiles": [{
                "id": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],
                "name": "Studio",
                "preferences": {
                    "visual": { "color_scheme": "dark" },
                    "motion": { "reduced_motion": "reduce" },
                    "accessibility": { "text_scale": 12500 }
                }
            }],
            "output_profile_selections": []
        }"#;

        let decoded: DesktopProfileDocument = serde_json::from_str(json)
            .expect("historical profile without a Bar edge remains readable");
        assert_eq!(decoded.revision, 4);
        assert!(decoded.profiles[0].preferences.bar.edge.is_none());
    }

    #[test]
    fn explicit_bar_edges_use_stable_names_and_round_trip() {
        let expected = [
            (BarEdgePreference::Top, "top"),
            (BarEdgePreference::Right, "right"),
            (BarEdgePreference::Bottom, "bottom"),
            (BarEdgePreference::Left, "left"),
        ];

        for (edge, stable_name) in expected {
            let preferences = DesktopPreferences {
                bar: BarPreferences {
                    edge: Some(edge),
                    ..BarPreferences::default()
                },
                ..DesktopPreferences::default()
            };
            let encoded = serde_json::to_value(&preferences).expect("serialize preferences");
            assert_eq!(encoded["bar"]["edge"], stable_name);
            let decoded: DesktopPreferences = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded.bar.edge, Some(edge));
        }
    }

    #[test]
    fn malformed_persisted_bar_edge_is_rejected() {
        let json = r#"{
            "schema_version": 1,
            "revision": 5,
            "profiles": [{
                "id": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],
                "name": "Studio",
                "preferences": { "bar": { "edge": "diagonal" } }
            }],
            "output_profile_selections": []
        }"#;

        assert!(serde_json::from_str::<DesktopProfileDocument>(json).is_err());
    }

    #[test]
    fn invalid_text_scale_is_rejected_during_deserialization() {
        let json = r#"{
            "schema_version": 1,
            "revision": 0,
            "profiles": [{
                "id": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],
                "name": "Studio",
                "preferences": { "text_scale": 4000 }
            }],
            "output_profile_selections": []
        }"#;

        assert!(serde_json::from_str::<DesktopProfileDocument>(json).is_err());
    }
}

#[cfg(test)]
mod donor_bar_preference_tests {
    use super::*;

    #[test]
    fn workspace_display_count_accepts_only_the_ratified_one_through_twenty_range() {
        assert_eq!(WorkspaceDisplayCount::try_from(1_i64).unwrap().get(), 1);
        assert_eq!(WorkspaceDisplayCount::try_from(20_i64).unwrap().get(), 20);
        for raw in [-1_i64, 0, 21, i64::MAX] {
            assert!(
                WorkspaceDisplayCount::try_from(raw).is_err(),
                "accepted {raw}"
            );
        }
    }

    #[test]
    fn workspace_display_mode_round_trips_only_symbolic_names() {
        for (mode, wire) in [
            (WorkspaceDisplayMode::Shapes, "\"shapes\""),
            (WorkspaceDisplayMode::Text, "\"text\""),
            (WorkspaceDisplayMode::Icons, "\"icons\""),
        ] {
            assert_eq!(serde_json::to_string(&mode).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<WorkspaceDisplayMode>(wire).unwrap(),
                mode
            );
        }
        for invalid in ["\"shape\"", "\"Shapes\"", "0", "3"] {
            assert!(serde_json::from_str::<WorkspaceDisplayMode>(invalid).is_err());
        }
    }

    #[test]
    fn donor_bar_overrides_preserve_absence_false_and_explicit_values() {
        let absent = BarPreferenceOverrides::default();
        assert_eq!(serde_json::to_string(&absent).unwrap(), "{}");

        let explicit = BarPreferenceOverrides {
            persistent: Some(false),
            show_on_hover: Some(true),
            workspace_count: Some(WorkspaceDisplayCount::try_from(20_i64).unwrap()),
            workspace_display: Some(WorkspaceDisplayMode::Icons),
            tray_compact: Some(false),
        };
        let json = serde_json::to_string(&explicit).unwrap();
        assert_eq!(
            serde_json::from_str::<BarPreferenceOverrides>(&json).unwrap(),
            explicit
        );
        assert!(json.contains("\"persistent\":false"));
        assert!(json.contains("\"workspace_count\":20"));
        assert!(json.contains("\"workspace_display\":\"icons\""));
    }

    #[test]
    fn persisted_bar_preferences_embed_exact_overrides_without_touching_edge() {
        let bar = BarPreferences {
            edge: Some(BarEdgePreference::Left),
            persistent: Some(true),
            show_on_hover: Some(false),
            workspace_count: Some(WorkspaceDisplayCount::try_from(7_i64).unwrap()),
            workspace_display: Some(WorkspaceDisplayMode::Text),
            tray_compact: Some(true),
        };
        assert_eq!(
            bar.overrides(),
            BarPreferenceOverrides {
                persistent: Some(true),
                show_on_hover: Some(false),
                workspace_count: Some(WorkspaceDisplayCount::try_from(7_i64).unwrap()),
                workspace_display: Some(WorkspaceDisplayMode::Text),
                tray_compact: Some(true),
            }
        );
        let encoded = serde_json::to_value(&bar).unwrap();
        assert_eq!(encoded["edge"], "left");
        assert_eq!(encoded["workspace_count"], 7);
    }
}
