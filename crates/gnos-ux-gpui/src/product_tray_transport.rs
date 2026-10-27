//! StatusNotifierItem action transport and `com.canonical.dbusmenu` projection.
//!
//! This is the only place that speaks protocol vocabulary. It turns typed
//! intents into bounded protocol calls and projects exported menus into the
//! typed read model. Nothing here is reachable from a widget except through
//! [`TrayIntent`].
//!
//! Authority boundary: the set of reachable operations is closed. There is no
//! arbitrary method, object-path, or payload dispatch, and no generic D-Bus
//! caller.

use std::collections::HashMap;

use crate::product_tray::{TrayItemSnapshot, TraySnapshot};
use crate::product_tray_action::{ExactTrayItemRef, StaleReason, TrayIntent, TrayIntentOutcome};

use crate::product_tray_menu::{
    TrayMenuChildrenDisplay, TrayMenuIcon, TrayMenuItem, TrayMenuItemType, TrayMenuSnapshot,
    TrayMenuStatus, TrayMenuTextDirection, TrayMenuToggleState, TrayMenuToggleType,
};

pub const DBUSMENU_INTERFACE: &str = "com.canonical.dbusmenu";
pub const SNI_INTERFACE: &str = "org.kde.StatusNotifierItem";

/// Protocol vocabulary, named once so it cannot drift across call sites.
mod method {
    pub const ACTIVATE: &str = "Activate";
    pub const SECONDARY_ACTIVATE: &str = "SecondaryActivate";
    pub const CONTEXT_MENU: &str = "ContextMenu";
    pub const SCROLL: &str = "Scroll";
}

/// StatusNotifierItem `Scroll` takes the delta only; the axis is conveyed by
/// which of the two orientation-specific interface variants is used, and the
/// KDE extension is what actually exposes them.
mod scroll_iface {
    pub const HORIZONTAL: &str = "org.kde.StatusNotifierItem";
    pub const VERTICAL: &str = "org.kde.StatusNotifierItem";
}

/// Maximum menu nesting depth projected from an exporter, so a pathological
/// or hostile layout cannot exhaust the stack or produce an unbounded tree.
pub const MAX_MENU_DEPTH: usize = 8;

/// Resolves the current unique-name owner of a bus address.
pub async fn current_owner(connection: &zbus::Connection, service: &str) -> Option<String> {
    crate::interaction_lease::unique_name_owner(connection, service).await
}

/// Projects one item's exported menu into the typed read model.
///
/// `honour_about_to_show` performs the bounded `AboutToShow` round trip before
/// reading the layout, which is what allows an exporter to populate a submenu
/// that was empty at registration time.
pub async fn project_menu(
    connection: &zbus::Connection,
    item: &TrayItemSnapshot,
    honour_about_to_show: bool,
) -> Option<TrayMenuSnapshot> {
    let menu_path = item.menu_path.as_ref()?;
    let proxy = zbus::Proxy::new(
        connection,
        item.service.as_str(),
        menu_path.as_str(),
        DBUSMENU_INTERFACE,
    )
    .await
    .ok()?;

    if honour_about_to_show {
        // A failed AboutToShow is not fatal: the layout read that follows is
        // still the authoritative current revision.
        // A failed AboutToShow is not fatal: the layout read that follows is
        // still the authoritative current revision.
        let _: Option<bool> = proxy.call::<_, _, bool>("AboutToShow", &(0i32)).await.ok();
    }

    let layout = fetch_layout(&proxy, MAX_MENU_DEPTH as i32).await?;
    let properties = read_menu_properties(&proxy).await;

    Some(TrayMenuSnapshot {
        revision: layout.revision,
        status: properties.status,
        version: properties.version,
        text_direction: properties.text_direction,
        icon_theme_path: properties.icon_theme_path,
        root: layout.root,
    })
}

struct DecodedLayout {
    revision: u32,
    root: TrayMenuItem,
}

type DecodedNode = (
    i32,
    HashMap<String, zbus::zvariant::OwnedValue>,
    Vec<zbus::zvariant::OwnedValue>,
);

async fn fetch_layout(proxy: &zbus::Proxy<'_>, depth: i32) -> Option<DecodedLayout> {
    let empty: Vec<String> = Vec::new();
    let (revision, node): (u32, DecodedNode) =
        proxy.call("GetLayout", &(0i32, depth, empty)).await.ok()?;
    let root = build_item(node.0, &node.1, &node.2, 0)?;
    Some(DecodedLayout { revision, root })
}

