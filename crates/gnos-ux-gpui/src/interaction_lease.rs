//! Cross-process interaction lease and host-claim substrate.
//!
//! Extracted by EXTERNAL-RUNTIME-01 from two independently correct, live
//! qualified implementations: `product_tray_ownership.rs` and
//! `product_mpris_ownership.rs`. Only behaviour that survived both moved here;
//! see `docs/workstreams/external-runtime/01/invariant-matrix.md` for what deliberately
//! did not.
//!
//! There are two levels, and they are not interchangeable:
//!
//! * [`InteractionLease`] is the *between-process* half. Exactly one process
//!   may hold a given well-known bus name; the bus arbitrates, not the process.
//!   A process-global cannot decide between outputs, because each output is
//!   served by its own process -- that was the defect both verticals shipped.
//! * [`record`] / [`claim_host`] / [`output_owns_interaction`] are the
//!   *within-process* half. Exactly one host in an owning process installs the
//!   dispatch sink.
//!
//! The lease is parametrised by a bus-name string. It is not a provider, not a
//! transport, and it knows nothing about any protocol: a caller acquires a name
//! and keeps the returned lease alive.

use std::sync::Mutex;

use gnos_ux_surface::envelope::RuntimeId;
use zbus::zvariant::Str;

/// A held cross-process interaction lease.
///
/// The connection is held inside the lease on purpose. A bus name is released
/// when its connection drops, so a caller that let the connection fall out of
/// scope would win the name, release it, and let the next process win as well.
/// Keeping the lease alive for the process lifetime is what makes ownership
/// permanent rather than transient.
pub struct InteractionLease {
    /// Retained for its lifetime: dropping it releases the bus name. Read by
    /// [`InteractionLease::connection`] so the owner can serve a relay on it.
    connection: zbus::Connection,
    owns: bool,
}

impl InteractionLease {
    /// Requests `name` for this process.
    ///
    /// `DoNotQueue` only, never `ReplaceExisting`: a later request must not be
    /// able to take the name from the process that already holds it, and a
    /// process's second host must not steal from its own first. A loser is told
    /// it lost rather than queued for a name it may never get.
    pub async fn acquire(name: &str) -> Result<Self, String> {
        let connection = zbus::Connection::session()
            .await
            .map_err(|error| error.to_string())?;
        let granted = connection
            .request_name_with_flags(
                name.to_owned(),
                zbus::fdo::RequestNameFlags::DoNotQueue.into(),
            )
            .await;
        let owns = matches!(granted, Ok(zbus::fdo::RequestNameReply::PrimaryOwner));
        Ok(Self { connection, owns })
    }

    /// Whether this process is the primary owner of the requested name.
    pub fn owns(&self) -> bool {
        self.owns
    }

    /// The retained connection.
    ///
    /// Used to serve the owner's relay interface on the same connection that
    /// holds the name, which is how a non-owner reaches the owner without a
    /// second well-known name.
    pub fn connection(&self) -> &zbus::Connection {
        &self.connection
    }
}

/// Runs `body` only for the first caller in this process.
///
/// The lease must be requested once per process: a second request from the same
/// process is answered by the bus as a rival claim, and the process would then
/// report itself as both owner and not-owner. This is the guard both verticals
/// need, and it is shared because a missed second request looks exactly like a
/// lost lease.
pub fn claim_once(asked: &Mutex<bool>) -> bool {
    let mut requested = asked
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *requested {
        return false;
    }
    *requested = true;
    true
}

/// Per-interaction host-claim state.
///
/// One instance must exist per product interaction. It is deliberately **not**
/// a single process-global: tray and Now Playing are independent interactions,
/// and a shared cell lets whichever claims first consume the other's right to
/// dispatch. That is not hypothetical -- EXTERNAL-RUNTIME-01 first shared
/// `claim_host` wholesale, Now Playing claimed first in `SceneHost::new`, the
/// tray host claim then returned `AlreadyOwned`, the tray dispatch sink was
/// never installed, and the tray produced **zero** remote calls while still
/// reporting `process_owns`. Each vertical therefore owns its own instance.
pub struct HostClaim {
    owner: Mutex<Option<String>>,
}

