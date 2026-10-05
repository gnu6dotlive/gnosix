//! Fixed R12 client: one send, then a fresh read. No automatic retry.
pub use in_gnu_os_desktop_profiles::{
    BAR_PROFILE_BUS_NAME as BUS_NAME, BAR_PROFILE_OBJECT_PATH as OBJECT_PATH,
};
pub use in_gnu_os_desktop_profiles::{
    BarEdgePreference, BarPreferenceOverrides, BarProfileSnapshot, BarWriteCode, BarWriteReceipt,
    DesktopProfileId, ProfileVersion, SetBarEdge, SetBarPreferenceOverrides, WorkspaceDisplayCount,
    WorkspaceDisplayMode,
};
use zbus::blocking::{Connection, Proxy};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarApplyState {
    Applied,
    Conflict,
    Unavailable,
    Failed,
    CommittedUnconfirmed,
    OutcomeUnknown,
}
#[derive(Clone, Debug)]
pub struct BarApplyResult {
    pub state: BarApplyState,
    pub receipt: Option<BarWriteReceipt>,
    pub projection: Option<BarProfileSnapshot>,
}
#[derive(Debug)]
enum SendFailure {
    Unavailable,
    Rejected,
    Unknown,
}
trait BarTransport {
    fn read(&self) -> Result<BarProfileSnapshot, String>;
    fn send(&self, request: &SetBarEdge) -> Result<BarWriteReceipt, SendFailure>;
    fn send_preferences(
        &self,
        _request: &SetBarPreferenceOverrides,
    ) -> Result<BarWriteReceipt, SendFailure> {
        Err(SendFailure::Rejected)
    }
}
pub struct ProfileClient {
    connection: Connection,
}
impl ProfileClient {
    pub fn session() -> Result<Self, String> {
        Connection::session()
            .map(|connection| Self { connection })
            .map_err(|e| e.to_string())
    }
    pub fn read(&self) -> Result<BarProfileSnapshot, String> {
        BarTransport::read(self)
    }
    pub fn apply_once(&self, request: &SetBarEdge) -> BarApplyResult {
        apply_once(self, request)
    }
    pub fn apply_preferences_once(&self, request: &SetBarPreferenceOverrides) -> BarApplyResult {
        apply_preferences_once(self, request)
    }
    fn proxy(&self) -> Result<Proxy<'_>, String> {
        let bus =
            zbus::blocking::fdo::DBusProxy::new(&self.connection).map_err(|e| e.to_string())?;
        let owner = bus
            .get_name_owner(zbus::names::BusName::try_from(BUS_NAME).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        // SAFETY: geteuid has no preconditions and mutates no memory.
        let auth = in_gnu_os_caller_auth::DbusCallerAuthenticator::from_connection(
            self.connection.clone(),
            unsafe { libc::geteuid() },
        );
        auth.authenticate_profile_authority(owner.clone())
            .map_err(|e| format!("untrusted profile authority: {e:?}"))?;
        Proxy::new(&self.connection, owner, OBJECT_PATH, BUS_NAME).map_err(|e| e.to_string())
    }
}
impl BarTransport for ProfileClient {
    fn read(&self) -> Result<BarProfileSnapshot, String> {
        let raw: String = self
            .proxy()?
            .call("ReadBarEdge", &())
            .map_err(|e| e.to_string())?;
        if raw.len() > 4096 {
            return Err("oversized profile projection".into());
        }
        let value: BarProfileSnapshot = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if !matches!(value.version, ProfileVersion::Current { .. }) {
            return Err("missing profile version".into());
        }
        Ok(value)
    }
    fn send(&self, request: &SetBarEdge) -> Result<BarWriteReceipt, SendFailure> {
        let ProfileVersion::Current { revision, etag } = request.expected else {
            return Err(SendFailure::Rejected);
        };
        let edge = match request.edge {
            BarEdgePreference::Top => "top",
            BarEdgePreference::Right => "right",
            BarEdgePreference::Bottom => "bottom",
            BarEdgePreference::Left => "left",
        };
        // Lookup/authentication failures here precede dispatch and are known
        // unavailability, unlike a lost response after sending the operation.
        let proxy = self.proxy().map_err(|_| SendFailure::Unavailable)?;
        let raw: String = proxy
            .call(
                "SetBarEdge",
                &(
                    request.request_id.to_vec(),
                    request.epoch.to_vec(),
                    request.profile_id.into_bytes().to_vec(),
                    revision,
                    etag.to_vec(),
                    edge,
                ),
            )
            .map_err(classify_send_error)?;
        if raw.len() > 4096 {
            return Err(SendFailure::Unknown);
        }
        let receipt: BarWriteReceipt =
            serde_json::from_str(&raw).map_err(|_| SendFailure::Unknown)?;
        if receipt.request_id != request.request_id {
            return Err(SendFailure::Unknown);
        }
        Ok(receipt)
    }
    fn send_preferences(
        &self,
        request: &SetBarPreferenceOverrides,
    ) -> Result<BarWriteReceipt, SendFailure> {
        let ProfileVersion::Current { revision, etag } = request.expected else {
            return Err(SendFailure::Rejected);
        };
        let preferences =
            serde_json::to_string(&request.preferences).map_err(|_| SendFailure::Rejected)?;
        if preferences.len() > 1024 {
            return Err(SendFailure::Rejected);
        }
        let proxy = self.proxy().map_err(|_| SendFailure::Unavailable)?;
        let raw: String = proxy
            .call(
                "SetBarPreferences",
                &(
                    request.request_id.to_vec(),
                    request.epoch.to_vec(),
                    request.profile_id.into_bytes().to_vec(),
                    revision,
                    etag.to_vec(),
                    preferences,
                ),
            )
            .map_err(classify_send_error)?;
        if raw.len() > 4096 {
            return Err(SendFailure::Unknown);
        }
        let receipt: BarWriteReceipt =
            serde_json::from_str(&raw).map_err(|_| SendFailure::Unknown)?;
        if receipt.request_id != request.request_id {
            return Err(SendFailure::Unknown);
        }
        Ok(receipt)
    }
}
fn classify_send_error(error: zbus::Error) -> SendFailure {
    match error {
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.AccessDenied" =>
        {
            SendFailure::Unavailable
        }
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.InvalidArgs" =>
        {
            SendFailure::Rejected
        }
        _ => SendFailure::Unknown,
    }
}