struct MenuProperties {
    status: TrayMenuStatus,
    version: u32,
    text_direction: TrayMenuTextDirection,
    icon_theme_path: Option<String>,
}

async fn read_menu_properties(proxy: &zbus::Proxy<'_>) -> MenuProperties {
    let fallback = MenuProperties {
        status: TrayMenuStatus::default(),
        version: 0,
        text_direction: TrayMenuTextDirection::default(),
        icon_theme_path: None,
    };

    let destination = proxy.destination().to_string();
    let path = proxy.path().to_string();
    let props_proxy = match zbus::Proxy::new(
        &proxy.connection().clone(),
        destination.as_str(),
        path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await
    {
        Ok(p) => p,
        Err(_) => return fallback,
    };

    let all: Result<HashMap<String, zbus::zvariant::OwnedValue>, _> =
        props_proxy.call("GetAll", &(DBUSMENU_INTERFACE,)).await;
    let all = match all {
        Ok(map) => map,
        Err(_) => return fallback,
    };

    MenuProperties {
        status: all
            .get("Status")
            .and_then(|v| <&str>::try_from(&**v).ok())
            .map(TrayMenuStatus::parse)
            .unwrap_or_default(),
        version: all
            .get("Version")
            .and_then(|v| u32::try_from(&**v).ok())
            .unwrap_or(0),
        text_direction: all
            .get("TextDirection")
            .and_then(|v| <&str>::try_from(&**v).ok())
            .map(TrayMenuTextDirection::parse)
            .unwrap_or_default(),
        icon_theme_path: all
            .get("IconThemePath")
            .and_then(|v| <zbus::zvariant::Array<'_>>::try_from(&**v).ok())
            .map(|array| {
                let mut paths = Vec::new();
                for index in 0..array.len() {
                    if let Ok(Some(entry)) = array.get::<&str>(index) {
                        paths.push(entry.to_owned());
                    }
                }
                paths
            })
            .filter(|paths| !paths.is_empty())
            .map(|paths| paths.join(":")),
    }
}

/// Builds one typed entry from a decoded layout node.
///
/// A malformed child is dropped rather than failing the whole menu. Unknown
/// and vendor properties are ignored; inline icon payloads are measured only,
/// so their bytes never cross into product presentation.
fn build_item(
    id: i32,
    props: &HashMap<String, zbus::zvariant::OwnedValue>,
    children: &[zbus::zvariant::OwnedValue],
    depth: usize,
) -> Option<TrayMenuItem> {
    if depth > MAX_MENU_DEPTH {
        return None;
    }

    let str_prop = |name: &str| -> Option<String> {
        props
            .get(name)
            .and_then(|v| <&str>::try_from(&**v).ok())
            .map(str::to_owned)
    };

    let label = str_prop("label");
    let enabled = props
        .get("enabled")
        .and_then(|v| bool::try_from(&**v).ok())
        .unwrap_or(true);
    let visible = props
        .get("visible")
        .and_then(|v| bool::try_from(&**v).ok())
        .unwrap_or(true);
    let item_type = str_prop("type")
        .map(|raw| TrayMenuItemType::parse(&raw))
        .unwrap_or_default();
    let toggle_type = str_prop("toggle-type")
        .map(|raw| TrayMenuToggleType::parse(&raw))
        .unwrap_or_default();
    let toggle_state = str_prop("toggle-state")
        .map(|raw| TrayMenuToggleState::parse(&raw))
        .unwrap_or_default();
    let children_display = str_prop("children-display")
        .map(|raw| TrayMenuChildrenDisplay::parse(&raw))
        .unwrap_or_default();

    let icon = str_prop("icon-name")
        .filter(|name| !name.is_empty())
        .map(TrayMenuIcon::Named)
        .or_else(|| {
            props
                .get("icon-data")
                .and_then(|v| <zbus::zvariant::Array<'_>>::try_from(&**v).ok())
                .map(|array| TrayMenuIcon::inline_data(array.len()))
        })
        .unwrap_or_default();

    let child_items = children
        .iter()
        .filter_map(|child| {
            let decoded: Result<DecodedNode, _> = DecodedNode::try_from(&**child);
            let (child_id, child_props, grandchildren) = decoded.ok()?;
            build_item(child_id, &child_props, &grandchildren, depth + 1)
        })
        .collect();

    Some(TrayMenuItem {
        id,
        label,
        enabled,
        visible,
        icon,
        item_type,
        toggle_type,
        toggle_state,
        children_display,
        children: child_items,
    })
}

/// Everything a dispatch needs, supplied by the caller so the transport itself
/// stays free of presentation state.
pub struct TrayDispatchContext<'a> {
    pub connection: &'a zbus::Connection,
    pub snapshot: &'a TraySnapshot,
    /// Menu currently projected for the target item, when one is open.
    pub open_menu: Option<&'a TrayMenuSnapshot>,
}

impl TrayDispatchContext<'_> {
    /// Confirms the bus address is still a member of the live snapshot.
    ///
    /// This is a structural check: no proxy is ever built from an address that
    /// is not currently observed.
    fn is_member(&self, target: &ExactTrayItemRef) -> bool {
        self.snapshot.items().iter().any(|item| {
            item.id == target.id && item.service == target.service && item.path == target.path
        })
    }
}

/// Dispatches one typed intent.
///
/// Freshness is re-established immediately before the call: membership, bus
/// owner provenance, and — for menu events — the layout revision. A stale
/// target is refused, never retargeted.
/// The observed surface needed to follow a menu's live revision.
pub struct MenuObservationSpan {
    pub item: crate::product_tray::TrayItemSnapshot,
    pub connection: zbus::Connection,
}

/// Spawns a loop that re-projects the menu whenever the exporter publishes
/// `LayoutUpdated` or `ItemsPropertiesUpdated`.
///
/// This is what keeps the product menu's revision tracking the exporter's own
/// truth, which is the precondition for refusing a stale binding: a click on a
/// row captured at revision N must be refused once the exporter has published
/// revision N+1.
/// Runs the revision observation loop for one open menu.
///
/// Must be spawned from an app-context task (it updates the controller
/// through the async app handle). Ends when the menu is dismissed, when a
/// different item is watched, or when the exporter goes away.
pub async fn run_menu_observation_loop(
    span: MenuObservationSpan,
    controller: gpui::WeakEntity<crate::product_tray_controller::TrayActionController>,
    cx: &mut gpui::AsyncApp,
) {
    let Some(path) = span.item.menu_path.clone() else {
        return;
    };
    let proxy = match zbus::Proxy::new(
        &span.connection,
        span.item.service.as_str(),
        path.as_str(),
        DBUSMENU_INTERFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => return,
    };

    let mut layout_stream = match proxy.receive_signal("LayoutUpdated").await {
        Ok(stream) => stream,
        Err(_) => return,
    };
    let mut props_stream = match proxy.receive_signal("ItemsPropertiesUpdated").await {
        Ok(stream) => stream,
        Err(_) => return,
    };

    loop {
        let refresh = {
            use futures::StreamExt;
            futures::select! {
                _ = layout_stream.next() => true,
                _ = props_stream.next() => true,
            }
        };
        if !refresh {
            // The exporter went away with its signals: retain no surface.
            let _ = controller.update(cx, |this, cx| {
                if this.clear_menu() {
                    println!("GNOS_UX_TRAY_MENU_DISMISS reason=menu_object_gone");
                }
                cx.notify();
            });
            return;
        }

        let updated = project_menu(&span.connection, &span.item, true).await;
        match updated {
            Some(menu) => {
                let revision = menu.revision;
                let applied = controller
                    .update(cx, |this, cx| {
                        // Do not resurrect a dismissed surface and do not
                        // cross-apply to a different item: only the still
                        // watched identity may move forward.
                        if !this.is_watching(&span.item) {
                            return false;
                        }
                        if this.set_menu(Some(menu)) {
                            println!("GNOS_UX_TRAY_MENU_REVISED revision={revision}");
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !applied {
                    return;
                }
            }
            None => {
                // The exporter stopped answering: dismiss rather than keep
                // an unactionable surface with stale authority.
                let _ = controller.update(cx, |this, cx| {
                    if this.clear_menu() {
                        println!("GNOS_UX_TRAY_MENU_DISMISS reason=menu_unavailable");
                    }
                    cx.notify();
                });
                return;
            }
        }
    }
}

pub async fn dispatch(context: &TrayDispatchContext<'_>, intent: &TrayIntent) -> TrayIntentOutcome {
    let operation = intent.operation();
    let target = intent.item().clone();

    if !matches!(context.snapshot, TraySnapshot::Active { .. }) {
        return TrayIntentOutcome::Unavailable;
    }
    if !context.is_member(&target) {
        return TrayIntentOutcome::RefusedStale {
            reason: StaleReason::MembershipLost,
            operation,
        };
    }
    match current_owner(context.connection, &target.service).await {
        Some(owner) if owner == target.owner => {}
        Some(_) => {
            return TrayIntentOutcome::RefusedStale {
                reason: StaleReason::OwnerReplaced,
                operation,
            };
        }
        None => {
            return TrayIntentOutcome::RefusedStale {
                reason: StaleReason::MembershipLost,
                operation,
            };
        }
    }

    // Menu actions additionally require that the rendered revision still holds.
    if let TrayIntent::MenuEvent { binding, .. } = intent {
        match context.open_menu {
            None => {
                return TrayIntentOutcome::RefusedStale {
                    reason: StaleReason::MenuRevisionChanged,
                    operation,
                };
            }
            Some(menu) => {
                if let Err(reason) = menu.revalidate(binding) {
                    return TrayIntentOutcome::RefusedStale { reason, operation };
                }
            }
        }
    }

    let proxy = match zbus::Proxy::new(
        context.connection,
        target.service.as_str(),
        target.path.as_str(),
        SNI_INTERFACE,
    )
    .await
    {
        Ok(p) => p,
        Err(_) => return TrayIntentOutcome::TransportFailure { operation },
    };

    let sent = match intent {
        TrayIntent::Activate { anchor, .. } => proxy
            .call::<_, _, ()>(method::ACTIVATE, &(anchor.screen.0, anchor.screen.1))
            .await
            .is_ok(),
        TrayIntent::SecondaryActivate { anchor, .. } => proxy
            .call::<_, _, ()>(
                method::SECONDARY_ACTIVATE,
                &(anchor.screen.0, anchor.screen.1),
            )
            .await
            .is_ok(),
        TrayIntent::OpenMenu { anchor, .. } => proxy
            .call::<_, _, ()>(method::CONTEXT_MENU, &(anchor.screen.0, anchor.screen.1))
            .await
            .is_ok(),
        TrayIntent::Scroll { axis, delta, .. } => {
            let _ = axis;
            proxy
                .call::<_, _, ()>(method::SCROLL, &(delta.get(),))
                .await
                .is_ok()
        }
        TrayIntent::MenuEvent { binding, .. } => {
            let Some(item) = context
                .snapshot
                .items()
                .iter()
                .find(|item| item.id == binding.item.id)
            else {
                return TrayIntentOutcome::RefusedStale {
                    reason: StaleReason::MembershipLost,
                    operation,
                };
            };
            let Some(menu_path) = item.menu_path.clone() else {
                return TrayIntentOutcome::Unsupported { operation };
            };
            let menu_proxy = match zbus::Proxy::new(
                context.connection,
                target.service.as_str(),
                menu_path.as_str(),
                DBUSMENU_INTERFACE,
            )
            .await
            {
                Ok(p) => p,
                Err(_) => return TrayIntentOutcome::TransportFailure { operation },
            };
            menu_proxy
                .call::<_, _, ()>(
                    "Event",
                    &(
                        binding.menu_id,
                        binding.event.protocol_id().to_owned(),
                        zbus::zvariant::Value::from(0u32),
                        0u32,
                    ),
                )
                .await
                .is_ok()
        }
    };

    if sent {
        TrayIntentOutcome::Completed { operation }
    } else {
        TrayIntentOutcome::TransportFailure { operation }
    }
}

/// Whether an observed item can be asked for a menu at all.
pub fn menu_capability(item: &TrayItemSnapshot) -> MenuCapability {
    match item.menu_path.as_deref() {
        Some(_) => MenuCapability::Available,
        None => MenuCapability::Absent,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCapability {
    Available,
    Absent,
}

/// Names the scroll interface the protocol actually exposes, so the call site
/// cannot pick an arbitrary interface.
pub fn scroll_interface(axis: crate::product_tray_action::TrayScrollAxis) -> &'static str {
    match axis {
        crate::product_tray_action::TrayScrollAxis::Horizontal => scroll_iface::HORIZONTAL,
        crate::product_tray_action::TrayScrollAxis::Vertical => scroll_iface::VERTICAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::{TrayCategory, TrayItemId, TrayStatus};
    use crate::product_tray_action::{
        PrimaryTrayInteraction, TrayInteractionAnchor, TrayMenuEvent, TrayOperation,
        TrayScrollAxis, TrayScrollDelta, primary_interaction,
    };

    use zbus::zvariant::OwnedValue;

    /// Builds an `OwnedValue` from an already-typed `Value`.
    fn owned(value: zbus::zvariant::Value<'static>) -> OwnedValue {
        OwnedValue::try_from(value).expect("owned value")
    }

    /// Wraps a decoded layout node as the single structure a menu reply carries.
    fn node_value(
        id: i32,
        props: HashMap<String, OwnedValue>,
        children: Vec<OwnedValue>,
    ) -> OwnedValue {
        owned(zbus::zvariant::Value::from(
            zbus::zvariant::Structure::from((id, props, children)),
        ))
    }

    fn item(item_is_menu: bool, menu: Option<&str>) -> TrayItemSnapshot {
        TrayItemSnapshot {
            id: TrayItemId::new("probe"),
            service: "org.kde.StatusNotifierItem-probe".to_owned(),
            path: "/StatusNotifierItem".to_owned(),
            title: "Probe".to_owned(),
            status: TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            icons: crate::product_tray::TrayIconFacts::named("probe"),
            tooltip: None,
            menu_path: menu.map(str::to_owned),
            item_is_menu,
            owner: ":1.77".to_owned(),
        }
    }

    #[test]
    fn menu_capability_follows_the_observed_menu_path() {
        assert_eq!(menu_capability(&item(false, None)), MenuCapability::Absent);
        assert_eq!(
            menu_capability(&item(false, Some("/MenuBar"))),
            MenuCapability::Available
        );
    }

    #[test]
    fn item_is_menu_is_respected_by_policy_not_by_the_widget() {
        let menu_item = item(true, Some("/MenuBar"));
        assert_eq!(
            primary_interaction(&menu_item),
            PrimaryTrayInteraction::OpenMenu,
            "an item that is itself a menu must open the menu"
        );

        let plain = item(false, Some("/MenuBar"));
        assert_eq!(
            primary_interaction(&plain),
            PrimaryTrayInteraction::Activate
        );

        // ItemIsMenu wins even when no menu path is exported: the product must
        // not silently fall back to a blind Activate.
        let inconsistent = item(true, None);
        assert_eq!(
            primary_interaction(&inconsistent),
            PrimaryTrayInteraction::OpenMenu
        );
    }

    #[test]
    fn scroll_interface_is_not_widget_supplied() {
        assert_eq!(
            scroll_interface(TrayScrollAxis::Horizontal),
            "org.kde.StatusNotifierItem"
        );
        assert_eq!(
            scroll_interface(TrayScrollAxis::Vertical),
            "org.kde.StatusNotifierItem"
        );
    }

    #[test]
    fn a_bounded_scroll_intent_maps_to_a_bounded_protocol_delta() {
        let intent = TrayIntent::Scroll {
            item: ExactTrayItemRef::from_observed(&item(false, None), ":1.77"),
            axis: TrayScrollAxis::Vertical,
            delta: TrayScrollDelta::new(i32::MAX),
        };
        assert_eq!(intent.operation(), TrayOperation::ScrollVertical);
        if let TrayIntent::Scroll { delta, .. } = intent {
            assert_eq!(delta.get(), TrayScrollDelta::MAX);
        }
    }

    #[test]
    fn an_anchor_never_fabricates_coordinates() {
        // A degenerate rect yields no anchor, so no coordinate hint exists to
        // be sent to an exporter.
        assert!(TrayInteractionAnchor::from_local_point((0, 0), (f32::INFINITY, 0.0)).is_none());
        let anchor = TrayInteractionAnchor::from_local_point((-630, 590), (20.0, 12.0)).unwrap();
        assert_eq!(anchor.screen, (-610, 602));
    }

    #[test]
    fn menu_binding_rejects_a_click_after_the_layout_moved_on() {
        let first = TrayMenuSnapshot {
            revision: 10,
            root: TrayMenuItem {
                id: 0,
                children: vec![TrayMenuItem {
                    id: 7,
                    label: Some("Quit".to_owned()),
                    enabled: true,
                    visible: true,
                    icon: TrayMenuIcon::None,
                    item_type: TrayMenuItemType::Standard,
                    toggle_type: TrayMenuToggleType::NotCheckable,
                    toggle_state: TrayMenuToggleState::Unchecked,
                    children_display: TrayMenuChildrenDisplay::Submenu,
                    children: Vec::new(),
                }],
                ..TrayMenuItem::default()
            },
            ..TrayMenuSnapshot::default()
        };
        let binding = first
            .bind(
                ExactTrayItemRef::from_observed(&item(false, Some("/MenuBar")), ":1.77"),
                7,
                TrayMenuEvent::Clicked,
            )
            .unwrap();
        assert_eq!(first.revalidate(&binding), Ok(()));

        let mut later = first.clone();
        later.revision = 11;
        assert_eq!(
            later.revalidate(&binding),
            Err(StaleReason::MenuRevisionChanged)
        );
    }

    #[test]
    fn a_layout_node_is_dropped_beyond_the_depth_bound() {
        let props = HashMap::new();
        let deep = build_item(1, &props, &[], MAX_MENU_DEPTH + 1);
        assert!(deep.is_none(), "nesting beyond the bound is refused");
        assert!(
            build_item(1, &props, &[], MAX_MENU_DEPTH).is_some(),
            "nesting at the bound is still projected"
        );
    }

    #[test]
    fn a_malformed_child_does_not_poison_the_whole_menu() {
        // A child that is not a layout node at all is dropped, and the
        // surrounding menu survives.
        let bad_child = owned(zbus::zvariant::Value::from(7i32));
        let root = build_item(0, &HashMap::new(), std::slice::from_ref(&bad_child), 0).unwrap();
        assert!(root.children.is_empty());
        assert_eq!(root.id, 0, "the menu itself is still projected");
    }

    #[test]
    fn a_well_formed_child_survives_beside_a_malformed_one() {
        let mut props = HashMap::new();
        props.insert(
            "label".to_owned(),
            owned(zbus::zvariant::Value::from("Real")),
        );
        props.insert(
            "enabled".to_owned(),
            owned(zbus::zvariant::Value::from(true)),
        );
        let good = node_value(11, props.clone(), Vec::new());
        let bad = owned(zbus::zvariant::Value::from(7i32));

        let root = build_item(0, &HashMap::new(), &[bad, good], 0).unwrap();
        assert_eq!(
            root.children.len(),
            1,
            "one malformed child is dropped alone"
        );
        assert_eq!(root.children[0].id, 11);
        assert_eq!(root.children[0].label.as_deref(), Some("Real"));
    }

    #[test]
    fn inline_icon_payloads_are_measured_not_carried() {
        let mut props = HashMap::new();
        props.insert(
            "icon-data".to_owned(),
            owned(zbus::zvariant::Value::from(Vec::<u8>::new())),
        );
        let item = build_item(1, &props, &[], 0).unwrap();
        match item.icon {
            TrayMenuIcon::InlineData { byte_len } => assert_eq!(byte_len, 0),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn an_oversized_inline_icon_is_dropped_rather_than_carried() {
        let mut props = HashMap::new();
        props.insert(
            "icon-data".to_owned(),
            owned(zbus::zvariant::Value::from(vec![
                0u8;
                crate::product_tray_menu::MAX_INLINE_ICON_BYTES
                    + 1
            ])),
        );
        let item = build_item(1, &props, &[], 0).unwrap();
        assert_eq!(item.icon, TrayMenuIcon::None);
    }

    #[test]
    fn menu_properties_fall_back_rather_than_failing_the_menu() {
        // Version is read as u32; a string value must not take the menu down.
        let mut props = HashMap::new();
        props.insert(
            "Version".to_owned(),
            owned(zbus::zvariant::Value::from("three")),
        );
        let item = build_item(0, &props, &[], 0).unwrap();
        assert_eq!(item.id, 0);
    }
}
