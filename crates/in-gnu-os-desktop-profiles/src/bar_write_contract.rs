//! Fixed Bar-edge protocol data; no store paths or writer capabilities.
use crate::{BarEdgePreference, DesktopProfileId, ProfileVersion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarProfileSnapshot {
    pub epoch: [u8; 16],
    pub profile_id: DesktopProfileId,
    pub version: ProfileVersion,
    pub identity_revision: u64,
    /// None means the effective Top default, not a malformed value.
    pub edge: Option<BarEdgePreference>,
    /// Explicit DONOR-02 overrides only. Absence never imports donor defaults.
    pub preferences: crate::BarPreferenceOverrides,
}
impl BarProfileSnapshot {
    pub fn effective_edge(&self) -> BarEdgePreference {
        self.edge.unwrap_or(BarEdgePreference::Top)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// The only mutation input; unsupported settings cannot be constructed.
/// ```compile_fail
/// use in_gnu_os_desktop_profiles::SetBarEdge;
/// fn unbounded(request: SetBarEdge) {
///     let other = SetBarEdge { arbitrary_preference: "exec", ..request };
/// }
/// ```
pub struct SetBarEdge {
    pub request_id: [u8; 16],
    pub epoch: [u8; 16],
    pub profile_id: DesktopProfileId,
    pub expected: ProfileVersion,
    pub edge: BarEdgePreference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetBarPreferenceOverrides {
    pub request_id: [u8; 16],
    pub epoch: [u8; 16],
    pub profile_id: DesktopProfileId,
    pub expected: ProfileVersion,
    pub preferences: crate::BarPreferenceOverrides,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarWriteCode {
    Applied,
    AlreadySatisfied,
    Conflict,
    Invalid,
    AuthorityUnavailable,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarWriteReceipt {
    pub request_id: [u8; 16],
    pub code: BarWriteCode,
    /// Exact committed version on success, or None on refusal/unknown outcome.
    pub version: Option<ProfileVersion>,
}

/// Narrow wire identity; this is not a general settings interface.
pub const BAR_PROFILE_BUS_NAME: &str = "in.gnu.os.DesktopProfiles1";
pub const BAR_PROFILE_OBJECT_PATH: &str = "/in/gnu/os/DesktopProfiles1";
