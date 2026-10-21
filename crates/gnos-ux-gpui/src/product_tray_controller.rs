//! Product action routing for tray interaction.
//!
//! Widgets emit a typed [`TrayIntent`] and receive a typed
//! [`TrayIntentOutcome`]. Nothing in this path exposes a D-Bus method, object
//! path, interface name, or error object to presentation, and the set of
//! reachable operations stays closed.

use gpui::{App, AppContext, Entity};

use crate::product_tray::TraySnapshot;
use crate::product_tray_action::{
    ExactTrayItemRef, PrimaryTrayInteraction, StaleReason, TrayIntent, TrayIntentOutcome,
    TrayMenuEvent, TrayOperation, primary_interaction_policy,
};
use crate::product_tray_menu::TrayMenuSnapshot;
use crate::product_tray_transport::{MAX_MENU_DEPTH, MenuCapability, TrayDispatchContext};

#[derive(Clone, Default)]
pub struct TrayActionController {
    menu: Option<TrayMenuSnapshot>,
    last_outcome: Option<TrayIntentOutcome>,
    /// Output that opened the current menu. Exactly one output owns the
    /// surface; other outputs never render it.
    open_requested_by: Option<String>,
    /// Bus identity currently followed by the revision observation loop.
    /// `None` means no loop is running.
    watching: Option<String>,
    /// Exact item identity the open menu belongs to, for lifetime validation.
    open_item: Option<ExactTrayItemRef>,
}

impl TrayActionController {
    /// Menu currently projected for the open item, if any.
    pub fn menu(&self) -> Option<&TrayMenuSnapshot> {
        self.menu.as_ref()
    }

    pub fn last_outcome(&self) -> Option<&TrayIntentOutcome> {
        self.last_outcome.as_ref()
    }

    pub fn clear_menu(&mut self) -> bool {
        self.open_requested_by = None;
        self.open_item = None;
        self.watching = None;
        self.menu.take().is_some()
    }

    pub fn open_item(&self) -> Option<&ExactTrayItemRef> {
        self.open_item.as_ref()
    }

    pub fn set_open(&mut self, output: Option<String>, item: Option<ExactTrayItemRef>) {
        self.open_requested_by = output;
        self.open_item = item;
    }

    /// Records which output opened the current menu.
    pub fn set_open_requested_by(&mut self, output: Option<String>) {
        self.open_requested_by = output;
    }

    /// The output that opened the current menu.
    pub fn open_requested_by(&self) -> Option<&str> {
        self.open_requested_by.as_deref()
    }

    /// Records the projected menu for the open item.
    pub fn set_menu(&mut self, menu: Option<TrayMenuSnapshot>) -> bool {
        if self.menu == menu {
            return false;
        }
        self.menu = menu;
        true
    }

    /// Refuses a menu action that no longer denotes its rendered target.
    pub fn revalidate_menu_binding(
        &self,
        binding: &crate::product_tray_action::TrayMenuEventBinding,
    ) -> Result<(), TrayIntentOutcome> {
        match self.menu.as_ref() {
            None => Err(TrayIntentOutcome::RefusedStale {
                reason: StaleReason::MenuRevisionChanged,
                operation: TrayOperation::MenuEventClicked,
            }),
            Some(menu) => {
                menu.revalidate(binding)
                    .map_err(|reason| TrayIntentOutcome::RefusedStale {
                        reason,
                        operation: TrayOperation::MenuEventClicked,
                    })
            }
        }
    }

    /// Resolves the primary interaction for a rendered item without touching
    /// the bus, so the policy stays testable and widget-independent.
    pub fn resolve_primary(&self, item_is_menu: bool, has_menu: bool) -> PrimaryTrayInteraction {
        primary_interaction_policy(item_is_menu, has_menu)
    }

    /// Projects an item's exported menu, honouring `AboutToShow`.
    ///
    /// The menu is captured at open time rather than at registration, because
    /// an exporter is allowed to populate a submenu only once it is about to be
    /// shown. Takes `&self` so the caller can await without holding a mutable
    /// borrow across the round trip.
    pub async fn read_menu(
        &self,
        connection: &zbus::Connection,
        item: &crate::product_tray::TrayItemSnapshot,
    ) -> (TrayIntentOutcome, Option<TrayMenuSnapshot>) {
        let operation = TrayOperation::ContextMenu;
        if crate::product_tray_transport::menu_capability(item) == MenuCapability::Absent {
            return (TrayIntentOutcome::Unsupported { operation }, None);
        }
        match crate::product_tray_transport::project_menu(connection, item, true).await {
            Some(menu) => (TrayIntentOutcome::Completed { operation }, Some(menu)),
            None => (TrayIntentOutcome::TransportFailure { operation }, None),
        }
    }

    /// Dispatches one typed intent against the currently projected menu.
    pub async fn dispatch(
        &self,
        connection: &zbus::Connection,
        snapshot: &TraySnapshot,
        intent: &TrayIntent,
    ) -> TrayIntentOutcome {
        let context = TrayDispatchContext {
            connection,
            snapshot,
            open_menu: self.menu.as_ref(),
        };
        crate::product_tray_transport::dispatch(&context, intent).await
    }

