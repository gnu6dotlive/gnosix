//! `org.freedesktop.Notifications` on a D-Bus connection.
//!
//! The name is requested with `DO_NOT_QUEUE` and without
//! `REPLACE_EXISTING`: when another server already owns it, this server
//! reports `NameTaken` and exports nothing. It never displaces the running
//! notification service.
//!
//! One task owns the store. Bus calls, surface commands and the single toast
//! deadline all funnel into it; there is no polling.

use crate::model::{
    CloseReason, ExpireTimeout, Hints, ImageSource, NotificationId, NotifyRequest, RawImage,
    Urgency, pair_actions,
};
use crate::store::{Closed, NotificationSnapshot, NotificationStore, StoreConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

pub const BUS_NAME: &str = "org.freedesktop.Notifications";
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
pub const SERVER_NAME: &str = "gnosix";
pub const SERVER_VENDOR: &str = "gnu6";
pub const SPEC_VERSION: &str = "1.2";
/// Only what Gnosix surfaces honour today. Markup, hyperlinks, images and
/// sounds are not advertised until they are rendered.
pub const CAPABILITIES: [&str; 3] = ["actions", "body", "persistence"];

/// Surface-originated requests. Every one is revalidated by the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    HoldToast(NotificationId),
    ReleaseToast(NotificationId),
    HideToast(NotificationId),
    Dismiss(NotificationId),
    Invoke(NotificationId, String),
    ClearAll,
    SetDoNotDisturb(bool),
    SetCentreOpen(bool),
    SetFullscreen(bool),
    /// SETTINGS-03B: the operator's preferences.
    SetPreferences(crate::prefs::NotificationPrefs),
}

/// What happened on the bus during one transition, for receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusEvent {
    Notified { id: NotificationId, replaced: bool },
    Closed(Closed),
    ActionInvoked { id: NotificationId, key: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerUpdate {
    pub snapshot: NotificationSnapshot,
    pub events: Vec<BusEvent>,
    /// Server clock when published, comparable with `received_ms`.
    pub now_ms: u64,
}

/// Cloneable handle surfaces use to send `Command`s.
#[derive(Clone, Debug)]
pub struct NotificationControl {
    sender: mpsc::UnboundedSender<Command>,
}

impl NotificationControl {
    /// `false` once the server has stopped.
    pub fn send(&self, command: Command) -> bool {
        self.sender.send(command).is_ok()
    }
}

#[derive(Debug)]
pub struct CommandReceiver(mpsc::UnboundedReceiver<Command>);

pub fn control_channel() -> (NotificationControl, CommandReceiver) {
    let (sender, receiver) = mpsc::unbounded_channel();
    (NotificationControl { sender }, CommandReceiver(receiver))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServeError {
    Connect(String),
    Export(String),
    /// Another server owns the name; nothing was exported.
    NameTaken {
        owner: Option<String>,
    },
    Request(String),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(e) => write!(f, "connect: {e}"),
            Self::Export(e) => write!(f, "export: {e}"),
            Self::NameTaken { owner } => write!(
                f,
                "{BUS_NAME} is owned by {}",
                owner.as_deref().unwrap_or("another client")
            ),
            Self::Request(e) => write!(f, "request name: {e}"),
        }
    }
}

impl std::error::Error for ServeError {}

type Publish = Box<dyn Fn(ServerUpdate) + Send + Sync>;

struct Shared {
    store: Mutex<NotificationStore>,
    epoch: Instant,
    deadline_changed: Notify,
    publish: Publish,
}

impl Shared {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn store(&self) -> MutexGuard<'_, NotificationStore> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Publishes outside the lock, then lets the task re-arm its deadline.
    fn publish(&self, events: Vec<BusEvent>) {
        let snapshot = self.store().snapshot();
        (self.publish)(ServerUpdate {
            snapshot,
            events,
            now_ms: self.now_ms(),
        });
        self.deadline_changed.notify_one();
    }
}