fn apply_once(transport: &impl BarTransport, request: &SetBarEdge) -> BarApplyResult {
    let receipt = transport.send(request);
    // Also reconcile unknown outcomes, but never infer attribution from matching value alone.
    let projection = transport
        .read()
        .ok()
        .filter(|p| p.profile_id == request.profile_id);
    let receipt = match receipt {
        Ok(receipt) => receipt,
        Err(error) => {
            return BarApplyResult {
                state: match error {
                    SendFailure::Unavailable => BarApplyState::Unavailable,
                    SendFailure::Rejected => BarApplyState::Failed,
                    SendFailure::Unknown => BarApplyState::OutcomeUnknown,
                },
                receipt: None,
                projection,
            };
        }
    };
    let state = match receipt.code {
        BarWriteCode::Applied | BarWriteCode::AlreadySatisfied => {
            if projection.as_ref().is_some_and(|p| {
                p.epoch == request.epoch
                    && Some(&p.version) == receipt.version.as_ref()
                    && p.edge == Some(request.edge)
            }) {
                BarApplyState::Applied
            } else {
                BarApplyState::CommittedUnconfirmed
            }
        }
        BarWriteCode::Conflict => BarApplyState::Conflict,
        BarWriteCode::AuthorityUnavailable => BarApplyState::Unavailable,
        BarWriteCode::Invalid | BarWriteCode::Failed => BarApplyState::Failed,
    };
    BarApplyResult {
        state,
        receipt: Some(receipt),
        projection,
    }
}

