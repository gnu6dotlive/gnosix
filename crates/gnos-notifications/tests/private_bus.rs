//! Qualifies the notification server against a private `dbus-daemon`.
//!
//! Real clients (`notify-send` from libnotify, and a zbus proxy) talk to a bus
//! only this test can see. The operator's session bus and its notification
//! service are never touched.

use gnos_notifications::{
    BusEvent, CloseReason, Command, NotificationId, ServeError, ServerUpdate, StoreConfig, Toast,
    Urgency, control_channel, serve_on,
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command as Process, Stdio},
    sync::mpsc,
    time::Duration,
};
use zbus::zvariant::Value;

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
            "gnos-notifications-bus-{}-{}",
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
        let mut child = Process::new("dbus-daemon")
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

    /// A real libnotify client bound to this bus only.
    fn notify_send(&self, args: &[&str]) -> Process {
        let mut command = Process::new("notify-send");
        command
            .env("DBUS_SESSION_BUS_ADDRESS", &self.address)
            .env_remove("DISPLAY")
            .args(args);
        command
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: std::collections::HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
    fn close_notification(&self, id: u32) -> zbus::Result<()>;
    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;
    fn get_server_information(&self) -> zbus::Result<(String, String, String, String)>;
    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;
    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;
}

struct Server {
    updates: mpsc::Receiver<ServerUpdate>,
    control: gnos_notifications::NotificationControl,
    task: tokio::task::JoinHandle<Result<(), ServeError>>,
}

impl Server {
    async fn start(bus: &PrivateBus, config: StoreConfig) -> Self {
        let connection = bus.connect().await;
        let (sender, updates) = mpsc::channel();
        let (control, commands) = control_channel();
        let task = tokio::spawn(async move {
            serve_on(
                &connection,
                config,
                move |update| {
                    let _ = sender.send(update);
                },
                commands,
            )
            .await
        });
        let server = Self {
            updates,
            control,
            task,
        };
        // The first publish proves the name is owned and the object exported.
        server.next().await;
        server
    }