struct Interface {
    shared: Arc<Shared>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Interface {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> u32 {
        let request = NotifyRequest {
            app_name,
            replaces_id,
            app_icon,
            summary,
            body,
            actions: pair_actions(&actions),
            hints: parse_hints(&hints),
            expire_timeout: ExpireTimeout::from_wire(expire_timeout),
        };
        let now = self.shared.now_ms();
        let receipt = self.shared.store().notify(request, now);
        let mut events = vec![BusEvent::Notified {
            id: receipt.id,
            replaced: receipt.replaced,
        }];
        for closed in receipt.evicted {
            let _ =
                Self::notification_closed(&emitter, closed.id.get(), closed.reason.wire()).await;
            events.push(BusEvent::Closed(closed));
        }
        self.shared.publish(events);
        receipt.id.get()
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        // Closing an unknown id is a no-op: nothing to report.
        let Some(id) = NotificationId::new(id) else {
            return;
        };
        let closed = self.shared.store().close(id, CloseReason::Closed);
        if let Some(closed) = closed {
            let _ =
                Self::notification_closed(&emitter, closed.id.get(), closed.reason.wire()).await;
            self.shared.publish(vec![BusEvent::Closed(closed)]);
        }
    }

    fn get_capabilities(&self) -> Vec<String> {
        CAPABILITIES.iter().map(|c| (*c).to_owned()).collect()
    }

    #[zbus(out_args("name", "vendor", "version", "spec_version"))]
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            SERVER_NAME.to_owned(),
            SERVER_VENDOR.to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
            SPEC_VERSION.to_owned(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

fn integer(value: &Value<'_>) -> Option<i64> {
    match value {
        Value::U8(v) => Some(i64::from(*v)),
        Value::I16(v) => Some(i64::from(*v)),
        Value::U16(v) => Some(i64::from(*v)),
        Value::I32(v) => Some(i64::from(*v)),
        Value::U32(v) => Some(i64::from(*v)),
        Value::I64(v) => Some(*v),
        Value::U64(v) => i64::try_from(*v).ok(),
        Value::Value(inner) => integer(inner),
        _ => None,
    }
}

fn text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.as_str().to_owned()),
        Value::Value(inner) => text(inner),
        _ => None,
    }
}

fn boolean(value: &Value<'_>) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Value(inner) => boolean(inner),
        // Some clients send resident/transient as a byte.
        other => integer(other).map(|v| v != 0),
    }
}