impl HostClaim {
    /// A fresh, unclaimed interaction.
    pub const fn new() -> Self {
        Self {
            owner: Mutex::new(None),
        }
    }

    /// Records a claim by one host within this process, returning whether this
    /// host won it.
    pub fn claim(&self, output: Option<&str>) -> OwnershipClaim {
        let mut guard = self
            .owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        record(&mut guard, output)
    }
}

impl Default for HostClaim {
    fn default() -> Self {
        Self::new()
    }
}

/// The pure host-claim rule, separated so it is testable without process-global
/// state, and so `claim_host` and its tests cannot drift apart.
pub fn record(owner: &mut Option<String>, output: Option<&str>) -> OwnershipClaim {
    if owner.is_none() {
        *owner = output.map(str::to_owned);
        OwnershipClaim::Claimed
    } else {
        OwnershipClaim::AlreadyOwned
    }
}

/// Outcome of a scene host claiming an interaction within its process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipClaim {
    /// This host owns the interaction.
    Claimed,
    /// Another host in this process already owns it.
    AlreadyOwned,
}

impl OwnershipClaim {
    /// The owner value the winning host records for itself: its own output, or
    /// `None` when it lost.
    pub fn claimed_output(self, runtime_output: Option<&RuntimeId>) -> Option<String> {
        match self {
            Self::Claimed => runtime_output.map(|output| output.as_str().to_owned()),
            Self::AlreadyOwned => None,
        }
    }
}

/// Whether `output` should install a dispatch sink for this interaction.
///
/// `process_owns` is the bus-arbitrated decision; `owner` is the host recorded
/// within this process. Both must agree, because a process that does not own the
/// interaction has no host that may dispatch.
pub fn output_owns_interaction(
    output: Option<&str>,
    owner: Option<&str>,
    process_owns: bool,
) -> bool {
    if !process_owns {
        return false;
    }
    match (output, owner) {
        (Some(output), Some(owner)) => output == owner,
        // A headless host with no output identity owns the interaction only if
        // no host in this process claimed it, so a headless qualification run is
        // the single dispatch source rather than a second one.
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(_), None) => false,
    }
}

const DBUS_SERVICE: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

