//! Qualifies the real BlueZ runner against a private `dbus-daemon` hosting a
//! fake `org.bluez`: absence, first appearance, a device connecting through
//! the provider's own property signal, provider loss and provider return under
//! a new unique owner. The host system bus and BlueZ are never touched.

use gnos_bluetooth::{BluetoothObservation, BluetoothProviderState, observe_on};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedObjectPath;

struct PrivateBus {
    child: Child,
    address: String,
    dir: std::path::PathBuf,
}

impl PrivateBus {
    fn start() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gnos-bluetooth-bus-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>"#,
                dir.display()
            ),
        )
        .unwrap();
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon is available");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        Self {
            child,
            address: line.trim().to_owned(),
            dir,
        }
    }

    fn builder(&self) -> zbus::connection::Builder<'static> {
        zbus::connection::Builder::address(self.address.as_str()).unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Adapter {
    powered: bool,
}

#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        self.powered
    }
    #[zbus(property)]
    fn power_state(&self) -> String {
        if self.powered { "on" } else { "off" }.into()
    }
    #[zbus(property)]
    fn discovering(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn pairable(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn discoverable(&self) -> bool {
        false
    }
}

struct Device {
    connected: bool,
}

#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(ADAPTER).unwrap()
    }
    #[zbus(property)]
    fn alias(&self) -> String {
        "Fixture Headset".into()
    }
    #[zbus(property)]
    fn icon(&self) -> String {
        "audio-headset".into()
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn bonded(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.connected
    }
    #[zbus(property)]
    fn trusted(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn blocked(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn services_resolved(&self) -> bool {
        self.connected
    }
}

const ADAPTER: &str = "/org/bluez/hci0";
const DEVICE: &str = "/org/bluez/hci0/dev_00_1B_66_00_00_01";

async fn bluez(bus: &PrivateBus, connected: bool) -> zbus::Connection {
    bus.builder()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap()
        .serve_at(ADAPTER, Adapter { powered: true })
        .unwrap()
        .serve_at(DEVICE, Device { connected })
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .build()
        .await
        .unwrap()
}

/// Wait for the first observation satisfying `accept`, skipping intermediate
/// ones (the runner may report a reconnecting provider before its baseline).
async fn until(
    rx: &mut mpsc::UnboundedReceiver<BluetoothObservation>,
    accept: impl Fn(&BluetoothObservation) -> bool,
) -> BluetoothObservation {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let observation = rx.recv().await.expect("runner alive");
            eprintln!(
                "OBS provider={:?} owner={:?} adapters={} devices={} connected={} changed={} gen={}",
                observation.snapshot.provider,
                observation.snapshot.provider_owner,
                observation.snapshot.adapters.len(),
                observation.snapshot.devices.len(),
                connected_devices(&observation),
                observation.changed,
                observation.snapshot.generation
            );
            if accept(&observation) {
                return observation;
            }
        }
    })
    .await
    .expect("expected observation within 5 s")
}

fn connected_devices(observation: &BluetoothObservation) -> usize {
    observation
        .snapshot
        .devices
        .values()
        .filter(|device| device.connected)
        .count()
}

#[tokio::test(flavor = "current_thread")]
async fn runner_follows_device_changes_and_provider_replacement() {
    let bus = PrivateBus::start();
    let observer_bus = bus.builder().build().await.unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let _ = observe_on(&observer_bus, move |observation| {
            let _ = tx.send(observation);
        })
        .await;
    });

    // 1. No BlueZ: unavailable, with no owner and no facts.
    let absent = until(&mut rx, |_| true).await;
    assert_eq!(
        absent.snapshot.provider,
        BluetoothProviderState::Unavailable
    );
    assert!(absent.snapshot.provider_owner.is_none());
    assert!(absent.snapshot.adapters.is_empty());

    // 2. BlueZ appears with a powered adapter and a disconnected headset.
    let service = bluez(&bus, false).await;
    let first = until(&mut rx, |o| {
        o.snapshot.provider == BluetoothProviderState::Available
    })
    .await;
    let first_owner = first.snapshot.provider_owner.clone().unwrap();
    assert_eq!(first.snapshot.adapters.len(), 1);
    assert_eq!(connected_devices(&first), 0);
    let device = first.snapshot.devices.values().next().unwrap();
    assert_eq!(device.icon.as_deref(), Some("audio-headset"));

    // 3. The headset connects; BlueZ's own PropertiesChanged drives the read.
    {
        let iface = service
            .object_server()
            .interface::<_, Device>(DEVICE)
            .await
            .unwrap();
        iface.get_mut().await.connected = true;
        iface
            .get()
            .await
            .connected_changed(iface.signal_emitter())
            .await
            .unwrap();
    }
    let connected = until(&mut rx, |o| connected_devices(o) == 1).await;
    assert!(
        connected.changed,
        "a device connecting is a semantic change"
    );
    assert!(connected.snapshot.generation > first.snapshot.generation);

    // 4. BlueZ disappears: every runtime fact is invalidated.
    drop(service);
    let gone = until(&mut rx, |o| {
        o.snapshot.provider == BluetoothProviderState::Unavailable
    })
    .await;
    assert!(gone.snapshot.devices.is_empty() && gone.snapshot.adapters.is_empty());
    assert!(gone.snapshot.provider_owner.is_none());

    // 5. BlueZ returns under a new unique owner: a fresh baseline, never the
    //    previous owner's objects.
    let _service = bluez(&bus, true).await;
    let back = until(&mut rx, |o| {
        o.snapshot.provider == BluetoothProviderState::Available
    })
    .await;
    let back_owner = back.snapshot.provider_owner.clone().unwrap();
    assert_ne!(back_owner, first_owner);
    assert!(
        back.snapshot
            .devices
            .keys()
            .all(|id| id.belongs_to(&back_owner))
    );
    assert_eq!(connected_devices(&back), 1);
}
