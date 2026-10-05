//! Pure settings contracts for Gnosix.
//!
//! This crate deliberately does not implement a daemon, CLI, polkit agent,
//! privileged helper, XDG writer, systemd controller, or secret transport.
//! It only defines the stable concepts needed by those future components.

use in_gnu_os_desktop_profiles::BarEdgePreference;
use std::{fmt, time::Duration};

/// Provenance shown next to the effective Bar edge in a Settings surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarEdgeSettingProvenance {
    Default,
    Explicit,
}

/// Read-only point-in-time values needed to present the Bar placement control.
/// A missing edge means profile recovery/unavailability prevented an honest
/// effective projection; revisions remain diagnostic provenance only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarEdgeSettingProjection {
    pub effective_edge: Option<BarEdgePreference>,
    pub provenance: Option<BarEdgeSettingProvenance>,
    pub profile_revision: Option<u64>,
    pub identity_revision: Option<u64>,
}

/// Typed pending user intent. It is not a write request or success receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingBarEdgeMutation {
    pub edge: BarEdgePreference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarEdgeMutationAvailability {
    Unavailable,
}

/// Small Settings control model for the four bounded Bar placement choices.
/// The production R12 writer host is not present, so choosing an edge can only
/// produce a pending typed intent and must never claim persistence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarEdgeSettingsControl {
    projection: BarEdgeSettingProjection,
    focused_choice: usize,
    pending: Option<PendingBarEdgeMutation>,
}

impl BarEdgeSettingsControl {
    pub const CHOICES: [BarEdgePreference; 4] = [
        BarEdgePreference::Top,
        BarEdgePreference::Right,
        BarEdgePreference::Bottom,
        BarEdgePreference::Left,
    ];

    pub const fn new(projection: BarEdgeSettingProjection) -> Self {
        let focused_choice = match projection.effective_edge {
            Some(BarEdgePreference::Top) => 0,
            Some(BarEdgePreference::Right) => 1,
            Some(BarEdgePreference::Bottom) => 2,
            Some(BarEdgePreference::Left) => 3,
            None => 0,
        };
        Self {
            projection,
            focused_choice,
            pending: None,
        }
    }

    pub const fn projection(&self) -> BarEdgeSettingProjection {
        self.projection
    }

    pub const fn mutation_availability(&self) -> BarEdgeMutationAvailability {
        BarEdgeMutationAvailability::Unavailable
    }

    pub const fn focused_edge(&self) -> BarEdgePreference {
        Self::CHOICES[self.focused_choice]
    }

    pub const fn pending(&self) -> Option<PendingBarEdgeMutation> {
        self.pending
    }

    pub fn move_focus(&mut self, delta: i8) {
        let count = Self::CHOICES.len() as i8;
        self.focused_choice = (self.focused_choice as i8 + delta).rem_euclid(count) as usize;
    }

    pub fn select_focused(&mut self) -> Option<PendingBarEdgeMutation> {
        let intent = PendingBarEdgeMutation {
            edge: self.focused_edge(),
        };
        if self.projection.effective_edge == Some(intent.edge) {
            self.pending = None;
            None
        } else {
            self.pending = Some(intent);
            Some(intent)
        }
    }

    pub fn select(&mut self, edge: BarEdgePreference) -> Option<PendingBarEdgeMutation> {
        self.focused_choice = Self::CHOICES
            .iter()
            .position(|choice| *choice == edge)
            .expect("all typed Bar edges are present in the bounded selector");
        self.select_focused()
    }

