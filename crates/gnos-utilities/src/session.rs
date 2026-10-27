//! Session and power (SURF-02A, S02–S05): typed logind requests.
//!
//! - **Suspend, reboot, power off:** `Manager.Suspend`, `Reboot`,
//!   `PowerOff`. logind reports each one's capability (`CanSuspend`, …);
//!   one it reports `no` or `na` is refused before anything is sent.
//! - **Lock:** `Session.Lock` on the caller's session. Whether anything
//!   locked is observed through `LockedHint`, never assumed.
//! - **Log out:** `Session.Terminate` on the caller's session.
//!
//! An action that a `block` inhibitor holds back (`sleep` for suspend,
//! `shutdown` for reboot and power off) is refused unless forced (S04).
//! Forcing sends the request as interactive, so logind asks polkit for the
//! matching ignore-inhibit privilege: no privilege is taken here.
//! `interactive` is also set when logind reports `challenge`, so polkit can
//! ask.

use crate::{LOGIND_BUS_NAME, LOGIND_PATH, ObservedInhibitor};

const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";
/// The caller's own session (or, for a caller outside any session, the
/// user's display session).
pub const OWN_SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SessionAction {
    Lock,
    Suspend,
    Logout,
    Reboot,
    PowerOff,
}

impl SessionAction {
    pub const ALL: [SessionAction; 5] = [
        SessionAction::Lock,
        SessionAction::Suspend,
        SessionAction::Logout,
        SessionAction::Reboot,
        SessionAction::PowerOff,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Suspend => "suspend",
            Self::Logout => "logout",
            Self::Reboot => "reboot",
            Self::PowerOff => "poweroff",
        }
    }

    /// S03: the heavy actions fire only after a long press.
    pub fn held(self) -> bool {
        matches!(self, Self::Logout | Self::Reboot | Self::PowerOff)
    }

    /// The inhibitor `what` that holds this action back, if any.
    pub fn inhibited_by(self) -> Option<&'static str> {
        match self {
            Self::Suspend => Some("sleep"),
            Self::Reboot | Self::PowerOff => Some("shutdown"),
            Self::Lock | Self::Logout => None,
        }
    }

    fn manager_method(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Suspend => Some(("CanSuspend", "Suspend")),
            Self::Reboot => Some(("CanReboot", "Reboot")),
            Self::PowerOff => Some(("CanPowerOff", "PowerOff")),
            Self::Lock | Self::Logout => None,
        }
    }
}

/// What logind answers to `CanSuspend` and its kin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Capability {
    Yes,
    /// Allowed after polkit authentication.
    Challenge,
    No,
    /// Not supported here (no swap for hibernation, a container, …).
    Na,
    Other(String),
}

impl Capability {
    pub fn parse(text: &str) -> Self {
        match text {
            "yes" => Self::Yes,
            "challenge" => Self::Challenge,
            "no" => Self::No,
            "na" => Self::Na,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn allows(&self) -> bool {
        matches!(self, Self::Yes | Self::Challenge)
    }
}

/// logind's capabilities for the manager actions; `None` when unread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionCapabilities {
    pub suspend: Option<Capability>,
    pub reboot: Option<Capability>,
    pub power_off: Option<Capability>,
}

impl SessionCapabilities {
    /// `None` for lock and log out (always offered: the session itself).
    pub fn of(&self, action: SessionAction) -> Option<&Capability> {
        match action {
            SessionAction::Suspend => self.suspend.as_ref(),
            SessionAction::Reboot => self.reboot.as_ref(),
            SessionAction::PowerOff => self.power_off.as_ref(),
            SessionAction::Lock | SessionAction::Logout => None,
        }
    }

