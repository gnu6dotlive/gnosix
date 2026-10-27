//! Typed StatusNotifierItem tray substrate and snapshot model.
//!
//! Separates external D-Bus facts from GPUI presentation.
//! The Bar consumes a derived presentation model only; no D-Bus proxies,
//! paths, or shell commands leak into GPUI widgets.

use gpui::{App, AppContext, Context, Entity, Global};
use std::{collections::HashMap, process, time::Duration};

/// Explicit architectural boundary decisions:
pub const WATCHER_OWNER: &str = "org.kde.StatusNotifierWatcher";
pub const HOST_ROLE: &str = "StatusNotifierHost";
pub const ITEM_OBSERVATION_PATH: &str = "org.freedesktop.DBus.Properties";
pub const COEXISTENCE_BEHAVIOUR: &str = "coexist_as_secondary_host";
pub const TRAY_ITEM_ACTIVATION: &str = "OPEN";

/// Stable, typed identifier for a tray item.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrayItemId(pub String);

impl TrayItemId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TrayItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// StatusNotifierItem status contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TrayStatus {
    #[default]
    Active,
    Passive,
    NeedsAttention,
}

impl TrayStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "NeedsAttention" => Self::NeedsAttention,
            "Passive" => Self::Passive,
            _ => Self::Active,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Passive => "Passive",
            Self::NeedsAttention => "NeedsAttention",
        }
    }
}

/// StatusNotifierItem category contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrayCategory {
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
    Other(String),
}

impl TrayCategory {
    pub fn parse(s: &str) -> Self {
        match s {
            "ApplicationStatus" => Self::ApplicationStatus,
            "Communications" => Self::Communications,
            "SystemServices" => Self::SystemServices,
            "Hardware" => Self::Hardware,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::ApplicationStatus => "ApplicationStatus",
            Self::Communications => "Communications",
            Self::SystemServices => "SystemServices",
            Self::Hardware => "Hardware",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// Typed representation of a tray icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayIcon {
    Named(String),
    Pixmap {
        width: i32,
        height: i32,
        argb_bytes: Vec<u8>,
    },
    None,
}

impl TrayIcon {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// One SNI pixmap exactly as published: ARGB32, network byte order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayPixmap {
    pub width: i32,
    pub height: i32,
    pub argb_bytes: Vec<u8>,
}

/// One SNI icon slot as the provider published it, with no precedence applied:
/// the themed name and every pixmap size. Selection between slots is a
/// presentation decision (`product_tray_presentation`), never done here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayIconSlot {
    pub name: Option<String>,
    pub pixmaps: Vec<TrayPixmap>,
}

impl TrayIconSlot {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            pixmaps: Vec::new(),
        }
    }

    pub fn from_pixmaps(pixmaps: Vec<TrayPixmap>) -> Self {
        Self {
            name: None,
            pixmaps,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.pixmaps.is_empty()
    }

    /// Legacy single-value view: the name when present, else the first pixmap.
    fn legacy_icon(&self) -> TrayIcon {
        match (&self.name, self.pixmaps.first()) {
            (Some(name), _) => TrayIcon::Named(name.clone()),
            (None, Some(pixmap)) => TrayIcon::Pixmap {
                width: pixmap.width,
                height: pixmap.height,
                argb_bytes: pixmap.argb_bytes.clone(),
            },
            (None, None) => TrayIcon::None,
        }
    }
}

/// Every icon fact of one StatusNotifierItem, kept apart: the normal icon, the
/// attention icon, the overlay and the per-item icon-theme directory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayIconFacts {
    pub icon: TrayIconSlot,
    pub attention: TrayIconSlot,
    pub overlay: TrayIconSlot,
    /// SNI `IconThemePath`: an extra directory searched for this item's names.
    pub theme_path: Option<String>,
}

impl From<TrayIcon> for TrayIconFacts {
    fn from(icon: TrayIcon) -> Self {
        match icon {
            TrayIcon::Named(name) => Self::named(name),
            TrayIcon::Pixmap {
                width,
                height,
                argb_bytes,
            } => Self {
                icon: TrayIconSlot::from_pixmaps(vec![TrayPixmap {
                    width,
                    height,
                    argb_bytes,
                }]),
                ..Self::default()
            },
            TrayIcon::None => Self::none(),
        }
    }
}