    async fn next(&self) -> ServerUpdate {
        for _ in 0..200 {
            if let Ok(update) = self.updates.try_recv() {
                return update;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no server update within 2 s");
    }

    /// Waits for an update satisfying `predicate`, skipping others.
    async fn until(&self, predicate: impl Fn(&ServerUpdate) -> bool) -> ServerUpdate {
        for _ in 0..50 {
            let update = self.next().await;
            if predicate(&update) {
                return update;
            }
        }
        panic!("expected update never arrived");
    }
}

fn id(raw: u32) -> NotificationId {
    NotificationId::new(raw).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn advertises_only_rendered_capabilities_and_identity() {
    let bus = PrivateBus::start();
    let _server = Server::start(&bus, StoreConfig::default()).await;
    let client = bus.connect().await;
    let proxy = NotificationsProxy::new(&client).await.unwrap();
    assert_eq!(
        proxy.get_capabilities().await.unwrap(),
        ["actions", "body", "persistence"]
    );
    let (name, vendor, _version, spec) = proxy.get_server_information().await.unwrap();
    assert_eq!(
        (name.as_str(), vendor.as_str(), spec.as_str()),
        ("gnosix", "gnu6", "1.2")
    );
    // Wire shape matches the specification exactly.
    let xml = zbus::fdo::IntrospectableProxy::builder(&client)
        .destination("org.freedesktop.Notifications")
        .unwrap()
        .path("/org/freedesktop/Notifications")
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    for needle in [
        r#"<method name="GetServerInformation">"#,
        r#"<arg name="spec_version" type="s" direction="out"/>"#,
        r#"<arg name="hints" type="a{sv}" direction="in"/>"#,
        r#"<signal name="NotificationClosed">"#,
        r#"<signal name="ActionInvoked">"#,
    ] {
        assert!(xml.contains(needle), "introspection lacks {needle}:\n{xml}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn libnotify_client_notifies_replaces_and_closes() {
    let bus = PrivateBus::start();
    let server = Server::start(&bus, StoreConfig::default()).await;

    let output = bus
        .notify_send(&[
            "-p",
            "-a",
            "probe",
            "-u",
            "critical",
            "-h",
            "int:value:40",
            "-c",
            "transfer",
            "Copying",
            "file one",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let raw: u32 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap();
    let update = server
        .until(|u| {
            u.events.contains(&BusEvent::Notified {
                id: id(raw),
                replaced: false,
            })
        })
        .await;
    let n = update.snapshot.get(id(raw)).unwrap();
    assert_eq!(
        (n.app_name.as_str(), n.summary.as_str(), n.body.as_str()),
        ("probe", "Copying", "file one")
    );
    assert_eq!(n.urgency, Urgency::Critical);
    assert_eq!(n.progress, Some(40));
    assert_eq!(n.category.as_deref(), Some("transfer"));
    assert!(n.toast.is_shown());

    // libnotify's own replace path keeps the id.
    let output = bus
        .notify_send(&[
            "-p",
            "-r",
            &raw.to_string(),
            "-h",
            "int:value:90",
            "Copying",
            "file two",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        raw.to_string()
    );
    let update = server
        .until(|u| {
            u.events.contains(&BusEvent::Notified {
                id: id(raw),
                replaced: true,
            })
        })
        .await;
    assert_eq!(update.snapshot.notifications.len(), 1);
    assert_eq!(update.snapshot.get(id(raw)).unwrap().progress, Some(90));

    // CloseNotification -> NotificationClosed(id, 3), exactly once.
    let client = bus.connect().await;
    let proxy = NotificationsProxy::new(&client).await.unwrap();
    let mut closed = proxy.receive_notification_closed().await.unwrap();
    proxy.close_notification(raw).await.unwrap();
    let signal = tokio::time::timeout(Duration::from_secs(2), futures_next(&mut closed))
        .await
        .unwrap();
    let args = signal.args().unwrap();
    assert_eq!((args.id, args.reason), (raw, CloseReason::Closed.wire()));
    proxy.close_notification(raw).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(300), futures_next(&mut closed))
            .await
            .is_err(),
        "closing an unknown id emits nothing"
    );
    let update = server.until(|u| u.snapshot.notifications.is_empty()).await;
    assert!(
        update
            .events
            .contains(&BusEvent::Closed(gnos_notifications::store::Closed {
                id: id(raw),
                reason: CloseReason::Closed
            }))
    );
}

async fn futures_next<S: futures_lite::StreamExt + Unpin>(stream: &mut S) -> S::Item {
    stream.next().await.expect("signal stream ended")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn action_reaches_the_waiting_libnotify_client() {
    let bus = PrivateBus::start();
    let server = Server::start(&bus, StoreConfig::default()).await;
    // notify-send blocks until an action is invoked and prints its key.
    let child = bus
        .notify_send(&["-A", "open=Open", "-A", "later=Later", "Build", "finished"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let update = server.until(|u| !u.snapshot.notifications.is_empty()).await;
    let n = update.snapshot.notifications[0].clone();
    assert_eq!(
        n.actions
            .iter()
            .map(|a| (a.key.as_str(), a.label.as_str()))
            .collect::<Vec<_>>(),
        [("open", "Open"), ("later", "Later")]
    );
    // Unknown keys are refused without any bus traffic.
    assert!(server.control.send(Command::Invoke(n.id, "nope".into())));
    assert!(server.control.send(Command::Invoke(n.id, "later".into())));
    let update = server.until(|u| !u.events.is_empty()).await;
    assert_eq!(
        update.events,
        [
            BusEvent::ActionInvoked {
                id: n.id,
                key: "later".into()
            },
            BusEvent::Closed(gnos_notifications::store::Closed {
                id: n.id,
                reason: CloseReason::Dismissed
            }),
        ]
    );
    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "later");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn toast_lifetime_is_separate_from_the_notification() {
    let bus = PrivateBus::start();
    let server = Server::start(&bus, StoreConfig::default()).await;
    let client = bus.connect().await;
    let proxy = NotificationsProxy::new(&client).await.unwrap();
    let mut closed = proxy.receive_notification_closed().await.unwrap();

    let kept = proxy
        .notify("a", 0, "", "kept", "", &[], Default::default(), 250)
        .await
        .unwrap();
    let mut transient_hints = std::collections::HashMap::new();
    transient_hints.insert("transient", Value::Bool(true));
    let transient = proxy
        .notify("a", 0, "", "transient", "", &[], transient_hints, 250)
        .await
        .unwrap();
    let held = proxy
        .notify("a", 0, "", "held", "", &[], Default::default(), 250)
        .await
        .unwrap();
    server.until(|u| u.snapshot.notifications.len() == 3).await;
    assert!(server.control.send(Command::HoldToast(id(held))));

    // Only the transient one leaves the bus, with reason 1.
    let signal = tokio::time::timeout(Duration::from_secs(2), futures_next(&mut closed))
        .await
        .unwrap();
    let args = signal.args().unwrap();
    assert_eq!(
        (args.id, args.reason),
        (transient, CloseReason::Expired.wire())
    );
    // The two 250 ms toasts may expire in separate wakes a few ms apart;
    // wait for the state where both have been applied.
    let update = server
        .until(|u| {
            u.snapshot
                .get(id(kept))
                .is_some_and(|n| n.toast == Toast::Hidden)
                && u.snapshot.get(id(transient)).is_none()
        })
        .await;
    assert!(
        update.snapshot.get(id(held)).unwrap().toast.is_shown(),
        "held toast stays"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), futures_next(&mut closed))
            .await
            .is_err(),
        "a toast timing out never closes a persistent notification"
    );
    // Release restarts the full interval, then it hides.
    assert!(server.control.send(Command::ReleaseToast(id(held))));
    server
        .until(|u| {
            u.snapshot
                .get(id(held))
                .is_some_and(|n| n.toast == Toast::Hidden)
        })
        .await;
    assert_eq!(server.updates.try_iter().count(), 0, "no idle churn");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_existing_owner_is_never_displaced() {
    let bus = PrivateBus::start();
    let first = Server::start(&bus, StoreConfig::default()).await;
    let connection = bus.connect().await;
    let (_control, commands) = control_channel();
    let refused = serve_on(&connection, StoreConfig::default(), |_| {}, commands).await;
    match refused {
        Err(ServeError::NameTaken { owner: Some(owner) }) => assert!(owner.starts_with(':')),
        other => panic!("expected NameTaken with an owner, got {other:?}"),
    }
    // The original server still answers.
    let client = bus.connect().await;
    let proxy = NotificationsProxy::new(&client).await.unwrap();
    let raw = proxy
        .notify("a", 0, "", "still here", "", &[], Default::default(), -1)
        .await
        .unwrap();
    first.until(|u| u.snapshot.get(id(raw)).is_some()).await;
    assert!(!first.task.is_finished());
}
