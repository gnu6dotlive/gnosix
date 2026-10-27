//! In-process Settings/read-projection coordinator. No files, writer or polling.
use crate::product_preferences::{EffectiveProductPreferences, PreferenceValueSource};
use gnos_ux_profile_client::{
    BarApplyResult, BarApplyState, BarEdgePreference, BarPreferenceOverrides, BarProfileSnapshot,
    ProfileClient, ProfileVersion, SetBarEdge, SetBarPreferenceOverrides,
};
use gpui::{App, AppContext, Context, Entity, Global};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileStatus {
    Loading,
    Current,
    Pending,
    Applied,
    Conflict,
    Unavailable,
    Failed,
    CommittedUnconfirmed,
    OutcomeUnknown,
}

#[derive(Clone)]
struct ProfileGlobal(Entity<ProfileSession>);
impl Global for ProfileGlobal {}

pub struct ProfileSession {
    pub snapshot: Option<BarProfileSnapshot>,
    pub status: ProfileStatus,
    pub last_request: Option<SetBarEdge>,
    pub last_preferences_request: Option<SetBarPreferenceOverrides>,
}
fn preference_request_from_snapshot(
    request_id: [u8; 16],
    snapshot: &BarProfileSnapshot,
    preferences: BarPreferenceOverrides,
) -> SetBarPreferenceOverrides {
    SetBarPreferenceOverrides {
        request_id,
        epoch: snapshot.epoch,
        profile_id: snapshot.profile_id,
        expected: snapshot.version.clone(),
        preferences,
    }
}