impl TrayIconFacts {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            icon: TrayIconSlot::named(name),
            ..Self::default()
        }
    }

    pub fn none() -> Self {
        Self::default()
    }

    pub fn with_attention(mut self, attention: TrayIconSlot) -> Self {
        self.attention = attention;
        self
    }

    pub fn with_overlay(mut self, overlay: TrayIconSlot) -> Self {
        self.overlay = overlay;
        self
    }

    pub fn with_theme_path(mut self, path: impl Into<String>) -> Self {
        self.theme_path = Some(path.into());
        self
    }
}

/// Typed representation of a tray tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayTooltip {
    pub title: String,
    pub description: String,
}

/// Complete typed snapshot of a single StatusNotifierItem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayItemSnapshot {
    pub id: TrayItemId,
    pub service: String,
    pub path: String,
    pub title: String,
    pub status: TrayStatus,
    pub category: TrayCategory,
    pub icons: TrayIconFacts,
    pub tooltip: Option<TrayTooltip>,
    pub menu_path: Option<String>,
    pub item_is_menu: bool,
    /// Unique-name owner resolved when this item was observed.
    ///
    /// A well-known bus name can be re-requested by another process while a
    /// rendered icon is still on screen, so activation binds to this
    /// provenance rather than to the address alone.
    pub owner: String,
}

impl TrayItemSnapshot {
    /// Legacy view used by the product Bar: the normal icon as one value.
    pub fn icon(&self) -> TrayIcon {
        self.icons.icon.legacy_icon()
    }

    /// Legacy view used by the product Bar: the attention icon when published.
    pub fn attention_icon(&self) -> Option<TrayIcon> {
        (!self.icons.attention.is_empty()).then(|| self.icons.attention.legacy_icon())
    }
}

/// Complete typed snapshot of the entire system tray.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum TraySnapshot {
    #[default]
    Unavailable,
    Active {
        items: Vec<TrayItemSnapshot>,
    },
}

impl TraySnapshot {
    /// Construct an active snapshot with deterministic ordering.
    pub fn from_items(mut items: Vec<TrayItemSnapshot>) -> Self {
        items.sort_by(|a, b| {
            a.id.cmp(&b.id)
                .then_with(|| a.service.cmp(&b.service))
                .then_with(|| a.path.cmp(&b.path))
        });
        Self::Active { items }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    pub fn items(&self) -> &[TrayItemSnapshot] {
        match self {
            Self::Unavailable => &[],
            Self::Active { items } => items.as_slice(),
        }
    }

    pub fn item_count(&self) -> usize {
        self.items().len()
    }

    pub fn find_item(&self, id: &TrayItemId) -> Option<&TrayItemSnapshot> {
        self.items().iter().find(|item| &item.id == id)
    }
}

/// Splits a StatusNotifierItem bus entry into service name and object path.
pub fn parse_sni_service_and_path(raw: &str) -> (String, String) {
    if let Some(slash_idx) = raw.find('/') {
        let service = &raw[..slash_idx];
        let path = &raw[slash_idx..];
        (service.to_string(), path.to_string())
    } else {
        (raw.to_string(), "/StatusNotifierItem".to_string())
    }
}

#[derive(Clone)]
struct TrayGlobal(Entity<TraySession>);
impl Global for TrayGlobal {}

/// In-process StatusNotifierItem observation coordinator.
pub struct TraySession {
    snapshot: TraySnapshot,
    /// Live provider connection, published once the observer has established
    /// it so the product action layer can dispatch without owning transport.
    connection: Option<zbus::Connection>,
}

impl TraySession {
    pub fn shared(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<TrayGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self {
            snapshot: TraySnapshot::Unavailable,
            connection: None,
        });
        cx.set_global(TrayGlobal(entity.clone()));
        entity.update(cx, |this, cx| this.start_listener(cx));
        entity
    }

    pub fn snapshot(&self) -> &TraySnapshot {
        &self.snapshot
    }

    /// The established provider connection, if the observer is live.
    pub fn connection(&self) -> Option<&zbus::Connection> {
        self.connection.as_ref()
    }

    fn set_connection(&mut self, connection: zbus::Connection, cx: &mut Context<Self>) {
        self.connection = Some(connection);
        cx.notify();
    }