fn apply_preferences_once(
    transport: &impl BarTransport,
    request: &SetBarPreferenceOverrides,
) -> BarApplyResult {
    let receipt = transport.send_preferences(request);
    let projection = transport
        .read()
        .ok()
        .filter(|projection| projection.profile_id == request.profile_id);
    let receipt = match receipt {
        Ok(receipt) => receipt,
        Err(error) => {
            return BarApplyResult {
                state: match error {
                    SendFailure::Unavailable => BarApplyState::Unavailable,
                    SendFailure::Rejected => BarApplyState::Failed,
                    SendFailure::Unknown => BarApplyState::OutcomeUnknown,
                },
                receipt: None,
                projection,
            };
        }
    };
    let state = match receipt.code {
        BarWriteCode::Applied | BarWriteCode::AlreadySatisfied => {
            if projection.as_ref().is_some_and(|projection| {
                projection.epoch == request.epoch
                    && Some(&projection.version) == receipt.version.as_ref()
                    && projection.preferences == request.preferences
            }) {
                BarApplyState::Applied
            } else {
                BarApplyState::CommittedUnconfirmed
            }
        }
        BarWriteCode::Conflict => BarApplyState::Conflict,
        BarWriteCode::AuthorityUnavailable => BarApplyState::Unavailable,
        BarWriteCode::Invalid | BarWriteCode::Failed => BarApplyState::Failed,
    };
    BarApplyResult {
        state,
        receipt: Some(receipt),
        projection,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Transport {
        sends: Cell<usize>,
        read_fails: bool,
        lost_receipt: bool,
    }
    fn snapshot() -> BarProfileSnapshot {
        BarProfileSnapshot {
            epoch: [1; 16],
            profile_id: DesktopProfileId::from_bytes([2; 16]),
            version: ProfileVersion::Current {
                revision: 2,
                etag: [3; 32],
            },
            identity_revision: 1,
            edge: Some(BarEdgePreference::Right),
            preferences: BarPreferenceOverrides::default(),
        }
    }
    impl BarTransport for Transport {
        fn read(&self) -> Result<BarProfileSnapshot, String> {
            if self.read_fails {
                Err("read unavailable".into())
            } else {
                Ok(snapshot())
            }
        }
        fn send(&self, request: &SetBarEdge) -> Result<BarWriteReceipt, SendFailure> {
            self.sends.set(self.sends.get() + 1);
            if self.lost_receipt {
                Err(SendFailure::Unknown)
            } else {
                Ok(BarWriteReceipt {
                    request_id: request.request_id,
                    code: BarWriteCode::Applied,
                    version: Some(snapshot().version),
                })
            }
        }
    }
    fn request() -> SetBarEdge {
        SetBarEdge {
            request_id: [7; 16],
            epoch: [1; 16],
            profile_id: DesktopProfileId::from_bytes([2; 16]),
            expected: ProfileVersion::Current {
                revision: 1,
                etag: [4; 32],
            },
            edge: BarEdgePreference::Right,
        }
    }
    #[test]
    fn receipt_without_fresh_projection_is_not_applied() {
        let t = Transport {
            sends: Cell::new(0),
            read_fails: true,
            lost_receipt: false,
        };
        assert_eq!(
            apply_once(&t, &request()).state,
            BarApplyState::CommittedUnconfirmed
        );
        assert_eq!(t.sends.get(), 1);
    }
    #[test]
    fn unknown_outcome_is_never_retried() {
        let t = Transport {
            sends: Cell::new(0),
            read_fails: false,
            lost_receipt: true,
        };
        assert_eq!(
            apply_once(&t, &request()).state,
            BarApplyState::OutcomeUnknown
        );
        assert_eq!(t.sends.get(), 1);
    }
    #[test]
    fn matching_fresh_projection_confirms_applied() {
        let t = Transport {
            sends: Cell::new(0),
            read_fails: false,
            lost_receipt: false,
        };
        assert_eq!(apply_once(&t, &request()).state, BarApplyState::Applied);
    }
    struct TerminalTransport {
        code: BarWriteCode,
        fresh: Option<BarProfileSnapshot>,
    }
    impl BarTransport for TerminalTransport {
        fn read(&self) -> Result<BarProfileSnapshot, String> {
            self.fresh.clone().ok_or("unavailable".into())
        }
        fn send(&self, request: &SetBarEdge) -> Result<BarWriteReceipt, SendFailure> {
            Ok(BarWriteReceipt {
                request_id: request.request_id,
                code: self.code,
                version: Some(snapshot().version),
            })
        }
    }
    #[test]
    fn stale_conflict_reconciles_fresh_truth_without_reporting_applied() {
        let result = apply_once(
            &TerminalTransport {
                code: BarWriteCode::Conflict,
                fresh: Some(snapshot()),
            },
            &request(),
        );
        assert_eq!(result.state, BarApplyState::Conflict);
        assert_eq!(result.projection, Some(snapshot()));
    }
    #[test]
    fn wrong_epoch_or_changed_revision_never_confirms_a_receipt() {
        for changed in 0..3 {
            let mut fresh = snapshot();
            match changed {
                0 => fresh.epoch = [9; 16],
                1 => {
                    fresh.version = ProfileVersion::Current {
                        revision: 3,
                        etag: [9; 32],
                    }
                }
                _ => fresh.profile_id = DesktopProfileId::from_bytes([8; 16]),
            }
            assert_eq!(
                apply_once(
                    &TerminalTransport {
                        code: BarWriteCode::Applied,
                        fresh: Some(fresh)
                    },
                    &request()
                )
                .state,
                BarApplyState::CommittedUnconfirmed
            );
        }
    }
    #[test]
    fn authority_refusal_and_failed_commit_are_not_success_even_when_value_matches() {
        for (code, state) in [
            (
                BarWriteCode::AuthorityUnavailable,
                BarApplyState::Unavailable,
            ),
            (BarWriteCode::Failed, BarApplyState::Failed),
            (BarWriteCode::Invalid, BarApplyState::Failed),
        ] {
            assert_eq!(
                apply_once(
                    &TerminalTransport {
                        code,
                        fresh: Some(snapshot())
                    },
                    &request()
                )
                .state,
                state
            );
        }
    }
    #[test]
    fn projection_wire_rejects_unknown_fields_and_invalid_edges() {
        let value = serde_json::to_value(snapshot()).unwrap();
        let mut invalid = value.clone();
        invalid["edge"] = serde_json::json!("diagonal");
        assert!(serde_json::from_value::<BarProfileSnapshot>(invalid).is_err());
        let mut invalid = value;
        invalid["arbitrary_setting"] = serde_json::json!(true);
        assert!(serde_json::from_value::<BarProfileSnapshot>(invalid).is_err());
    }
    #[test]
    fn known_authentication_refusal_is_not_an_ambiguous_commit() {
        let error = zbus::Error::MethodError(
            zbus::names::OwnedErrorName::try_from("org.freedesktop.DBus.Error.AccessDenied")
                .unwrap(),
            None,
            zbus::Message::method_call("/test", "Test")
                .unwrap()
                .build(&())
                .unwrap(),
        );
        assert!(matches!(
            classify_send_error(error),
            SendFailure::Unavailable
        ));
    }
}

#[cfg(test)]
mod donor11_tests {
    use super::*;

    struct PreferenceTransport {
        fresh: BarProfileSnapshot,
        code: BarWriteCode,
        lose_receipt: bool,
    }
    impl BarTransport for PreferenceTransport {
        fn read(&self) -> Result<BarProfileSnapshot, String> {
            Ok(self.fresh.clone())
        }
        fn send(&self, _request: &SetBarEdge) -> Result<BarWriteReceipt, SendFailure> {
            unreachable!("edge path is not part of this test")
        }
        fn send_preferences(
            &self,
            request: &SetBarPreferenceOverrides,
        ) -> Result<BarWriteReceipt, SendFailure> {
            if self.lose_receipt {
                Err(SendFailure::Unknown)
            } else {
                Ok(BarWriteReceipt {
                    request_id: request.request_id,
                    code: self.code,
                    version: Some(self.fresh.version.clone()),
                })
            }
        }
    }

    fn request() -> SetBarPreferenceOverrides {
        SetBarPreferenceOverrides {
            request_id: [22; 16],
            epoch: [1; 16],
            profile_id: DesktopProfileId::from_bytes([2; 16]),
            expected: ProfileVersion::Current {
                revision: 4,
                etag: [4; 32],
            },
            preferences: BarPreferenceOverrides {
                persistent: Some(false),
                show_on_hover: Some(true),
                workspace_count: Some(WorkspaceDisplayCount::try_from(7_i64).unwrap()),
                workspace_display: Some(WorkspaceDisplayMode::Text),
                tray_compact: Some(true),
            },
        }
    }

    fn fresh(preferences: BarPreferenceOverrides) -> BarProfileSnapshot {
        BarProfileSnapshot {
            epoch: [1; 16],
            profile_id: DesktopProfileId::from_bytes([2; 16]),
            version: ProfileVersion::Current {
                revision: 5,
                etag: [5; 32],
            },
            identity_revision: 4,
            edge: None,
            preferences,
        }
    }

    #[test]
    fn preference_batch_requires_matching_fresh_projection_after_receipt() {
        let request = request();
        let transport = PreferenceTransport {
            fresh: fresh(request.preferences),
            code: BarWriteCode::Applied,
            lose_receipt: false,
        };
        assert_eq!(
            apply_preferences_once(&transport, &request).state,
            BarApplyState::Applied
        );

        let transport = PreferenceTransport {
            fresh: fresh(BarPreferenceOverrides::default()),
            code: BarWriteCode::Applied,
            lose_receipt: false,
        };
        assert_eq!(
            apply_preferences_once(&transport, &request).state,
            BarApplyState::CommittedUnconfirmed
        );
    }

    #[test]
    fn lost_preference_receipt_is_never_inferred_from_matching_value() {
        let request = request();
        let transport = PreferenceTransport {
            fresh: fresh(request.preferences),
            code: BarWriteCode::Applied,
            lose_receipt: true,
        };
        assert_eq!(
            apply_preferences_once(&transport, &request).state,
            BarApplyState::OutcomeUnknown
        );
    }
}
