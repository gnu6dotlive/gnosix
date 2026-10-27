//! Event-driven BlueZ observation loop.
//!
//! Consumers receive typed snapshots and never receive bus objects. Provider
//! owner changes invalidate runtime provenance before a fresh baseline is read.

use crate::bluez_observer::{
    BLUEZ_BUS_NAME, BluetoothObserver, BluetoothObserverError, BluetoothObserverOptions,
    published_object_paths, resolve_owner,
};
use crate::model::BluetoothSnapshot;
use futures::StreamExt;

#[derive(Clone, Debug)]
pub struct BluetoothObservation {
    pub snapshot: BluetoothSnapshot,
    pub changed: bool,
    pub baselined: bool,
}

pub fn run_observer<F>(mut on_snapshot: F) -> Result<(), BluetoothObserverError>
where
    F: FnMut(BluetoothObservation) + Send + 'static,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;

    runtime.block_on(async move {
        let connection = zbus::Connection::system()
            .await
            .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;
        observe_on(&connection, &mut on_snapshot).await
    })
}

/// Observe BlueZ over an existing connection until the connection ends.
pub async fn observe_on<F>(
    connection: &zbus::Connection,
    mut on_snapshot: F,
) -> Result<(), BluetoothObserverError>
where
    F: FnMut(BluetoothObservation),
{
    // Every subscription is established before the read it guards: a change
    // published between a read and a later subscription would otherwise be
    // lost until some unrelated event.
    let connection = connection.clone();
    let mut observer = BluetoothObserver::new(BluetoothObserverOptions::default());
    let bus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;
    let mut owner_changes = bus
        .receive_name_owner_changed_with_args(&[(0, BLUEZ_BUS_NAME)])
        .await
        .map_err(|error| BluetoothObserverError::Connect(error.to_string()))?;
    let mut object_events = subscribe_object_manager(&connection).await;
    let mut property_events = subscribe_properties(&connection).await;
    let changed = observer.refresh_on(&connection).await?;
    on_snapshot(observation(&observer, changed));

    loop {
        tokio::select! {
            Some(_) = owner_changes.next() => {
                match resolve_owner(&connection).await {
                    Ok(None) => {
                        if observer.observe_provider_gone() {
                            on_snapshot(observation(&observer, true));
                        }
                        object_events = empty_streams();
                        property_events = empty_streams();
                    }
                    Ok(Some(owner)) => {
                        if observer.state().owner() != Some(owner.as_str())
                            && observer.observe_provider_owner(&owner)
                        {
                            on_snapshot(observation(&observer, true));
                        }
                        object_events = subscribe_object_manager(&connection).await;
                        property_events = subscribe_properties(&connection).await;
                        match observer.refresh_on(&connection).await {
                            Ok(changed) => on_snapshot(observation(&observer, changed)),
                            Err(error) => {
                                eprintln!("GNOS_BLUETOOTH_OBSERVER error phase=owner {error}")
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("GNOS_BLUETOOTH_OBSERVER error phase=owner-resolve {error}")
                    }
                }
            }
            Some(_) = object_events.next(), if !object_events.is_empty() => {
                property_events = subscribe_properties(&connection).await;
                match observer.refresh_on(&connection).await {
                    Ok(changed) => on_snapshot(observation(&observer, changed)),
                    Err(error) => {
                        eprintln!("GNOS_BLUETOOTH_OBSERVER error phase=objects {error}")
                    }
                }
            }
            Some(_) = property_events.next(), if !property_events.is_empty() => {
                match observer.refresh_on(&connection).await {
                    Ok(changed) => on_snapshot(observation(&observer, changed)),
                    Err(error) => {
                        eprintln!("GNOS_BLUETOOTH_OBSERVER error phase=properties {error}")
                    }
                }
            }
        }
    }
}

fn observation(observer: &BluetoothObserver, changed: bool) -> BluetoothObservation {
    BluetoothObservation {
        snapshot: observer.snapshot(),
        changed,
        baselined: observer.state().baselined(),
    }
}

type Streams = futures::stream::SelectAll<zbus::proxy::SignalStream<'static>>;

fn empty_streams() -> Streams {
    futures::stream::select_all(Vec::new())
}

async fn subscribe_object_manager(connection: &zbus::Connection) -> Streams {
    let Ok(Some(_)) = resolve_owner(connection).await else {
        return empty_streams();
    };
    let Ok(proxy) = zbus::Proxy::new(
        connection,
        BLUEZ_BUS_NAME,
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    else {
        return empty_streams();
    };
    let Ok(stream) = proxy.receive_all_signals().await else {
        return empty_streams();
    };
    futures::stream::select_all(vec![stream])
}
async fn subscribe_properties(connection: &zbus::Connection) -> Streams {
    let Ok(Some(owner)) = resolve_owner(connection).await else {
        return empty_streams();
    };
    let Ok(paths) = published_object_paths(connection, &owner).await else {
        return empty_streams();
    };

    let mut streams = Vec::new();
    for path in paths {
        let Ok(proxy) = zbus::Proxy::new(
            connection,
            BLUEZ_BUS_NAME,
            path.as_str(),
            "org.freedesktop.DBus.Properties",
        )
        .await
        else {
            continue;
        };
        if let Ok(stream) = proxy.receive_all_signals().await {
            streams.push(stream);
        }
    }
    futures::stream::select_all(streams)
}