    /// Whether logind answered at all.
    pub fn observed(&self) -> bool {
        self.suspend.is_some() || self.reboot.is_some() || self.power_off.is_some()
    }
}

/// The inhibitors that block `action` now (`block` mode only; a `delay`
/// inhibitor only delays it).
pub fn blockers(
    inhibitors: &[ObservedInhibitor],
    action: SessionAction,
) -> Vec<&ObservedInhibitor> {
    let Some(what) = action.inhibited_by() else {
        return Vec::new();
    };
    inhibitors
        .iter()
        .filter(|i| i.mode == "block" && i.what.split(':').any(|w| w == what))
        .collect()
}

/// Why a session request was not sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRefusal {
    /// logind does not offer it (`no`, `na`, or an unknown answer).
    NotAvailable(Capability),
    /// logind was never heard from for it.
    Unobserved,
    /// Held back by this many `block` inhibitors; needs force.
    Blocked(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionOutcome {
    Accepted,
    Refused(SessionRefusal),
    Denied(String),
    TransportFailure(String),
}

/// Whether `action` may be sent, given what logind reports now, and if so
/// whether it goes as interactive (polkit may ask).
pub fn revalidate(
    capabilities: &SessionCapabilities,
    inhibitors: &[ObservedInhibitor],
    action: SessionAction,
    force: bool,
) -> Result<bool, SessionRefusal> {
    let mut interactive = false;
    if action.manager_method().is_some() {
        match capabilities.of(action) {
            None => return Err(SessionRefusal::Unobserved),
            Some(c) if !c.allows() => return Err(SessionRefusal::NotAvailable(c.clone())),
            Some(c) => interactive = *c == Capability::Challenge,
        }
    }
    let blocked = blockers(inhibitors, action).len();
    if blocked > 0 && !force {
        return Err(SessionRefusal::Blocked(blocked));
    }
    Ok(interactive || (force && blocked > 0))
}

/// A logind error, typed.
pub fn classify(name: &str, message: &str) -> SessionOutcome {
    if name.ends_with("AccessDenied")
        || name.ends_with("InteractiveAuthorizationRequired")
        || name.ends_with("NotAuthorized")
    {
        SessionOutcome::Denied(format!("{name}: {message}"))
    } else if name.ends_with("BlockedByInhibitorLock") {
        SessionOutcome::Refused(SessionRefusal::Blocked(0))
    } else {
        SessionOutcome::TransportFailure(format!("{name}: {message}"))
    }
}

/// Read logind's capabilities for the manager actions.
pub async fn read_capabilities(connection: &zbus::Connection) -> SessionCapabilities {
    let mut out = SessionCapabilities::default();
    for action in [
        SessionAction::Suspend,
        SessionAction::Reboot,
        SessionAction::PowerOff,
    ] {
        let Some((can, _)) = action.manager_method() else {
            continue;
        };
        let answer = connection
            .call_method(
                Some(LOGIND_BUS_NAME),
                LOGIND_PATH,
                Some(MANAGER_IFACE),
                can,
                &(),
            )
            .await
            .ok()
            .and_then(|m| m.body().deserialize::<String>().ok())
            .map(|s| Capability::parse(&s));
        match action {
            SessionAction::Suspend => out.suspend = answer,
            SessionAction::Reboot => out.reboot = answer,
            SessionAction::PowerOff => out.power_off = answer,
            SessionAction::Lock | SessionAction::Logout => {}
        }
    }
    out
}

/// Revalidate, then ask logind for `action`.
pub async fn request_session(
    connection: &zbus::Connection,
    capabilities: &SessionCapabilities,
    inhibitors: &[ObservedInhibitor],
    action: SessionAction,
    force: bool,
) -> SessionOutcome {
    let interactive = match revalidate(capabilities, inhibitors, action, force) {
        Ok(i) => i,
        Err(refusal) => return SessionOutcome::Refused(refusal),
    };
    let reply = match action.manager_method() {
        Some((_, method)) => {
            connection
                .call_method(
                    Some(LOGIND_BUS_NAME),
                    LOGIND_PATH,
                    Some(MANAGER_IFACE),
                    method,
                    &(interactive,),
                )
                .await
        }
        None => {
            let method = if action == SessionAction::Lock {
                "Lock"
            } else {
                "Terminate"
            };
            connection
                .call_method(
                    Some(LOGIND_BUS_NAME),
                    OWN_SESSION_PATH,
                    Some(SESSION_IFACE),
                    method,
                    &(),
                )
                .await
        }
    };
    match reply {
        Ok(_) => SessionOutcome::Accepted,
        Err(zbus::Error::MethodError(name, message, _)) => {
            classify(name.as_str(), message.as_deref().unwrap_or(""))
        }
        Err(error) => SessionOutcome::TransportFailure(error.to_string()),
    }
}

/// Whether logind reports the caller's session locked (`LockedHint`).
pub async fn session_locked(connection: &zbus::Connection) -> Result<bool, zbus::Error> {
    let reply = connection
        .call_method(
            Some(LOGIND_BUS_NAME),
            OWN_SESSION_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(SESSION_IFACE, "LockedHint"),
        )
        .await?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
    bool::try_from(value).map_err(zbus::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inhibitor(what: &str, who: &str, mode: &str) -> ObservedInhibitor {
        ObservedInhibitor {
            what: what.to_owned(),
            who: who.to_owned(),
            why: "busy".to_owned(),
            mode: mode.to_owned(),
            uid: 1000,
            pid: 42,
        }
    }

    fn capable() -> SessionCapabilities {
        SessionCapabilities {
            suspend: Some(Capability::Yes),
            reboot: Some(Capability::Challenge),
            power_off: Some(Capability::Yes),
        }
    }

    #[test]
    fn capabilities_parse_and_only_yes_or_challenge_allow() {
        assert_eq!(Capability::parse("yes"), Capability::Yes);
        assert_eq!(Capability::parse("challenge"), Capability::Challenge);
        assert!(!Capability::parse("no").allows());
        assert!(!Capability::parse("na").allows());
        assert!(!Capability::parse("maybe").allows());
        assert!(capable().observed());
        assert!(!SessionCapabilities::default().observed());
    }

    #[test]
    fn only_block_inhibitors_of_the_matching_kind_hold_an_action_back() {
        let list = [
            inhibitor("shutdown:sleep", "Steam", "block"),
            inhibitor("sleep", "NetworkManager", "delay"),
            inhibitor("idle", "Gnosix", "block"),
            inhibitor("handle-lid-switch", "UPower", "block"),
        ];
        let who = |a| {
            blockers(&list, a)
                .iter()
                .map(|i| i.who.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(who(SessionAction::PowerOff), ["Steam"]);
        assert_eq!(who(SessionAction::Reboot), ["Steam"]);
        assert_eq!(who(SessionAction::Suspend), ["Steam"]);
        assert!(who(SessionAction::Lock).is_empty());
        assert!(who(SessionAction::Logout).is_empty());
    }

    #[test]
    fn revalidation_refuses_what_logind_does_not_offer_and_needs_force_when_blocked() {
        let caps = capable();
        assert_eq!(
            revalidate(&caps, &[], SessionAction::PowerOff, false),
            Ok(false)
        );
        // `challenge`: interactive, so polkit can ask.
        assert_eq!(
            revalidate(&caps, &[], SessionAction::Reboot, false),
            Ok(true)
        );
        let no = SessionCapabilities {
            suspend: Some(Capability::Na),
            ..caps.clone()
        };
        assert_eq!(
            revalidate(&no, &[], SessionAction::Suspend, true),
            Err(SessionRefusal::NotAvailable(Capability::Na)),
            "force does not override a missing capability"
        );
        assert_eq!(
            revalidate(
                &SessionCapabilities::default(),
                &[],
                SessionAction::PowerOff,
                false
            ),
            Err(SessionRefusal::Unobserved)
        );
        let blocked = [inhibitor("shutdown", "Steam", "block")];
        assert_eq!(
            revalidate(&caps, &blocked, SessionAction::PowerOff, false),
            Err(SessionRefusal::Blocked(1))
        );
        // Forced: sent interactive, logind asks polkit to ignore the lock.
        assert_eq!(
            revalidate(&caps, &blocked, SessionAction::PowerOff, true),
            Ok(true)
        );
        // Lock and log out need no capability and no inhibitor holds them.
        let none = SessionCapabilities::default();
        assert_eq!(
            revalidate(&none, &blocked, SessionAction::Lock, false),
            Ok(false)
        );
        assert_eq!(
            revalidate(&none, &blocked, SessionAction::Logout, false),
            Ok(false)
        );
    }

    #[test]
    fn logind_errors_are_typed() {
        assert!(matches!(
            classify(
                "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
                ""
            ),
            SessionOutcome::Denied(_)
        ));
        assert!(matches!(
            classify("org.freedesktop.DBus.Error.AccessDenied", "no"),
            SessionOutcome::Denied(_)
        ));
        assert_eq!(
            classify("org.freedesktop.login1.BlockedByInhibitorLock", ""),
            SessionOutcome::Refused(SessionRefusal::Blocked(0))
        );
        assert!(matches!(
            classify("org.freedesktop.DBus.Error.NoReply", ""),
            SessionOutcome::TransportFailure(_)
        ));
        assert!(SessionAction::PowerOff.held() && !SessionAction::Lock.held());
    }
}