/// Resolves the current unique-name owner of a bus address.
///
/// Returns `None` when the address currently has no owner, which is how a
/// disappeared item is distinguished from one whose name was recycled by a
/// different process. A well-known name is not an identity; this is how the
/// owner behind one is recovered and carried with an observation.
pub async fn unique_name_owner(connection: &zbus::Connection, service: &str) -> Option<String> {
    if service.starts_with(':') {
        // A unique name is its own provenance, but it still has to be live.
        let owned: bool = connection
            .call_method(
                Some(DBUS_SERVICE),
                DBUS_PATH,
                Some(DBUS_INTERFACE),
                "NameHasOwner",
                &(Str::from(service),),
            )
            .await
            .ok()?
            .body()
            .deserialize()
            .ok()?;
        return owned.then(|| service.to_owned());
    }

    let owner: String = connection
        .call_method(
            Some(DBUS_SERVICE),
            DBUS_PATH,
            Some(DBUS_INTERFACE),
            "GetNameOwner",
            &(Str::from(service),),
        )
        .await
        .ok()?
        .body()
        .deserialize()
        .ok()?;

    Some(owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_host_to_claim_owns_the_interaction() {
        let mut owner = None;
        assert_eq!(
            record(&mut owner, Some("HDMI-A-1")),
            OwnershipClaim::Claimed
        );
        assert_eq!(owner.as_deref(), Some("HDMI-A-1"));
        assert!(output_owns_interaction(
            Some("HDMI-A-1"),
            owner.as_deref(),
            true
        ));
    }

    #[test]
    fn a_process_that_does_not_own_never_dispatches() {
        // The cross-process half. The multi-output gate proved that arbitrating
        // only within a process let three processes each dispatch, so a
        // non-owning process must be refused before any host is consulted.
        let owner = Some("HDMI-A-1".to_owned());
        assert!(
            !output_owns_interaction(Some("HDMI-A-1"), owner.as_deref(), false),
            "a process that lost the bus name must not dispatch, even for its own host"
        );
    }

    #[test]
    fn a_second_host_in_an_owning_process_never_owns_the_interaction() {
        let mut owner = None;
        record(&mut owner, Some("HDMI-A-1"));
        assert_eq!(
            record(&mut owner, Some("DP-2")),
            OwnershipClaim::AlreadyOwned
        );
        assert!(!output_owns_interaction(
            Some("DP-2"),
            owner.as_deref(),
            true
        ));
        assert!(output_owns_interaction(
            Some("HDMI-A-1"),
            owner.as_deref(),
            true
        ));
    }

    #[test]
    fn many_hosts_still_yield_exactly_one_dispatch_source() {
        let mut owner = None;
        let outputs = ["HDMI-A-1", "DP-2", "DP-3", "eDP-1"];
        let owners = outputs
            .iter()
            .filter(|output| record(&mut owner, Some(output)) == OwnershipClaim::Claimed)
            .count();
        assert_eq!(owners, 1, "exactly one host may own the interaction");
        let dispatch_sources = outputs
            .iter()
            .filter(|output| output_owns_interaction(Some(output), owner.as_deref(), true))
            .count();
        assert_eq!(dispatch_sources, 1);
    }

    #[test]
    fn three_processes_produce_exactly_one_dispatch_source() {
        let mut dispatch_sources = 0;
        for index in 0..3 {
            let process_owns = index == 0;
            let mut owner = None;
            for output in ["DP-1", "DP-2"] {
                record(&mut owner, Some(output));
            }
            dispatch_sources += ["DP-1", "DP-2"]
                .iter()
                .filter(|output| {
                    output_owns_interaction(Some(output), owner.as_deref(), process_owns)
                })
                .count();
        }
        assert_eq!(
            dispatch_sources, 1,
            "N processes rendering the same state must still dispatch once"
        );
    }

    #[test]
    fn ownership_is_stable_regardless_of_claim_order() {
        let outputs = ["DP-2", "HDMI-A-1"];
        let mut owner = None;
        for output in outputs {
            record(&mut owner, Some(output));
        }
        for _ in 0..3 {
            let count = outputs
                .iter()
                .filter(|output| output_owns_interaction(Some(output), owner.as_deref(), true))
                .count();
            assert_eq!(count, 1, "a re-render must not move ownership");
        }
    }

    #[test]
    fn a_headless_run_owns_the_interaction_only_when_nothing_else_did() {
        let mut owner = None;
        assert!(output_owns_interaction(None, owner.as_deref(), true));
        record(&mut owner, Some("HDMI-A-1"));
        assert!(!output_owns_interaction(None, owner.as_deref(), true));
    }

    #[test]
    fn a_host_present_before_any_claim_is_not_a_dispatch_source() {
        assert!(!output_owns_interaction(Some("DP-2"), None, true));
    }

    #[test]
    fn only_the_first_caller_requests_a_lease() {
        let asked = Mutex::new(false);
        assert!(claim_once(&asked));
        assert!(!claim_once(&asked), "a second request must be refused");
        assert!(!claim_once(&asked));
    }

    #[test]
    fn one_host_claim_has_exactly_one_winner() {
        let interaction = HostClaim::new();
        assert_eq!(interaction.claim(Some("HDMI-A-1")), OwnershipClaim::Claimed);
        assert_eq!(
            interaction.claim(Some("DP-2")),
            OwnershipClaim::AlreadyOwned
        );
    }

    #[test]
    fn independent_interactions_have_independent_host_claims() {
        // The regression EXTERNAL-RUNTIME-01 shipped when `claim_host` was one
        // shared static: a first interaction consumed the claim and a second
        // interaction in the same process was left with no host to dispatch
        // from. Two interactions must claim independently.
        let tray = HostClaim::new();
        let now_playing = HostClaim::new();
        assert_eq!(
            now_playing.claim(Some("HDMI-A-1")),
            OwnershipClaim::Claimed,
            "Now Playing claims its own output"
        );
        assert_eq!(
            tray.claim(Some("HDMI-A-1")),
            OwnershipClaim::Claimed,
            "tray must still claim its own output in the same process"
        );
    }
}
