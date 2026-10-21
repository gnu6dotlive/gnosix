//! The observation loop, owned by this crate.
//!
//! Consumers get typed snapshots through a callback and never see the bus. This
//! is what lets the product crate depend on `gnos-network` without depending on
//! `zbus`: the protocol stays behind this module.
//!
//! Event-driven, with no polling. A signal is a wake-up, not a value: the
//! observer re-reads typed facts and the reducer decides whether product truth
//! actually changed, so protocol chatter that leaves the product summary alone
//! produces no callback with `changed = true`.

use crate::model::NetworkSnapshot;
use crate::nm_observer::published_object_paths;
use crate::nm_observer::{
    NetworkObserver, NetworkObserverError, NetworkObserverOptions, resolve_owner,
};
use futures::StreamExt;

const NM: &str = "org.freedesktop.NetworkManager";

/// One observed snapshot, with the two facts a consumer needs to read it
/// correctly.
///
/// `changed` and `baselined` are separate on purpose. Collapsing them into one
/// boolean loses the difference between "the provider answered for the first
/// time" and "a network fact moved", which is exactly the distinction that keeps
/// enumeration from being reported as a network event.
#[derive(Clone, Debug)]
pub struct NetworkObservation {
    pub snapshot: NetworkSnapshot,
    /// A product-relevant change, as decided by the reducer.
    pub changed: bool,
    /// Whether a baseline exists for the current provider owner.
    pub baselined: bool,
}

/// Runs observation until the process ends, publishing each accepted snapshot.
pub fn run_observer(
    mut on_snapshot: impl FnMut(NetworkObservation) + Send + 'static,
) -> Result<(), NetworkObserverError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;

    runtime.block_on(async move {
        let mut observer = NetworkObserver::new(NetworkObserverOptions::default());

        let changed = observer.refresh().await?;
        on_snapshot(observation(&observer, changed));

        let connection = zbus::Connection::system()
            .await
            .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
        let bus = zbus::Proxy::new(&connection, NM, "/org/freedesktop/DBus", NM)
            .await
            .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
        let mut owner_changes = bus
            .receive_all_signals()
            .await
            .map_err(|error| NetworkObserverError::Connect(error.to_string()))?;
        let mut streams = subscribe_all(&connection).await;

        loop {
            tokio::select! {
                changed = owner_changes.next() => {
                    let _ = changed;
                    // Provider lifecycle decides whether the runtime identities we
                    // hold are still ours, so it is handled before anything else.
                    match resolve_owner(&connection).await {
                        Ok(None) => {
                            if observer.observe_provider_gone() {
                                on_snapshot(observation(&observer, true));
                            }
                        }
                        Ok(Some(owner)) => {
                            if observer.state().owner() != Some(owner.as_str())
                                && observer.observe_provider_owner(&owner)
                            {
                                on_snapshot(observation(&observer, true));
                            }
                            match observer.refresh().await {
                                Ok(changed) => on_snapshot(observation(&observer, changed)),
                                Err(error) => {
                                    eprintln!("GNOS_NETWORK_OBSERVER error phase=owner {error}")
                                }
                            }
                            streams = subscribe_all(&connection).await;
                        }
                        Err(error) => {
                            eprintln!("GNOS_NETWORK_OBSERVER error phase=owner {error}")
                        }
                    }
                }
                signal = streams.next() => {
                    let _ = signal;
                    match observer.refresh().await {
                        Ok(changed) => on_snapshot(observation(&observer, changed)),
                        Err(error) => {
                            eprintln!("GNOS_NETWORK_OBSERVER error phase=device {error}")
                        }
                    }
                    // Subscriptions are deliberately NOT rebuilt on every wake-up:
                    // re-adding a match rule for every published object after every
                    // signal contends with NetworkManager for the system bus, which
                    // starved an earlier qualification's own stimulus.
                }
            }
        }
    })
}

fn observation(observer: &NetworkObserver, changed: bool) -> NetworkObservation {
    NetworkObservation {
        snapshot: observer.snapshot(),
        changed,
        baselined: observer.state().baselined(),
    }
}

type Streams = futures::stream::SelectAll<zbus::proxy::SignalStream<'static>>;

/// Subscribes to every object the provider currently publishes, on the
/// interfaces that actually carry that object's signals.
async fn subscribe_all(connection: &zbus::Connection) -> Streams {
    let Ok(Some(owner)) = resolve_owner(connection).await else {
        return futures::stream::select_all(Vec::new());
    };
    let Ok(paths) = published_object_paths(connection, &owner).await else {
        return futures::stream::select_all(Vec::new());
    };

    let mut streams = Vec::new();
    for path in paths {
        let interfaces: &[&str] = if path.contains("/Devices/") {
            &[
                "org.freedesktop.NetworkManager.Device",
                "org.freedesktop.NetworkManager.Device.Wireless",
                "org.freedesktop.DBus.Properties",
            ]
        } else if path.contains("/ActiveConnection/") {
            &[
                "org.freedesktop.NetworkManager.Connection.Active",
                "org.freedesktop.DBus.Properties",
            ]
        } else {
            &[NM, "org.freedesktop.DBus.Properties"]
        };
        for interface in interfaces.iter().copied() {
            if let Ok(proxy) = zbus::Proxy::new(connection, NM, path.as_str(), interface).await
                && let Ok(stream) = proxy.receive_all_signals().await
            {
                streams.push(stream);
            }
        }
    }
    futures::stream::select_all(streams)
}
