//! Qualifies the real event observer against a private `dbus-daemon`.
//!
//! Fake UPower and power-profiles services are registered, changed, dropped and
//! restarted on a bus that only this test can see. The host system bus and its
//! services are never touched.

use gnos_power::{
    BatteryPercentage, PROFILES_BUS_NAME, PROFILES_PATH, PowerObservation, PowerProfile,
    PowerSnapshot, PowerWakeCause, UPOWER_BUS_NAME, UPOWER_DISPLAY_DEVICE_PATH, UPOWER_PATH,
    observe_on,
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::sync::mpsc;

struct PrivateBus {
    child: Child,
    address: String,
    _dir: TempDir,
}

struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl PrivateBus {
    fn start() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gnos-power-bus-{}-{}",
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
            _dir: TempDir(dir),
        }
    }

    async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Device {
    kind: u32,
    present: bool,
    percentage: f64,
    state: u32,
}

#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Device {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        self.kind
    }
    #[zbus(property)]
    fn is_present(&self) -> bool {
        self.present
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.percentage
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.state
    }
}

struct Root {
    on_battery: bool,
}

#[zbus::interface(name = "org.freedesktop.UPower")]
impl Root {
    #[zbus(property)]
    fn on_battery(&self) -> bool {
        self.on_battery
    }
}

struct Profiles {
    active: String,
}

#[zbus::interface(name = "net.hadess.PowerProfiles")]
impl Profiles {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.active.clone()
    }
}

async fn upower(bus: &PrivateBus, device: Device, on_battery: bool) -> zbus::Connection {
    zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .serve_at(UPOWER_DISPLAY_DEVICE_PATH, device)
        .unwrap()
        .serve_at(UPOWER_PATH, Root { on_battery })
        .unwrap()
        .name(UPOWER_BUS_NAME)
        .unwrap()
        .build()
        .await
        .unwrap()
}

async fn profiles(bus: &PrivateBus, active: &str) -> zbus::Connection {
    zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .serve_at(
            PROFILES_PATH,
            Profiles {
                active: active.into(),
            },
        )
        .unwrap()
        .name(PROFILES_BUS_NAME)
        .unwrap()
        .build()
        .await
        .unwrap()
}

async fn next(rx: &mut mpsc::UnboundedReceiver<PowerObservation>) -> PowerObservation {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("observation within 5 s")
        .expect("observer alive")
}

/// After `next`, nothing else may arrive: unchanged facts are suppressed.
async fn quiet(rx: &mut mpsc::UnboundedReceiver<PowerObservation>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(600), rx.recv())
            .await
            .is_err(),
        "no further observation"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn observer_follows_events_owner_loss_and_return_on_a_private_bus() {
    let bus = PrivateBus::start();
    let observer_bus = bus.connect().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        observe_on(&observer_bus, move |observation| {
            let _ = tx.send(observation);
        })
        .await
        .unwrap();
    });

    // 1. Nothing on the bus: unavailable, not "no battery" and not a profile.
    let baseline = next(&mut rx).await;
    assert_eq!(baseline.snapshot, PowerSnapshot::Unavailable);
    assert_eq!(baseline.cause, PowerWakeCause::Baseline);

    // 2. A desktop (no laptop battery) appears, then its profile daemon.
    let desktop = Device {
        kind: 0,
        present: false,
        percentage: 0.0,
        state: 0,
    };
    let upower_service = upower(&bus, desktop, false).await;
    let observed = next(&mut rx).await;
    assert_eq!(
        observed.snapshot,
        PowerSnapshot::NoBatteryProfileUnavailable
    );
    assert_eq!(observed.cause, PowerWakeCause::Owner);
    let profiles_service = profiles(&bus, "balanced").await;
    assert_eq!(
        next(&mut rx).await.snapshot,
        PowerSnapshot::Profile(PowerProfile::Balanced)
    );
    quiet(&mut rx).await;

    // 3. A property change is delivered through the service's own signal.
    {
        let iface = profiles_service
            .object_server()
            .interface::<_, Profiles>(PROFILES_PATH)
            .await
            .unwrap();
        iface.get_mut().await.active = "performance".into();
        iface
            .get()
            .await
            .active_profile_changed(iface.signal_emitter())
            .await
            .unwrap();
    }
    let changed = next(&mut rx).await;
    assert_eq!(
        changed.snapshot,
        PowerSnapshot::Profile(PowerProfile::Performance)
    );
    assert_eq!(changed.cause, PowerWakeCause::Properties);
    quiet(&mut rx).await;

    // 4. The profile daemon disappears: the profile is unknown, not Balanced.
    drop(profiles_service);
    assert_eq!(
        next(&mut rx).await.snapshot,
        PowerSnapshot::NoBatteryProfileUnavailable
    );
    // 5. It returns with a different profile.
    let profiles_service = profiles(&bus, "power-saver").await;
    assert_eq!(
        next(&mut rx).await.snapshot,
        PowerSnapshot::Profile(PowerProfile::PowerSaver)
    );

    // 6. UPower disappears: the whole fact is unavailable.
    drop(upower_service);
    assert_eq!(next(&mut rx).await.snapshot, PowerSnapshot::Unavailable);

    // 7. A laptop battery replaces it; its exact percentage is preserved and a
    //    sub-percent change is a change.
    let laptop = Device {
        kind: 2,
        present: true,
        percentage: 19.6,
        state: 2,
    };
    let upower_service = upower(&bus, laptop, true).await;
    let battery = |percent| PowerSnapshot::Battery {
        percentage: BatteryPercentage::new(percent).unwrap(),
        charging: false,
        on_battery: true,
    };
    assert_eq!(next(&mut rx).await.snapshot, battery(19.6));
    {
        let iface = upower_service
            .object_server()
            .interface::<_, Device>(UPOWER_DISPLAY_DEVICE_PATH)
            .await
            .unwrap();
        iface.get_mut().await.percentage = 19.8;
        iface
            .get()
            .await
            .percentage_changed(iface.signal_emitter())
            .await
            .unwrap();
    }
    assert_eq!(next(&mut rx).await.snapshot, battery(19.8));
    quiet(&mut rx).await;
    drop(profiles_service);
}