    pub const fn accessible_name(edge: BarEdgePreference) -> &'static str {
        match edge {
            BarEdgePreference::Top => "Top",
            BarEdgePreference::Right => "Right",
            BarEdgePreference::Bottom => "Bottom",
            BarEdgePreference::Left => "Left",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SettingId(String);

impl SettingId {
    pub fn parse(value: impl Into<String>) -> Result<Self, SettingIdError> {
        let value = value.into();
        if value.is_empty()
            || value.starts_with('.')
            || value.ends_with('.')
            || value.split('.').any(|part| {
                part.is_empty()
                    || !part.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                    })
            })
        {
            return Err(SettingIdError(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SettingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingIdError(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SettingCategory {
    Applications,
    Appearance,
    Accessibility,
    Input,
    Session,
    Privacy,
    Network,
    Storage,
    System,
    Developer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingScope {
    User,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyStrategy {
    Immediate,
    HotReload,
    RestartUserService,
    RestartSession,
    RebootRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeRequirement {
    Never,
    PolkitInteractive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingDescriptor {
    pub id: SettingId,
    pub category: SettingCategory,
    pub scope: SettingScope,
    pub apply: ApplyStrategy,
    pub privilege: PrivilegeRequirement,
    pub title: String,
    pub description: String,
    pub search_terms: Vec<String>,
}

impl SettingDescriptor {
    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return true;
        }

        self.id.as_str().contains(&query)
            || self.title.to_ascii_lowercase().contains(&query)
            || self.description.to_ascii_lowercase().contains(&query)
            || self
                .search_terms
                .iter()
                .any(|term| term.to_ascii_lowercase().contains(&query))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DesktopEntryId(String);

impl DesktopEntryId {
    pub fn parse(value: impl Into<String>) -> Result<Self, DesktopEntryIdError> {
        let value = value.into();
        if value == ".desktop"
            || !value.ends_with(".desktop")
            || value.contains('/')
            || value.contains('\\')
            || value.contains('\0')
            || value.trim() != value
        {
            return Err(DesktopEntryIdError(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopEntryIdError(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefaultApplicationTarget {
    MimeType(String),
    UriScheme(String),
}

impl DefaultApplicationTarget {
    pub fn mime(value: impl Into<String>) -> Result<Self, DefaultApplicationTargetError> {
        let value = value.into();
        if value.is_empty()
            || value.contains(char::is_whitespace)
            || !value.contains('/')
            || value.starts_with('/')
            || value.ends_with('/')
        {
            return Err(DefaultApplicationTargetError(value));
        }
        Ok(Self::MimeType(value))
    }

    pub fn uri_scheme(value: impl Into<String>) -> Result<Self, DefaultApplicationTargetError> {
        let value = value.into().to_ascii_lowercase();
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(DefaultApplicationTargetError(value));
        };
        if !first.is_ascii_alphabetic()
            || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            return Err(DefaultApplicationTargetError(value));
        }
        Ok(Self::UriScheme(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultApplicationTargetError(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultApplicationSelection {
    pub target: DefaultApplicationTarget,
    pub desktop_entry: DesktopEntryId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractiveQueueMode {
    SingleActiveFifo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElevationPolicy {
    pub enabled: bool,
    pub timeout: Option<Duration>,
    pub queue_mode: InteractiveQueueMode,
}

#[cfg(test)]
mod bar_edge_tests {
    use super::*;

    #[test]
    fn selector_exposes_four_semantic_choices_and_keyboard_order() {
        assert_eq!(
            BarEdgeSettingsControl::CHOICES.map(BarEdgeSettingsControl::accessible_name),
            ["Top", "Right", "Bottom", "Left"]
        );
        let projection = BarEdgeSettingProjection {
            effective_edge: Some(BarEdgePreference::Top),
            provenance: Some(BarEdgeSettingProvenance::Default),
            profile_revision: Some(8),
            identity_revision: Some(12),
        };
        let mut control = BarEdgeSettingsControl::new(projection);
        assert_eq!(control.focused_edge(), BarEdgePreference::Top);
        control.move_focus(1);
        assert_eq!(control.focused_edge(), BarEdgePreference::Right);
        assert_eq!(
            control.select_focused().unwrap().edge,
            BarEdgePreference::Right
        );
        control.move_focus(-1);
        assert_eq!(control.focused_edge(), BarEdgePreference::Top);
        control.move_focus(-1);
        assert_eq!(control.focused_edge(), BarEdgePreference::Left);
    }

    #[test]
    fn selection_creates_pending_intent_without_claiming_persistence() {
        let mut control = BarEdgeSettingsControl::new(BarEdgeSettingProjection {
            effective_edge: Some(BarEdgePreference::Top),
            provenance: Some(BarEdgeSettingProvenance::Default),
            profile_revision: Some(3),
            identity_revision: Some(4),
        });
        let intent = control.select(BarEdgePreference::Bottom).unwrap();
        assert_eq!(intent.edge, BarEdgePreference::Bottom);
        assert_eq!(control.pending(), Some(intent));
        assert_eq!(
            control.mutation_availability(),
            BarEdgeMutationAvailability::Unavailable
        );
        assert_eq!(
            control.projection().effective_edge,
            Some(BarEdgePreference::Top)
        );
    }

    #[test]
    fn unavailable_effective_projection_is_not_filled_with_top() {
        let control = BarEdgeSettingsControl::new(BarEdgeSettingProjection {
            effective_edge: None,
            provenance: None,
            profile_revision: None,
            identity_revision: Some(5),
        });
        assert_eq!(control.projection().effective_edge, None);
        assert_eq!(control.projection().provenance, None);
        assert_eq!(control.focused_edge(), BarEdgePreference::Top);
    }

    #[test]
    fn selecting_the_effective_value_clears_a_stale_pending_intent() {
        let mut control = BarEdgeSettingsControl::new(BarEdgeSettingProjection {
            effective_edge: Some(BarEdgePreference::Top),
            provenance: Some(BarEdgeSettingProvenance::Default),
            profile_revision: Some(3),
            identity_revision: Some(4),
        });
        control.select(BarEdgePreference::Right);
        assert!(control.pending().is_some());
        assert_eq!(control.select(BarEdgePreference::Top), None);
        assert_eq!(control.pending(), None);
    }
}

impl Default for ElevationPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: Some(Duration::from_secs(120)),
            queue_mode: InteractiveQueueMode::SingleActiveFifo,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SettingsRequestId(pub [u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsRequestState {
    Queued,
    Applying,
    AwaitingUserAuthorization,
    Applied,
    Denied,
    Expired,
    Failed,
    Cancelled,
}

impl SettingsRequestState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Applied | Self::Denied | Self::Expired | Self::Failed | Self::Cancelled
        )
    }

    pub const fn requires_user_attention(self) -> bool {
        matches!(self, Self::AwaitingUserAuthorization)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsFailureCode {
    InvalidValue,
    UnknownSetting,
    StaleRevision,
    AuthorizationDenied,
    AuthorizationExpired,
    AuthorizationUnavailable,
    ApplyFailed,
    RollbackFailed,
    BackendUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_ids_are_searchable_namespaced_keys() {
        assert_eq!(
            SettingId::parse("apps.defaults.web").unwrap().as_str(),
            "apps.defaults.web"
        );
        assert!(SettingId::parse("Apps.Default").is_err());
        assert!(SettingId::parse("apps..web").is_err());
        assert!(SettingId::parse(".apps").is_err());
    }

    #[test]
    fn descriptors_search_id_title_description_and_terms() {
        let descriptor = SettingDescriptor {
            id: SettingId::parse("apps.defaults.web").unwrap(),
            category: SettingCategory::Applications,
            scope: SettingScope::User,
            apply: ApplyStrategy::Immediate,
            privilege: PrivilegeRequirement::Never,
            title: "Default web browser".into(),
            description: "Application used for HTTP and HTTPS links".into(),
            search_terms: vec!["browser".into(), "http".into()],
        };
        assert!(descriptor.matches_query("web"));
        assert!(descriptor.matches_query("HTTP"));
        assert!(!descriptor.matches_query("bluetooth"));
    }

    #[test]
    fn desktop_entry_ids_are_not_paths_or_opaque_commands() {
        assert_eq!(
            DesktopEntryId::parse("google-chrome-beta.desktop")
                .unwrap()
                .as_str(),
            "google-chrome-beta.desktop"
        );
        assert!(DesktopEntryId::parse("/usr/share/applications/foo.desktop").is_err());
        assert!(DesktopEntryId::parse("../foo.desktop").is_err());
        assert!(DesktopEntryId::parse(r"..\foo.desktop").is_err());
        assert!(DesktopEntryId::parse("foo").is_err());
        assert!(DesktopEntryId::parse(".desktop").is_err());
        assert_eq!(
            DesktopEntryId::parse("vendor..editor.desktop")
                .unwrap()
                .as_str(),
            "vendor..editor.desktop"
        );
    }

    #[test]
    fn default_application_target_distinguishes_mime_and_scheme() {
        assert_eq!(
            DefaultApplicationTarget::mime("text/html").unwrap(),
            DefaultApplicationTarget::MimeType("text/html".into())
        );
        assert_eq!(
            DefaultApplicationTarget::uri_scheme("HTTPS").unwrap(),
            DefaultApplicationTarget::UriScheme("https".into())
        );
        assert!(DefaultApplicationTarget::mime("html").is_err());
        assert!(DefaultApplicationTarget::uri_scheme("9http").is_err());
    }

    #[test]
    fn elevation_defaults_to_one_interactive_request_with_bounded_wait() {
        let policy = ElevationPolicy::default();
        assert!(policy.enabled);
        assert_eq!(policy.timeout, Some(Duration::from_secs(120)));
        assert_eq!(policy.queue_mode, InteractiveQueueMode::SingleActiveFifo);
    }

    #[test]
    fn awaiting_authorization_is_nonterminal_user_attention() {
        assert!(SettingsRequestState::AwaitingUserAuthorization.requires_user_attention());
        assert!(!SettingsRequestState::AwaitingUserAuthorization.is_terminal());
        assert!(SettingsRequestState::Expired.is_terminal());
    }
}