    pub fn set_snapshot(&mut self, snapshot: TraySnapshot, cx: &mut Context<Self>) -> bool {
        if self.snapshot != snapshot {
            self.snapshot = snapshot.clone();
            // Prime the relay hub at observation time (menu projection is
            // published separately, where it is owned).
            if let Some(connection) = &self.connection {
                crate::product_tray_relay::publish_snapshot(connection.clone(), snapshot);
            }
            cx.notify();
            true
        } else {
            false
        }
    }

    fn start_listener(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let pid = process::id();
            let host_name = format!("org.kde.StatusNotifierHost-gnosix-{pid}");

            // Run provider observation loop
            loop {
                let observer_res = run_sni_observer(&host_name, &this, cx).await;
                if let Err(e) = observer_res {
                    eprintln!("GNOS_UX_TRAY_OBSERVER_ERROR: {e}");
                    let _ = this.update(&mut *cx, |this, cx| {
                        this.set_snapshot(TraySnapshot::Unavailable, cx);
                    });
                }
                // Backoff before reconnecting if disconnected
                cx.background_executor().timer(Duration::from_secs(2)).await;
            }
        })
        .detach();
    }
}

/// Connects to D-Bus, registers as StatusNotifierHost, and processes updates.
async fn run_sni_observer(
    host_name: &str,
    session: &gpui::WeakEntity<TraySession>,
    cx: &mut gpui::AsyncApp,
) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("dbus_session_connect_failed: {e}"))?;

    let watcher_proxy = zbus::Proxy::new(
        &connection,
        WATCHER_OWNER,
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await
    .map_err(|e| format!("watcher_proxy_creation_failed: {e}"))?;

    // Register our host on the watcher (coexisting as secondary host alongside existing hosts)
    let _: () = watcher_proxy
        .call("RegisterStatusNotifierHost", &(host_name))
        .await
        .map_err(|e| format!("register_host_failed: {e}"))?;

    // Publish the established connection so the product action layer can dispatch
    // typed intents without owning any transport of its own.
    let _ = session.update(cx, |this, cx| {
        this.set_connection(connection.clone(), cx);
    });

    println!(
        "GNOS_UX_TRAY_HOST_REGISTERED host={host_name} watcher={WATCHER_OWNER} coexistence={COEXISTENCE_BEHAVIOUR}"
    );

    // Initial query of registered items
    let initial_items: Vec<String> = watcher_proxy
        .get_property("RegisteredStatusNotifierItems")
        .await
        .unwrap_or_default();

    let initial_snapshot = fetch_all_items(&connection, &initial_items).await;
    let initial_count = initial_snapshot.item_count();
    let _ = session.update(cx, |this, cx| {
        this.set_snapshot(initial_snapshot, cx);
    });

    println!("GNOS_UX_TRAY_SNAPSHOT source=initial resolved={initial_count}");

    // Listen for registered / unregistered signals.
    //
    // The signal payload names the item that changed, so membership is applied
    // from that payload instead of re-reading RegisteredStatusNotifierItems.
    // Re-reading the property inside the signal handler observes the watcher
    // before its own bookkeeping is visible, which silently drops the change.
    use futures::StreamExt;
    let mut reg_stream = watcher_proxy
        .receive_signal("StatusNotifierItemRegistered")
        .await
        .map_err(|e| format!("receive_reg_signal_failed: {e}"))?;
    let mut unreg_stream = watcher_proxy
        .receive_signal("StatusNotifierItemUnregistered")
        .await
        .map_err(|e| format!("receive_unreg_signal_failed: {e}"))?;

    loop {
        futures::select! {
            reg_signal = reg_stream.next() => {
                let Some(message) = reg_signal else {
                    return Err("registration_stream_closed".to_string());
                };
                let Some(entry) = signal_payload_str(&message) else {
                    continue;
                };
                // The item is fetched directly from its own bus entry, so
                // appearance does not depend on the watcher property catching up.
                let item = fetch_single_item(&connection, &entry).await;
                let (resolved_count, changed) = apply_membership(session, cx, &entry, item);
                println!(
                    "GNOS_UX_TRAY_SIGNAL member=Registered resolved={resolved_count} changed={changed}"
                );
            },
            unreg_signal = unreg_stream.next() => {
                let Some(message) = unreg_signal else {
                    return Err("unregistration_stream_closed".to_string());
                };
                let Some(entry) = signal_payload_str(&message) else {
                    continue;
                };
                let (resolved_count, changed) = remove_membership(session, cx, &entry);
                println!(
                    "GNOS_UX_TRAY_SIGNAL member=Unregistered resolved={resolved_count} changed={changed}"
                );
            },
        }
    }
}