    /// Applies a resolved outcome and any newly projected menu.
    pub fn apply(
        &mut self,
        menu: Option<TrayMenuSnapshot>,
        outcome: TrayIntentOutcome,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(menu) = menu {
            self.set_menu(Some(menu));
        }
        self.record(outcome);
        cx.notify();
    }

    /// Records which bus identity the revision observation loop follows.
    ///
    /// Returns `true` when the caller should spawn the loop. Re-calling for
    /// the identity already being watched returns `false` rather than starting
    /// a second loop. Clearing the menu stops watching, which also ends any
    /// running loop without resurrecting the surface.
    pub fn note_observing(&mut self, item: &crate::product_tray::TrayItemSnapshot) -> bool {
        let key = format!("{}:{}", item.service, item.path);
        if self.watching.as_deref() == Some(&key) {
            return false;
        }
        self.watching = Some(key);
        true
    }

    /// Whether the observation loop is still following exactly this item.
    pub fn is_watching(&self, item: &crate::product_tray::TrayItemSnapshot) -> bool {
        let key = format!("{}:{}", item.service, item.path);
        self.watching.as_deref() == Some(&key)
    }

    /// Binds a menu row to the revision it was rendered from.
    pub fn bind_menu_item(
        &self,
        item: ExactTrayItemRef,
        menu_id: i32,
    ) -> Option<crate::product_tray_action::TrayMenuEventBinding> {
        self.menu
            .as_ref()?
            .bind(item, menu_id, TrayMenuEvent::Clicked)
    }

    fn record(&mut self, outcome: TrayIntentOutcome) -> TrayIntentOutcome {
        self.last_outcome = Some(outcome.clone());
        outcome
    }
}

/// Shared controller entity, so the tray can be driven from the scene host
/// without introducing a second action owner.
/// Projects an item's exported menu, honouring `AboutToShow`.
///
/// The menu is captured at open time rather than at registration, because an
/// exporter is allowed to populate a submenu only once it is about to be shown.
pub async fn read_exported_menu(
    connection: &zbus::Connection,
    item: &crate::product_tray::TrayItemSnapshot,
) -> (TrayIntentOutcome, Option<TrayMenuSnapshot>) {
    let operation = TrayOperation::ContextMenu;
    if crate::product_tray_transport::menu_capability(item) == MenuCapability::Absent {
        return (TrayIntentOutcome::Unsupported { operation }, None);
    }
    match crate::product_tray_transport::project_menu(connection, item, true).await {
        Some(menu) => (TrayIntentOutcome::Completed { operation }, Some(menu)),
        None => (TrayIntentOutcome::TransportFailure { operation }, None),
    }
}

/// Dispatches one typed intent against an explicitly supplied menu projection.
pub async fn dispatch_intent(
    connection: &zbus::Connection,
    snapshot: &TraySnapshot,
    open_menu: Option<&TrayMenuSnapshot>,
    intent: &TrayIntent,
) -> TrayIntentOutcome {
    let context = TrayDispatchContext {
        connection,
        snapshot,
        open_menu,
    };
    crate::product_tray_transport::dispatch(&context, intent).await
}

#[derive(Clone)]
pub struct TrayActionGlobal(pub Entity<TrayActionController>);

impl gpui::Global for TrayActionGlobal {}

/// Returns the shared controller, creating it on first use.
pub fn shared_controller(cx: &mut App) -> Entity<TrayActionController> {
    if let Some(global) = cx.try_global::<TrayActionGlobal>() {
        return global.0.clone();
    }
    let entity = cx.new(|_| TrayActionController::default());
    cx.set_global(TrayActionGlobal(entity.clone()));
    entity
}

