//! Keep awake: a logind idle inhibitor (U01).
//!
//! `Inhibit("idle", who, why, "block")` returns a descriptor; the inhibitor
//! lasts exactly as long as the descriptor is open, so dropping the
//! [`IdleInhibitor`] releases it (also when the shell exits or crashes).
//! Holding it is confirmed by observation: logind lists it among its
//! inhibitors, for this process.

pub const LOGIND_BUS_NAME: &str = "org.freedesktop.login1";
pub const LOGIND_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";

/// The held inhibitor. Dropping it releases the lock.
#[derive(Debug)]
pub struct IdleInhibitor {
    _fd: zbus::zvariant::OwnedFd,
}

#[derive(Debug)]
pub enum IdleInhibitOutcome {
    Held(IdleInhibitor),
    Denied(String),
    TransportFailure(String),
}

/// One logind inhibitor, as `ListInhibitors` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedInhibitor {
    pub what: String,
    pub who: String,
    pub why: String,
    pub mode: String,
    pub uid: u32,
    pub pid: u32,
}

/// Whether `inhibitors` hold an idle block by `who` from process `pid`.
pub fn holds_idle(inhibitors: &[ObservedInhibitor], who: &str, pid: u32) -> bool {
    inhibitors.iter().any(|i| {
        i.who == who && i.pid == pid && i.mode == "block" && i.what.split(':').any(|w| w == "idle")
    })
}

fn classify(name: &str, message: &str) -> IdleInhibitOutcome {
    if name.ends_with("AccessDenied") || name.ends_with("InteractiveAuthorizationRequired") {
        IdleInhibitOutcome::Denied(format!("{name}: {message}"))
    } else {
        IdleInhibitOutcome::TransportFailure(format!("{name}: {message}"))
    }
}

/// Ask logind (on `connection`, the system bus) for an idle inhibitor.
pub async fn hold_idle_inhibitor(
    connection: &zbus::Connection,
    who: &str,
    why: &str,
) -> IdleInhibitOutcome {
    let reply = connection
        .call_method(
            Some(LOGIND_BUS_NAME),
            LOGIND_PATH,
            Some(MANAGER_IFACE),
            "Inhibit",
            &("idle", who, why, "block"),
        )
        .await;
    match reply {
        Ok(message) => match message.body().deserialize::<zbus::zvariant::OwnedFd>() {
            Ok(fd) => IdleInhibitOutcome::Held(IdleInhibitor { _fd: fd }),
            Err(error) => IdleInhibitOutcome::TransportFailure(error.to_string()),
        },
        Err(zbus::Error::MethodError(name, message, _)) => {
            classify(name.as_str(), message.as_deref().unwrap_or(""))
        }
        Err(error) => IdleInhibitOutcome::TransportFailure(error.to_string()),
    }
}

/// logind's inhibitors, for observation.
pub async fn list_inhibitors(
    connection: &zbus::Connection,
) -> Result<Vec<ObservedInhibitor>, zbus::Error> {
    let reply = connection
        .call_method(
            Some(LOGIND_BUS_NAME),
            LOGIND_PATH,
            Some(MANAGER_IFACE),
            "ListInhibitors",
            &(),
        )
        .await?;
    let rows: Vec<(String, String, String, String, u32, u32)> = reply.body().deserialize()?;
    Ok(rows
        .into_iter()
        .map(|(what, who, why, mode, uid, pid)| ObservedInhibitor {
            what,
            who,
            why,
            mode,
            uid,
            pid,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(what: &str, who: &str, mode: &str, pid: u32) -> ObservedInhibitor {
        ObservedInhibitor {
            what: what.into(),
            who: who.into(),
            why: "Keep awake".into(),
            mode: mode.into(),
            uid: 1000,
            pid,
        }
    }

    #[test]
    fn only_our_own_idle_block_counts_as_held() {
        let ours = row("idle", "Gnosix", "block", 42);
        assert!(holds_idle(std::slice::from_ref(&ours), "Gnosix", 42));
        assert!(holds_idle(
            &[row("sleep:idle", "Gnosix", "block", 42)],
            "Gnosix",
            42
        ));
        assert!(!holds_idle(
            &[row("idle", "Gnosix", "block", 43)],
            "Gnosix",
            42
        ));
        assert!(!holds_idle(
            &[row("idle", "Gnosix", "delay", 42)],
            "Gnosix",
            42
        ));
        assert!(!holds_idle(
            &[row("sleep", "Gnosix", "block", 42)],
            "Gnosix",
            42
        ));
        assert!(!holds_idle(
            &[row("idle", "firefox", "block", 42)],
            "Gnosix",
            42
        ));
    }

    #[test]
    fn authorization_errors_are_denials() {
        assert!(matches!(
            classify("org.freedesktop.DBus.Error.AccessDenied", "no"),
            IdleInhibitOutcome::Denied(_)
        ));
        assert!(matches!(
            classify("org.freedesktop.DBus.Error.ServiceUnknown", "no"),
            IdleInhibitOutcome::TransportFailure(_)
        ));
    }
}
