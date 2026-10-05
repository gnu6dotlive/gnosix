//! systemd user-session host for the in.gnu.os activation authority.
//!
//! This process owns the Hyprland observation/execution adapter and reports
//! READY only while a stable, supported session can be observed. With the
//! explicit `r13-production-dbus` feature it also serves the authenticated
//! R13 user-session D-Bus boundary and routes admitted exact targets through
//! the existing bounded executor. Completed request receipts are retained only
//! in a bounded, process/epoch-local ledger; requests and targets are never
//! persisted or replayed into another host epoch.

use std::{
    env,
    os::unix::net::UnixDatagram,
    path::Path,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use in_gnu_os_activation::{ActivationExecutionBackend, ActivationSessionEpoch};
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    flag,
};
use uuid::Uuid;

use crate::{SUPPORTED_HYPRLAND_VERSION, activation_execution::HyprlandActivationExecutor};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Readiness {
    Starting,
    Ready { session: String },
    NotReady { reason: &'static str },
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionCheck {
    Current,
    Replaced,
}

fn session_check(expected: &str, observed: &str) -> SessionCheck {
    if expected == observed {
        SessionCheck::Current
    } else {
        SessionCheck::Replaced
    }
}

impl Readiness {
    fn status(&self) -> String {
        match self {
            Self::Starting => "STATUS=Starting: waiting for Hyprland observation".into(),
            Self::Ready { session } => format!("READY=1\nSTATUS=Ready: Hyprland session {session}"),
            Self::NotReady { reason } => format!("STATUS=Not ready: {reason}"),
            Self::Stopping => "STOPPING=1\nSTATUS=Stopping without replay".into(),
        }
    }
}

fn notify_systemd(message: &str) -> Result<(), String> {
    let Some(socket) = env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    let path = socket.to_string_lossy();
    if path.starts_with('@') {
        return Err("abstract NOTIFY_SOCKET is unsupported by this host".into());
    }
    let socket = UnixDatagram::unbound().map_err(|error| error.to_string())?;
    socket
        .send_to(message.as_bytes(), Path::new(path.as_ref()))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn report(readiness: &Readiness) -> Result<(), String> {
    notify_systemd(&readiness.status())
}

fn verify_session(executor: &mut HyprlandActivationExecutor) -> Result<String, String> {
    let observation = executor
        .observe()
        .map_err(|_| "Hyprland observation unavailable".to_string())?;
    let session = observation.runtime_session().to_owned();
    if session.is_empty() {
        return Err("empty Hyprland session identity".into());
    }
    let version = std::process::Command::new("hyprctl")
        .args(["-i", &session, "version"])
        .output()
        .map_err(|error| format!("Hyprland version query failed: {error}"))?;
    let version_text = String::from_utf8_lossy(&version.stdout);
    if !version.status.success() || !version_text.contains(SUPPORTED_HYPRLAND_VERSION) {
        return Err("unsupported or unavailable Hyprland version".into());
    }
    Ok(session)
}

/// Entry point exposed only by the explicit `r14-production-host` feature.
/// There is no request socket, command argument, or caller-controlled target.
pub fn run() -> ExitCode {
    match run_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("in.gnu.os activation host stopped: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner() -> Result<(), String> {
    let stopped = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&stopped)).map_err(|error| error.to_string())?;
    flag::register(SIGINT, Arc::clone(&stopped)).map_err(|error| error.to_string())?;

    let mut readiness = Readiness::Starting;
    report(&readiness)?;
    let initial_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| "SESSION_UNAVAILABLE: HYPRLAND_INSTANCE_SIGNATURE missing".to_string())?;
    let epoch = ActivationSessionEpoch::from_bytes(*Uuid::new_v4().as_bytes());
    let mut initial_executor =
        HyprlandActivationExecutor::for_production_host(epoch, &initial_signature)
            .map_err(|error| format!("cannot construct initial observer: {error:?}"))?;
    let observed = verify_session(&mut initial_executor)
        .map_err(|reason| format!("SESSION_UNAVAILABLE: {reason}"))?;
    if session_check(&initial_signature, &observed) != SessionCheck::Current {
        return Err("initial Hyprland observation belongs to a different session".into());
    }
    let bound_signature = initial_signature.clone();

    #[cfg(feature = "r13-production-dbus")]
    let runtime = Arc::new(std::sync::Mutex::new(
        crate::production_dbus::ProductionActivationRuntime::new(epoch, initial_executor),
    ));
    #[cfg(feature = "r13-production-dbus")]
    let _dbus_boundary = crate::production_dbus::build_production_boundary(Arc::clone(&runtime))
        .map_err(|error| format!("cannot establish authenticated R13 D-Bus boundary: {error}"))?;

    #[cfg(not(feature = "r13-production-dbus"))]
    let mut executor = initial_executor;

    readiness = Readiness::Ready {
        session: initial_signature,
    };
    report(&readiness)?;

    while !stopped.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(250));
        if stopped.load(Ordering::Relaxed) {
            break;
        }

        #[cfg(feature = "r13-production-dbus")]
        let observed_session = runtime
            .lock()
            .map_err(|_| "SESSION_UNAVAILABLE: activation runtime lock poisoned".to_string())?
            .observe_runtime_session();

        #[cfg(not(feature = "r13-production-dbus"))]
        let observed_session = executor
            .observe()
            .map(|observation| observation.runtime_session().to_owned());

        match observed_session {
            Ok(session) if session_check(&bound_signature, &session) == SessionCheck::Current => {}
            Ok(_) | Err(_) => {
                // Exit after publishing NOT_READY. systemd starts a fresh
                // process from its current imported session environment; the
                // next process gets a new epoch and an empty replay ledger.
                readiness = Readiness::NotReady {
                    reason: "Hyprland session unavailable; process epoch invalidated",
                };
                report(&readiness)?;
                return Err("SESSION_UNAVAILABLE: Hyprland observation lost".into());
            }
        }
    }
    readiness = Readiness::Stopping;
    report(&readiness)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_not_process_liveness() {
        assert!(
            Readiness::Starting
                .status()
                .contains("waiting for Hyprland observation")
        );
        assert!(
            Readiness::Ready {
                session: "sig".into()
            }
            .status()
            .contains("Ready")
        );
        assert!(
            Readiness::NotReady { reason: "lost" }
                .status()
                .contains("Not ready")
        );
        assert!(Readiness::Stopping.status().contains("STOPPING=1"));
    }

    #[test]
    fn ready_state_is_bound_to_one_session_signature() {
        assert_eq!(
            session_check("old-signature", "old-signature"),
            SessionCheck::Current
        );
        assert_eq!(
            session_check("old-signature", "new-signature"),
            SessionCheck::Replaced
        );
    }
}