/// Extracts the single string argument of a StatusNotifierWatcher signal.
fn signal_payload_str(message: &zbus::Message) -> Option<String> {
    let body = message.body();
    let text = body.deserialize::<zbus::zvariant::Str>().ok()?;
    Some(text.to_string())
}

/// Adds or replaces one item in the current snapshot, keyed by bus entry.
///
/// A re-registration of the same bus entry replaces rather than duplicates.
/// A registration whose properties could not be resolved removes any stale
/// entry for that bus entry, so a vanished item cannot linger.
fn upsert_membership(
    current: &TraySnapshot,
    entry: &str,
    item: Option<TrayItemSnapshot>,
) -> TraySnapshot {
    let mut items = current.items().to_vec();
    let (service, path) = parse_sni_service_and_path(entry);
    items.retain(|existing| existing.service != service || existing.path != path);
    if let Some(item) = item {
        items.push(item);
    }
    TraySnapshot::from_items(items)
}

/// Removes one item from the current snapshot, keyed by bus entry.
///
/// Removing an absent entry yields an identical snapshot, which is how an
/// unobserved or already-absent item produces no runtime churn.
fn drop_membership(current: &TraySnapshot, entry: &str) -> TraySnapshot {
    let mut items = current.items().to_vec();
    let (service, path) = parse_sni_service_and_path(entry);
    items.retain(|existing| existing.service != service || existing.path != path);
    TraySnapshot::from_items(items)
}

/// Adds or replaces one item in the published snapshot, keyed by bus entry.
///
/// Returns the resulting item count and whether the published snapshot changed.
fn apply_membership(
    session: &gpui::WeakEntity<TraySession>,
    cx: &mut gpui::AsyncApp,
    entry: &str,
    item: Option<TrayItemSnapshot>,
) -> (usize, bool) {
    session
        .update(cx, |this, cx| {
            let next = upsert_membership(&this.snapshot, entry, item);
            let resolved = next.item_count();
            let changed = this.set_snapshot(next, cx);
            (resolved, changed)
        })
        .unwrap_or((0, false))
}

/// Removes one item from the published snapshot, keyed by bus entry.
///
/// Returns the resulting item count and whether the published snapshot changed.
fn remove_membership(
    session: &gpui::WeakEntity<TraySession>,
    cx: &mut gpui::AsyncApp,
    entry: &str,
) -> (usize, bool) {
    session
        .update(cx, |this, cx| {
            let next = drop_membership(&this.snapshot, entry);
            let resolved = next.item_count();
            let changed = this.set_snapshot(next, cx);
            (resolved, changed)
        })
        .unwrap_or((0, false))
}

/// Fetch properties for all given item entries.
async fn fetch_all_items(connection: &zbus::Connection, raw_items: &[String]) -> TraySnapshot {
    let mut items = Vec::new();
    for raw in raw_items {
        if let Some(item_snap) = fetch_single_item(connection, raw).await {
            items.push(item_snap);
        }
    }
    TraySnapshot::from_items(items)
}

