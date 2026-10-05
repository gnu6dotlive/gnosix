//! Shared process authentication for the canonical GNOS.UX caller.
//! Authentication conveys no R12 or R13 mutation capability.
//!
//! This module authenticates one D-Bus sender as the current
//! `gnos-ux-scene.service` invocation. Same UID is necessary but never
//! sufficient. A Linux pidfd from the bus and systemd's GetUnitByPIDFD are
//! mandatory; there is no PID/path/cmdline fallback.

use zbus::{
    blocking::{Connection, Proxy, fdo::DBusProxy},
    message::Header,
    zvariant::OwnedObjectPath,
};

pub const REQUIRED_CALLER_UNIT: &str = "gnos-ux-scene.service";
pub const PROFILE_AUTHORITY_UNIT: &str = "in-gnu-os-profiles.service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerClaim {
    pub unix_user_id: Option<u32>,
    pub has_process_fd: bool,
    pub unit_id: Option<String>,
    pub invocation_id: Option<[u8; 16]>,
    pub current_unit_invocation_id: Option<[u8; 16]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerAuthRefusal {
    MissingSender,
    CredentialLookupFailed,
    WrongUnixUser,
    MissingProcessFd,
    UnitLookupFailed,
    WrongUnit,
    MissingInvocationId,
    InvocationMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCaller {
    pub unit_id: String,
    pub invocation_id: [u8; 16],
}

pub fn validate_claim(
    expected_uid: u32,
    claim: CallerClaim,
) -> Result<AuthenticatedCaller, CallerAuthRefusal> {
    validate_unit_claim(expected_uid, claim, REQUIRED_CALLER_UNIT)
}

fn validate_unit_claim(
    expected_uid: u32,
    claim: CallerClaim,
    required_unit: &str,
) -> Result<AuthenticatedCaller, CallerAuthRefusal> {
    if claim.unix_user_id != Some(expected_uid) {
        return Err(CallerAuthRefusal::WrongUnixUser);
    }
    if !claim.has_process_fd {
        return Err(CallerAuthRefusal::MissingProcessFd);
    }
    if claim.unit_id.as_deref() != Some(required_unit) {
        return Err(CallerAuthRefusal::WrongUnit);
    }
    let invocation_id = claim
        .invocation_id
        .ok_or(CallerAuthRefusal::MissingInvocationId)?;
    if claim.current_unit_invocation_id != Some(invocation_id) {
        return Err(CallerAuthRefusal::InvocationMismatch);
    }

    Ok(AuthenticatedCaller {
        unit_id: required_unit.to_owned(),
        invocation_id,
    })
}

pub struct DbusCallerAuthenticator {
    connection: Connection,
    expected_uid: u32,
}

impl DbusCallerAuthenticator {
    pub fn session() -> Result<Self, zbus::Error> {
        Ok(Self {
            connection: Connection::session()?,
            // SAFETY: geteuid has no preconditions and does not mutate memory.
            expected_uid: unsafe { libc::geteuid() },
        })
    }

    pub fn from_connection(connection: Connection, expected_uid: u32) -> Self {
        Self {
            connection,
            expected_uid,
        }
    }

    pub fn authenticate(
        &self,
        header: &Header<'_>,
    ) -> Result<AuthenticatedCaller, CallerAuthRefusal> {
        let sender = header
            .sender()
            .ok_or(CallerAuthRefusal::MissingSender)?
            .to_owned();

        self.authenticate_sender(sender.into(), REQUIRED_CALLER_UNIT)
    }

    /// Authenticate the unique owner before the client trusts a profile receipt.
    /// Calls stay pinned to this unique bus name, preventing name-owner races.
    pub fn authenticate_profile_authority(
        &self,
        sender: zbus::names::OwnedUniqueName,
    ) -> Result<AuthenticatedCaller, CallerAuthRefusal> {
        self.authenticate_sender(sender, PROFILE_AUTHORITY_UNIT)
    }

    fn authenticate_sender(
        &self,
        sender: zbus::names::OwnedUniqueName,
        required_unit: &str,
    ) -> Result<AuthenticatedCaller, CallerAuthRefusal> {
        let dbus = DBusProxy::new(&self.connection)
            .map_err(|_| CallerAuthRefusal::CredentialLookupFailed)?;
        let credentials = dbus
            .get_connection_credentials(sender.into())
            .map_err(|_| CallerAuthRefusal::CredentialLookupFailed)?;

        let process_fd = credentials
            .process_fd()
            .ok_or(CallerAuthRefusal::MissingProcessFd)?;

        let manager = Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .map_err(|_| CallerAuthRefusal::UnitLookupFailed)?;

        let (unit_path, unit_id, invocation): (OwnedObjectPath, String, Vec<u8>) = manager
            .call("GetUnitByPIDFD", process_fd)
            .map_err(|_| CallerAuthRefusal::UnitLookupFailed)?;

        let invocation_id: [u8; 16] = invocation
            .as_slice()
            .try_into()
            .map_err(|_| CallerAuthRefusal::MissingInvocationId)?;

        // GetUnitByInvocationID returns an invocation-keyed alias object path
        // on current systemd releases, not necessarily the canonical unit path.
        // Instead, re-read InvocationID from the canonical unit object returned
        // by GetUnitByPIDFD. If the unit restarted after the pidfd lookup, the
        // current property differs from the pidfd-bound invocation and we fail.
        let unit = Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            unit_path,
            "org.freedesktop.systemd1.Unit",
        )
        .map_err(|_| CallerAuthRefusal::UnitLookupFailed)?;
        let current_invocation: Vec<u8> = unit
            .get_property("InvocationID")
            .map_err(|_| CallerAuthRefusal::InvocationMismatch)?;
        let current_unit_invocation_id: [u8; 16] = current_invocation
            .as_slice()
            .try_into()
            .map_err(|_| CallerAuthRefusal::InvocationMismatch)?;

        validate_unit_claim(
            self.expected_uid,
            CallerClaim {
                unix_user_id: credentials.unix_user_id(),
                has_process_fd: true,
                unit_id: Some(unit_id),
                invocation_id: Some(invocation_id),
                current_unit_invocation_id: Some(current_unit_invocation_id),
            },
            required_unit,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_claim() -> CallerClaim {
        CallerClaim {
            unix_user_id: Some(1000),
            has_process_fd: true,
            unit_id: Some(REQUIRED_CALLER_UNIT.to_owned()),
            invocation_id: Some([7; 16]),
            current_unit_invocation_id: Some([7; 16]),
        }
    }

    #[test]
    fn exact_current_unit_is_accepted() {
        let result = validate_claim(1000, good_claim()).unwrap();
        assert_eq!(result.unit_id, REQUIRED_CALLER_UNIT);
        assert_eq!(result.invocation_id, [7; 16]);
    }

    #[test]
    fn same_uid_wrong_unit_is_refused() {
        let mut claim = good_claim();
        claim.unit_id = Some("attacker.service".into());
        assert_eq!(
            validate_claim(1000, claim),
            Err(CallerAuthRefusal::WrongUnit)
        );
    }

    #[test]
    fn missing_pidfd_has_no_pid_fallback() {
        let mut claim = good_claim();
        claim.has_process_fd = false;
        assert_eq!(
            validate_claim(1000, claim),
            Err(CallerAuthRefusal::MissingProcessFd)
        );
    }

    #[test]
    fn wrong_uid_is_refused_even_for_expected_unit() {
        assert_eq!(
            validate_claim(2000, good_claim()),
            Err(CallerAuthRefusal::WrongUnixUser)
        );
    }

    #[test]
    fn stale_or_mismatched_invocation_is_refused() {
        let mut claim = good_claim();
        claim.current_unit_invocation_id = Some([8; 16]);
        assert_eq!(
            validate_claim(1000, claim),
            Err(CallerAuthRefusal::InvocationMismatch)
        );
    }
    #[test]
    fn authority_name_is_not_authenticated_as_the_ui_or_another_process() {
        let mut claim = good_claim();
        assert_eq!(
            validate_unit_claim(1000, claim.clone(), PROFILE_AUTHORITY_UNIT),
            Err(CallerAuthRefusal::WrongUnit)
        );
        claim.unit_id = Some(PROFILE_AUTHORITY_UNIT.into());
        assert!(validate_unit_claim(1000, claim.clone(), PROFILE_AUTHORITY_UNIT).is_ok());
        assert_eq!(
            validate_claim(1000, claim),
            Err(CallerAuthRefusal::WrongUnit)
        );
    }
}