/// Menu nesting depth the product renders, matching the projection bound.
pub const RENDERED_MENU_DEPTH: usize = MAX_MENU_DEPTH;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::{TrayCategory, TrayItemId, TrayItemSnapshot, TrayStatus};
    use crate::product_tray_action::TrayMenuEventBinding;
    use crate::product_tray_menu::{TrayMenuIcon, TrayMenuItem, TrayMenuSnapshot};

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

    fn menu(revision: u32, label: &str) -> TrayMenuSnapshot {
        TrayMenuSnapshot {
            revision,
            status: crate::product_tray_menu::TrayMenuStatus::Normal,
            version: 3,
            text_direction: crate::product_tray_menu::TrayMenuTextDirection::LeftToRight,
            icon_theme_path: None,
            root: TrayMenuItem {
                id: 0,
                children: vec![TrayMenuItem {
                    id: 4,
                    label: Some(label.to_owned()),
                    enabled: true,
                    visible: true,
                    icon: TrayMenuIcon::None,
                    item_type: crate::product_tray_menu::TrayMenuItemType::Standard,
                    toggle_type: crate::product_tray_menu::TrayMenuToggleType::NotCheckable,
                    toggle_state: crate::product_tray_menu::TrayMenuToggleState::Unchecked,
                    children_display: crate::product_tray_menu::TrayMenuChildrenDisplay::Submenu,
                    children: Vec::new(),
                }],
                ..TrayMenuItem::default()
            },
        }
    }

    fn item_ref() -> ExactTrayItemRef {
        ExactTrayItemRef::from_observed(&item(false, Some("/MenuBar")), ":1.77")
    }

    #[test]
    fn an_item_that_is_a_menu_resolves_to_the_menu_path() {
        let controller = TrayActionController::default();
        assert_eq!(
            controller.resolve_primary(true, true),
            PrimaryTrayInteraction::OpenMenu
        );
        // Even without an exported menu path, the product must not fall back to
        // a blind Activate.
        assert_eq!(
            controller.resolve_primary(true, false),
            PrimaryTrayInteraction::OpenMenu
        );
        assert_eq!(
            controller.resolve_primary(false, true),
            PrimaryTrayInteraction::Activate
        );
    }

    #[test]
    fn a_binding_cannot_be_created_without_a_projected_menu() {
        let controller = TrayActionController::default();
        assert!(controller.bind_menu_item(item_ref(), 4).is_none());
    }

    #[test]
    fn a_binding_is_bound_to_the_revision_it_was_rendered_from() {
        let mut controller = TrayActionController::default();
        controller.set_menu(Some(menu(77, "Quit")));
        let binding = controller
            .bind_menu_item(item_ref(), 4)
            .expect("entry exists in this revision");
        assert_eq!(binding.revision, 77);
        assert_eq!(binding.rendered_label.as_deref(), Some("Quit"));
        assert_eq!(controller.revalidate_menu_binding(&binding), Ok(()));
    }

    #[test]
    fn a_layout_update_invalidates_an_already_rendered_action() {
        let mut controller = TrayActionController::default();
        controller.set_menu(Some(menu(77, "Quit")));
        let binding = controller.bind_menu_item(item_ref(), 4).unwrap();

        controller.set_menu(Some(menu(78, "Quit")));
        let outcome = controller.revalidate_menu_binding(&binding).unwrap_err();
        assert!(
            matches!(
                outcome,
                TrayIntentOutcome::RefusedStale {
                    reason: StaleReason::MenuRevisionChanged,
                    ..
                }
            ),
            "a click rendered before LayoutUpdated must be refused, not retargeted"
        );
    }

    #[test]
    fn a_repurposed_entry_is_refused_even_without_a_revision_change() {
        let mut controller = TrayActionController::default();
        controller.set_menu(Some(menu(77, "Quit")));
        let binding = controller.bind_menu_item(item_ref(), 4).unwrap();

        controller.set_menu(Some(menu(77, "Restart")));
        let outcome = controller.revalidate_menu_binding(&binding).unwrap_err();
        assert!(matches!(
            outcome,
            TrayIntentOutcome::RefusedStale {
                reason: StaleReason::MenuItemAbsent,
                ..
            }
        ));
    }

    #[test]
    fn clearing_the_menu_invalidates_every_outstanding_binding() {
        let mut controller = TrayActionController::default();
        controller.set_menu(Some(menu(77, "Quit")));
        let binding = controller.bind_menu_item(item_ref(), 4).unwrap();

        assert!(controller.clear_menu());
        let outcome = controller.revalidate_menu_binding(&binding).unwrap_err();
        assert!(matches!(
            outcome,
            TrayIntentOutcome::RefusedStale {
                reason: StaleReason::MenuRevisionChanged,
                ..
            }
        ));
    }

    #[test]
    fn setting_an_identical_menu_is_not_a_change() {
        let mut controller = TrayActionController::default();
        assert!(controller.set_menu(Some(menu(77, "Quit"))));
        assert!(
            !controller.set_menu(Some(menu(77, "Quit"))),
            "an unchanged projection must not churn the controller"
        );
    }

    #[test]
    fn outcome_tones_map_to_product_feedback() {
        assert_eq!(
            TrayIntentOutcome::Completed {
                operation: TrayOperation::Activate
            }
            .feedback_tone(),
            crate::product_tray_action::TrayOutcomeTone::Success
        );
        assert_eq!(
            TrayIntentOutcome::Unsupported {
                operation: TrayOperation::ContextMenu
            }
            .feedback_tone(),
            crate::product_tray_action::TrayOutcomeTone::Neutral
        );
        assert_eq!(
            TrayIntentOutcome::TransportFailure {
                operation: TrayOperation::Activate
            }
            .feedback_tone(),
            crate::product_tray_action::TrayOutcomeTone::Danger
        );
        assert!(
            !TrayIntentOutcome::Unavailable.is_completed(),
            "an unavailable provider is not a completed action"
        );
    }

    #[test]
    fn a_binding_never_carries_protocol_vocabulary() {
        // The binding is product data: it names a menu id and a revision, not a
        // method or an object path.
        let mut controller = TrayActionController::default();
        controller.set_menu(Some(menu(1, "Quit")));
        let binding: TrayMenuEventBinding = controller.bind_menu_item(item_ref(), 4).unwrap();
        assert_eq!(binding.event.protocol_id(), "clicked");
        assert!(binding.item.path.starts_with('/'));
        assert!(binding.menu_id > 0);
    }
}