/// Fetch a single item's properties safely without letting errors propagate.
async fn fetch_single_item(connection: &zbus::Connection, raw: &str) -> Option<TrayItemSnapshot> {
    let (service, path) = parse_sni_service_and_path(raw);
    let proxy = zbus::Proxy::new(
        connection,
        service.as_str(),
        path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await
    .ok()?;

    let props_result: Result<HashMap<String, zbus::zvariant::OwnedValue>, _> =
        proxy.call("GetAll", &("org.kde.StatusNotifierItem")).await;

    let props = match props_result {
        Ok(p) => p,
        Err(_) => {
            // Malformed item or unreachable service does not crash or poison the tray
            return None;
        }
    };

    let id = props
        .get("Id")
        .and_then(|v| <&str>::try_from(&**v).ok())
        .map(TrayItemId::new)
        .unwrap_or_else(|| {
            let fallback = path.rsplit('/').next().unwrap_or(&service);
            TrayItemId::new(fallback)
        });

    let title = props
        .get("Title")
        .and_then(|v| <&str>::try_from(&**v).ok())
        .unwrap_or_else(|| id.as_str())
        .to_string();

    let status = props
        .get("Status")
        .and_then(|v| <&str>::try_from(&**v).ok())
        .map(TrayStatus::parse)
        .unwrap_or(TrayStatus::Active);

    let category = props
        .get("Category")
        .and_then(|v| <&str>::try_from(&**v).ok())
        .map(TrayCategory::parse)
        .unwrap_or(TrayCategory::ApplicationStatus);

    let slot = |name_key: &str, pixmap_key: &str| TrayIconSlot {
        name: props
            .get(name_key)
            .and_then(|v| <&str>::try_from(&**v).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        pixmaps: props.get(pixmap_key).map(parse_pixmaps).unwrap_or_default(),
    };
    let icons = TrayIconFacts {
        icon: slot("IconName", "IconPixmap"),
        attention: slot("AttentionIconName", "AttentionIconPixmap"),
        overlay: slot("OverlayIconName", "OverlayIconPixmap"),
        theme_path: props
            .get("IconThemePath")
            .and_then(|v| <&str>::try_from(&**v).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    };

    let tooltip = props.get("ToolTip").and_then(parse_tooltip);

    let menu_path = props
        .get("Menu")
        .and_then(|v| <zbus::zvariant::ObjectPath<'_>>::try_from(&**v).ok())
        .map(|p| p.as_str().to_string());

    let item_is_menu = props
        .get("ItemIsMenu")
        .and_then(|v| bool::try_from(&**v).ok())
        .unwrap_or(false);

    let owner = crate::interaction_lease::unique_name_owner(connection, &service)
        .await
        .unwrap_or_else(|| service.clone());

    Some(TrayItemSnapshot {
        id,
        service,
        path,
        title,
        status,
        category,
        icons,
        tooltip,
        menu_path,
        item_is_menu,
        owner,
    })
}

/// Every `(width, height, ARGB bytes)` entry of an SNI pixmap array, in the
/// order published. Entries with a non-positive size or a length that does not
/// match `width * height * 4` are malformed and skipped, never repaired.
fn parse_pixmaps(val: &zbus::zvariant::OwnedValue) -> Vec<TrayPixmap> {
    let Ok(array) = <zbus::zvariant::Array<'_>>::try_from(&**val) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|entry| <(i32, i32, Vec<u8>)>::try_from(entry).ok())
        .filter(|(w, h, bytes)| {
            *w > 0 && *h > 0 && bytes.len() == (*w as usize) * (*h as usize) * 4
        })
        .map(|(width, height, argb_bytes)| TrayPixmap {
            width,
            height,
            argb_bytes,
        })
        .collect()
}

fn parse_tooltip(val: &zbus::zvariant::OwnedValue) -> Option<TrayTooltip> {
    // ToolTip is (s, a(iiay), s, s) -> (icon_name, icon_pixmap, title, description)
    let structure = <zbus::zvariant::Structure<'_>>::try_from(&**val).ok()?;
    let fields = structure.fields();
    if fields.len() >= 4 {
        let title = <&str>::try_from(&fields[2]).unwrap_or("").to_string();
        let description = <&str>::try_from(&fields[3]).unwrap_or("").to_string();
        if !title.is_empty() || !description.is_empty() {
            return Some(TrayTooltip { title, description });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sni_service_and_path_handles_slash_and_colon_formats() {
        let (service, path) =
            parse_sni_service_and_path("org.kde.StatusNotifierItem-3204-1/StatusNotifierItem");
        assert_eq!(service, "org.kde.StatusNotifierItem-3204-1");
        assert_eq!(path, "/StatusNotifierItem");

        let (service, path) = parse_sni_service_and_path(":1.1220/org/blueman/sni");
        assert_eq!(service, ":1.1220");
        assert_eq!(path, "/org/blueman/sni");

        let (service, path) =
            parse_sni_service_and_path(":1.1373/org/ayatana/NotificationItem/steam");
        assert_eq!(service, ":1.1373");
        assert_eq!(path, "/org/ayatana/NotificationItem/steam");

        let (service, path) = parse_sni_service_and_path("org.example.NoSlash");
        assert_eq!(service, "org.example.NoSlash");
        assert_eq!(path, "/StatusNotifierItem");
    }

    #[test]
    fn deterministic_ordering_policy_sorts_items_by_identity() {
        let item1 = TrayItemSnapshot {
            id: TrayItemId::new("steam"),
            service: ":1.1373".into(),
            path: "/steam".into(),
            title: "Steam".into(),
            status: TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            icons: TrayIconFacts::named("steam_tray_mono"),
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        };
        let item2 = TrayItemSnapshot {
            id: TrayItemId::new("blueman"),
            service: ":1.1220".into(),
            path: "/blueman".into(),
            title: "Bluetooth".into(),
            status: TrayStatus::Active,
            category: TrayCategory::Hardware,
            icons: TrayIconFacts::named("blueman-active"),
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        };
        let item3 = TrayItemSnapshot {
            id: TrayItemId::new("tty7"),
            service: "org.kde.StatusNotifierItem-1085071-1".into(),
            path: "/StatusNotifierItem".into(),
            title: "tty7".into(),
            status: TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            icons: TrayIconFacts {
                icon: TrayIconSlot::from_pixmaps(vec![TrayPixmap {
                    width: 32,
                    height: 32,
                    argb_bytes: vec![0; 4096],
                }]),
                ..TrayIconFacts::default()
            },
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        };

        let snapshot = TraySnapshot::from_items(vec![item1.clone(), item2.clone(), item3.clone()]);
        let ordered_ids: Vec<_> = snapshot.items().iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ordered_ids, vec!["blueman", "steam", "tty7"]);

        // Same items provided in reverse order yields identical snapshot
        let snapshot_rev = TraySnapshot::from_items(vec![item3, item1, item2]);
        assert_eq!(snapshot, snapshot_rev);
    }

    #[test]
    fn unavailable_state_is_explicit_and_empty() {
        let snap = TraySnapshot::Unavailable;
        assert!(!snap.is_available());
        assert_eq!(snap.item_count(), 0);
        assert_eq!(snap.items(), &[]);
    }

    #[test]
    fn malformed_item_isolation_does_not_poison_tray() {
        let items = vec![
            TrayItemSnapshot {
                id: TrayItemId::new("valid-item"),
                service: ":1.1".into(),
                path: "/valid".into(),
                title: "Valid".into(),
                status: TrayStatus::Active,
                category: TrayCategory::ApplicationStatus,
                icons: TrayIconFacts::named("valid-icon"),
                tooltip: None,
                menu_path: None,
                item_is_menu: false,
                owner: ":1.donor15".to_owned(),
            },
            TrayItemSnapshot {
                id: TrayItemId::new("broken-icon-item"),
                service: ":1.2".into(),
                path: "/broken".into(),
                title: "Broken".into(),
                status: TrayStatus::Active,
                category: TrayCategory::ApplicationStatus,
                icons: TrayIconFacts::none(),
                tooltip: None,
                menu_path: None,
                item_is_menu: false,
                owner: ":1.donor15".to_owned(),
            },
        ];
        let snapshot = TraySnapshot::from_items(items);
        assert!(snapshot.is_available());
        assert_eq!(snapshot.item_count(), 2);
        assert_eq!(
            snapshot
                .find_item(&TrayItemId::new("broken-icon-item"))
                .unwrap()
                .icon(),
            TrayIcon::None
        );
    }

    fn member(service: &str, path: &str, id: &str, title: &str) -> TrayItemSnapshot {
        TrayItemSnapshot {
            id: TrayItemId::new(id),
            service: service.to_owned(),
            path: path.to_owned(),
            title: title.to_owned(),
            status: TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            icons: TrayIconFacts::named("probe-icon"),
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        }
    }

    #[test]
    fn registration_uses_signal_payload_not_property_reread() {
        // The watcher emits the signal before its own RegisteredStatusNotifierItems
        // property reflects the new entry. Applying membership from the signal
        // payload is therefore what makes an appearing item observable.
        let empty = TraySnapshot::from_items(vec![]);
        let entry = "org.kde.StatusNotifierItem-probe-1/StatusNotifierItem";
        let item = member(
            "org.kde.StatusNotifierItem-probe-1",
            "/StatusNotifierItem",
            "probe",
            "Probe",
        );

        let grown = upsert_membership(&empty, entry, Some(item.clone()));
        assert_eq!(grown.item_count(), 1);
        assert!(grown.find_item(&TrayItemId::new("probe")).is_some());
    }

    #[test]
    fn re_registration_replaces_rather_than_duplicates() {
        let entry = ":1.55/org/ayatana/NotificationItem/app";
        let first = upsert_membership(
            &TraySnapshot::from_items(vec![]),
            entry,
            Some(member(
                ":1.55",
                "/org/ayatana/NotificationItem/app",
                "app",
                "First",
            )),
        );
        let second = upsert_membership(
            &first,
            entry,
            Some(member(
                ":1.55",
                "/org/ayatana/NotificationItem/app",
                "app",
                "Second",
            )),
        );

        assert_eq!(second.item_count(), 1, "same bus entry must not duplicate");
        assert_eq!(
            second.items()[0].title,
            "Second",
            "re-registration replaces the previous observation"
        );
    }

    #[test]
    fn unresolvable_registration_clears_a_stale_entry() {
        let entry = ":1.77/org/blueman/sni";
        let present = upsert_membership(
            &TraySnapshot::from_items(vec![]),
            entry,
            Some(member(":1.77", "/org/blueman/sni", "blueman", "Bluetooth")),
        );
        assert_eq!(present.item_count(), 1);

        // A registration whose properties cannot be read must not leave the
        // previous observation standing for that bus entry.
        let unresolved = upsert_membership(&present, entry, None);
        assert_eq!(
            unresolved.item_count(),
            0,
            "unreadable item must not survive as a stale entry"
        );
    }

    #[test]
    fn disappearance_is_clean_and_idempotent() {
        let entry = "org.kde.StatusNotifierItem-3204-1/StatusNotifierItem";
        let other = ":1.1220/org/blueman/sni";
        let seeded = TraySnapshot::from_items(vec![
            member(
                "org.kde.StatusNotifierItem-3204-1",
                "/StatusNotifierItem",
                "shelly",
                "Shelly",
            ),
            member(":1.1220", "/org/blueman/sni", "blueman", "Bluetooth"),
        ]);
        assert_eq!(seeded.item_count(), 2);

        let after_drop = drop_membership(&seeded, entry);
        assert_eq!(after_drop.item_count(), 1);
        assert!(after_drop.find_item(&TrayItemId::new("shelly")).is_none());
        assert!(after_drop.find_item(&TrayItemId::new("blueman")).is_some());

        // Dropping an entry that is already absent is an exact no-op, which is
        // what keeps an unobserved disappearance from churning the runtime.
        assert_eq!(drop_membership(&after_drop, entry), after_drop);
        assert_eq!(drop_membership(&after_drop, other).item_count(), 0);
    }

    #[test]
    fn identity_is_stable_across_snapshot_refresh() {
        let entry = ":1.1373/org/ayatana/NotificationItem/steam";
        let item = member(
            ":1.1373",
            "/org/ayatana/NotificationItem/steam",
            "steam",
            "Steam",
        );

        let grown = upsert_membership(&TraySnapshot::from_items(vec![]), entry, Some(item.clone()));
        let refreshed = upsert_membership(&grown, entry, Some(item.clone()));
        assert_eq!(
            grown, refreshed,
            "re-observing an unchanged item must not perturb the snapshot"
        );
    }

    #[test]
    fn status_and_category_parse_roundtrip() {
        assert_eq!(TrayStatus::parse("Active"), TrayStatus::Active);
        assert_eq!(TrayStatus::parse("Passive"), TrayStatus::Passive);
        assert_eq!(
            TrayStatus::parse("NeedsAttention"),
            TrayStatus::NeedsAttention
        );
        assert_eq!(TrayStatus::parse("unknown"), TrayStatus::Active);

        assert_eq!(
            TrayCategory::parse("ApplicationStatus"),
            TrayCategory::ApplicationStatus
        );
        assert_eq!(
            TrayCategory::parse("Communications"),
            TrayCategory::Communications
        );
        assert_eq!(
            TrayCategory::parse("SystemServices"),
            TrayCategory::SystemServices
        );
        assert_eq!(TrayCategory::parse("Hardware"), TrayCategory::Hardware);
        assert_eq!(
            TrayCategory::parse("Custom"),
            TrayCategory::Other("Custom".into())
        );
    }
}