impl ProfileSession {
    pub fn shared(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<ProfileGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self {
            snapshot: None,
            status: ProfileStatus::Unavailable,
            last_request: None,
            last_preferences_request: None,
        });
        cx.set_global(ProfileGlobal(entity.clone()));
        entity.update(cx, |this, cx| this.refresh(cx));
        entity
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if matches!(self.status, ProfileStatus::Pending | ProfileStatus::Loading) {
            return;
        }
        self.status = ProfileStatus::Loading;
        cx.notify();
        let work = cx
            .background_executor()
            .spawn(async { ProfileClient::session().and_then(|client| client.read()) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.snapshot = result.ok();
                this.status = if this.snapshot.is_some() {
                    ProfileStatus::Current
                } else {
                    ProfileStatus::Unavailable
                };
                cx.notify();
            });
        })
        .detach();
    }
    pub fn select(&mut self, edge: BarEdgePreference, cx: &mut Context<Self>) {
        if matches!(self.status, ProfileStatus::Pending | ProfileStatus::Loading) {
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            self.status = ProfileStatus::Unavailable;
            cx.notify();
            return;
        };
        let request = SetBarEdge {
            request_id: *uuid::Uuid::new_v4().as_bytes(),
            epoch: snapshot.epoch,
            profile_id: snapshot.profile_id,
            expected: snapshot.version.clone(),
            edge,
        };
        self.last_request = Some(request.clone());
        self.status = ProfileStatus::Pending;
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            ProfileClient::session().map(|client| client.apply_once(&request))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => this.complete(result),
                    Err(_) => {
                        this.snapshot = None;
                        this.status = ProfileStatus::Unavailable;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub fn select_preferences(
        &mut self,
        preferences: BarPreferenceOverrides,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.status, ProfileStatus::Pending | ProfileStatus::Loading) {
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            self.status = ProfileStatus::Unavailable;
            cx.notify();
            return;
        };
        let request = preference_request_from_snapshot(
            *uuid::Uuid::new_v4().as_bytes(),
            snapshot,
            preferences,
        );
        self.last_preferences_request = Some(request.clone());
        self.status = ProfileStatus::Pending;
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            ProfileClient::session().map(|client| client.apply_preferences_once(&request))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => this.complete(result),
                    Err(_) => {
                        this.snapshot = None;
                        this.status = ProfileStatus::Unavailable;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn complete(&mut self, result: BarApplyResult) {
        self.snapshot = result.projection;
        self.status = match result.state {
            BarApplyState::Applied => ProfileStatus::Applied,
            BarApplyState::Conflict => ProfileStatus::Conflict,
            BarApplyState::Unavailable => ProfileStatus::Unavailable,
            BarApplyState::Failed => ProfileStatus::Failed,
            BarApplyState::CommittedUnconfirmed => ProfileStatus::CommittedUnconfirmed,
            BarApplyState::OutcomeUnknown => ProfileStatus::OutcomeUnknown,
        };
    }
}
/// Merge only this validated preference. Other preference domains retain their
/// independently established provenance; this is not a whole-profile projection.
pub fn apply_bar_projection(
    effective: &mut EffectiveProductPreferences,
    snapshot: Option<&BarProfileSnapshot>,
) {
    if let Some(snapshot) = snapshot {
        effective.bar_edge = Some(snapshot.effective_edge());
        effective.bar_edge_source = Some(if snapshot.edge.is_some() {
            PreferenceValueSource::DurableProfile
        } else {
            PreferenceValueSource::ProductDefault
        });
        effective.bar_overrides = snapshot.preferences.clone();
        effective.identity_revision = Some(snapshot.identity_revision);
        effective.profile_revision = match snapshot.version {
            ProfileVersion::Current { revision, .. } => Some(revision),
            ProfileVersion::Absent => None,
        };
    } else {
        effective.bar_edge = None;
        effective.bar_edge_source = None;
        effective.bar_overrides = BarPreferenceOverrides::default();
        effective.identity_revision = None;
        effective.profile_revision = None;
    }
}

/// Coalesce validated changes until the current endpoint settles. This avoids
/// restarting from an endpoint while geometry is still in flight.
pub fn take_relocation_target(
    pending: &mut Option<gnos_ux_surface::envelope::SurfaceEdge>,
    current: gnos_ux_surface::envelope::SurfaceEdge,
    active: bool,
) -> Option<gnos_ux_surface::envelope::SurfaceEdge> {
    if active {
        return None;
    }
    pending.take().filter(|target| *target != current)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_fresh_read_never_promotes_settings_to_applied() {
        let mut session = ProfileSession {
            snapshot: None,
            status: ProfileStatus::Pending,
            last_request: None,
            last_preferences_request: None,
        };
        session.complete(BarApplyResult {
            state: BarApplyState::CommittedUnconfirmed,
            receipt: None,
            projection: None,
        });
        assert_eq!(session.status, ProfileStatus::CommittedUnconfirmed);
        assert!(session.snapshot.is_none());
    }
    #[test]
    fn bar_projection_does_not_claim_other_preference_authority() {
        let mut preferences = EffectiveProductPreferences::default();
        let original = preferences.clone();
        let snapshot = BarProfileSnapshot {
            epoch: [1; 16],
            profile_id: gnos_ux_profile_client::DesktopProfileId::from_bytes([2; 16]),
            version: ProfileVersion::Current {
                revision: 7,
                etag: [3; 32],
            },
            identity_revision: 9,
            edge: Some(BarEdgePreference::Bottom),
            preferences: BarPreferenceOverrides::default(),
        };
        apply_bar_projection(&mut preferences, Some(&snapshot));
        assert_eq!(preferences.bar_edge, Some(BarEdgePreference::Bottom));
        assert_eq!(preferences.profile_revision, Some(7));
        assert_eq!(preferences.projection_state, original.projection_state);
        assert_eq!(preferences.color_scheme, original.color_scheme);
        assert_eq!(preferences.reduced_motion, original.reduced_motion);
        apply_bar_projection(&mut preferences, None);
        assert_eq!(preferences.bar_edge, None);
    }
    #[test]
    fn pending_projection_coalesces_without_restarting_active_geometry() {
        use gnos_ux_surface::envelope::SurfaceEdge::*;
        let mut pending = Some(Left);
        assert_eq!(take_relocation_target(&mut pending, Right, true), None);
        assert_eq!(pending, Some(Left));
        pending = Some(Bottom);
        assert_eq!(
            take_relocation_target(&mut pending, Right, false),
            Some(Bottom)
        );
        assert_eq!(pending, None);
        pending = Some(Bottom);
        assert_eq!(take_relocation_target(&mut pending, Bottom, false), None);
    }
    #[test]
    fn conflict_preserves_fresh_truth_without_claiming_own_success() {
        let mut session = ProfileSession {
            snapshot: None,
            status: ProfileStatus::Pending,
            last_request: None,
            last_preferences_request: None,
        };
        session.complete(BarApplyResult {
            state: BarApplyState::Conflict,
            receipt: None,
            projection: None,
        });
        assert_eq!(session.status, ProfileStatus::Conflict);
    }
}

#[cfg(test)]
mod donor11_tests {
    use super::*;
    use gnos_ux_profile_client::{
        BarPreferenceOverrides, WorkspaceDisplayCount, WorkspaceDisplayMode,
    };

    #[test]
    fn donor_overrides_projection_is_exact_and_clears_when_authority_disappears() {
        let overrides = BarPreferenceOverrides {
            persistent: Some(false),
            show_on_hover: Some(true),
            workspace_count: Some(WorkspaceDisplayCount::try_from(8_i64).unwrap()),
            workspace_display: Some(WorkspaceDisplayMode::Icons),
            tray_compact: Some(false),
            ..Default::default()
        };
        let mut preferences = EffectiveProductPreferences::default();
        let snapshot = BarProfileSnapshot {
            epoch: [1; 16],
            profile_id: gnos_ux_profile_client::DesktopProfileId::from_bytes([2; 16]),
            version: ProfileVersion::Current {
                revision: 3,
                etag: [3; 32],
            },
            identity_revision: 4,
            edge: None,
            preferences: overrides.clone(),
        };
        apply_bar_projection(&mut preferences, Some(&snapshot));
        assert_eq!(preferences.bar_overrides, overrides);
        apply_bar_projection(&mut preferences, None);
        assert_eq!(preferences.bar_overrides, BarPreferenceOverrides::default());
    }

    #[test]
    fn preference_request_uses_current_snapshot_cas_and_does_not_change_edge() {
        let snapshot = BarProfileSnapshot {
            epoch: [7; 16],
            profile_id: gnos_ux_profile_client::DesktopProfileId::from_bytes([8; 16]),
            version: ProfileVersion::Current {
                revision: 9,
                etag: [10; 32],
            },
            identity_revision: 4,
            edge: Some(BarEdgePreference::Left),
            preferences: BarPreferenceOverrides::default(),
        };
        let desired = BarPreferenceOverrides {
            persistent: Some(true),
            ..Default::default()
        };
        let request = preference_request_from_snapshot([11; 16], &snapshot, desired.clone());
        assert_eq!(request.request_id, [11; 16]);
        assert_eq!(request.epoch, snapshot.epoch);
        assert_eq!(request.profile_id, snapshot.profile_id);
        assert_eq!(request.expected, snapshot.version);
        assert_eq!(request.preferences, desired);
        assert_eq!(snapshot.edge, Some(BarEdgePreference::Left));
    }
}
