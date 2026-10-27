//! Process-global, event-driven network observation for the product.
//!
//! One observer per process, shared by every `SceneHost`, publishing typed
//! snapshots to `ProductRuntime`. Read-only: this session owns no mutation path
//! and constructs no intent.
//!
//! The observer runs on its own thread because `gnos-network` drives the system
//! bus, and a blocked bus call must never stall a compositor frame. Snapshots
//! cross to the GPUI side over an unbounded channel, exactly as the audio
//! session does.

use futures::{StreamExt, channel::mpsc};
use gnos_network::{NetworkSnapshot, NetworkState};
use gpui::{AppContext, Context, Entity};

pub struct NetworkSession {
    snapshot: NetworkSnapshot,
    started: bool,
}

#[derive(Clone)]
struct NetworkGlobal(Entity<NetworkSession>);
impl gpui::Global for NetworkGlobal {}

/// The truth before any observation: no provider, nothing known.
///
/// Deliberately not "disconnected": before the first observation the product
/// does not know whether there is a network, only that it has not looked.
fn unobserved_snapshot() -> NetworkSnapshot {
    NetworkState::new().snapshot()
}

impl NetworkSession {
    fn new() -> Self {
        Self {
            snapshot: unobserved_snapshot(),
            started: false,
        }
    }

    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<NetworkGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self::new());
        cx.set_global(NetworkGlobal(entity.clone()));
        entity
    }

    pub fn snapshot(&self) -> &NetworkSnapshot {
        &self.snapshot
    }

    pub const fn started(&self) -> bool {
        self.started
    }

    fn set_snapshot(&mut self, snapshot: NetworkSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

/// Starts observation once per process.
pub fn ensure_observation(session: &Entity<NetworkSession>, cx: &mut gpui::App) {
    let session = session.clone();
    session.update(cx, |this, cx| {
        if this.started {
            return;
        }
        this.started = true;
        let entity = cx.entity();
        let (sender, mut receiver) = mpsc::unbounded::<NetworkSnapshot>();
        let thread_sender = sender.clone();
        let spawn = std::thread::Builder::new()
            .name("gnos-network-observer".into())
            .spawn(move || {
                let result = gnos_network::run_observer(move |observation| {
                    // The product reacts to the snapshot, not to the flags: the
                    // reducer has already suppressed anything that did not change
                    // product truth, and `ProductRuntime::set_network` compares
                    // again. `changed` and `baselined` exist for qualification
                    // receipts.
                    let _ = thread_sender.unbounded_send(observation.snapshot);
                });
                if let Err(error) = result {
                    // A provider that could not be observed at all is reported as
                    // such, and never as a disconnected network.
                    eprintln!("GNOS_UX_NETWORK_OBSERVER_FAIL error={error}");
                    let _ = sender.unbounded_send(unobserved_snapshot());
                }
            });
        if let Err(error) = spawn {
            this.started = false;
            eprintln!("GNOS_UX_NETWORK_THREAD_FAIL error={error}");
            return;
        }
        cx.spawn(async move |_this, task_cx| {
            while let Some(snapshot) = receiver.next().await {
                entity.update(&mut *task_cx, |this, cx| this.set_snapshot(snapshot, cx));
            }
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_network::NetworkProviderState;

    #[test]
    fn session_starts_unobserved_and_not_started() {
        let session = NetworkSession::new();
        assert_eq!(
            session.snapshot().provider,
            NetworkProviderState::Unavailable
        );
        assert!(
            session.snapshot().provider_owner.is_none(),
            "before the first observation nothing is known, not even an owner"
        );
        assert!(session.snapshot().globals.is_none());
        assert!(!session.started());
    }
}