fn image_data(value: &Value<'_>) -> Option<RawImage> {
    let fields = match value {
        Value::Structure(s) => s.fields(),
        Value::Value(inner) => return image_data(inner),
        _ => return None,
    };
    let [
        width,
        height,
        rowstride,
        alpha,
        bits,
        channels,
        Value::Array(bytes),
    ] = fields
    else {
        return None;
    };
    if bytes.len() > crate::model::IMAGE_DATA_LIMIT {
        return None;
    }
    let data = bytes
        .iter()
        .map(|byte| match byte {
            Value::U8(b) => Some(*b),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;
    RawImage::new(
        i32::try_from(integer(width)?).ok()?,
        i32::try_from(integer(height)?).ok()?,
        i32::try_from(integer(rowstride)?).ok()?,
        boolean(alpha)?,
        i32::try_from(integer(bits)?).ok()?,
        i32::try_from(integer(channels)?).ok()?,
        data,
    )
}

/// Validates the hints Gnosix understands; each malformed hint is dropped on
/// its own. `image-data` wins over `image-path`, deprecated spellings last.
pub fn parse_hints(raw: &HashMap<String, OwnedValue>) -> Hints {
    let get = |key: &str| raw.get(key).map(|v| &**v);
    let image = ["image-data", "image_data", "icon_data"]
        .into_iter()
        .find_map(|key| get(key).and_then(image_data))
        .map(ImageSource::Data)
        .or_else(|| {
            ["image-path", "image_path"]
                .into_iter()
                .find_map(|key| get(key).and_then(text))
                .filter(|path| !path.is_empty())
                .map(ImageSource::Path)
        });
    Hints {
        urgency: get("urgency")
            .and_then(integer)
            .and_then(Urgency::from_wire),
        category: get("category").and_then(text).filter(|s| !s.is_empty()),
        desktop_entry: get("desktop-entry")
            .and_then(text)
            .filter(|s| !s.is_empty()),
        resident: get("resident").and_then(boolean).unwrap_or(false),
        transient: get("transient").and_then(boolean).unwrap_or(false),
        image,
        progress: get("value")
            .and_then(integer)
            .and_then(Hints::progress_from_wire),
    }
}

async fn emit(emitter: &SignalEmitter<'_>, events: &[BusEvent]) {
    for event in events {
        let _ = match event {
            BusEvent::Closed(closed) => {
                Interface::notification_closed(emitter, closed.id.get(), closed.reason.wire()).await
            }
            BusEvent::ActionInvoked { id, key } => {
                Interface::action_invoked(emitter, id.get(), key).await
            }
            BusEvent::Notified { .. } => Ok(()),
        };
    }
}

fn apply(store: &mut NotificationStore, command: Command, now_ms: u64) -> Option<Vec<BusEvent>> {
    let closed = |closed: Option<Closed>| closed.map(|c| vec![BusEvent::Closed(c)]);
    match command {
        Command::HoldToast(id) => store.hold_toast(id).then(Vec::new),
        Command::ReleaseToast(id) => store.release_toast(id, now_ms).then(Vec::new),
        Command::HideToast(id) => store.hide_toast(id).then(Vec::new),
        Command::Dismiss(id) => closed(store.close(id, CloseReason::Dismissed)),
        Command::Invoke(id, key) => store.invoke(id, &key).map(|receipt| {
            let mut events = vec![BusEvent::ActionInvoked {
                id: receipt.id,
                key: receipt.key,
            }];
            events.extend(receipt.closed.map(BusEvent::Closed));
            events
        }),
        Command::ClearAll => {
            let closed = store.clear_all();
            (!closed.is_empty()).then(|| closed.into_iter().map(BusEvent::Closed).collect())
        }
        Command::SetDoNotDisturb(enabled) => store.set_do_not_disturb(enabled).then(Vec::new),
        Command::SetCentreOpen(open) => store.set_centre_open(open).then(Vec::new),
        Command::SetFullscreen(fullscreen) => {
            store.set_fullscreen(fullscreen);
            None
        }
        Command::SetPreferences(prefs) => store.set_prefs(prefs).then(Vec::new),
    }
}

/// Serves until the connection fails. Returns `NameTaken` immediately when
/// another server owns the name.
pub async fn serve_on<F>(
    connection: &zbus::Connection,
    config: StoreConfig,
    publish: F,
    commands: CommandReceiver,
) -> Result<(), ServeError>
where
    F: Fn(ServerUpdate) + Send + Sync + 'static,
{
    let shared = Arc::new(Shared {
        store: Mutex::new(NotificationStore::new(config)),
        epoch: Instant::now(),
        deadline_changed: Notify::new(),
        publish: Box::new(publish),
    });
    let objects = connection.object_server();
    objects
        .at(
            OBJECT_PATH,
            Interface {
                shared: shared.clone(),
            },
        )
        .await
        .map_err(|e| ServeError::Export(e.to_string()))?;
    let reply = connection
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
        .await;
    match reply {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {}
        Ok(RequestNameReply::Exists | RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => {
            let _ = objects.remove::<Interface, _>(OBJECT_PATH).await;
            let owner = match DBusProxy::new(connection).await {
                Ok(proxy) => proxy
                    .get_name_owner(BUS_NAME.try_into().expect("valid bus name"))
                    .await
                    .ok()
                    .map(|owner| owner.to_string()),
                Err(_) => None,
            };
            return Err(ServeError::NameTaken { owner });
        }
        Err(error) => {
            let _ = objects.remove::<Interface, _>(OBJECT_PATH).await;
            return Err(ServeError::Request(error.to_string()));
        }
    }
    let emitter = SignalEmitter::new(connection, OBJECT_PATH)
        .map_err(|e| ServeError::Export(e.to_string()))?;
    shared.publish(Vec::new());
    let mut commands = commands.0;
    let mut control_open = true;
    loop {
        let deadline = shared.store().next_deadline();
        let sleep_until = deadline.map(|ms| shared.epoch + std::time::Duration::from_millis(ms));
        tokio::select! {
            command = commands.recv(), if control_open => {
                let Some(command) = command else {
                    // No surface left; keep serving the bus.
                    control_open = false;
                    continue;
                };
                let now = shared.now_ms();
                let events = apply(&mut shared.store(), command, now);
                if let Some(events) = events {
                    emit(&emitter, &events).await;
                    shared.publish(events);
                }
            }
            () = async {
                match sleep_until {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                let now = shared.now_ms();
                let outcome = shared.store().expire_due(now);
                if outcome.changed {
                    let events: Vec<_> = outcome.closed.into_iter().map(BusEvent::Closed).collect();
                    emit(&emitter, &events).await;
                    shared.publish(events);
                }
            }
            () = shared.deadline_changed.notified() => {}
        }
    }
}

/// Blocking entry point on the session bus, for a dedicated thread.
pub fn run_session<F>(
    config: StoreConfig,
    publish: F,
    commands: CommandReceiver,
) -> Result<(), ServeError>
where
    F: Fn(ServerUpdate) + Send + Sync + 'static,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ServeError::Connect(e.to_string()))?;
    runtime.block_on(async move {
        let connection = zbus::Connection::session()
            .await
            .map_err(|e| ServeError::Connect(e.to_string()))?;
        serve_on(&connection, config, publish, commands).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Structure;

    fn owned(value: Value<'static>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    #[test]
    fn hints_are_typed_individually() {
        let mut raw = HashMap::new();
        raw.insert("urgency".to_owned(), owned(Value::U8(2)));
        raw.insert("category".to_owned(), owned(Value::from("im.received")));
        raw.insert("resident".to_owned(), owned(Value::Bool(true)));
        raw.insert("transient".to_owned(), owned(Value::U8(1)));
        raw.insert("value".to_owned(), owned(Value::I32(150)));
        raw.insert(
            "image-path".to_owned(),
            owned(Value::from("dialog-information")),
        );
        let hints = parse_hints(&raw);
        assert_eq!(hints.urgency, Some(Urgency::Critical));
        assert_eq!(hints.category.as_deref(), Some("im.received"));
        assert!(hints.resident && hints.transient);
        assert_eq!(hints.progress, None, "out-of-range value is dropped alone");
        assert_eq!(
            hints.image,
            Some(ImageSource::Path("dialog-information".into()))
        );
    }

    #[test]
    fn image_data_wins_and_is_validated() {
        let pixels: Vec<u8> = vec![255; 2 * 2 * 4];
        let structure = Structure::from((2i32, 2i32, 8i32, true, 8i32, 4i32, pixels));
        let mut raw = HashMap::new();
        raw.insert("image-data".to_owned(), owned(Value::Structure(structure)));
        raw.insert("image-path".to_owned(), owned(Value::from("/tmp/x.png")));
        match parse_hints(&raw).image {
            Some(ImageSource::Data(image)) => {
                assert_eq!((image.width, image.height, image.has_alpha), (2, 2, true))
            }
            other => panic!("expected image data, got {other:?}"),
        }
        // Malformed image-data falls back to image-path.
        let bad = Structure::from((2i32, 2i32, 8i32, true, 8i32, 4i32, vec![0u8; 3]));
        raw.insert("image-data".to_owned(), owned(Value::Structure(bad)));
        assert_eq!(
            parse_hints(&raw).image,
            Some(ImageSource::Path("/tmp/x.png".into()))
        );
    }

    #[test]
    fn surface_commands_map_to_bus_events() {
        let mut store = NotificationStore::new(StoreConfig::default());
        let request = NotifyRequest {
            app_name: "a".into(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: "s".into(),
            body: String::new(),
            actions: pair_actions(&["default".into(), "Open".into()]),
            hints: Hints::default(),
            expire_timeout: ExpireTimeout::ServerDefault,
        };
        let id = store.notify(request, 0).id;
        assert_eq!(apply(&mut store, Command::HoldToast(id), 1), Some(vec![]));
        assert_eq!(apply(&mut store, Command::SetFullscreen(true), 1), None);
        assert_eq!(
            apply(&mut store, Command::Invoke(id, "default".into()), 1),
            Some(vec![
                BusEvent::ActionInvoked {
                    id,
                    key: "default".into()
                },
                BusEvent::Closed(Closed {
                    id,
                    reason: CloseReason::Dismissed
                }),
            ])
        );
        assert_eq!(apply(&mut store, Command::Dismiss(id), 1), None);
    }
}
